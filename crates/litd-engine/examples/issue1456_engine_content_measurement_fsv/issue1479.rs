use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

use calyx_assay::A37_DIVERSITY_GATE_PASSED;
use litd_engine::calyx_content::{
    EngineCalyxContentError, EngineCalyxContentStore, EngineContentA37AdmissionRef,
    EngineContentA37InputReport, EngineContentA37MultiAnchorConfig,
    EngineContentConstellationReadback, build_engine_content_a37_admission_report,
    measure_engine_content_assay_report, write_engine_content_a37_admission,
};
use serde_json::{Value, json};

#[path = "issue1479_partitioned.rs"]
mod issue1479_partitioned;

const A37_ADMISSION_CF_ENV: &str = "LITD_ENGINE_CONTENT_A37_ADMISSION_CF_ROOT";
const A37_ADMISSION_KEY_ENV: &str = "LITD_ENGINE_CONTENT_A37_ADMISSION_KEY";
const A37_GENERATE_ADMISSION_ENV: &str = "LITD_ENGINE_CONTENT_A37_GENERATE_ADMISSION";
const DEFAULT_A37_ADMISSION_KEY: &str = "a37_multi_anchor_admission";

pub(super) struct GeneratedAdmission {
    pub admission: Option<EngineContentA37AdmissionRef>,
    pub summary: Value,
}

pub(super) fn admission_ref_from_env() -> Option<EngineContentA37AdmissionRef> {
    let cf_root = env::var_os(A37_ADMISSION_CF_ENV).map(PathBuf::from)?;
    let association_key = env::var(A37_ADMISSION_KEY_ENV)
        .ok()
        .filter(|key| !key.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_A37_ADMISSION_KEY.to_string());
    Some(EngineContentA37AdmissionRef::new(cf_root, association_key))
}

pub(super) fn admission_config_summary(admission: Option<&EngineContentA37AdmissionRef>) -> Value {
    match admission {
        Some(admission) => serde_json::json!({
            "configured": true,
            "cf_root": admission.cf_root.display().to_string(),
            "association_key": admission.association_key,
            "cf_root_env": A37_ADMISSION_CF_ENV,
            "association_key_env": A37_ADMISSION_KEY_ENV,
            "generate_admission_env": A37_GENERATE_ADMISSION_ENV
        }),
        None => serde_json::json!({
            "configured": false,
            "cf_root_env": A37_ADMISSION_CF_ENV,
            "association_key_env": A37_ADMISSION_KEY_ENV,
            "generate_admission_env": A37_GENERATE_ADMISSION_ENV
        }),
    }
}

pub(super) fn generation_enabled() -> bool {
    env::var(A37_GENERATE_ADMISSION_ENV)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

pub(super) fn skipped_generation(reason: &str) -> GeneratedAdmission {
    GeneratedAdmission {
        admission: None,
        summary: json!({
            "attempted": false,
            "reason": reason
        }),
    }
}

pub(super) fn generate_exact_a37_admission(
    store: &EngineCalyxContentStore,
    run_dir: &Path,
    main_readbacks: &[EngineContentConstellationReadback],
) -> Result<GeneratedAdmission, EngineCalyxContentError> {
    let input_dir = run_dir.join("issue1479-a37-inputs");
    fs::create_dir_all(&input_dir).map_err(content_store_failed)?;
    let mut inputs = Vec::new();
    let candidates = issue1479_partitioned::measure_partitioned_candidates(store, main_readbacks)?;
    for candidate in candidates {
        push_card_input(
            &mut inputs,
            &input_dir,
            candidate.target_class,
            &candidate.domain,
            &candidate.readbacks,
            &candidate.labels,
        )?;
    }
    validate_candidate_roster(main_readbacks, &inputs)?;
    let roster = main_readbacks
        .first()
        .ok_or_else(|| a37_generation_error_value("main content readbacks are empty"))?
        .measurement
        .frozen_roster
        .clone();
    let config = EngineContentA37MultiAnchorConfig::default();
    let initial = build_engine_content_a37_admission_report(&inputs, &config, &roster)?;
    let selected = select_redundancy_bound_inputs(&inputs, &initial);
    let report = if selected.len() >= 2 && selected.len() != inputs.len() {
        build_engine_content_a37_admission_report(&selected, &config, &roster)?
    } else {
        initial
    };
    let report_path = run_dir.join("issue1479-a37-admission-report.json");
    write_json(&report_path, &report)?;
    let admission_ref = EngineContentA37AdmissionRef::new(
        run_dir.join("issue1479-a37-admission-cf"),
        DEFAULT_A37_ADMISSION_KEY,
    );
    let readback = if report.gate_passed {
        Some(write_engine_content_a37_admission(&admission_ref, &report)?)
    } else {
        None
    };
    Ok(GeneratedAdmission {
        admission: readback.as_ref().map(|_| admission_ref),
        summary: json!({
            "attempted": true,
            "candidate_report_count": inputs.len(),
            "selected_report_count": report.report_count,
            "selected_sources": report.source_reports,
            "aggregate_report_path": report_path.display().to_string(),
            "gate_passed": report.gate_passed,
            "status": report.status,
            "weakest_lens": report.weakest_lens,
            "min_best_marginal_bits": report.min_best_marginal_bits,
            "min_marginal_bits": report.min_marginal_bits,
            "passing_lens_count": report.passing_lens_count,
            "lens_count": report.lens_count,
            "target_summaries": report.target_summaries,
            "lenses": report.lenses,
            "db_readback": readback
        }),
    })
}

fn validate_candidate_roster(
    main_readbacks: &[EngineContentConstellationReadback],
    inputs: &[EngineContentA37InputReport],
) -> Result<(), EngineCalyxContentError> {
    let Some(main) = main_readbacks.first() else {
        return a37_generation_error("main content readbacks are empty");
    };
    let Some(candidate) = inputs.first() else {
        return a37_generation_error("A37 admission candidate inputs are empty");
    };
    let candidate_roster = candidate
        .card
        .lenses
        .iter()
        .map(|lens| (lens.slot.get(), lens.name.clone()))
        .collect::<Vec<_>>();
    let main_roster = main
        .measurement
        .measured_slot_ids
        .iter()
        .copied()
        .zip(main.measurement.measured_lens_names.iter().cloned())
        .collect::<Vec<_>>();
    if candidate_roster != main_roster {
        return a37_generation_error("candidate A37 roster differs from main measured roster");
    }
    Ok(())
}

pub(super) fn exact_sparse_multi_roster_gate_passed(
    assay_gate: &Value,
    roster: &[Value],
    measured_learned_lenses: usize,
) -> bool {
    if assay_gate["passed"] != true
        || assay_gate["assayed_lens_count"].as_u64() != Some(measured_learned_lenses as u64)
        || !accepted_outcome(assay_gate)
    {
        return false;
    }
    let Some(assayed) = assay_gate["assayed_lenses"].as_array() else {
        return false;
    };
    if assayed.len() != roster.len() || assayed.len() != measured_learned_lenses {
        return false;
    }
    let has_sparse = assayed.iter().any(|lens| {
        lens["lens_name"] == "a38_spladepp_v1_sparse"
            && lens["source_vector_kind"] == "sparse"
            && lens["assay_projection"] == "sparse_hash_dense_8192"
            && lens["assay_dim"].as_u64() == Some(8192)
            && lens["assay_vector_sha256"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
    });
    let has_colbert = assayed.iter().any(|lens| {
        lens["lens_name"] == "a38_answerai_colbert_small_v1"
            && lens["source_vector_kind"] == "multi"
            && lens["assay_projection"] == "multi_mean_dense"
            && lens["assay_dim"].as_u64() == Some(384)
            && lens["assay_vector_sha256"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
    });
    let db_admission_valid = assay_gate["outcome"] == "db_native_a37_admission_persisted"
        && assay_gate["a37_gate_source"] == "db_native_multi_anchor_admission"
        && assay_gate["a37_admission"]["report"]["status"] == A37_DIVERSITY_GATE_PASSED
        && assay_gate["a37_admission"]["report"]["gate_passed"] == true
        && assay_gate["a37_admission"]["report"]["lens_count"].as_u64()
            == Some(measured_learned_lenses as u64);
    let single_card_valid = assay_gate["outcome"] == "gate_passed_persisted";
    has_sparse && has_colbert && (single_card_valid || db_admission_valid)
}

fn accepted_outcome(assay_gate: &Value) -> bool {
    assay_gate["outcome"] == "gate_passed_persisted"
        || assay_gate["outcome"] == "db_native_a37_admission_persisted"
}

fn push_card_input(
    inputs: &mut Vec<EngineContentA37InputReport>,
    input_dir: &Path,
    target_class: usize,
    domain: &str,
    readbacks: &[EngineContentConstellationReadback],
    labels: &[bool],
) -> Result<(), EngineCalyxContentError> {
    let report =
        measure_engine_content_assay_report(&format!("issue1479-a37-{domain}"), readbacks, labels)?;
    let source = input_dir
        .join(format!("{target_class:02}-{domain}.json"))
        .display()
        .to_string();
    let input =
        EngineContentA37InputReport::from_assay_report(target_class, domain, &source, &report);
    write_json(Path::new(&source), &input)?;
    inputs.push(input);
    Ok(())
}

fn select_redundancy_bound_inputs(
    inputs: &[EngineContentA37InputReport],
    report: &litd_engine::calyx_content::EngineContentA37AdmissionReport,
) -> Vec<EngineContentA37InputReport> {
    let passing_sources = report
        .target_summaries
        .iter()
        .filter(|target| target.redundancy_bound_pass)
        .map(|target| target.report_path.as_str())
        .collect::<BTreeSet<_>>();
    if passing_sources.len() < 2 || passing_sources.len() == inputs.len() {
        return inputs.to_vec();
    }
    inputs
        .iter()
        .filter(|input| passing_sources.contains(input.source.as_str()))
        .cloned()
        .collect()
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), EngineCalyxContentError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(content_store_failed)?;
    fs::write(path, bytes).map_err(content_store_failed)
}

fn content_store_failed(error: impl std::fmt::Display) -> EngineCalyxContentError {
    EngineCalyxContentError::Codec(error.to_string())
}

fn a37_generation_error<T>(reason: impl ToString) -> Result<T, EngineCalyxContentError> {
    Err(a37_generation_error_value(reason))
}

fn a37_generation_error_value(reason: impl ToString) -> EngineCalyxContentError {
    EngineCalyxContentError::Measurement {
        phase: "a37_admission".to_string(),
        reason: reason.to_string(),
    }
}
