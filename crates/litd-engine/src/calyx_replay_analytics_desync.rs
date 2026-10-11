//! #907: desync forensics over divergent replays (Lodestar-style grounded
//! search for the first divergence). Two replay records that claim the same
//! match are re-executed in lockstep, tick by tick; the first tick where the
//! deterministic state hashes disagree is pinpointed, together with the
//! diverging bodies (field-level deltas) and any order-log divergence.
//!
//! Strictly offline: this never runs in the sim hot path. The forensics
//! report is persisted to the Calyx-backed database and read back from it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    EngineError,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxRecord, EngineCalyxRecordKind,
    },
    calyx_replay_analytics::{
        EngineReplayAnalyticsError, codec_failed, expect_row_id, expect_schema, validate_id,
    },
    sim::FixedTickSim,
    sim_order::SimOrderQueue,
    sim_replay::SimReplayRecord,
};

pub const DESYNC_FORENSICS_SCHEMA: &str = "litd.engine.replay_analytics.desync_forensics.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesyncVerdict {
    /// Both replays executed identically through every tick.
    Clean,
    /// The replays diverged at `first_divergent_tick`.
    Diverged,
    /// The replays do not even share an initial state.
    InitialStateDivergence,
}

/// Field-level delta for one body at the first divergent tick (raw
/// fixed-point values so the evidence is exact).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesyncBodyDelta {
    pub body_index: u32,
    pub present_in_a: bool,
    pub present_in_b: bool,
    pub position_a_raw: [i64; 3],
    pub position_b_raw: [i64; 3],
    pub velocity_a_raw: [i64; 3],
    pub velocity_b_raw: [i64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesyncForensicsPayload {
    pub schema: String,
    pub case_id: String,
    pub verdict: DesyncVerdict,
    pub run_ticks: u64,
    pub compared_ticks: u64,
    pub first_divergent_tick: Option<u64>,
    pub divergent_tick_hash_a: Option<String>,
    pub divergent_tick_hash_b: Option<String>,
    pub diverging_bodies: Vec<DesyncBodyDelta>,
    /// Ticks whose scheduled order fingerprints differ between the two
    /// records — a divergent input log is the classic desync root cause.
    pub divergent_order_ticks: Vec<u64>,
    pub order_log_divergence: bool,
    pub initial_state_hash_a: String,
    pub initial_state_hash_b: String,
    pub final_state_hash_a: Option<String>,
    pub final_state_hash_b: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesyncForensicsReadback {
    pub database: EngineCalyxDatabaseReadback,
    pub payload: DesyncForensicsPayload,
}

pub fn desync_forensics_row_id(case_id: &str) -> String {
    format!("desync_forensics/{case_id}")
}

/// Run lockstep forensics over two replay records of the same claimed match,
/// persist the report, and read it back from the source of truth.
pub fn run_desync_forensics<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
    database: &EngineCalyxDatabase,
    case_id: &str,
    replay_a: &SimReplayRecord,
    replay_b: &SimReplayRecord,
) -> Result<DesyncForensicsReadback, EngineReplayAnalyticsError> {
    validate_id("case_id", case_id)?;
    ensure_comparable(case_id, replay_a, replay_b)?;
    let payload = lockstep_forensics::<MAX_BODIES, MAX_ORDERS>(case_id, replay_a, replay_b)?;
    let row_id = desync_forensics_row_id(case_id);
    let stored = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::Provenance,
        row_id.clone(),
        serde_json::to_value(&payload).map_err(codec_failed)?,
        json!({
            "source_issue": 907,
            "case_id": case_id,
            "verdict": verdict_str(payload.verdict),
            "first_divergent_tick": payload.first_divergent_tick,
            "diverging_body_count": payload.diverging_bodies.len(),
            "order_log_divergence": payload.order_log_divergence
        }),
    ))?;
    parse_forensics_readback(stored, &row_id)
}

/// Read a previously stored forensics report back from the source of truth.
pub fn read_desync_forensics(
    database: &EngineCalyxDatabase,
    case_id: &str,
) -> Result<DesyncForensicsReadback, EngineReplayAnalyticsError> {
    validate_id("case_id", case_id)?;
    let row_id = desync_forensics_row_id(case_id);
    let stored = database.get_record(EngineCalyxRecordKind::Provenance, &row_id)?;
    parse_forensics_readback(stored, &row_id)
}

fn ensure_comparable(
    case_id: &str,
    replay_a: &SimReplayRecord,
    replay_b: &SimReplayRecord,
) -> Result<(), EngineReplayAnalyticsError> {
    if replay_a.initial_snapshot.config != replay_b.initial_snapshot.config {
        return forensics_error(case_id, "replays are not comparable: sim configs differ");
    }
    if replay_a.run_ticks != replay_b.run_ticks {
        return forensics_error(
            case_id,
            format!(
                "replays are not comparable: run_ticks {} != {}",
                replay_a.run_ticks, replay_b.run_ticks
            ),
        );
    }
    if replay_a.run_ticks == 0 {
        return forensics_error(case_id, "replays are not comparable: run_ticks is zero");
    }
    Ok(())
}

fn lockstep_forensics<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
    case_id: &str,
    replay_a: &SimReplayRecord,
    replay_b: &SimReplayRecord,
) -> Result<DesyncForensicsPayload, EngineReplayAnalyticsError> {
    let initial_a = format!("{:016X}", replay_a.initial_snapshot.state_hash);
    let initial_b = format!("{:016X}", replay_b.initial_snapshot.state_hash);
    let divergent_order_ticks = divergent_order_ticks(replay_a, replay_b);
    let mut payload = DesyncForensicsPayload {
        schema: DESYNC_FORENSICS_SCHEMA.to_string(),
        case_id: case_id.to_string(),
        verdict: DesyncVerdict::Clean,
        run_ticks: replay_a.run_ticks,
        compared_ticks: 0,
        first_divergent_tick: None,
        divergent_tick_hash_a: None,
        divergent_tick_hash_b: None,
        diverging_bodies: Vec::new(),
        order_log_divergence: !divergent_order_ticks.is_empty(),
        divergent_order_ticks,
        initial_state_hash_a: initial_a,
        initial_state_hash_b: initial_b,
        final_state_hash_a: None,
        final_state_hash_b: None,
    };
    if replay_a.initial_snapshot.state_hash != replay_b.initial_snapshot.state_hash {
        payload.verdict = DesyncVerdict::InitialStateDivergence;
        return Ok(payload);
    }
    let mut sim_a: FixedTickSim<MAX_BODIES> = restore(case_id, replay_a, "a")?;
    let mut sim_b: FixedTickSim<MAX_BODIES> = restore(case_id, replay_b, "b")?;
    let mut queue_a = SimOrderQueue::<MAX_ORDERS>::default();
    let mut queue_b = SimOrderQueue::<MAX_ORDERS>::default();
    for order in &replay_a.orders {
        enqueue(case_id, &mut queue_a, *order, "a")?;
    }
    for order in &replay_b.orders {
        enqueue(case_id, &mut queue_b, *order, "b")?;
    }
    for _ in 0..replay_a.run_ticks {
        let step_a = step(case_id, &mut queue_a, &mut sim_a, "a")?;
        let step_b = step(case_id, &mut queue_b, &mut sim_b, "b")?;
        payload.compared_ticks += 1;
        if step_a.state_hash != step_b.state_hash {
            payload.verdict = DesyncVerdict::Diverged;
            payload.first_divergent_tick = Some(step_a.tick);
            payload.divergent_tick_hash_a = Some(format!("{:016X}", step_a.state_hash));
            payload.divergent_tick_hash_b = Some(format!("{:016X}", step_b.state_hash));
            payload.diverging_bodies = diverging_bodies(&sim_a, &sim_b);
            return Ok(payload);
        }
    }
    let final_a = sim_a.state_hash();
    let final_b = sim_b.state_hash();
    payload.final_state_hash_a = Some(format!("{final_a:016X}"));
    payload.final_state_hash_b = Some(format!("{final_b:016X}"));
    if final_a != replay_a.expected_final_hash {
        return forensics_error(
            case_id,
            format!(
                "replay a record is corrupt: expected_final_hash={:016X} actual={final_a:016X}",
                replay_a.expected_final_hash
            ),
        );
    }
    if final_b != replay_b.expected_final_hash {
        return forensics_error(
            case_id,
            format!(
                "replay b record is corrupt: expected_final_hash={:016X} actual={final_b:016X}",
                replay_b.expected_final_hash
            ),
        );
    }
    Ok(payload)
}

fn divergent_order_ticks(replay_a: &SimReplayRecord, replay_b: &SimReplayRecord) -> Vec<u64> {
    let per_tick = |record: &SimReplayRecord| {
        let mut map = BTreeMap::<u64, Vec<u64>>::new();
        for order in &record.orders {
            map.entry(order.execute_tick)
                .or_default()
                .push(order.fingerprint());
        }
        for fingerprints in map.values_mut() {
            fingerprints.sort_unstable();
        }
        map
    };
    let ticks_a = per_tick(replay_a);
    let ticks_b = per_tick(replay_b);
    let mut divergent = Vec::new();
    for tick in ticks_a.keys().chain(ticks_b.keys()) {
        if ticks_a.get(tick) != ticks_b.get(tick) && !divergent.contains(tick) {
            divergent.push(*tick);
        }
    }
    divergent.sort_unstable();
    divergent
}

fn diverging_bodies<const MAX_BODIES: usize>(
    sim_a: &FixedTickSim<MAX_BODIES>,
    sim_b: &FixedTickSim<MAX_BODIES>,
) -> Vec<DesyncBodyDelta> {
    let states_a: BTreeMap<u32, _> = sim_a
        .body_states()
        .map(|state| (state.id.index(), state))
        .collect();
    let states_b: BTreeMap<u32, _> = sim_b
        .body_states()
        .map(|state| (state.id.index(), state))
        .collect();
    let mut deltas = Vec::new();
    for index in states_a.keys().chain(states_b.keys()) {
        let state_a = states_a.get(index);
        let state_b = states_b.get(index);
        if state_a == state_b
            || deltas
                .iter()
                .any(|delta: &DesyncBodyDelta| delta.body_index == *index)
        {
            continue;
        }
        let raw = |vec: Option<&crate::sim::SimBodyState>,
                   pick: fn(&crate::sim::SimBodyState) -> [i64; 3]| {
            vec.map(pick).unwrap_or([0; 3])
        };
        deltas.push(DesyncBodyDelta {
            body_index: *index,
            present_in_a: state_a.is_some(),
            present_in_b: state_b.is_some(),
            position_a_raw: raw(state_a, position_raw),
            position_b_raw: raw(state_b, position_raw),
            velocity_a_raw: raw(state_a, velocity_raw),
            velocity_b_raw: raw(state_b, velocity_raw),
        });
    }
    deltas
}

fn position_raw(state: &crate::sim::SimBodyState) -> [i64; 3] {
    [
        state.position.x.raw(),
        state.position.y.raw(),
        state.position.z.raw(),
    ]
}

fn velocity_raw(state: &crate::sim::SimBodyState) -> [i64; 3] {
    [
        state.velocity.x.raw(),
        state.velocity.y.raw(),
        state.velocity.z.raw(),
    ]
}

fn restore<const MAX_BODIES: usize>(
    case_id: &str,
    record: &SimReplayRecord,
    side: &str,
) -> Result<FixedTickSim<MAX_BODIES>, EngineReplayAnalyticsError> {
    record
        .initial_snapshot
        .restore::<MAX_BODIES>()
        .map_err(|error| engine_forensics(case_id, side, "restore", error))
}

fn enqueue<const MAX_ORDERS: usize>(
    case_id: &str,
    queue: &mut SimOrderQueue<MAX_ORDERS>,
    order: crate::sim_order::SimOrder,
    side: &str,
) -> Result<(), EngineReplayAnalyticsError> {
    queue
        .enqueue(order)
        .map(|_| ())
        .map_err(|error| engine_forensics(case_id, side, "enqueue", error))
}

fn step<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
    case_id: &str,
    queue: &mut SimOrderQueue<MAX_ORDERS>,
    sim: &mut FixedTickSim<MAX_BODIES>,
    side: &str,
) -> Result<crate::sim_order::SimOrderStepReport, EngineReplayAnalyticsError> {
    queue
        .step(sim)
        .map_err(|error| engine_forensics(case_id, side, "step", error))
}

fn parse_forensics_readback(
    database: EngineCalyxDatabaseReadback,
    expected_row_id: &str,
) -> Result<DesyncForensicsReadback, EngineReplayAnalyticsError> {
    expect_row_id(&database, expected_row_id)?;
    let payload: DesyncForensicsPayload =
        serde_json::from_value(database.payload.clone()).map_err(codec_failed)?;
    expect_schema(&payload.schema, DESYNC_FORENSICS_SCHEMA)?;
    if payload.verdict == DesyncVerdict::Diverged && payload.first_divergent_tick.is_none() {
        return Err(EngineReplayAnalyticsError::ReadbackMismatch {
            field: "forensics.first_divergent_tick".to_string(),
            expected: "present for a diverged verdict".to_string(),
            actual: "none".to_string(),
        });
    }
    Ok(DesyncForensicsReadback { database, payload })
}

const fn verdict_str(verdict: DesyncVerdict) -> &'static str {
    match verdict {
        DesyncVerdict::Clean => "clean",
        DesyncVerdict::Diverged => "diverged",
        DesyncVerdict::InitialStateDivergence => "initial_state_divergence",
    }
}

fn forensics_error<T>(
    case_id: &str,
    reason: impl ToString,
) -> Result<T, EngineReplayAnalyticsError> {
    Err(EngineReplayAnalyticsError::Forensics {
        case_id: case_id.to_string(),
        reason: reason.to_string(),
    })
}

fn engine_forensics(
    case_id: &str,
    side: &str,
    phase: &str,
    error: EngineError,
) -> EngineReplayAnalyticsError {
    EngineReplayAnalyticsError::Forensics {
        case_id: case_id.to_string(),
        reason: format!("side={side} phase={phase} error={error}"),
    }
}
