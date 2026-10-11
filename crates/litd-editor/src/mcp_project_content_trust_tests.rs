use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    content_trust::{
        ContentTrustPolicy, SharedContentImport, SharedContentOrigin, read_content_trust_ledger,
    },
    mcp_project_tools::{
        CONTENT_TRUST_TOOL, ContentTrustToolArgs, READ_CONTENT_TRUST_TOOL, call_mcp_project_tool,
        read_mcp_project_state,
    },
};

#[test]
fn issue1317_content_trust_dispatch_persists_real_aster_rows() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1317_content_trust_dispatch", "manual").unwrap();
    let work = root.join("workspace");
    clean_dir(&work);
    let paths = Paths::new(&work);

    let before = source_truth_summary(
        &paths,
        &[
            "issue1317-trusted-ledger",
            "issue1317-blocked-ledger",
            "issue1317-consent-ledger",
            "issue1317-invalid-ledger",
        ],
    );
    println!(
        "issue1317 before {}",
        serde_json::to_string(&before).unwrap()
    );

    let trusted = call_mcp_project_tool(
        CONTENT_TRUST_TOOL,
        args_value(
            &paths,
            "issue1317-trusted",
            "issue1317-trusted-ledger",
            trusted_import(),
        ),
    )
    .unwrap();
    assert_eq!(trusted.result["decision"]["status"], "Trusted");
    assert_eq!(
        trusted.result["decision"]["code"],
        "LITD-TRUST-TRUSTED-SOURCE"
    );

    let trusted_read = call_mcp_project_tool(
        READ_CONTENT_TRUST_TOOL,
        read_args(&paths, "issue1317-trusted-ledger"),
    )
    .unwrap();
    assert_eq!(trusted_read.result["ledger"]["decision_count"], 1);
    assert_eq!(
        trusted_read.result["ledger"]["decisions"][0]["status"],
        "Trusted"
    );

    let mut blocked_import = ugc_import("issue1317-blocked-import");
    blocked_import.body.push_str(" malware_payload");
    call_mcp_project_tool(
        CONTENT_TRUST_TOOL,
        args_value(
            &paths,
            "issue1317-blocked",
            "issue1317-blocked-ledger",
            blocked_import,
        ),
    )
    .unwrap();
    let blocked_read = call_mcp_project_tool(
        READ_CONTENT_TRUST_TOOL,
        read_args(&paths, "issue1317-blocked-ledger"),
    )
    .unwrap();
    assert_eq!(
        blocked_read.result["ledger"]["decisions"][0]["status"],
        "Refused"
    );
    assert_eq!(
        blocked_read.result["ledger"]["decisions"][0]["code"],
        "LITD-TRUST-BLOCKED-CONTENT"
    );

    let mut consent_import = ugc_import("issue1317-consent-import");
    consent_import.consent_scopes.clear();
    call_mcp_project_tool(
        CONTENT_TRUST_TOOL,
        args_value(
            &paths,
            "issue1317-consent",
            "issue1317-consent-ledger",
            consent_import,
        ),
    )
    .unwrap();
    let consent_read = call_mcp_project_tool(
        READ_CONTENT_TRUST_TOOL,
        read_args(&paths, "issue1317-consent-ledger"),
    )
    .unwrap();
    assert_eq!(
        consent_read.result["ledger"]["decisions"][0]["status"],
        "Refused"
    );
    assert_eq!(
        consent_read.result["ledger"]["decisions"][0]["code"],
        "LITD-TRUST-CONSENT-MISSING"
    );

    let mut invalid_import = ugc_import("issue1317-invalid-import");
    invalid_import.import_id.push('\n');
    let invalid_before =
        read_content_trust_ledger(&paths.aster_dir, "issue1317-invalid-ledger").unwrap();
    let invalid_error = call_mcp_project_tool(
        CONTENT_TRUST_TOOL,
        args_value(
            &paths,
            "issue1317-invalid",
            "issue1317-invalid-ledger",
            invalid_import,
        ),
    )
    .expect_err("invalid import id must fail closed before persistence");
    let invalid_after =
        read_content_trust_ledger(&paths.aster_dir, "issue1317-invalid-ledger").unwrap();
    assert!(invalid_before.is_none());
    assert!(invalid_after.is_none());
    assert!(format!("{invalid_error:?}").contains("editor.content_trust.import.import_id"));

    let after = source_truth_summary(
        &paths,
        &[
            "issue1317-trusted-ledger",
            "issue1317-blocked-ledger",
            "issue1317-consent-ledger",
            "issue1317-invalid-ledger",
        ],
    );
    println!("issue1317 after {}", serde_json::to_string(&after).unwrap());

    let state_readback = read_mcp_project_state(&paths.state_path).unwrap();
    assert_eq!(state_readback.state.update_count, 3);

    let report = json!({
        "schema_version": 1,
        "issue": 1317,
        "source_of_truth": "Aster Graph CF content-trust ledger rows plus MCP project state JSON",
        "tool_name": CONTENT_TRUST_TOOL,
        "readback_tool_name": READ_CONTENT_TRUST_TOOL,
        "before": before,
        "after": after,
        "happy_path": {
            "ledger_id": "issue1317-trusted-ledger",
            "expected_status": "Trusted",
            "expected_code": "LITD-TRUST-TRUSTED-SOURCE",
            "actual": trusted_read.result["ledger"]["decisions"][0],
        },
        "edge_cases": [
            {
                "name": "blocked term",
                "ledger_id": "issue1317-blocked-ledger",
                "expected_status": "Refused",
                "expected_code": "LITD-TRUST-BLOCKED-CONTENT",
                "actual": blocked_read.result["ledger"]["decisions"][0],
            },
            {
                "name": "missing consent",
                "ledger_id": "issue1317-consent-ledger",
                "expected_status": "Refused",
                "expected_code": "LITD-TRUST-CONSENT-MISSING",
                "actual": consent_read.result["ledger"]["decisions"][0],
            },
            {
                "name": "invalid import id",
                "ledger_id": "issue1317-invalid-ledger",
                "expected_error_contains": "editor.content_trust.import.import_id",
                "before_present": invalid_before.is_some(),
                "after_present": invalid_after.is_some(),
                "actual_error": format!("{invalid_error:?}"),
            }
        ],
        "state_readback": {
            "path": state_readback.path,
            "sha256": state_readback.sha256,
            "update_count": state_readback.state.update_count,
        },
    });
    let report_readback = fsv_run.write_report(&report).unwrap();
    println!(
        "ISSUE1317_CONTENT_TRUST_FSV report={} sha256={} run_id={} state_update_count={} trusted_sha={} blocked_sha={} consent_sha={}",
        report_readback.path.display(),
        report_readback.sha256,
        fsv_run.run_id(),
        state_readback.state.update_count,
        after["issue1317-trusted-ledger"]["sha256"],
        after["issue1317-blocked-ledger"]["sha256"],
        after["issue1317-consent-ledger"]["sha256"],
    );
}

struct Paths {
    state_path: PathBuf,
    ledger_path: PathBuf,
    trust_state_path: PathBuf,
    aster_dir: PathBuf,
}

impl Paths {
    fn new(root: &Path) -> Self {
        Self {
            state_path: root.join("project-state.json"),
            ledger_path: root.join("project-tool-ledger.json"),
            trust_state_path: root.join("content-trust-state.json"),
            aster_dir: root.join("content-trust-aster"),
        }
    }
}

#[test]
fn issue1518_content_trust_rejects_out_of_root_paths_without_writing() {
    // #1518 confinement close gate for the mcp_project_tools sub-tools (content_trust
    // representative): a poisoned state_path (absolute-escape) and a ..-traversal
    // trust_aster_dir are both rejected before any fs effect, leaving the filesystem
    // untouched.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_run, root) =
        crate::fsv_support::artifact_dir("issue1518_content_trust", "confine").unwrap();
    let work = root.join("workspace");
    clean_dir(&work);
    let paths = Paths::new(&work);

    // --- EDGE 1: absolute state_path OUTSIDE the workspace root ---
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-ct");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).ok();
    }
    let mut poisoned = args_value(
        &paths,
        "issue1518-escape",
        "issue1518-ledger",
        trusted_import(),
    );
    poisoned["state_path"] = json!(escape_dir.join("project-state.json").display().to_string());
    let e1 = call_mcp_project_tool(CONTENT_TRUST_TOOL, poisoned)
        .unwrap_err()
        .to_string();
    println!("issue1518 content_trust absolute-escape rejected: {e1}");
    assert!(
        e1.contains("outside the project root"),
        "expected outside-root rejection, got: {e1}"
    );
    assert!(!escape_dir.exists());

    // --- EDGE 2: ..-traversal trust_aster_dir ---
    let mut poisoned2 = args_value(
        &paths,
        "issue1518-traversal",
        "issue1518-ledger",
        trusted_import(),
    );
    poisoned2["trust_aster_dir"] = json!("ct/../../litd-issue1518-ct-traversal");
    let e2 = call_mcp_project_tool(CONTENT_TRUST_TOOL, poisoned2)
        .unwrap_err()
        .to_string();
    println!("issue1518 content_trust traversal rejected: {e2}");
    assert!(
        e2.contains("parent-directory"),
        "expected traversal rejection, got: {e2}"
    );
    assert!(!std::path::Path::new("../litd-issue1518-ct-traversal").exists());
}

fn args_value(
    paths: &Paths,
    request_id: &str,
    trust_ledger_id: &str,
    import: SharedContentImport,
) -> Value {
    json!(ContentTrustToolArgs {
        request_id: request_id.to_string(),
        requested_by: "issue1317-agent".to_string(),
        state_path: paths.state_path.display().to_string(),
        state_id: "issue1317-project-state".to_string(),
        ledger_path: paths.ledger_path.display().to_string(),
        // Stable per-ledger id (like state_id): the project tool ledger now appends,
        // so every write to one ledger_path must share one ledger_id (#1490). The
        // request_id varies per call and is recorded inside the appended record.
        ledger_id: "issue1317-project-ledger".to_string(),
        trust_state_path: paths.trust_state_path.display().to_string(),
        trust_aster_dir: paths.aster_dir.display().to_string(),
        trust_ledger_id: trust_ledger_id.to_string(),
        policy: policy(),
        import,
    })
}

fn read_args(paths: &Paths, trust_ledger_id: &str) -> Value {
    json!({
        "state_path": paths.state_path,
        "trust_aster_dir": paths.aster_dir,
        "trust_ledger_id": trust_ledger_id,
    })
}

fn policy() -> ContentTrustPolicy {
    ContentTrustPolicy {
        policy_id: "issue1317-ecosystem-policy".to_string(),
        allowed_purposes: vec!["ecosystem_share".to_string(), "play_session".to_string()],
        required_consent_scopes: vec!["ecosystem_share".to_string()],
        trusted_sources: vec!["litd://first-party/library/core".to_string()],
        min_grounding_refs: 2,
        blocked_terms: vec!["malware_payload".to_string()],
        escalation_terms: vec!["copyright_dispute".to_string()],
        escalation_queue: "trust-safety-review".to_string(),
    }
}

fn trusted_import() -> SharedContentImport {
    SharedContentImport {
        import_id: "issue1317-trusted-import".to_string(),
        content_id: "firstparty.template.cinder-gate".to_string(),
        origin: SharedContentOrigin::FirstParty,
        creator_id: "litd-core-team".to_string(),
        source_uri: "litd://first-party/library/core".to_string(),
        declared_purpose: "ecosystem_share".to_string(),
        consent_scopes: vec!["ecosystem_share".to_string()],
        grounding_refs: Vec::new(),
        body: "First party cinder gate template".to_string(),
        requested_by: "issue1317-agent".to_string(),
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
        requested_by: "issue1317-agent".to_string(),
    }
}

fn source_truth_summary(paths: &Paths, ledger_ids: &[&str]) -> Value {
    let mut rows = serde_json::Map::new();
    for ledger_id in ledger_ids {
        let readback = read_content_trust_ledger(&paths.aster_dir, ledger_id).unwrap_or(None);
        rows.insert(
            (*ledger_id).to_string(),
            match readback {
                Some(readback) => json!({
                    "present": true,
                    "sha256": readback.sha256,
                    "seq": readback.seq,
                    "graph_rows": readback.graph_rows,
                    "decision_count": readback.decision_count,
                }),
                None => json!({ "present": false }),
            },
        );
    }
    Value::Object(rows)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
