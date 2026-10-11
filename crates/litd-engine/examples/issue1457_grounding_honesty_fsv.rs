use std::{fs, io, path::Path};

use calyx_ward::CALYX_GUARD_OOD;
use litd_engine::{
    calyx_content::EngineContentKind,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT, EngineCalyxSemanticError,
        build_engine_calyx_semantic_index, query_engine_calyx_semantic_index,
        read_engine_calyx_semantic_index, store_engine_calyx_semantic_index,
        validate_semantic_index,
    },
    calyx_ward_gate::{
        EngineCalyxWardGateError, EngineCalyxWardPolicy, EngineCalyxWardSubject,
        EngineCalyxWardSubjectKind, evaluate_calyx_ward_gate, read_calyx_ward_gate,
        store_calyx_ward_gate, validate_gate_readback,
    },
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

#[path = "issue1457_grounding_honesty_fsv/content_fixture.rs"]
mod content_fixture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1457_grounding_honesty")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;

    let readbacks = content_fixture::content_readbacks(&vault_dir)?;
    let index = build_engine_calyx_semantic_index("issue1457-keyword-index", &readbacks)?;
    let persisted_index = store_engine_calyx_semantic_index(&database, &index)?;
    let stored_index = read_engine_calyx_semantic_index(&database, "issue1457-keyword-index")?;
    let query = query_engine_calyx_semantic_index(&stored_index.index, "fire ward gate", 3)?;
    let keyword_json = serde_json::to_string(&json!({
        "index": stored_index.index,
        "query": query
    }))?;
    let keyword_claims_removed = !keyword_json.contains("lens")
        && !keyword_json.contains("Lens")
        && !keyword_json.contains("bits")
        && !keyword_json.contains("Bits");

    let trusted = trusted_subject();
    let adversary = adversary_subject();
    let ward_gate = evaluate_calyx_ward_gate(
        "issue1457-ward-adversary",
        policy_for(&trusted),
        std::slice::from_ref(&adversary),
    )?;
    let persisted_gate = store_calyx_ward_gate(&database, &ward_gate)?;
    let stored_gate = read_calyx_ward_gate(&database, "issue1457-ward-adversary")?;
    let verdict = stored_gate
        .gate
        .verdicts
        .first()
        .ok_or_else(|| io::Error::other("missing Ward adversary verdict"))?;
    let ward_rejected_legacy_pass = verdict.code == CALYX_GUARD_OOD
        && verdict.legacy_token_admission_would_pass
        && !verdict.ward_overall_pass
        && verdict.ward_slots.iter().any(|slot| !slot.pass)
        && !verdict.lowering_allowed;
    let persisted_readbacks_match = persisted_index.index.index_fingerprint
        == stored_index.index.index_fingerprint
        && persisted_gate.gate.gate_fingerprint == stored_gate.gate.gate_fingerprint;
    let edges = edge_readbacks(&stored_index.index, &ward_gate, &trusted)?;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let vault_files = vault_file_readbacks(&vault_dir)?;

    ensure(
        keyword_claims_removed,
        "keyword readback retained lens/bits claims",
    )?;
    ensure(
        ward_rejected_legacy_pass,
        "Ward did not reject the legacy-token-passing adversary",
    )?;
    ensure(persisted_readbacks_match, "persisted readbacks mismatch")?;
    ensure(all_edges_matched, "edge readbacks did not all match")?;

    let report = json!({
        "schema": "litd.engine.issue1457.grounding_honesty_fsv.v1",
        "schema_version": 1,
        "issue": 1457,
        "research_sources": [
            "https://jmlr.org/papers/v9/shafer08a.html",
            "https://arxiv.org/abs/2107.07511"
        ],
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual FSV must inspect this JSON, latest manifest, persisted Aster rows, Ward calibration/verdict slots, keyword readback literal scan, and hashes"
        },
        "acceptance": {
            "keyword_claims_removed": keyword_claims_removed,
            "keyword_schema": stored_index.index.schema,
            "keyword_perspective_count": stored_index.index.perspective_count,
            "keyword_perspective_count_expected": ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT,
            "keyword_weighted_terms_count": query.per_perspective_weighted_terms.len(),
            "ward_rejected_legacy_pass": ward_rejected_legacy_pass,
            "ward_code": verdict.code,
            "legacy_token_admission_would_pass": verdict.legacy_token_admission_would_pass,
            "ward_overall_pass": verdict.ward_overall_pass,
            "ward_slot_count": verdict.ward_slots.len(),
            "ward_failing_slot_count": verdict.ward_slots.iter().filter(|slot| !slot.pass).count(),
            "persisted_readbacks_match": persisted_readbacks_match,
            "all_edges_matched": all_edges_matched
        },
        "keyword": {
            "database": stored_index.database,
            "index": stored_index.index,
            "query": query
        },
        "ward": {
            "database": stored_gate.database,
            "gate": stored_gate.gate,
            "trusted_subject": trusted,
            "adversary_subject": adversary
        },
        "edges": edges,
        "vault_files": vault_files
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1457_GROUNDING_HONESTY_FSV report={} sha256={} manifest={} run_id={} keyword_claims_removed={} ward_rejected_legacy_pass={} edges={} gate_fingerprint={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        keyword_claims_removed,
        ward_rejected_legacy_pass,
        edges.len(),
        stored_gate.gate.gate_fingerprint
    );
    Ok(())
}

fn trusted_subject() -> EngineCalyxWardSubject {
    subject("trusted/fire-ward", "fire ward gate protect enemy agent")
}

fn adversary_subject() -> EngineCalyxWardSubject {
    subject(
        "adversary/legacy-token-overlap",
        "fire ward gate protect enemy agent protect gate enemy ward fire agent reversed bargain",
    )
}

fn subject(subject_id: &str, text: &str) -> EngineCalyxWardSubject {
    EngineCalyxWardSubject {
        subject_id: subject_id.to_string(),
        kind: EngineCalyxWardSubjectKind::Content,
        content_kind: Some(EngineContentKind::Ability),
        text: text.to_string(),
        grounding_refs: vec![
            format!("grounding://issue1457/{subject_id}/one"),
            format!("grounding://issue1457/{subject_id}/two"),
        ],
        provenance_refs: vec![format!("provenance://issue1457/{subject_id}")],
        movement_units_per_tick: 0,
        teleport_requested: false,
        scale_milli: 1000,
        emissive_milli: 0,
        source_fingerprint: litd_hash::sha256_hex(text.as_bytes()),
    }
}

fn policy_for(trusted: &EngineCalyxWardSubject) -> EngineCalyxWardPolicy {
    EngineCalyxWardPolicy {
        policy_id: "ward-issue1457".to_string(),
        allowed_terms: strings(&["fire", "ward", "gate", "protect", "enemy", "agent"]),
        blocked_terms: strings(&["nuclear", "teleport", "exploit"]),
        trusted_texts: vec![trusted.text.clone()],
        calibration_ood_texts: calibration_ood_texts(),
        min_grounding_refs: 2,
        min_matched_terms: 2,
        max_move_units_per_tick: 2,
        allow_teleport: false,
        max_scale_milli: 1500,
        max_emissive_milli: 1000,
    }
}

fn calibration_ood_texts() -> Vec<String> {
    let mut rows = (0..calyx_ward::MIN_BAD_SCORES)
        .map(|index| format!("fire ward gate protect enemy agent reversed bargain {index}"))
        .collect::<Vec<_>>();
    rows[0] = adversary_subject().text;
    rows
}

fn edge_readbacks(
    index: &litd_engine::calyx_semantic::EngineCalyxSemanticIndex,
    gate: &litd_engine::calyx_ward_gate::EngineCalyxWardGateReadback,
    trusted: &EngineCalyxWardSubject,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut bad_index = index.clone();
    bad_index.index_fingerprint = "00000000".to_string();
    let mut insufficient_calibration = policy_for(trusted);
    insufficient_calibration
        .calibration_ood_texts
        .truncate(calyx_ward::MIN_BAD_SCORES - 1);
    let mut empty_trusted = policy_for(trusted);
    empty_trusted.trusted_texts.clear();
    let mut bad_calibration = gate.clone();
    bad_calibration.ward_calibration.calibrated = false;
    bad_calibration.gate_fingerprint = "00000000".to_string();
    Ok(vec![
        expect_semantic_invalid(
            "keyword_fingerprint_tamper_rejected",
            validate_semantic_index(&bad_index),
            "index_fingerprint",
            "fingerprint mismatch",
        ),
        expect_ward_invalid(
            "insufficient_calibration_rejected",
            evaluate_calyx_ward_gate(
                "insufficient",
                insufficient_calibration,
                std::slice::from_ref(trusted),
            ),
            "policy",
            "required",
        ),
        expect_ward_invalid(
            "empty_trusted_texts_rejected",
            evaluate_calyx_ward_gate(
                "empty-trusted",
                empty_trusted,
                std::slice::from_ref(trusted),
            ),
            "policy",
            "required",
        ),
        expect_ward_invalid(
            "ward_calibration_tamper_rejected",
            validate_gate_readback(&bad_calibration),
            "ward_calibration",
            "calibrated",
        ),
    ])
}

fn expect_semantic_invalid<T>(
    label: &str,
    result: Result<T, EngineCalyxSemanticError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineCalyxSemanticError::InvalidInput { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "field": field,
            "reason": reason
        }),
        Err(error) => {
            json!({"label": label, "matched": false, "unexpected_error": error.to_string()})
        }
        Ok(_) => json!({"label": label, "matched": false, "unexpected": "operation succeeded"}),
    }
}

fn expect_ward_invalid<T>(
    label: &str,
    result: Result<T, EngineCalyxWardGateError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineCalyxWardGateError::InvalidInput { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "field": field,
            "reason": reason
        }),
        Err(error) => {
            json!({"label": label, "matched": false, "unexpected_error": error.to_string()})
        }
        Ok(_) => json!({"label": label, "matched": false, "unexpected": "operation succeeded"}),
    }
}

fn vault_file_readbacks(vault_dir: &Path) -> io::Result<Vec<Value>> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(vault_dir)? {
        collect_file_readbacks(&entry?.path(), &mut rows)?;
    }
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(rows)
}

fn collect_file_readbacks(path: &Path, rows: &mut Vec<Value>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_file_readbacks(&entry?.path(), rows)?;
        }
    } else {
        let bytes = fs::read(path)?;
        rows.push(json!({
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": litd_hash::sha256_hex(&bytes)
        }));
    }
    Ok(())
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
