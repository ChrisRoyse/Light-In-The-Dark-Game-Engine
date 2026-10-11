use std::{fs, io, path::Path};

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use calyx_ward::CALYX_GUARD_OOD;
use litd_engine::{
    calyx_agent_runtime::{EngineWorldSignal, EngineWorldSignalKind, activate_calyx_agents},
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_ward_gate::{
        EngineCalyxWardGateError, EngineCalyxWardGateReadback, EngineCalyxWardPolicy,
        EngineCalyxWardSubject, evaluate_calyx_ward_gate, read_calyx_ward_gate,
        store_calyx_ward_gate, validate_gate_readback, ward_subject_from_agent_activation,
        ward_subject_from_content,
    },
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1121_calyx_ward_gate")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database.clone());

    let ability = store.put_content(&content(
        EngineContentKind::Ability,
        "ability-fire-ward",
        "Fire Ward",
        json!({
            "ability_id": "fire-ward",
            "summary": "grounded fire ward protects the gate",
            "tags": ["fire", "ward", "gate", "protect"]
        }),
    ))?;
    let agent = store.put_content(&content(
        EngineContentKind::Agent,
        "agent-guard",
        "Gate Guard",
        json!({
            "agent_id": "guard",
            "trigger_terms": ["enemy", "gate"],
            "prompt_template": "Agent {agent}: protect gate from {signal}",
            "target_body": 2,
            "order_velocity": [1, 0, 0]
        }),
    ))?;
    let runtime = activate_calyx_agents(
        "issue1121-agent",
        &[agent],
        &[signal("signal-enemy", &["enemy", "gate"], "enemy at gate")],
    )?;
    let pass_subjects = vec![
        ward_subject_from_content(&ability)?,
        ward_subject_from_agent_activation(&runtime.activations[0])?,
    ];
    let policy = policy_for(&pass_subjects);
    let pass_gate =
        evaluate_calyx_ward_gate("issue1121-ward-pass", policy.clone(), &pass_subjects)?;
    let pass_persisted = store_calyx_ward_gate(&database, &pass_gate)?;
    let pass_stored = read_calyx_ward_gate(&database, "issue1121-ward-pass")?;

    let refuse_subjects = refusal_subjects(&store, &pass_subjects)?;
    let refuse_gate =
        evaluate_calyx_ward_gate("issue1121-ward-refuse", policy.clone(), &refuse_subjects)?;
    let refuse_persisted = store_calyx_ward_gate(&database, &refuse_gate)?;
    let refuse_stored = read_calyx_ward_gate(&database, "issue1121-ward-refuse")?;
    let edges = edge_readbacks(&database, &pass_gate, &pass_subjects)?;
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let pass_acceptance = pass_gate.subject_count == 2
        && pass_gate.admitted_count == 2
        && pass_gate.refused_count == 0
        && !pass_gate.fail_closed
        && pass_gate
            .verdicts
            .iter()
            .all(|verdict| verdict.admitted && verdict.lowering_allowed)
        && pass_persisted.gate.gate_fingerprint == pass_gate.gate_fingerprint
        && pass_stored.gate.gate_fingerprint == pass_gate.gate_fingerprint;
    let refusal_codes = refuse_gate
        .verdicts
        .iter()
        .map(|verdict| verdict.code.clone())
        .collect::<Vec<_>>();
    let refusal_acceptance = refuse_gate.subject_count == 3
        && refuse_gate.admitted_count == 0
        && refuse_gate.refused_count == 3
        && refuse_gate.fail_closed
        && refuse_gate
            .verdicts
            .iter()
            .all(|verdict| !verdict.admitted && !verdict.lowering_allowed)
        && refusal_codes.contains(&CALYX_GUARD_OOD.to_string())
        && refusal_codes.contains(&"LITD-WARD-REFUSED-UNGROUNDED".to_string())
        && refusal_codes.contains(&"LITD-WARD-REFUSED-ENVELOPE".to_string())
        && refuse_persisted.gate.gate_fingerprint == refuse_gate.gate_fingerprint
        && refuse_stored.gate.gate_fingerprint == refuse_gate.gate_fingerprint;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));

    ensure(pass_acceptance, "Ward admitted-path acceptance failed")?;
    ensure(refusal_acceptance, "Ward refusal-path acceptance failed")?;
    ensure(all_edges_matched, "Ward edge readback acceptance failed")?;

    let report = json!({
        "schema": "litd.engine.issue1121.calyx_ward_gate_fsv.v1",
        "schema_version": 1,
        "issue": 1121,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, Aster vault rows, Ward ledgers, verdicts, and hashes"
        },
        "happy_path": {
            "content_readback": {
                "id": ability.id,
                "cx_id": ability.cx_id,
                "input_sha256": ability.input_sha256,
                "database_seq": ability.database.seq,
                "database_sha256": ability.database.sha256,
                "row_key": ability.database.row_key
            },
            "agent_runtime": runtime,
            "pass_gate": pass_gate,
            "pass_database": pass_stored.database,
            "refuse_gate": refuse_gate,
            "refuse_database": refuse_stored.database,
            "vault_files": vault_files
        },
        "acceptance": {
            "pass_acceptance": pass_acceptance,
            "pass_subject_count": pass_stored.gate.subject_count,
            "pass_admitted_count": pass_stored.gate.admitted_count,
            "pass_refused_count": pass_stored.gate.refused_count,
            "pass_gate_fingerprint": pass_stored.gate.gate_fingerprint,
            "pass_database_sha256": pass_stored.database.sha256,
            "refusal_acceptance": refusal_acceptance,
            "refuse_subject_count": refuse_stored.gate.subject_count,
            "refuse_admitted_count": refuse_stored.gate.admitted_count,
            "refuse_refused_count": refuse_stored.gate.refused_count,
            "refuse_fail_closed": refuse_stored.gate.fail_closed,
            "refusal_codes": refusal_codes,
            "refuse_gate_fingerprint": refuse_stored.gate.gate_fingerprint,
            "refuse_database_sha256": refuse_stored.database.sha256,
            "all_refused_lowering_disallowed": refuse_stored.gate.verdicts.iter().all(|verdict| !verdict.lowering_allowed),
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1121_CALYX_WARD_GATE_FSV report={} sha256={} manifest={} run_id={} admitted={} refused={} refusal_codes={} edges={} pass_fingerprint={} refuse_fingerprint={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        pass_gate.admitted_count,
        refuse_gate.refused_count,
        refusal_codes.join(","),
        edges.len(),
        pass_gate.gate_fingerprint,
        refuse_gate.gate_fingerprint
    );
    Ok(())
}

fn refusal_subjects(
    store: &EngineCalyxContentStore,
    trusted: &[EngineCalyxWardSubject],
) -> Result<Vec<EngineCalyxWardSubject>, Box<dyn std::error::Error>> {
    let ood = store.put_content(&content(
        EngineContentKind::Ability,
        "ability-ood",
        "Teleport Nuke",
        json!({
            "ability_id": "teleport-nuke",
            "summary": "nuclear teleport exploit beyond the world envelope",
            "tags": ["nuclear", "teleport", "exploit"]
        }),
    ))?;
    let mut ungrounded = ward_subject_from_content(&ood)?;
    ungrounded.subject_id = "manual/ungrounded".to_string();
    ungrounded.text = trusted[0].text.clone();
    ungrounded.grounding_refs = vec!["single-ref".to_string()];
    let fast_agent = store.put_content(&content(
        EngineContentKind::Agent,
        "agent-fast",
        "Fast Guard",
        json!({
            "agent_id": "fast",
            "trigger_terms": ["enemy"],
            "prompt_template": "Agent {agent}: protect gate from {signal}",
            "target_body": 3,
            "order_velocity": [5, 0, 0]
        }),
    ))?;
    let fast_runtime = activate_calyx_agents(
        "issue1121-agent-fast",
        &[fast_agent],
        &[signal("signal-fast", &["enemy"], "enemy at gate")],
    )?;
    Ok(vec![
        ward_subject_from_content(&ood)?,
        ungrounded,
        ward_subject_from_agent_activation(&fast_runtime.activations[0])?,
    ])
}

fn edge_readbacks(
    database: &EngineCalyxDatabase,
    gate: &EngineCalyxWardGateReadback,
    subjects: &[EngineCalyxWardSubject],
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut bad_policy = policy_for(subjects);
    bad_policy.allowed_terms.clear();
    let mut bad_counts = gate.clone();
    bad_counts.admitted_count += 1;
    let mut bad_lowering = gate.clone();
    bad_lowering.verdicts[0].lowering_allowed = false;
    let mut bad_fingerprint = gate.clone();
    bad_fingerprint.gate_fingerprint = "00000000".to_string();
    let mut bad_subject = subjects[0].clone();
    bad_subject.text.clear();
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
            "empty_subject_text_rejected",
            evaluate_calyx_ward_gate("bad-subject", policy_for(subjects), &[bad_subject]),
            "subject",
            "required",
        ),
        expect_invalid_input(
            "gate_count_tamper_rejected",
            validate_gate_readback(&bad_counts),
            "counts",
            "must match",
        ),
        expect_invalid_input(
            "lowering_allowed_tamper_rejected",
            validate_gate_readback(&bad_lowering),
            "verdict.lowering_allowed",
            "admitted",
        ),
        expect_invalid_input(
            "gate_fingerprint_tamper_rejected",
            validate_gate_readback(&bad_fingerprint),
            "gate_fingerprint",
            "fingerprint mismatch",
        ),
        missing_gate_edge(database),
    ])
}

fn policy_for(trusted: &[EngineCalyxWardSubject]) -> EngineCalyxWardPolicy {
    EngineCalyxWardPolicy {
        policy_id: "ward-issue1121".to_string(),
        allowed_terms: vec![
            "fire".to_string(),
            "ward".to_string(),
            "gate".to_string(),
            "protect".to_string(),
            "enemy".to_string(),
            "agent".to_string(),
        ],
        blocked_terms: vec![
            "nuclear".to_string(),
            "teleport".to_string(),
            "exploit".to_string(),
        ],
        trusted_texts: trusted.iter().map(|subject| subject.text.clone()).collect(),
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
        .map(|index| {
            format!(
                "fire ward gate protect enemy agent contradictory route {} void bargain cold ledger",
                index
            )
        })
        .collect::<Vec<_>>();
    rows[0] =
        "fire ward gate protect enemy agent protect gate enemy ward fire agent reversed bargain"
            .to_string();
    rows
}

fn content(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue1121/{id}"),
            author: "issue1121-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1121:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1121_grounding_anchor".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn signal(signal_id: &str, terms: &[&str], summary: &str) -> EngineWorldSignal {
    EngineWorldSignal {
        signal_id: signal_id.to_string(),
        world_id: "world-ashen-reach".to_string(),
        tick: 12,
        kind: EngineWorldSignalKind::Combat,
        strength_milli: 900,
        terms: terms.iter().map(|term| term.to_string()).collect(),
        summary: summary.to_string(),
        provenance_refs: vec![
            "grounding://signal/source".to_string(),
            format!("signal://issue1121/{signal_id}"),
        ],
    }
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
            "reason": reason,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
        }),
    }
}

fn missing_gate_edge(database: &EngineCalyxDatabase) -> Value {
    match read_calyx_ward_gate(database, "missing-ward-gate") {
        Err(EngineCalyxWardGateError::Persistence(error)) => json!({
            "label": "missing_persisted_gate_rejected",
            "matched": error.to_string().contains("missing-ward-gate"),
            "error": error.to_string(),
        }),
        Err(error) => json!({
            "label": "missing_persisted_gate_rejected",
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": "missing_persisted_gate_rejected",
            "matched": false,
            "unexpected": "read succeeded",
        }),
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
            "sha256": litd_hash::sha256_hex(&bytes),
        }));
    }
    Ok(())
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
