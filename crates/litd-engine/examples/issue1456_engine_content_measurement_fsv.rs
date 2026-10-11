use std::{fs, path::Path};

use calyx_assay::{A37_DIVERSITY_GATE_PASSED, MIN_ASSAY_SAMPLES};
use calyx_core::{Anchor, AnchorKind, AnchorValue};
use calyx_registry::audit_vault_registry_contracts;
use litd_engine::{
    calyx_content::{
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentA37AdmissionRef,
        EngineContentKind, EngineContentMeasurementConfig, EngineContentRecord,
        EngineContentSource, read_engine_content_assay, resolve_engine_content_panel_vault,
        store_engine_content_assay, store_engine_content_assay_with_a37_admission,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

#[path = "issue1456_engine_content_measurement_fsv/fixture.rs"]
mod fixture;
#[path = "issue1456_engine_content_measurement_fsv/issue1479.rs"]
mod issue1479;
#[path = "issue1456_engine_content_measurement_fsv/roster.rs"]
mod roster;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    match std::env::args().nth(1).as_deref() {
        Some("--unavailable-edge") => run_unavailable_edge(),
        Some("--happy") | None => run_happy(),
        Some(other) => {
            Err(format!("unknown mode {other}; use --happy or --unavailable-edge").into())
        }
    }
}

fn run_happy() -> Result<(), Box<dyn std::error::Error>> {
    let run = begin_issue_run("issue1456_engine_content_measurement")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let real_panel_vault = resolve_engine_content_panel_vault()?;
    let registry_audit = audit_vault_registry_contracts(&real_panel_vault)?;
    if !registry_audit.valid {
        return Err("real panel registry contract audit failed".into());
    }
    let measurement =
        EngineContentMeasurementConfig::from_loaded_cpu_explicit_panel_vault(&real_panel_vault)?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(database.clone(), measurement);
    let mut readbacks = Vec::new();
    let mut labels = Vec::new();
    for idx in 0..MIN_ASSAY_SAMPLES {
        let positive = idx % 2 == 0;
        labels.push(positive);
        let record = content_record(idx, positive);
        readbacks.push(store.put_content(&record)?);
    }
    let first = store.get_content_for_editor(EngineContentKind::Story, "story-issue1456-00")?;
    let configured_admission_ref = issue1479::admission_ref_from_env();
    let generated_admission = if configured_admission_ref.is_some() {
        issue1479::skipped_generation("environment admission configured")
    } else if !issue1479::generation_enabled() {
        issue1479::skipped_generation(
            "set LITD_ENGINE_CONTENT_A37_GENERATE_ADMISSION=1 to attempt live generation",
        )
    } else {
        issue1479::generate_exact_a37_admission(&store, run.run_dir(), &readbacks)?
    };
    let admission_ref = configured_admission_ref.or(generated_admission.admission.clone());
    let assay_gate =
        content_assay_gate_summary(&database, &readbacks, &labels, admission_ref.as_ref())?;
    let assay_gate_satisfied = assay_gate["accepted"].as_bool().unwrap_or(false);
    let roster = roster::measured_real_roster(&real_panel_vault, &first)?;
    let exact_roster_gate_passed = issue1479::exact_sparse_multi_roster_gate_passed(
        &assay_gate,
        &roster,
        first.measurement.measured_learned_lenses,
    );
    let sub_ten = sub_ten_edge(&database)?;
    let sub_ten_fails_closed = sub_ten_edge_passed(&sub_ten);
    let report = json!({
        "schema": "litd.engine.issue1456.measurement_fsv.v1",
        "issue": 1456,
        "mode": "happy",
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "real_panel_vault": real_panel_vault.display().to_string(),
            "measurement_policy": "vault_cpu_explicit_text_learned_roster",
            "a37_admission": issue1479::admission_config_summary(admission_ref.as_ref()),
            "manual_gate": "Manual Synapse FSV must read this report, the latest manifest, and the persisted Aster rows under engine_vault_dir; command success alone is not proof."
        },
        "sufficiency_argument": "The minimal sufficient labeled corpus is MIN_ASSAY_SAMPLES=50 because Calyx ensemble_card fails closed below that count. The run uses 25 positive and 25 negative known-I/O records plus a real loaded constellation-24 panel derived from frozen vault artifacts. A37 phase-exit is satisfied only when the exact measured sparse/multi roster is assayed with gate_passed or an exact-roster DB/template-native admission is read back.",
        "registry_contract_audit": {
            "checked_count": registry_audit.checked_count,
            "valid": registry_audit.valid,
            "diff_count": registry_audit.diffs.len()
        },
        "content_readback": roster::content_readback_summary(&first, &roster),
        "a37_admission_generation": generated_admission.summary,
        "assay_gate": assay_gate,
        "edges": {
            "sub_ten_panel": sub_ten,
            "single_class_assay_labels": single_class_assay_edge(&database, &readbacks)?
        },
        "vault_readback": vault_readback(&vault_dir)?,
        "acceptance": {
            "real_panel_registry_contracts_valid": registry_audit.valid,
            "measured_learned_lenses_at_least_10": first.measurement.measured_learned_lenses >= 10,
            "association_families_at_least_2": first.measurement.association_family_count >= 2,
            "all_measured_slots_non_absent": roster.iter().all(|row| row["vector"]["kind"] != "absent"),
            "synthetic_testpass_anchor_dropped": first.measurement.dropped_synthetic_anchor_count >= 1 && first.constellation.anchors.is_empty(),
            "a37_gate_satisfied": assay_gate_satisfied,
            "exact_sparse_multi_roster_gate_passed": exact_roster_gate_passed,
            "sub_ten_fails_closed": sub_ten_fails_closed,
        }
    });
    let readback = run.write_report(&report)?;
    for field in [
        "real_panel_registry_contracts_valid",
        "measured_learned_lenses_at_least_10",
        "association_families_at_least_2",
        "all_measured_slots_non_absent",
        "synthetic_testpass_anchor_dropped",
        "a37_gate_satisfied",
        "exact_sparse_multi_roster_gate_passed",
        "sub_ten_fails_closed",
    ] {
        if readback.json["acceptance"][field] != true {
            return Err(format!(
                "persisted engine content measurement acceptance {field} was false: {}",
                readback.json["acceptance"]
            )
            .into());
        }
    }
    println!(
        "ISSUE1456_CONTENT_MEASUREMENT_FSV report={} sha256={} samples={} measured_lenses={} a37_gate_satisfied={} vault={}",
        readback.path.display(),
        readback.sha256,
        readbacks.len(),
        first.measurement.measured_learned_lenses,
        assay_gate_satisfied,
        vault_dir.display()
    );
    Ok(())
}

fn run_unavailable_edge() -> Result<(), Box<dyn std::error::Error>> {
    let run = begin_issue_run("issue1456_engine_content_measurement_unavailable")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let result = store.put_content(&content_record(0, true));
    let edge = measurement_error_edge(result);
    if edge["passed"] != true {
        return Err(format!("expected unavailable embedder failure, got {edge}").into());
    }
    let report = json!({
        "schema": "litd.engine.issue1456.measurement_fsv.v1",
        "issue": 1456,
        "mode": "unavailable-edge",
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "manual_gate": "Run this mode from a shell with ORT_DYLIB_PATH and CALYX_ORT_CAPI removed; manually inspect the report and environment readback."
        },
        "environment_readback": {
            "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some(),
            "CALYX_ORT_CAPI_present": std::env::var_os("CALYX_ORT_CAPI").is_some()
        },
        "edge": edge
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1456_UNAVAILABLE_EDGE_FSV report={} sha256={} passed={}",
        readback.path.display(),
        readback.sha256,
        readback.json["edge"]["passed"]
    );
    Ok(())
}

fn content_assay_gate_summary(
    database: &EngineCalyxDatabase,
    readbacks: &[litd_engine::calyx_content::EngineContentConstellationReadback],
    labels: &[bool],
    admission: Option<&EngineContentA37AdmissionRef>,
) -> Result<Value, EngineCalyxContentError> {
    let assay_id = "issue1456-real-panel-content-assay";
    let result = match admission {
        Some(admission) => store_engine_content_assay_with_a37_admission(
            database, assay_id, readbacks, labels, admission,
        ),
        None => store_engine_content_assay(database, assay_id, readbacks, labels),
    };
    match result {
        Ok(stored) => {
            let assay = read_engine_content_assay(database, assay_id)?;
            let gate = &assay.report.ensemble.a37_diversity;
            let gate_passed = gate.status == A37_DIVERSITY_GATE_PASSED
                && gate.family_span_pass
                && gate.redundancy_bound_pass
                && gate.no_collapse_pass;
            let db_admission_passed =
                assay
                    .report
                    .a37_admission
                    .as_ref()
                    .is_some_and(|admission| {
                        admission.readback_matches
                            && admission.report.status == A37_DIVERSITY_GATE_PASSED
                            && admission.report.gate_passed
                    });
            let accepted = gate_passed || db_admission_passed;
            let accepted = accepted && stored.database.sha256 == assay.database.sha256;
            Ok(json!({
                "passed": accepted,
                "accepted": accepted,
                "failed_closed": false,
                "outcome": if db_admission_passed { "db_native_a37_admission_persisted" } else { "gate_passed_persisted" },
                "stored_row_sha256": stored.database.sha256,
                "read_row_sha256": assay.database.sha256,
                "row_id": assay.database.id,
                "row_key": assay.database.row_key,
                "sample_count": assay.report.sample_count,
                "positive_count": assay.report.positive_count,
                "negative_count": assay.report.negative_count,
                "measured_learned_lenses": assay.report.measured_learned_lenses,
                "assayed_dense_lenses": assay.report.assayed_dense_lenses,
                "assayed_lens_count": assay.report.assayed_lens_count,
                "assayed_lenses": assay.report.assayed_lenses,
                "a37_gate_source": assay.report.a37_gate_source,
                "a37_admission": assay.report.a37_admission,
                "sufficiency_state": assay.report.sufficiency_state,
                "ensemble_sufficient": assay.report.ensemble.sufficient,
                "panel_bits": assay.report.ensemble.panel_bits,
                "anchor_entropy_bits": assay.report.ensemble.anchor_entropy_bits,
                "n_eff": assay.report.ensemble.n_eff,
                "deficit_bits": assay.report.ensemble.deficit_bits,
                "a37_diversity": gate,
                "keep_count": assay.report.ensemble.keep_count,
                "park_count": assay.report.ensemble.park_count,
                "retire_count": assay.report.ensemble.retire_count
            }))
        }
        Err(EngineCalyxContentError::Measurement { phase, reason }) if phase == "a37_gate" => {
            let row_absent_after_failure = matches!(
                read_engine_content_assay(database, assay_id),
                Err(EngineCalyxContentError::Persistence(_))
            );
            Ok(json!({
                "passed": false,
                "accepted": false,
                "failed_closed": row_absent_after_failure
                    && reason.contains("status=diagnostic_only")
                    && reason.contains("no_collapse_pass=false"),
                "outcome": "diagnostic_only_failed_closed",
                "phase": phase,
                "reason": reason,
                "row_absent_after_failure": row_absent_after_failure
            }))
        }
        Err(EngineCalyxContentError::Measurement { phase, reason }) if phase == "a37_admission" => {
            let row_absent_after_failure = matches!(
                read_engine_content_assay(database, assay_id),
                Err(EngineCalyxContentError::Persistence(_))
            );
            Ok(json!({
                "passed": false,
                "accepted": false,
                "failed_closed": row_absent_after_failure,
                "outcome": "a37_admission_failed_closed",
                "phase": phase,
                "reason": reason,
                "row_absent_after_failure": row_absent_after_failure
            }))
        }
        Err(error) => Ok(json!({
            "passed": false,
            "accepted": false,
            "failed_closed": false,
            "outcome": "unexpected_error",
            "message": error.to_string()
        })),
    }
}

fn content_record(index: usize, positive: bool) -> EngineContentRecord {
    let theme = if positive {
        "radiant ward healing sanctuary dawn mercy protects villagers"
    } else {
        "venom mire curse famine dusk cruelty poisons raiders"
    };
    let id = format!("story-issue1456-{index:02}");
    EngineContentRecord {
        kind: EngineContentKind::Story,
        id: id.clone(),
        display_name: format!("Issue 1456 measured story {index:02}"),
        payload: json!({
            "id": id,
            "assay_positive": positive,
            "known_label": if positive { "protective" } else { "harmful" },
            "scene_text": format!("{theme} sample {index:02}"),
            "expected_known_io": if positive { 4 } else { 2 }
        }),
        source: EngineContentSource {
            path: format!("synthetic://issue1456/{index:02}"),
            author: "issue1456-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1456:{index}:{positive}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1456_synthetic_self_attestation_probe".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn sub_ten_edge(database: &EngineCalyxDatabase) -> Result<Value, EngineCalyxContentError> {
    let store = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "preloaded:fsv-sub-ten",
            fixture::fixture_panel_state(9),
        ),
    );
    Ok(measurement_error_edge(
        store.put_content(&content_record(900, true)),
    ))
}

fn sub_ten_edge_passed(edge: &Value) -> bool {
    edge["passed"] == true && edge["phase"] == "panel_gate"
}

fn single_class_assay_edge(
    database: &EngineCalyxDatabase,
    readbacks: &[litd_engine::calyx_content::EngineContentConstellationReadback],
) -> Result<Value, EngineCalyxContentError> {
    let labels = vec![true; readbacks.len()];
    match store_engine_content_assay(database, "issue1456-single-class-edge", readbacks, &labels) {
        Err(EngineCalyxContentError::InvalidContent { field, reason }) => Ok(json!({
            "passed": field == "content_assay.labels",
            "error_kind": "invalid_content",
            "field": field,
            "reason": reason
        })),
        Err(error) => Ok(json!({
            "passed": false,
            "error_kind": "unexpected_error",
            "message": error.to_string()
        })),
        Ok(_) => Ok(json!({
            "passed": false,
            "error_kind": "missing_error",
            "message": "single-class assay labels unexpectedly succeeded"
        })),
    }
}

fn measurement_error_edge(
    result: Result<
        litd_engine::calyx_content::EngineContentConstellationReadback,
        EngineCalyxContentError,
    >,
) -> Value {
    match result {
        Err(EngineCalyxContentError::Measurement { phase, reason }) => json!({
            "passed": reason.contains("at least 10")
                || reason.contains("CALYX_LENS_UNREACHABLE")
                || reason.contains("ORT_DYLIB_PATH")
                || reason.contains("CALYX_ORT_CAPI"),
            "error_kind": "measurement",
            "phase": phase,
            "reason": reason
        }),
        Err(error) => json!({
            "passed": false,
            "error_kind": "unexpected_error",
            "message": error.to_string()
        }),
        Ok(readback) => json!({
            "passed": false,
            "error_kind": "missing_error",
            "message": "measurement unexpectedly succeeded",
            "row_sha256": readback.database.sha256
        }),
    }
}

fn vault_readback(vault_dir: &Path) -> Result<Value, EngineCalyxContentError> {
    let mut files = Vec::new();
    collect_files(vault_dir, &mut files)?;
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(json!({
        "file_count": files.len(),
        "first_files": files.into_iter().take(16).collect::<Vec<_>>()
    }))
}

fn collect_files(path: &Path, out: &mut Vec<Value>) -> Result<(), EngineCalyxContentError> {
    for entry in fs::read_dir(path).map_err(content_store_failed)? {
        let entry = entry.map_err(content_store_failed)?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            let bytes = fs::read(&path).map_err(content_store_failed)?;
            out.push(json!({
                "path": path.display().to_string(),
                "bytes": bytes.len(),
                "sha256": litd_hash::sha256_hex(&bytes)
            }));
        }
    }
    Ok(())
}

fn content_store_failed(error: impl std::fmt::Display) -> EngineCalyxContentError {
    EngineCalyxContentError::Codec(error.to_string())
}
