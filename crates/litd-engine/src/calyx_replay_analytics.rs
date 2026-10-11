//! #907: offline replay & telemetry analytics loop — ingest real match replays
//! (tick-stamped attack event log + structured unit/ability stats) into the
//! Calyx-backed engine database as structured rows, then run measured-balance
//! Assay, desync forensics, and crash-recurrence Oracle passes over them.
//!
//! Strictly OFFLINE and read-only with respect to the sim: nothing here runs
//! inside `World.Step()` or the render frame (#777 §6). Every replay is
//! re-executed through the deterministic `SimReplayRecord` path (hash
//! verified) before anything is stored; outcomes are derived from the actual
//! executed attack events, never from caller claims.

use std::collections::{BTreeMap, BTreeSet};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    EngineError,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
    sim_replay::SimReplayRecord,
};

pub const REPLAY_MATCH_SCHEMA: &str = "litd.engine.replay_analytics.match.v1";
pub const REPLAY_MATCH_BATCH_SCHEMA: &str = "litd.engine.replay_analytics.batch.v1";

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineReplayAnalyticsError {
    #[error("REPLAY_ANALYTICS_INVALID_INPUT: field={field} reason={reason}")]
    InvalidInput { field: String, reason: String },
    #[error("REPLAY_ANALYTICS_OUTCOME: match_id={match_id} reason={reason}")]
    Outcome { match_id: String, reason: String },
    #[error("REPLAY_ANALYTICS_REPLAY_EXECUTION: match_id={match_id} error={error}")]
    ReplayExecution { match_id: String, error: String },
    #[error("REPLAY_ANALYTICS_ASSAY: detail={0}")]
    Assay(String),
    #[error("REPLAY_ANALYTICS_FORENSICS: case_id={case_id} reason={reason}")]
    Forensics { case_id: String, reason: String },
    #[error("REPLAY_ANALYTICS_ORACLE: forecast_id={forecast_id} reason={reason}")]
    Oracle { forecast_id: String, reason: String },
    #[error("REPLAY_ANALYTICS_PERSISTENCE: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("REPLAY_ANALYTICS_CODEC: {0}")]
    Codec(String),
    #[error(
        "REPLAY_ANALYTICS_READBACK_MISMATCH: field={field} expected={expected} actual={actual}"
    )]
    ReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },
}

/// One real match: a hash-verified deterministic replay plus the structured
/// unit/ability stats that were in force for the subject team, and the body
/// rosters that define the two sides. The win/loss label is NOT an input —
/// it is derived from the executed replay's attack events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayMatchRecord {
    pub match_id: String,
    pub replay: SimReplayRecord,
    pub subject_bodies: Vec<u32>,
    pub opponent_bodies: Vec<u32>,
    /// Structured stats in milli-units, e.g. "damage_milli" -> 4200.
    pub stats_milli: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMatchOutcome {
    SubjectWin,
    SubjectLoss,
}

impl ReplayMatchOutcome {
    pub const fn is_win(self) -> bool {
        matches!(self, Self::SubjectWin)
    }
}

/// Tick-stamped event log row extracted from the executed replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayTickEvent {
    pub tick: u64,
    pub attacker: u32,
    pub target: u32,
    pub damage_fp: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayMatchStoredPayload {
    pub schema: String,
    pub batch_id: String,
    pub match_id: String,
    pub outcome: ReplayMatchOutcome,
    pub subject_damage_fp: i64,
    pub opponent_damage_fp: i64,
    pub stats_milli: BTreeMap<String, i64>,
    pub tick_events: Vec<ReplayTickEvent>,
    pub replay_ron_sha256: String,
    pub initial_state_hash: String,
    pub final_state_hash: String,
    pub run_ticks: u64,
    pub orders_applied: u64,
    pub attack_events_total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayMatchReadback {
    pub database: EngineCalyxDatabaseReadback,
    pub payload: ReplayMatchStoredPayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayBatchPayload {
    pub schema: String,
    pub batch_id: String,
    pub match_row_ids: Vec<String>,
    pub match_ids: Vec<String>,
    pub match_count: usize,
    pub win_count: usize,
    pub loss_count: usize,
    pub stat_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayBatchReadback {
    pub database: EngineCalyxDatabaseReadback,
    pub payload: ReplayBatchPayload,
    pub matches: Vec<ReplayMatchReadback>,
}

pub fn replay_match_row_id(batch_id: &str, match_id: &str) -> String {
    format!("replay_match/{batch_id}/{match_id}")
}

pub fn replay_batch_row_id(batch_id: &str) -> String {
    format!("replay_match_batch/{batch_id}")
}

/// Ingest a batch of real match replays into the Calyx-backed engine database.
/// Each replay is re-executed (deterministic, hash-verified); the win/loss
/// outcome and the tick-stamped event log come from that execution.
pub fn store_replay_match_batch<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
    database: &EngineCalyxDatabase,
    batch_id: &str,
    matches: &[ReplayMatchRecord],
) -> Result<ReplayBatchReadback, EngineReplayAnalyticsError> {
    validate_id("batch_id", batch_id)?;
    validate_batch(matches)?;
    let stat_keys: Vec<String> = matches[0].stats_milli.keys().cloned().collect();
    let mut match_readbacks = Vec::with_capacity(matches.len());
    let mut win_count = 0_usize;
    for record in matches {
        let payload = execute_and_derive::<MAX_BODIES, MAX_ORDERS>(batch_id, record)?;
        if payload.outcome.is_win() {
            win_count += 1;
        }
        let row_id = replay_match_row_id(batch_id, &record.match_id);
        let stored = database.put_record(&EngineCalyxRecord::new(
            EngineCalyxRecordKind::Provenance,
            row_id.clone(),
            serde_json::to_value(&payload).map_err(codec_failed)?,
            json!({
                "source_issue": 907,
                "batch_id": batch_id,
                "match_id": record.match_id,
                "outcome": if payload.outcome.is_win() { "subject_win" } else { "subject_loss" },
                "replay_ron_sha256": payload.replay_ron_sha256,
                "tick_event_count": payload.tick_events.len()
            }),
        ))?;
        match_readbacks.push(parse_match_readback(stored, &row_id)?);
    }
    let batch_payload = ReplayBatchPayload {
        schema: REPLAY_MATCH_BATCH_SCHEMA.to_string(),
        batch_id: batch_id.to_string(),
        match_row_ids: matches
            .iter()
            .map(|record| replay_match_row_id(batch_id, &record.match_id))
            .collect(),
        match_ids: matches
            .iter()
            .map(|record| record.match_id.clone())
            .collect(),
        match_count: matches.len(),
        win_count,
        loss_count: matches.len() - win_count,
        stat_keys,
    };
    let batch_row_id = replay_batch_row_id(batch_id);
    let stored = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::Provenance,
        batch_row_id.clone(),
        serde_json::to_value(&batch_payload).map_err(codec_failed)?,
        json!({
            "source_issue": 907,
            "batch_id": batch_id,
            "match_count": batch_payload.match_count,
            "win_count": batch_payload.win_count,
            "loss_count": batch_payload.loss_count
        }),
    ))?;
    let database_readback = parse_batch_row(stored, &batch_row_id)?;
    Ok(ReplayBatchReadback {
        payload: database_readback.1,
        database: database_readback.0,
        matches: match_readbacks,
    })
}

/// Read a previously ingested batch (batch row + every match row) back from
/// the source of truth.
pub fn read_replay_match_batch(
    database: &EngineCalyxDatabase,
    batch_id: &str,
) -> Result<ReplayBatchReadback, EngineReplayAnalyticsError> {
    validate_id("batch_id", batch_id)?;
    let batch_row_id = replay_batch_row_id(batch_id);
    let stored = database.get_record(EngineCalyxRecordKind::Provenance, &batch_row_id)?;
    let (database_readback, payload) = parse_batch_row(stored, &batch_row_id)?;
    let mut matches = Vec::with_capacity(payload.match_row_ids.len());
    for row_id in &payload.match_row_ids {
        let stored = database.get_record(EngineCalyxRecordKind::Provenance, row_id)?;
        matches.push(parse_match_readback(stored, row_id)?);
    }
    Ok(ReplayBatchReadback {
        database: database_readback,
        payload,
        matches,
    })
}

fn execute_and_derive<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
    batch_id: &str,
    record: &ReplayMatchRecord,
) -> Result<ReplayMatchStoredPayload, EngineReplayAnalyticsError> {
    let outcome = record
        .replay
        .replay::<MAX_BODIES, MAX_ORDERS>()
        .map_err(|error| EngineReplayAnalyticsError::ReplayExecution {
            match_id: record.match_id.clone(),
            error: error.to_string(),
        })?;
    let final_snapshot = &outcome.final_snapshot;
    if final_snapshot.attack_events.len() as u64 != final_snapshot.attack_events_total {
        return Err(EngineReplayAnalyticsError::Outcome {
            match_id: record.match_id.clone(),
            reason: format!(
                "attack event evidence truncated: window={} total={}; shorten the match or raise retention",
                final_snapshot.attack_events.len(),
                final_snapshot.attack_events_total
            ),
        });
    }
    let subject: BTreeSet<u32> = record.subject_bodies.iter().copied().collect();
    let opponent: BTreeSet<u32> = record.opponent_bodies.iter().copied().collect();
    let mut subject_damage = 0_i64;
    let mut opponent_damage = 0_i64;
    let mut tick_events = Vec::with_capacity(final_snapshot.attack_events.len());
    for event in &final_snapshot.attack_events {
        let attacker = event.attacker.index();
        if subject.contains(&attacker) {
            subject_damage = checked_damage(subject_damage, event.damage_fp, &record.match_id)?;
        } else if opponent.contains(&attacker) {
            opponent_damage = checked_damage(opponent_damage, event.damage_fp, &record.match_id)?;
        } else {
            return Err(EngineReplayAnalyticsError::Outcome {
                match_id: record.match_id.clone(),
                reason: format!("attacker body {attacker} belongs to neither team roster"),
            });
        }
        tick_events.push(ReplayTickEvent {
            tick: event.tick,
            attacker,
            target: event.target.index(),
            damage_fp: event.damage_fp,
        });
    }
    if subject_damage == opponent_damage {
        return Err(EngineReplayAnalyticsError::Outcome {
            match_id: record.match_id.clone(),
            reason: format!(
                "ambiguous outcome: subject_damage_fp={subject_damage} == opponent_damage_fp={opponent_damage}"
            ),
        });
    }
    let replay_ron = record
        .replay
        .to_ron_string()
        .map_err(|error| engine_codec(&record.match_id, error))?;
    Ok(ReplayMatchStoredPayload {
        schema: REPLAY_MATCH_SCHEMA.to_string(),
        batch_id: batch_id.to_string(),
        match_id: record.match_id.clone(),
        outcome: if subject_damage > opponent_damage {
            ReplayMatchOutcome::SubjectWin
        } else {
            ReplayMatchOutcome::SubjectLoss
        },
        subject_damage_fp: subject_damage,
        opponent_damage_fp: opponent_damage,
        stats_milli: record.stats_milli.clone(),
        tick_events,
        replay_ron_sha256: sha256_hex(replay_ron.as_bytes()),
        initial_state_hash: format!("{:016X}", outcome.initial_hash),
        final_state_hash: format!("{:016X}", outcome.final_hash),
        run_ticks: record.replay.run_ticks,
        orders_applied: outcome.orders_applied,
        attack_events_total: final_snapshot.attack_events_total,
    })
}

fn validate_batch(matches: &[ReplayMatchRecord]) -> Result<(), EngineReplayAnalyticsError> {
    if matches.is_empty() {
        return invalid("matches", "at least one match replay is required");
    }
    let expected_keys: Vec<&String> = matches[0].stats_milli.keys().collect();
    if expected_keys.is_empty() {
        return invalid("stats_milli", "at least one structured stat is required");
    }
    let mut seen = BTreeSet::new();
    for record in matches {
        validate_id("match_id", &record.match_id)?;
        if !seen.insert(record.match_id.clone()) {
            return invalid(
                "match_id",
                format!("duplicate match id {}", record.match_id),
            );
        }
        if record.subject_bodies.is_empty() || record.opponent_bodies.is_empty() {
            return invalid("team_rosters", "both team rosters must be non-empty");
        }
        let subject: BTreeSet<u32> = record.subject_bodies.iter().copied().collect();
        if record.opponent_bodies.iter().any(|id| subject.contains(id)) {
            return invalid("team_rosters", "team rosters must be disjoint");
        }
        let keys: Vec<&String> = record.stats_milli.keys().collect();
        if keys != expected_keys {
            return invalid(
                "stats_milli",
                format!(
                    "match {} stat roster differs from the batch roster",
                    record.match_id
                ),
            );
        }
    }
    Ok(())
}

fn parse_match_readback(
    database: EngineCalyxDatabaseReadback,
    expected_row_id: &str,
) -> Result<ReplayMatchReadback, EngineReplayAnalyticsError> {
    expect_row_id(&database, expected_row_id)?;
    let payload: ReplayMatchStoredPayload =
        serde_json::from_value(database.payload.clone()).map_err(codec_failed)?;
    expect_schema(&payload.schema, REPLAY_MATCH_SCHEMA)?;
    Ok(ReplayMatchReadback { database, payload })
}

fn parse_batch_row(
    database: EngineCalyxDatabaseReadback,
    expected_row_id: &str,
) -> Result<(EngineCalyxDatabaseReadback, ReplayBatchPayload), EngineReplayAnalyticsError> {
    expect_row_id(&database, expected_row_id)?;
    let payload: ReplayBatchPayload =
        serde_json::from_value(database.payload.clone()).map_err(codec_failed)?;
    expect_schema(&payload.schema, REPLAY_MATCH_BATCH_SCHEMA)?;
    if payload.match_row_ids.len() != payload.match_count
        || payload.win_count + payload.loss_count != payload.match_count
    {
        return Err(EngineReplayAnalyticsError::ReadbackMismatch {
            field: "batch.counts".to_string(),
            expected: format!("{} rows, win+loss==count", payload.match_count),
            actual: format!(
                "rows={} win={} loss={}",
                payload.match_row_ids.len(),
                payload.win_count,
                payload.loss_count
            ),
        });
    }
    Ok((database, payload))
}

pub(crate) fn expect_row_id(
    database: &EngineCalyxDatabaseReadback,
    expected: &str,
) -> Result<(), EngineReplayAnalyticsError> {
    if database.id != expected {
        return Err(EngineReplayAnalyticsError::ReadbackMismatch {
            field: "database.id".to_string(),
            expected: expected.to_string(),
            actual: database.id.clone(),
        });
    }
    Ok(())
}

pub(crate) fn expect_schema(
    actual: &str,
    expected: &str,
) -> Result<(), EngineReplayAnalyticsError> {
    if actual != expected {
        return Err(EngineReplayAnalyticsError::ReadbackMismatch {
            field: "schema".to_string(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }
    Ok(())
}

pub(crate) fn validate_id(field: &str, value: &str) -> Result<(), EngineReplayAnalyticsError> {
    if value.trim().is_empty()
        || value.contains('/')
        || value.chars().any(|ch| ch.is_control() || ch == ' ')
    {
        return invalid(
            field,
            "id must be non-empty, printable, without spaces or '/'",
        );
    }
    Ok(())
}

pub(crate) fn invalid<T>(
    field: impl Into<String>,
    reason: impl ToString,
) -> Result<T, EngineReplayAnalyticsError> {
    Err(EngineReplayAnalyticsError::InvalidInput {
        field: field.into(),
        reason: reason.to_string(),
    })
}

pub(crate) fn codec_failed(error: impl std::fmt::Display) -> EngineReplayAnalyticsError {
    EngineReplayAnalyticsError::Codec(error.to_string())
}

fn engine_codec(match_id: &str, error: EngineError) -> EngineReplayAnalyticsError {
    EngineReplayAnalyticsError::ReplayExecution {
        match_id: match_id.to_string(),
        error: error.to_string(),
    }
}

fn checked_damage(
    total: i64,
    delta: i64,
    match_id: &str,
) -> Result<i64, EngineReplayAnalyticsError> {
    total
        .checked_add(delta)
        .ok_or_else(|| EngineReplayAnalyticsError::Outcome {
            match_id: match_id.to_string(),
            reason: "team damage total overflowed i64".to_string(),
        })
}
