use std::{fs, path::PathBuf};

use super::*;

#[test]
fn untrusted_ugc_is_provisional_until_grounded() {
    let decision = evaluate_content_trust(&ugc_import("issue1208-provisional"), &policy()).unwrap();

    assert_eq!(decision.status, ContentTrustStatus::Provisional);
    assert_eq!(decision.code, "LITD-TRUST-PROVISIONAL-COLD-START");
    assert!(decision.grounding_refs.len() < policy().min_grounding_refs);
    println!(
        "issue1208 happy import={} status={:?} code={} grounding_refs={}",
        decision.import_id,
        decision.status,
        decision.code,
        decision.grounding_refs.len()
    );
}

#[test]
fn trusted_content_is_accepted_when_consent_and_purpose_match() {
    let decision = evaluate_content_trust(&trusted_import("issue1208-trusted"), &policy()).unwrap();

    assert_eq!(decision.status, ContentTrustStatus::Trusted);
    assert_eq!(decision.code, "LITD-TRUST-TRUSTED-SOURCE");
    assert!(decision.missing_consent_scopes.is_empty());
}

#[test]
fn purpose_violation_and_missing_consent_fail_closed() {
    let mut wrong_purpose = ugc_import("issue1208-purpose");
    wrong_purpose.declared_purpose = "train_external_model".to_string();
    let purpose = evaluate_content_trust(&wrong_purpose, &policy()).unwrap();
    assert_eq!(purpose.status, ContentTrustStatus::Refused);
    assert_eq!(purpose.code, "LITD-TRUST-PURPOSE-DENIED");

    let mut missing = ugc_import("issue1208-consent");
    missing.consent_scopes.clear();
    let consent = evaluate_content_trust(&missing, &policy()).unwrap();
    assert_eq!(consent.status, ContentTrustStatus::Refused);
    assert_eq!(consent.code, "LITD-TRUST-CONSENT-MISSING");
    assert_eq!(consent.missing_consent_scopes, vec!["ecosystem_share"]);
    println!(
        "issue1208 edges purpose={} consent_missing={:?}",
        purpose.code, consent.missing_consent_scopes
    );
}

#[test]
fn escalation_path_is_recorded_for_review_terms() {
    let mut import = grounded_ugc_import("issue1208-escalate");
    import.body.push_str(" copyright_dispute");

    let decision = evaluate_content_trust(&import, &policy()).unwrap();

    assert_eq!(decision.status, ContentTrustStatus::Escalated);
    assert_eq!(decision.code, "LITD-TRUST-ESCALATION-REQUIRED");
    let escalation = decision.escalation.as_ref().unwrap();
    assert_eq!(escalation.queue, "trust-safety-review");
    assert_eq!(escalation.matched_terms, vec!["copyright_dispute"]);
}

#[test]
fn blocked_terms_and_invalid_formats_fail_closed() {
    let mut blocked = ugc_import("issue1208-blocked");
    blocked.body.push_str(" malware_payload");
    let decision = evaluate_content_trust(&blocked, &policy()).unwrap();
    assert_eq!(decision.status, ContentTrustStatus::Refused);
    assert_eq!(decision.code, "LITD-TRUST-BLOCKED-CONTENT");

    let mut bad_policy = policy();
    bad_policy.blocked_terms.push(" ".to_string());
    let error = evaluate_content_trust(&ugc_import("issue1208-bad-policy"), &bad_policy)
        .expect_err("blank blocked term must fail closed");
    assert!(format!("{error:?}").contains("editor.content_trust.policy.blocked_terms"));

    let mut bad_import = ugc_import("issue1208-bad-import");
    bad_import.grounding_refs.push("\n".to_string());
    let error = evaluate_content_trust(&bad_import, &policy())
        .expect_err("blank grounding ref must fail closed");
    assert!(format!("{error:?}").contains("editor.content_trust.import.grounding_refs"));
}

#[test]
fn tampered_decision_status_code_fails_closed() {
    let mut decision =
        evaluate_content_trust(&ugc_import("issue1208-tampered"), &policy()).unwrap();
    decision.status = ContentTrustStatus::Trusted;
    let ledger = ContentTrustLedger {
        schema: CONTENT_TRUST_SCHEMA.to_string(),
        ledger_id: "issue1208-tampered-ledger".to_string(),
        decision_count: 1,
        decisions: vec![decision],
    };
    let error = validate_content_trust_ledger(&ledger)
        .expect_err("inconsistent decision status/code must fail closed");
    assert!(format!("{error:?}").contains("editor.content_trust.decision.trusted"));
}

#[test]
fn trust_ledger_persists_to_aster_and_reads_back() {
    let ledger = build_content_trust_ledger(
        "issue1208-ledger-test",
        vec![
            evaluate_content_trust(&ugc_import("issue1208-provisional"), &policy()).unwrap(),
            evaluate_content_trust(&trusted_import("issue1208-trusted"), &policy()).unwrap(),
            evaluate_content_trust(&grounded_ugc_import("issue1208-grounded"), &policy()).unwrap(),
        ],
    )
    .unwrap();
    let root = test_root("ledger");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("content-trust-ledger.json");
    let aster_dir = root.join("aster");

    let writeback = write_content_trust_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_content_trust_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_content_trust_ledger(&readback.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(readback.bytes, writeback.bytes);
    assert_eq!(parsed.decision_count, 3);
    assert_eq!(parsed.decisions[0].status, ContentTrustStatus::Provisional);
    println!(
        "issue1208 aster decisions={} graph_rows={} sha={}",
        parsed.decision_count, readback.graph_rows, readback.sha256
    );
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
        requested_by: "issue1208-test".to_string(),
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
        requested_by: "issue1208-test".to_string(),
    }
}

fn test_root(label: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1208", label)
}
