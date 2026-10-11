//! FSV: authoring persist is decoupled from the >=10-learned-lens gate (#1791).
//!
//! With NO ORT and no model cache, author + persist + readback + LOWER RPG content
//! end-to-end into a `pending_measurement` row; then, with the addon available,
//! backfill completes the row to >=10 measured learned lenses with two-phase
//! provenance -- reusing the existing calyx-registry BackfillScheduler as the
//! durable, resumable campaign ledger. Three edge cases prove the boundaries.
//!
//! The Aster vault rows are the source of truth: after each phase a BRAND-NEW
//! database handle reopens the persisted vault and reads the row back (durable
//! pending, then durable complete). The harness also rereads its own written
//! report and asserts every acceptance flag gates the process. Command success
//! alone is not proof.

use std::error::Error;

use calyx_registry::{BackfillConfig, BackfillScheduler};
use litd_engine::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE, ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentConstellationReadback,
        EngineContentKind, EngineContentMeasurementConfig, EngineContentRecord,
        EngineContentSource,
    },
    calyx_content_backfill::pending_backfill_requests,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseConfig, EngineCalyxRecord, EngineCalyxRecordKind,
    },
    fsv_artifacts::begin_issue_run,
    rpg::persist_and_lower_rpg_authored_content,
};
use serde_json::{Value, json};

#[path = "issue1791_pending_measurement_backfill_fsv/fixture.rs"]
mod fixture;

const PANEL_LENSES: usize = 12;
const CONTENT_KIND: EngineContentKind = EngineContentKind::Terrain;
const CONTENT_ID: &str = "area-ashen-vale-issue1791";

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1791_pending_measurement_backfill")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let scheduler_path = run.run_dir().join("backfill-scheduler.json");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;

    // Phase 1: AI addon ABSENT. Author + persist + lower + readback -> pending.
    let addon_absent = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue1791",
            fixture::unavailable_panel_state(PANEL_LENSES),
        ),
    );
    let lowering = persist_and_lower_rpg_authored_content(&addon_absent, &[content_record()])?;
    let pending = addon_absent.get_content_for_editor(CONTENT_KIND, CONTENT_ID)?;
    let phase1 = phase1_readback_summary(&pending, &lowering)?;

    // Separate readback of the SOURCE OF TRUTH (#800): reopen the persisted Aster
    // vault with a BRAND-NEW database handle -- no shared in-memory state with the
    // writer -- and confirm the pending row is durable and correct on-disk BEFORE
    // any backfill runs. This is the vault-row readback the FSV rule demands.
    let pending_on_disk = fresh_vault_readback(&vault_dir, CONTENT_KIND, CONTENT_ID)?;

    // Durable scheduler: enqueue the pending cells and prove they survive a reopen.
    let scheduler = scheduler_state(&scheduler_path, &pending)?;

    // Phase 2: AI addon AVAILABLE. Backfill completes the SAME row.
    let addon_present = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-present:issue1791",
            fixture::available_panel_state(PANEL_LENSES),
        ),
    );
    let outcome = addon_present.backfill_pending_content(CONTENT_KIND, CONTENT_ID)?;
    let complete = addon_present.get_content_for_editor(CONTENT_KIND, CONTENT_ID)?;
    let phase2 = phase2_backfill_summary(&pending, &complete, &outcome)?;

    // Separate readback again: a fresh handle over the same vault must now see the
    // COMPLETED row on-disk, proving the backfill transition is durable.
    let complete_on_disk = fresh_vault_readback(&vault_dir, CONTENT_KIND, CONTENT_ID)?;
    let separate_readback =
        separate_readback_summary(&pending, &pending_on_disk, &complete, &complete_on_disk);

    let edges = json!({
        "idempotent_rebackfill": idempotent_edge(&addon_present)?,
        "partial_availability_fails_closed": partial_edge(&database)?,
        "forged_complete_rejected": forged_complete_edge(&database)?,
    });

    let acceptance = acceptance(&phase1, &phase2, &edges, &scheduler, &separate_readback);
    let report = json!({
        "schema": "litd.engine.issue1791.pending_backfill_fsv.v1",
        "issue": 1791,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "scheduler_path": scheduler_path.display().to_string(),
            "environment": {
                "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some(),
                "CALYX_ORT_CAPI_present": std::env::var_os("CALYX_ORT_CAPI").is_some()
            },
            "manual_gate": "Manual FSV must read this report, the latest manifest, and the persisted Aster rows under engine_vault_dir; command success alone is not proof."
        },
        "sufficiency_argument": "One authored RPG area is the smallest input that exercises the whole vertical slice: explicit/deterministic persist under an addon-absent panel (all 12 declared learned slots absent(LensUnavailable)), a durable readback that is pending_measurement, a lowered sim package proving lowering needs no learned lens, then addon-present backfill re-measuring the SAME input to the SAME cx_id with >=10 learned vectors and two-phase provenance. 12 declared lenses > the 10-lens gate proves the gate stays binding on the declaration while persistence is unconditional.",
        "phase1_pending": phase1,
        "phase2_backfill": phase2,
        "separate_vault_readback": separate_readback,
        "durable_scheduler": scheduler,
        "edges": edges,
        "acceptance": acceptance
    });

    let readback = run.write_report(&report)?;
    for (field, value) in readback.json["acceptance"]
        .as_object()
        .expect("acceptance object")
    {
        if value != true {
            return Err(format!(
                "persisted issue1791 acceptance {field} was not true: {}",
                readback.json["acceptance"]
            )
            .into());
        }
    }
    println!(
        "ISSUE1791_PENDING_BACKFILL_FSV report={} sha256={} phase1_state={} phase2_state={} measured={} cx_stable={}",
        readback.path.display(),
        readback.sha256,
        phase1["measurement_state"],
        phase2["phase2_state"],
        phase2["phase2_measured_learned_lenses"],
        phase2["cx_id_stable"],
    );
    Ok(())
}

fn phase1_readback_summary(
    pending: &EngineContentConstellationReadback,
    lowering: &litd_engine::rpg::RpgAuthoredContentLoweringReadback,
) -> Result<Value, EngineCalyxContentError> {
    let measurement = &pending.measurement;
    let pending_slots_all_unavailable = measurement.pending_learned_lenses.iter().all(|lens| {
        matches!(
            pending
                .constellation
                .slots
                .get(&calyx_core::SlotId::new(lens.slot)),
            Some(calyx_core::SlotVector::Absent {
                reason: calyx_core::AbsentReason::LensUnavailable
            })
        )
    });
    Ok(json!({
        "measurement_state": measurement.measurement_state,
        "declared_learned_lenses": measurement.declared_learned_lenses,
        "measured_learned_lenses": measurement.measured_learned_lenses,
        "pending_learned_lenses": measurement.pending_learned_lenses.len(),
        "association_family_count": measurement.association_family_count,
        "pending_slots_all_lens_unavailable": pending_slots_all_unavailable,
        "cx_id": pending.cx_id,
        "row_seq": pending.database.seq,
        "row_sha256": pending.database.sha256,
        "lowered_row_count": lowering.row_count,
        "lowered_field_count": lowering.lowered.first().map(|row| row.field_count).unwrap_or(0),
        "lowered_package_fingerprint": lowering.package_fingerprint,
        "is_pending": measurement.measurement_state == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        "declared_at_least_10": measurement.declared_learned_lenses >= 10,
        "measured_is_zero": measurement.measured_learned_lenses == 0,
        "lowered_ok": lowering.row_count == 1 && !lowering.lowered.is_empty(),
    }))
}

fn phase2_backfill_summary(
    pending: &EngineContentConstellationReadback,
    complete: &EngineContentConstellationReadback,
    outcome: &litd_engine::calyx_content_backfill::EngineContentBackfillOutcome,
) -> Result<Value, EngineCalyxContentError> {
    let measured_slots_dense = complete.measurement.measured_slot_ids.iter().all(|slot| {
        matches!(
            complete
                .constellation
                .slots
                .get(&calyx_core::SlotId::new(*slot)),
            Some(calyx_core::SlotVector::Dense { .. })
        )
    });
    Ok(json!({
        "was_pending": outcome.was_pending,
        "completed": outcome.completed,
        "phase1_state": outcome.phase1_state,
        "phase1_pending_lenses": outcome.phase1_pending_lenses,
        "phase1_seq": outcome.phase1_seq,
        "phase2_state": complete.measurement.measurement_state,
        "phase2_measured_learned_lenses": complete.measurement.measured_learned_lenses,
        "phase2_pending_lenses": complete.measurement.pending_learned_lenses.len(),
        "cx_id_before": pending.cx_id,
        "cx_id_after": complete.cx_id,
        "cx_id_stable": pending.cx_id == complete.cx_id,
        "seq_advanced": complete.database.seq > pending.database.seq,
        "provenance_backfill": complete.database.provenance.get("backfill").cloned().unwrap_or(Value::Null),
        "measured_slots_all_dense": measured_slots_dense,
        "is_complete": complete.measurement.measurement_state == ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE,
        "measured_at_least_10": complete.measurement.measured_learned_lenses >= 10,
        "provenance_records_two_phases": complete
            .database
            .provenance
            .get("backfill")
            .and_then(|value| value.get("phase1_state"))
            .and_then(Value::as_str)
            == Some(ENGINE_CONTENT_MEASUREMENT_STATE_PENDING),
    }))
}

fn scheduler_state(
    path: &std::path::Path,
    pending: &EngineContentConstellationReadback,
) -> Result<Value, Box<dyn Error>> {
    let requests = pending_backfill_requests(pending)?;
    let mut scheduler = BackfillScheduler::open(path, BackfillConfig::default())?;
    for request in &requests {
        scheduler.enqueue(request.clone())?;
    }
    // Reopen from disk: the enqueued campaign must survive a process restart.
    let reopened = BackfillScheduler::open(path, BackfillConfig::default())?;
    let marks = reopened.watermarks();
    let enqueued = requests.len();
    Ok(json!({
        "enqueued_requests": enqueued,
        "durable_watermarks": marks.len(),
        "all_incomplete_before_backfill": marks.iter().all(|mark| !mark.complete),
        "pending_candidates": marks.iter().map(|mark| mark.pending).sum::<usize>(),
        "durable_matches_enqueued": marks.len() == enqueued && enqueued == 12,
    }))
}

fn idempotent_edge(store: &EngineCalyxContentStore) -> Result<Value, EngineCalyxContentError> {
    let before = store.get_content_for_editor(CONTENT_KIND, CONTENT_ID)?;
    let outcome = store.backfill_pending_content(CONTENT_KIND, CONTENT_ID)?;
    let after = store.get_content_for_editor(CONTENT_KIND, CONTENT_ID)?;
    Ok(json!({
        "before_state": before.measurement.measurement_state,
        "after_state": after.measurement.measurement_state,
        "was_pending": outcome.was_pending,
        "completed": outcome.completed,
        "seq_before": before.database.seq,
        "seq_after": after.database.seq,
        "passed": !outcome.was_pending
            && !outcome.completed
            && before.database.seq == after.database.seq
            && after.measurement.measurement_state == ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE,
    }))
}

fn partial_edge(database: &EngineCalyxDatabase) -> Result<Value, EngineCalyxContentError> {
    // 8 available + 4 unavailable = 12 declared, only 8 measurable: an inconsistent
    // runtime the measure path must reject (all-or-none), persisting nothing.
    let store = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-partial:issue1791",
            fixture::mixed_panel_state(8, 4),
        ),
    );
    let mut record = content_record();
    record.id = "area-partial-issue1791".to_string();
    let result = store.put_content(&record);
    let row_absent = matches!(
        store.get_content_for_editor(CONTENT_KIND, "area-partial-issue1791"),
        Err(EngineCalyxContentError::Persistence(_))
    );
    match result {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => Ok(json!({
            "phase": phase,
            "reason": reason,
            "row_absent_after_failure": row_absent,
            "passed": phase == "measurement_availability"
                && reason.contains("partial")
                && row_absent,
        })),
        Err(other) => Ok(json!({ "passed": false, "unexpected_error": other.to_string() })),
        Ok(_) => Ok(json!({ "passed": false, "unexpected": "partial availability persisted" })),
    }
}

fn forged_complete_edge(database: &EngineCalyxDatabase) -> Result<Value, EngineCalyxContentError> {
    // Author a genuine pending row, then forge its stored measurement_state to
    // `complete` while measured=0. Readback must reject it: the >=10 gate stays
    // binding for measurement claims, so a pending row cannot masquerade as
    // complete (#778 at its real scope).
    let store = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent-forge:issue1791",
            fixture::unavailable_panel_state(PANEL_LENSES),
        ),
    );
    let mut record = content_record();
    record.id = "area-forge-issue1791".to_string();
    let genuine = store.put_content(&record)?;
    let row_id = "terrain/area-forge-issue1791";
    let stored = database.get_record(EngineCalyxRecordKind::ContentConstellation, row_id)?;
    let mut payload = stored.payload;
    payload["measurement"]["measurement_state"] =
        Value::String(ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE.to_string());
    let write = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::ContentConstellation,
        row_id,
        payload,
        json!({ "source_issue": 1791, "forged": true }),
    ))?;
    let rejected = matches!(
        store.get_content_for_editor(CONTENT_KIND, "area-forge-issue1791"),
        Err(EngineCalyxContentError::ReadbackMismatch { .. })
    );
    Ok(json!({
        "genuine_state": genuine.measurement.measurement_state,
        "forged_write_seq": write.seq,
        "rejected_on_readback": rejected,
        "passed": rejected,
    }))
}

fn separate_readback_summary(
    pending: &EngineContentConstellationReadback,
    pending_on_disk: &EngineContentConstellationReadback,
    complete: &EngineContentConstellationReadback,
    complete_on_disk: &EngineContentConstellationReadback,
) -> Value {
    json!({
        "pending_row": {
            "measurement_state": pending_on_disk.measurement.measurement_state,
            "measured_learned_lenses": pending_on_disk.measurement.measured_learned_lenses,
            "declared_learned_lenses": pending_on_disk.measurement.declared_learned_lenses,
            "pending_learned_lenses": pending_on_disk.measurement.pending_learned_lenses.len(),
            "row_seq": pending_on_disk.database.seq,
            "row_sha256": pending_on_disk.database.sha256,
            "sha256_matches_writer": pending_on_disk.database.sha256 == pending.database.sha256,
            "cx_id_matches_writer": pending_on_disk.cx_id == pending.cx_id,
            "is_pending": pending_on_disk.measurement.measurement_state
                == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        },
        "complete_row": {
            "measurement_state": complete_on_disk.measurement.measurement_state,
            "measured_learned_lenses": complete_on_disk.measurement.measured_learned_lenses,
            "row_seq": complete_on_disk.database.seq,
            "row_sha256": complete_on_disk.database.sha256,
            "sha256_matches_writer": complete_on_disk.database.sha256 == complete.database.sha256,
            "is_complete": complete_on_disk.measurement.measurement_state
                == ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE,
        }
    })
}

/// Open a BRAND-NEW database + store on the same on-disk vault and read a row.
/// The persistence layer holds no in-memory cache, so this is a genuine readback
/// of the durable Aster bytes -- proof independent of the writer's handle.
fn fresh_vault_readback(
    vault_dir: &std::path::Path,
    kind: EngineContentKind,
    id: &str,
) -> Result<EngineContentConstellationReadback, Box<dyn Error>> {
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            "separate-readback:issue1791",
            fixture::unavailable_panel_state(PANEL_LENSES),
        ),
    );
    Ok(store.get_content_for_editor(kind, id)?)
}

fn acceptance(
    phase1: &Value,
    phase2: &Value,
    edges: &Value,
    scheduler: &Value,
    separate: &Value,
) -> Value {
    json!({
        "phase1_persisted_pending_without_ort": phase1["is_pending"],
        "phase1_declared_gate_binding": phase1["declared_at_least_10"],
        "phase1_no_learned_measured": phase1["measured_is_zero"],
        "phase1_all_pending_slots_lens_unavailable": phase1["pending_slots_all_lens_unavailable"],
        "phase1_lowered_rpg_content": phase1["lowered_ok"],
        "separate_readback_pending_durable": separate["pending_row"]["is_pending"],
        "separate_readback_pending_sha_matches": separate["pending_row"]["sha256_matches_writer"],
        "separate_readback_pending_cx_matches": separate["pending_row"]["cx_id_matches_writer"],
        "separate_readback_complete_durable": separate["complete_row"]["is_complete"],
        "separate_readback_complete_sha_matches": separate["complete_row"]["sha256_matches_writer"],
        "scheduler_durable": scheduler["durable_matches_enqueued"],
        "scheduler_all_incomplete_before_backfill": scheduler["all_incomplete_before_backfill"],
        "phase2_completed": phase2["completed"],
        "phase2_is_complete": phase2["is_complete"],
        "phase2_measured_at_least_10": phase2["measured_at_least_10"],
        "phase2_cx_id_stable": phase2["cx_id_stable"],
        "phase2_seq_advanced": phase2["seq_advanced"],
        "phase2_measured_slots_dense": phase2["measured_slots_all_dense"],
        "phase2_two_phase_provenance": phase2["provenance_records_two_phases"],
        "edge_idempotent_rebackfill": edges["idempotent_rebackfill"]["passed"],
        "edge_partial_fails_closed": edges["partial_availability_fails_closed"]["passed"],
        "edge_forged_complete_rejected": edges["forged_complete_rejected"]["passed"],
    })
}

fn content_record() -> EngineContentRecord {
    EngineContentRecord {
        kind: CONTENT_KIND,
        id: CONTENT_ID.to_string(),
        display_name: "Ashen Vale".to_string(),
        payload: json!({
            "schema": "litd.rpg.area.v1",
            "surface_kind": "area_region",
            "id": CONTENT_ID,
            "region_name": "Ashen Vale",
            "terrain_tag": "ashen_road",
            "acoustics": "muffled_open_ash",
            "description": "A scorched vale of grey ash where the light barely holds."
        }),
        source: EngineContentSource {
            path: "editor://areas/ashen-vale".to_string(),
            author: "issue1791-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(b"issue1791:area-ashen-vale"),
        },
        anchors: Vec::new(),
    }
}
