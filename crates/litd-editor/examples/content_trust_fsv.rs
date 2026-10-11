use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::content_trust::{
    ContentTrustDecision, ContentTrustLedger, ContentTrustPolicy, ContentTrustStatus,
    SharedContentImport, SharedContentOrigin, build_content_trust_ledger, evaluate_content_trust,
    parse_content_trust_ledger, read_content_trust_ledger, write_content_trust_ledger,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_CONTENT_TRUST_FSV",
        "issue1208_content_trust",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1208 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("content-trust-ledger.json");
    let aster_dir = root.join("aster");
    let before = path_state(&output_path, &state_path, &aster_dir);

    let policy = policy();
    let provisional = evaluate_content_trust(&ugc_import("issue1208-provisional"), &policy)?;
    let grounded = evaluate_content_trust(&grounded_ugc_import("issue1208-grounded"), &policy)?;
    let trusted = evaluate_content_trust(&trusted_import("issue1208-trusted"), &policy)?;
    let purpose = evaluate_content_trust(&wrong_purpose_import(), &policy)?;
    let consent = evaluate_content_trust(&missing_consent_import(), &policy)?;
    let blocked = evaluate_content_trust(&blocked_import(), &policy)?;
    let escalation = evaluate_content_trust(&escalation_import(), &policy)?;
    let ledger = build_content_trust_ledger(
        "issue1208-content-trust-fsv",
        vec![
            provisional.clone(),
            grounded.clone(),
            trusted.clone(),
            purpose.clone(),
            consent.clone(),
            blocked.clone(),
            escalation.clone(),
        ],
    )?;
    verify_expected_decisions(&ledger)?;

    let writeback = write_content_trust_ledger(&state_path, &aster_dir, &ledger)?;
    let readback = read_content_trust_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing issue1208 content-trust ledger")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err("state file bytes differ from Aster content-trust ledger bytes".into());
    }
    let parsed = parse_content_trust_ledger(&readback.bytes)?;
    verify_expected_decisions(&parsed)?;

    let document = json!({
        "schema": "litd.issue1208.content_trust_fsv.v1",
        "issue": 1208,
        "source_of_truth": "Aster content-trust ledger row and JSON mirror readback",
        "before": before,
        "happy_path": {
            "before": {
                "policy_id": policy.policy_id,
                "allowed_purposes": policy.allowed_purposes,
                "required_consent_scopes": policy.required_consent_scopes,
                "min_grounding_refs": policy.min_grounding_refs,
                "trusted_sources": policy.trusted_sources
            },
            "after": {
                "state_path": state_path,
                "state_sha256": sha256_hex(&disk_bytes),
                "aster_sha256": readback.sha256,
                "writeback_sha256": writeback.sha256,
                "state_matches_aster": disk_bytes == readback.bytes,
                "decision_count": parsed.decision_count,
                "graph_rows": readback.graph_rows,
                "seq": readback.seq,
                "cell_key": writeback.cell_key,
                "index_key": writeback.index_key,
                "trusted_first_party": summarize_decision(&trusted),
                "passed": true
            }
        },
        "edges": [
            {
                "edge": "cold_start_provisional_until_grounded",
                "before": {
                    "untrusted_origin": "UserGenerated",
                    "ungrounded_refs": 1,
                    "grounded_refs": 2,
                    "expected_min_grounding_refs": 2
                },
                "after": {
                    "ungrounded": summarize_decision(&provisional),
                    "grounded": summarize_decision(&grounded),
                    "passed": provisional.status == ContentTrustStatus::Provisional
                        && grounded.status == ContentTrustStatus::Trusted
                }
            },
            {
                "edge": "purpose_limit_violation_fails_closed",
                "before": {"declared_purpose": "train_external_model"},
                "after": summarize_decision(&purpose)
            },
            {
                "edge": "missing_consent_fails_closed",
                "before": {"consent_scopes": []},
                "after": summarize_decision(&consent)
            },
            {
                "edge": "blocked_content_fails_closed",
                "before": {"matched_term": "malware_payload"},
                "after": summarize_decision(&blocked)
            },
            {
                "edge": "escalation_route_recorded",
                "before": {"matched_term": "copyright_dispute"},
                "after": summarize_decision(&escalation)
            }
        ]
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1208 content trust FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "ledger decisions={} graph_rows={} state_matches_aster={} sha={}",
        parsed.decision_count,
        readback.graph_rows,
        disk_bytes == readback.bytes,
        readback.sha256
    );
    println!(
        "edges provisional={} grounded={} purpose={} consent={} blocked={} escalation={}",
        provisional.code, grounded.code, purpose.code, consent.code, blocked.code, escalation.code
    );
    Ok(())
}

fn verify_expected_decisions(
    ledger: &ContentTrustLedger,
) -> Result<(), Box<dyn std::error::Error>> {
    expect_decision(
        ledger,
        "issue1208-provisional",
        ContentTrustStatus::Provisional,
        "LITD-TRUST-PROVISIONAL-COLD-START",
    )?;
    expect_decision(
        ledger,
        "issue1208-grounded",
        ContentTrustStatus::Trusted,
        "LITD-TRUST-GROUNDED-IMPORT",
    )?;
    expect_decision(
        ledger,
        "issue1208-trusted",
        ContentTrustStatus::Trusted,
        "LITD-TRUST-TRUSTED-SOURCE",
    )?;
    expect_decision(
        ledger,
        "issue1208-purpose",
        ContentTrustStatus::Refused,
        "LITD-TRUST-PURPOSE-DENIED",
    )?;
    expect_decision(
        ledger,
        "issue1208-consent",
        ContentTrustStatus::Refused,
        "LITD-TRUST-CONSENT-MISSING",
    )?;
    expect_decision(
        ledger,
        "issue1208-blocked",
        ContentTrustStatus::Refused,
        "LITD-TRUST-BLOCKED-CONTENT",
    )?;
    expect_decision(
        ledger,
        "issue1208-escalation",
        ContentTrustStatus::Escalated,
        "LITD-TRUST-ESCALATION-REQUIRED",
    )?;
    Ok(())
}

fn expect_decision(
    ledger: &ContentTrustLedger,
    import_id: &str,
    status: ContentTrustStatus,
    code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let decision = ledger
        .decisions
        .iter()
        .find(|decision| decision.import_id == import_id)
        .ok_or_else(|| format!("missing content-trust decision {import_id}"))?;
    if decision.status != status || decision.code != code {
        return Err(format!(
            "{import_id} expected {status:?}/{code}, got {:?}/{}",
            decision.status, decision.code
        )
        .into());
    }
    Ok(())
}

fn summarize_decision(decision: &ContentTrustDecision) -> serde_json::Value {
    json!({
        "import_id": decision.import_id,
        "content_id": decision.content_id,
        "status": format!("{:?}", decision.status),
        "code": decision.code,
        "reasons": decision.reasons,
        "declared_purpose": decision.declared_purpose,
        "consent_scopes": decision.consent_scopes,
        "missing_consent_scopes": decision.missing_consent_scopes,
        "grounding_ref_count": decision.grounding_refs.len(),
        "source_uri": decision.source_uri,
        "creator_id": decision.creator_id,
        "fingerprint": decision.content_fingerprint,
        "escalation": decision.escalation.as_ref().map(|route| json!({
            "queue": route.queue,
            "reason": route.reason,
            "matched_terms": route.matched_terms
        }))
    })
}

fn policy() -> ContentTrustPolicy {
    ContentTrustPolicy {
        policy_id: "issue1208-cold-start-policy".to_string(),
        allowed_purposes: vec!["ecosystem_share".to_string(), "play_session".to_string()],
        required_consent_scopes: vec!["ecosystem_share".to_string()],
        trusted_sources: vec!["litd://first-party/library/core".to_string()],
        min_grounding_refs: 2,
        blocked_terms: vec!["malware_payload".to_string()],
        escalation_terms: vec!["copyright_dispute".to_string()],
        escalation_queue: "trust-safety-review".to_string(),
    }
}

fn ugc_import(import_id: &str) -> SharedContentImport {
    SharedContentImport {
        import_id: import_id.to_string(),
        content_id: "ugc.map.fragment.ashen-bridge".to_string(),
        origin: SharedContentOrigin::UserGenerated,
        creator_id: "creator-17".to_string(),
        source_uri: "ugc://creator-17/ashen-bridge".to_string(),
        declared_purpose: "ecosystem_share".to_string(),
        consent_scopes: vec!["ecosystem_share".to_string()],
        grounding_refs: vec!["Aster://ugc/ashen-bridge/source".to_string()],
        body: "Ashen bridge encounter for Dawnward scouts".to_string(),
        requested_by: "issue1208-fsv".to_string(),
    }
}

fn grounded_ugc_import(import_id: &str) -> SharedContentImport {
    let mut import = ugc_import(import_id);
    import
        .grounding_refs
        .push("Aster://ugc/ashen-bridge/manifest".to_string());
    import
}

fn trusted_import(import_id: &str) -> SharedContentImport {
    SharedContentImport {
        import_id: import_id.to_string(),
        content_id: "firstparty.template.cinder-gate".to_string(),
        origin: SharedContentOrigin::FirstParty,
        creator_id: "litd-core-team".to_string(),
        source_uri: "litd://first-party/library/core".to_string(),
        declared_purpose: "ecosystem_share".to_string(),
        consent_scopes: vec!["ecosystem_share".to_string()],
        grounding_refs: Vec::new(),
        body: "First party cinder gate template".to_string(),
        requested_by: "issue1208-fsv".to_string(),
    }
}

fn wrong_purpose_import() -> SharedContentImport {
    let mut import = ugc_import("issue1208-purpose");
    import.declared_purpose = "train_external_model".to_string();
    import
}

fn missing_consent_import() -> SharedContentImport {
    let mut import = ugc_import("issue1208-consent");
    import.consent_scopes.clear();
    import
}

fn blocked_import() -> SharedContentImport {
    let mut import = ugc_import("issue1208-blocked");
    import.body.push_str(" malware_payload");
    import
}

fn escalation_import() -> SharedContentImport {
    let mut import = grounded_ugc_import("issue1208-escalation");
    import.body.push_str(" copyright_dispute");
    import
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            if fs::metadata(path)?.len() != expected_len {
                return Err("issue1208 FSV artifact byte readback mismatch".into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("issue1208 FSV artifact byte-count stabilization failed".into())
}

fn path_state(output_path: &PathBuf, state_path: &PathBuf, aster_dir: &Path) -> serde_json::Value {
    json!({
        "artifact_exists": output_path.exists(),
        "artifact_bytes": fs::metadata(output_path).map(|metadata| metadata.len()).unwrap_or(0),
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "aster_dir_exists": aster_dir.exists()
    })
}
