use std::{fs, io, path::Path};

use calyx_ward::CALYX_GUARD_OOD;
use litd_engine::{
    calyx_content::EngineContentKind,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_ward_gate::{
        EngineCalyxWardGateError, EngineCalyxWardGateReadback, EngineCalyxWardPolicy,
        EngineCalyxWardSubject, EngineCalyxWardSubjectKind, evaluate_calyx_ward_gate,
        read_calyx_ward_gate, store_calyx_ward_gate, validate_gate_readback,
    },
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue859_agent_ward_envelope")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;

    let subjects = ward_subjects();
    let policy = policy_for(&subjects);
    let gate = evaluate_calyx_ward_gate("issue859-agent-ward-envelope", policy, &subjects)?;
    let persisted = store_calyx_ward_gate(&database, &gate)?;
    let stored = read_calyx_ward_gate(&database, "issue859-agent-ward-envelope")?;
    let edges = edge_readbacks(&database, &gate, &subjects)?;
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let allowed_ok = verdict(&stored.gate, "agent.guard.allowed_patrol")
        .map(|verdict| {
            verdict.admitted
                && verdict.lowering_allowed
                && verdict.code == "LITD-WARD-ADMITTED"
                && verdict.reasons.is_empty()
        })
        .unwrap_or(false);
    let move_blocked = refused_envelope(
        &stored.gate,
        "agent.guard.illegal_sprint",
        "movement exceeds Ward envelope",
    );
    let teleport_blocked = refused_envelope(
        &stored.gate,
        "agent.guard.illegal_teleport",
        "teleport is outside Ward movement envelope",
    );
    let visual_blocked = refused_envelope(
        &stored.gate,
        "agent.guard.broken_visual",
        "visual envelope exceeded",
    );
    let behavior_blocked = verdict(&stored.gate, "agent.guard.ood_behavior")
        .map(|verdict| {
            !verdict.admitted
                && !verdict.lowering_allowed
                && verdict.code == CALYX_GUARD_OOD
                && !verdict.blocked_terms.is_empty()
                && !verdict.ward_overall_pass
        })
        .unwrap_or(false);
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let stored_matches = persisted.gate.gate_fingerprint == gate.gate_fingerprint
        && stored.gate.gate_fingerprint == gate.gate_fingerprint
        && stored.database.payload == serde_json::to_value(&gate)?;
    let provenance_matches = stored.database.provenance["source_schema"]
        == json!("litd.engine.calyx_ward_gate.v1")
        && stored.database.provenance["gate_id"] == json!("issue859-agent-ward-envelope");
    let acceptance = stored.gate.subject_count == 5
        && stored.gate.admitted_count == 1
        && stored.gate.refused_count == 4
        && stored.gate.fail_closed
        && allowed_ok
        && move_blocked
        && teleport_blocked
        && visual_blocked
        && behavior_blocked
        && stored
            .gate
            .verdicts
            .iter()
            .filter(|verdict| !verdict.admitted)
            .all(|verdict| !verdict.lowering_allowed)
        && stored_matches
        && provenance_matches
        && all_edges_matched;
    ensure(acceptance, "issue859 Ward envelope acceptance failed")?;

    let report = json!({
        "schema": "litd.engine.issue859.agent_ward_envelope_fsv.v1",
        "schema_version": 1,
        "issue": 859,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual FSV must inspect this JSON, manifest, Aster WardGateLedger row, verdicts, hashes, and Synapse bitmap"
        },
        "research_sources": [
            "https://owasp.org/www-community/Fail_securely",
            "https://owasp.org/www-project-secure-coding-practices-quick-reference-guide/stable-en/02-checklist/05-checklist",
            "https://csrc.nist.gov/pubs/sp/800/207/final",
            "https://gafferongames.com/post/deterministic_lockstep/"
        ],
        "happy_path": {
            "gate": gate,
            "stored_gate": stored.gate,
            "database": stored.database,
            "vault_files": vault_files
        },
        "acceptance": {
            "accepted_count": stored.gate.admitted_count,
            "refused_count": stored.gate.refused_count,
            "subject_count": stored.gate.subject_count,
            "allowed_ok": allowed_ok,
            "move_blocked": move_blocked,
            "teleport_blocked": teleport_blocked,
            "visual_blocked": visual_blocked,
            "behavior_blocked": behavior_blocked,
            "fail_closed": stored.gate.fail_closed,
            "all_refused_lowering_disallowed": stored.gate.verdicts.iter().filter(|verdict| !verdict.admitted).all(|verdict| !verdict.lowering_allowed),
            "stored_matches": stored_matches,
            "provenance_matches": provenance_matches,
            "all_edges_matched": all_edges_matched,
            "gate_fingerprint": stored.gate.gate_fingerprint,
            "database_sha256": stored.database.sha256,
            "refusal_codes": stored.gate.verdicts.iter().filter(|verdict| !verdict.admitted).map(|verdict| verdict.code.clone()).collect::<Vec<_>>()
        },
        "edges": edges
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE859_AGENT_WARD_ENVELOPE_FSV report={} sha256={} manifest={} run_id={} subjects={} accepted={} refused={} gate_fingerprint={} database_sha256={} edges={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        stored.gate.subject_count,
        stored.gate.admitted_count,
        stored.gate.refused_count,
        stored.gate.gate_fingerprint,
        stored.database.sha256,
        edges.len()
    );
    Ok(())
}

fn policy_for(subjects: &[EngineCalyxWardSubject]) -> EngineCalyxWardPolicy {
    EngineCalyxWardPolicy {
        policy_id: "ward-issue859".to_string(),
        allowed_terms: strings(&["agent", "guard", "gate", "patrol", "move", "ward", "visual"]),
        blocked_terms: strings(&["nuclear", "exploit", "desync"]),
        trusted_texts: subjects
            .iter()
            .filter(|subject| subject.subject_id == "agent.guard.allowed_patrol")
            .map(|subject| subject.text.clone())
            .collect(),
        calibration_ood_texts: calibration_ood_texts(),
        min_grounding_refs: 2,
        min_matched_terms: 2,
        max_move_units_per_tick: 2,
        allow_teleport: false,
        max_scale_milli: 1500,
        max_emissive_milli: 1200,
    }
}

fn calibration_ood_texts() -> Vec<String> {
    let mut rows = (0..calyx_ward::MIN_BAD_SCORES)
        .map(|index| {
            format!(
                "agent guard gate patrol move ward desync exploit impossible visual {}",
                index
            )
        })
        .collect::<Vec<_>>();
    rows[0] = "agent guard nuclear exploit desync".to_string();
    rows
}

fn ward_subjects() -> Vec<EngineCalyxWardSubject> {
    vec![
        subject(
            "agent.guard.allowed_patrol",
            "agent guard gate patrol move ward",
            2,
            false,
            1200,
            800,
        ),
        subject(
            "agent.guard.illegal_sprint",
            "agent guard gate patrol move ward",
            5,
            false,
            1200,
            800,
        ),
        subject(
            "agent.guard.illegal_teleport",
            "agent guard gate patrol move ward",
            1,
            true,
            1200,
            800,
        ),
        subject(
            "agent.guard.broken_visual",
            "agent guard gate visual ward",
            1,
            false,
            2500,
            1800,
        ),
        subject(
            "agent.guard.ood_behavior",
            "agent guard nuclear exploit desync",
            1,
            false,
            1000,
            0,
        ),
    ]
}

fn subject(
    subject_id: &str,
    text: &str,
    movement_units_per_tick: u32,
    teleport_requested: bool,
    scale_milli: u32,
    emissive_milli: u32,
) -> EngineCalyxWardSubject {
    EngineCalyxWardSubject {
        subject_id: subject_id.to_string(),
        kind: EngineCalyxWardSubjectKind::AgentActivation,
        content_kind: Some(EngineContentKind::Agent),
        text: text.to_string(),
        grounding_refs: vec![
            format!("grounding://issue859/{subject_id}/envelope"),
            format!("world-state://issue859/{subject_id}/signal"),
        ],
        provenance_refs: vec![format!("agent-action://issue859/{subject_id}")],
        movement_units_per_tick,
        teleport_requested,
        scale_milli,
        emissive_milli,
        source_fingerprint: litd_hash::sha256_hex(subject_id.as_bytes()),
    }
}

fn edge_readbacks(
    database: &EngineCalyxDatabase,
    gate: &EngineCalyxWardGateReadback,
    subjects: &[EngineCalyxWardSubject],
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut bad_policy = policy_for(subjects);
    bad_policy.max_move_units_per_tick = 0;
    let mut bad_subject = subjects[0].clone();
    bad_subject.subject_id.clear();
    let mut bad_count = gate.clone();
    bad_count.refused_count += 1;
    let mut bad_lowering = gate.clone();
    let admitted = bad_lowering
        .verdicts
        .iter_mut()
        .find(|verdict| verdict.admitted)
        .ok_or_else(|| io::Error::other("expected admitted verdict"))?;
    admitted.lowering_allowed = false;
    let mut bad_fingerprint = gate.clone();
    bad_fingerprint.gate_fingerprint = "00000000".to_string();
    Ok(vec![
        expect_invalid_input(
            "invalid_policy_rejected",
            evaluate_calyx_ward_gate("bad-policy", bad_policy, subjects),
            "policy",
            "required",
        ),
        expect_invalid_input(
            "empty_gate_id_rejected",
            evaluate_calyx_ward_gate("", policy_for(subjects), subjects),
            "gate_id",
            "non-empty",
        ),
        expect_invalid_input(
            "empty_subjects_rejected",
            evaluate_calyx_ward_gate("empty-subjects", policy_for(subjects), &[]),
            "subjects",
            "at least one",
        ),
        expect_invalid_input(
            "bad_subject_rejected",
            evaluate_calyx_ward_gate("bad-subject", policy_for(subjects), &[bad_subject]),
            "subject",
            "required",
        ),
        expect_invalid_input(
            "gate_count_tamper_rejected",
            validate_gate_readback(&bad_count),
            "counts",
            "must match",
        ),
        expect_invalid_input(
            "lowering_tamper_rejected",
            validate_gate_readback(&bad_lowering),
            "verdict.lowering_allowed",
            "admitted",
        ),
        expect_invalid_input(
            "fingerprint_tamper_rejected",
            validate_gate_readback(&bad_fingerprint),
            "gate_fingerprint",
            "fingerprint mismatch",
        ),
        missing_gate_edge(database),
    ])
}

fn expect_invalid_input<T>(
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
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string()
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded"
        }),
    }
}

fn missing_gate_edge(database: &EngineCalyxDatabase) -> Value {
    match read_calyx_ward_gate(database, "missing-issue859-gate") {
        Err(EngineCalyxWardGateError::Persistence(error)) => json!({
            "label": "missing_persisted_gate_rejected",
            "matched": error.to_string().contains("missing-issue859-gate"),
            "error": error.to_string()
        }),
        Err(error) => json!({
            "label": "missing_persisted_gate_rejected",
            "matched": false,
            "unexpected_error": error.to_string()
        }),
        Ok(_) => json!({
            "label": "missing_persisted_gate_rejected",
            "matched": false,
            "unexpected": "read succeeded"
        }),
    }
}

fn verdict<'a>(
    gate: &'a EngineCalyxWardGateReadback,
    subject_id: &str,
) -> Option<&'a litd_engine::calyx_ward_gate::EngineCalyxWardVerdict> {
    gate.verdicts
        .iter()
        .find(|verdict| verdict.subject_id == subject_id)
}

fn refused_envelope(
    gate: &EngineCalyxWardGateReadback,
    subject_id: &str,
    expected_reason: &str,
) -> bool {
    verdict(gate, subject_id)
        .map(|verdict| {
            !verdict.admitted
                && !verdict.lowering_allowed
                && verdict.code == "LITD-WARD-REFUSED-ENVELOPE"
                && verdict
                    .reasons
                    .iter()
                    .any(|reason| reason == expected_reason)
        })
        .unwrap_or(false)
}

fn vault_file_readbacks(vault_dir: &Path) -> io::Result<Vec<Value>> {
    let mut rows = Vec::new();
    if vault_dir.exists() {
        for entry in fs::read_dir(vault_dir)? {
            collect_file_readbacks(&entry?.path(), &mut rows)?;
        }
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
