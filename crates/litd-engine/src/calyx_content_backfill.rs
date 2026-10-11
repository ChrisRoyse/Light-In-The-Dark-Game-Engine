//! Addon-time backfill of pending content measurements (#1791).
//!
//! Phase 1 (authoring) persisted the explicit/deterministic constellation with
//! `measurement_state = pending_measurement` because the AI addon (ONNX runtime +
//! models) was absent, so every declared learned slot is `absent(LensUnavailable)`.
//! Phase 2 (this module) runs once the addon is available: it re-measures the
//! SAME authored input -- reproducing the same `cx_id` -- fills the learned slots,
//! and re-persists the completed constellation with two-phase provenance. The
//! seq bump on the shared Aster row is the durable ledger of the transition.
//!
//! Backfill is driven through the existing calyx-registry `BackfillScheduler`
//! (durable, throttled, resumable): [`pending_backfill_requests`] turns a pending
//! row into scheduler requests keyed by the exact (slot, lens, cx_id) cells.

use calyx_core::{CxId, LensId, SlotId};
use calyx_registry::{BackfillPriority, BackfillRequest};
use serde_json::json;

use crate::calyx_content::{
    ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE, EngineCalyxContentError, EngineCalyxContentStore,
    EngineContentConstellationReadback, EngineContentKind, EngineContentRecord,
    constellation_payload,
};

pub const ENGINE_CONTENT_BACKFILL_PROVENANCE_SCHEMA: &str =
    "litd.engine.content_backfill.provenance.v1";

/// Outcome of a single-row backfill attempt (#1791).
#[derive(Debug, Clone, PartialEq)]
pub struct EngineContentBackfillOutcome {
    pub kind: EngineContentKind,
    pub id: String,
    pub cx_id: String,
    /// The row was `pending_measurement` when this call started.
    pub was_pending: bool,
    /// This call completed the measurement and persisted it.
    pub completed: bool,
    pub phase1_state: String,
    pub phase1_pending_lenses: usize,
    pub phase1_seq: u64,
    pub phase1_sha256: String,
    pub phase2_state: String,
    pub phase2_measured_lenses: usize,
    pub readback: EngineContentConstellationReadback,
}

pub fn backfill_pending_content(
    store: &EngineCalyxContentStore,
    kind: EngineContentKind,
    id: &str,
) -> Result<EngineContentBackfillOutcome, EngineCalyxContentError> {
    let current = store.get_content_for_editor(kind, id)?;
    let phase1_state = current.measurement.measurement_state.clone();
    let phase1_pending = current.measurement.pending_learned_lenses.len();
    let phase1_seq = current.database.seq;
    let phase1_sha256 = current.database.sha256.clone();

    if phase1_state == ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE {
        // Already complete: idempotent no-op.
        let measured = current.measurement.measured_learned_lenses;
        let cx_id = current.cx_id.clone();
        return Ok(EngineContentBackfillOutcome {
            kind,
            id: id.to_string(),
            cx_id,
            was_pending: false,
            completed: false,
            phase1_state: phase1_state.clone(),
            phase1_pending_lenses: phase1_pending,
            phase1_seq,
            phase1_sha256,
            phase2_state: phase1_state,
            phase2_measured_lenses: measured,
            readback: current,
        });
    }

    // Reconstruct the exact authored record from the stored payload -- which keeps
    // the full source (author + source_hash), unlike the editor readback -- so the
    // re-measured canonical input and cx_id match byte-for-byte.
    let payload = store.read_content_payload(kind, id)?;
    let record = EngineContentRecord {
        kind: payload.kind,
        id: payload.id.clone(),
        display_name: payload.display_name.clone(),
        payload: payload.payload.clone(),
        source: payload.source.clone(),
        anchors: payload.constellation.anchors.clone(),
    };
    let remeasured = constellation_payload(store.database(), store.measurement(), &record)?;
    let phase2_state = remeasured.measurement.measurement_state.clone();
    let phase2_measured = remeasured.measurement.measured_learned_lenses;

    if phase2_state != ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE {
        // Addon still unavailable: no progress. The durable pending row is left
        // exactly as it was (idempotent), and the caller is told honestly.
        return Ok(EngineContentBackfillOutcome {
            kind,
            id: id.to_string(),
            cx_id: current.cx_id.clone(),
            was_pending: true,
            completed: false,
            phase1_state,
            phase1_pending_lenses: phase1_pending,
            phase1_seq,
            phase1_sha256,
            phase2_state,
            phase2_measured_lenses: phase2_measured,
            readback: current,
        });
    }

    let remeasured_cx = remeasured.constellation.cx_id.to_string();
    if remeasured_cx != current.cx_id {
        return Err(EngineCalyxContentError::ReadbackMismatch {
            field: "backfill.cx_id".to_string(),
            expected: current.cx_id.clone(),
            actual: remeasured_cx,
        });
    }

    let provenance = json!({
        "source_path": record.source.path,
        "source_hash": record.source.source_hash,
        "author": record.source.author,
        "source_issue": 1456,
        "constellation_id": remeasured_cx,
        "measurement_state": ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE,
        "pending_learned_lenses": remeasured.measurement.pending_learned_lenses.len(),
        "backfill": {
            "schema": ENGINE_CONTENT_BACKFILL_PROVENANCE_SCHEMA,
            "issue": 1791,
            "phase1_state": phase1_state.clone(),
            "phase1_pending_lenses": phase1_pending,
            "phase1_seq": phase1_seq,
            "phase1_sha256": phase1_sha256.clone(),
            "phase2_state": ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE,
            "phase2_measured_lenses": phase2_measured,
        }
    });
    let readback = store.persist_content_payload(&record, remeasured, provenance)?;
    let cx_id = readback.cx_id.clone();
    Ok(EngineContentBackfillOutcome {
        kind,
        id: id.to_string(),
        cx_id,
        was_pending: true,
        completed: true,
        phase1_state,
        phase1_pending_lenses: phase1_pending,
        phase1_seq,
        phase1_sha256,
        phase2_state,
        phase2_measured_lenses: phase2_measured,
        readback,
    })
}

/// Turn a pending row into durable [`BackfillRequest`]s -- one per deferred learned
/// cell -- for the existing calyx-registry `BackfillScheduler`. The row's `cx_id`
/// is the single candidate; the scheduler then owns throttling, concurrency, and
/// crash-resumable progress across the backfill campaign (#1791).
pub fn pending_backfill_requests(
    readback: &EngineContentConstellationReadback,
) -> Result<Vec<BackfillRequest>, EngineCalyxContentError> {
    let cx_id = readback
        .cx_id
        .parse::<CxId>()
        .map_err(|error| id_parse_error("cx_id", &readback.cx_id, error))?;
    let mut requests = Vec::with_capacity(readback.measurement.pending_learned_lenses.len());
    for pending in &readback.measurement.pending_learned_lenses {
        let lens_id = pending
            .lens_id
            .parse::<LensId>()
            .map_err(|error| id_parse_error("lens_id", &pending.lens_id, error))?;
        requests.push(BackfillRequest {
            slot_id: SlotId::new(pending.slot),
            lens_id,
            priority: BackfillPriority::Normal,
            candidates: vec![cx_id],
        });
    }
    Ok(requests)
}

fn id_parse_error(
    field: &str,
    value: &str,
    error: impl std::fmt::Display,
) -> EngineCalyxContentError {
    EngineCalyxContentError::InvalidContent {
        field: format!("backfill.{field}"),
        reason: format!("parse {value}: {error}"),
    }
}
