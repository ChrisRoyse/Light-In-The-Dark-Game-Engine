use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    content_trust::{
        ContentTrustPolicy, SharedContentImport, SharedContentOrigin, build_content_trust_ledger,
        evaluate_content_trust, read_content_trust_ledger, write_content_trust_ledger,
    },
    mcp_project_tools::{
        MODERATION_SAFETY_TOOL, SOCIAL_GRAPH_TOOL, call_mcp_project_tool,
        inspect_litdworld_archive, read_delivery_store_manifest, read_hub_launcher_state,
        read_litdgame_install_manifest, read_litdworld_project, read_mcp_project_state,
        read_moderation_safety_state, read_multiplayer_session_state, read_publish_catalog,
        read_social_graph_state,
    },
    quick_share::{
        LITD_RUNTIME_ID, QuickShareLedger, QuickSharePackage, QuickSharePackageWriteback,
        build_quick_share_ledger, load_quick_share_package, read_quick_share_ledger,
        write_quick_share_ledger,
    },
};

use crate::mcp_project_social_test_support::{
    FixturePaths, accept_friendship, create_account_args, create_friend, create_host,
    prepare_multiplayer_fixture_for_issue, send_friendship, set_friend_presence, set_host_presence,
    social_summary,
};

#[test]
fn issue1362_project_ecosystem_stores_live_in_calyx_aster_fsv() {
    let paths = prepare_multiplayer_fixture_for_issue("issue1362_project_ecosystem_calyx", "happy");
    let quick_aster = paths.root.join("quick-share-ledger-aster");
    let trust_aster = paths.root.join("content-trust-aster");
    let quick_legacy = paths.root.join("quick-share-ledger.json");
    let trust_legacy = paths.root.join("content-trust-ledger.json");
    let moderation_path = paths.root.join("moderation-safety-state.json");

    let before = source_state(&paths, &moderation_path, &quick_aster, &trust_aster);

    create_host(&paths);
    create_friend(&paths);
    send_friendship(&paths);
    accept_friendship(&paths);
    set_host_presence(&paths);
    set_friend_presence(&paths);
    write_owner_role(&paths, &moderation_path);
    let trust = write_trust_ledger(&trust_legacy, &trust_aster);
    let quick = write_quick_ledger(&paths.package_path, &quick_legacy, &quick_aster);

    let project = read_litdworld_project(&paths.project_path).unwrap();
    let project_archive = inspect_litdworld_archive(&paths.project_path).unwrap();
    let state = read_mcp_project_state(&paths.state_path).unwrap();
    let catalog = read_publish_catalog(&paths.catalog_path).unwrap();
    let delivery = read_delivery_store_manifest(&paths.delivery_manifest_path).unwrap();
    let install = read_litdgame_install_manifest(&paths.install_manifest_path).unwrap();
    let hub = read_hub_launcher_state(&paths.hub_state_path).unwrap();
    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).unwrap();
    let social = read_social_graph_state(&paths.social_state_path).unwrap();
    let moderation = read_moderation_safety_state(&moderation_path).unwrap();
    let trust_read = read_content_trust_ledger(&trust_aster, "issue1362-trust-ledger")
        .unwrap()
        .unwrap();
    let quick_read = read_quick_share_ledger(&quick_aster, "issue1362-quick-ledger")
        .unwrap()
        .unwrap();

    assert_eq!(project.project_id, "project.ashen_reach_issue1160");
    assert_eq!(project_archive.zip_magic, "PK0304");
    assert_eq!(project_archive.manifest.schema, "litd.worldarchive.v1");
    assert_eq!(project_archive.project, project);
    assert!(paths.project_path.is_file());
    assert_eq!(state.project_count, 1);
    assert_eq!(catalog.catalog.entry_count, 1);
    assert_eq!(delivery.manifest.entry_count, 1);
    assert_eq!(install.manifest.record_count, 1);
    assert_eq!(hub.state.result_count, 1);
    assert_eq!(multiplayer.state.lobby_count, 1);
    assert_eq!(social.state.account_count, 2);
    assert_eq!(social.state.friendship_count, 1);
    assert_eq!(social.state.presence_count, 2);
    assert_eq!(moderation.state.role_assignment_count, 1);
    assert_eq!(trust_read.sha256, trust.sha256);
    assert_eq!(quick_read.sha256, quick.sha256);
    assert_no_legacy_files(&[
        &paths.state_path,
        &paths.catalog_path,
        &paths.delivery_manifest_path,
        &paths.install_manifest_path,
        &paths.hub_state_path,
        &paths.multiplayer_state_path,
        &paths.social_state_path,
        &moderation_path,
        &quick_legacy,
        &trust_legacy,
    ]);

    let edges = json!({
        "stale_json_path_rejected": stale_json_path_edge(&paths),
        "empty_social_tags_rejected": empty_social_tags_edge(&paths),
        "invalid_quick_ledger_id_rejected": invalid_quick_ledger_id_edge(&paths, &quick),
    });
    let after = source_state(&paths, &moderation_path, &quick_aster, &trust_aster);
    let physical = physical_summary(&[
        ("project", paths.project_path.clone()),
        ("project_state", paths.state_path.clone()),
        ("catalog", paths.catalog_path.clone()),
        ("delivery", paths.delivery_manifest_path.clone()),
        ("install", paths.install_manifest_path.clone()),
        ("hub", paths.hub_state_path.clone()),
        ("multiplayer", paths.multiplayer_state_path.clone()),
        ("social", paths.social_state_path.clone()),
        ("moderation", moderation_path.clone()),
        ("quick_share", quick_aster.clone()),
        ("content_trust", trust_aster.clone()),
    ]);
    let legacy = legacy_file_states(&[
        paths.state_path.clone(),
        paths.catalog_path.clone(),
        paths.delivery_manifest_path.clone(),
        paths.install_manifest_path.clone(),
        paths.hub_state_path.clone(),
        paths.multiplayer_state_path.clone(),
        paths.social_state_path.clone(),
        moderation_path.clone(),
        quick_legacy.clone(),
        trust_legacy.clone(),
    ]);
    let report = json!({
        "schema_version": 1,
        "issue": 1362,
        "source_of_truth": "verified LitD world ZIP archive plus Calyx Aster vault rows for project/social/multiplayer/moderation/content-trust/quick-share structured records",
        "before": before,
        "after": after,
        "happy_path": {
            "project_id": project.project_id,
            "project_archive_path": project_archive.path,
            "project_archive_sha256": project_archive.sha256,
            "project_archive_zip_magic": project_archive.zip_magic,
            "project_archive_manifest_schema": project_archive.manifest.schema,
            "project_state_sha256": state.sha256,
            "catalog_sha256": catalog.sha256,
            "delivery_sha256": delivery.sha256,
            "install_sha256": install.sha256,
            "hub_sha256": hub.sha256,
            "multiplayer_sha256": multiplayer.sha256,
            "social_sha256": social.sha256,
            "moderation_sha256": moderation.sha256,
            "quick_share_sha256": quick_read.sha256,
            "content_trust_sha256": trust_read.sha256,
        },
        "edges": edges,
        "physical_aster_files": physical,
        "legacy_json_file_states": legacy,
    });
    let readback = paths.fsv_run.write_report(&report).unwrap();
    println!(
        "ISSUE1362_CALYX_FSV_REPORT path={} bytes={} sha256={} aster_files={} social_accounts={} moderation_roles={} quick_sha={} trust_sha={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["physical_aster_files"]["total_files"],
        social.state.account_count,
        moderation.state.role_assignment_count,
        quick_read.sha256,
        trust_read.sha256,
    );
}

fn write_owner_role(paths: &FixturePaths, moderation_path: &Path) {
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        json!({
            "request_id": "issue1362-owner-role",
            "requested_by": "litd-editor-issue1362-test",
            "state_path": paths.state_path,
            "state_id": "issue1160-project-state",
            "ledger_path": paths.root.join("moderation-ledger.json"),
            "ledger_id": "issue1362-moderation-ledger",
            "moderation_state_path": moderation_path,
            "moderation_state_id": "issue1362-moderation-state",
            "social_state_path": paths.social_state_path,
            "catalog_path": paths.catalog_path,
            "multiplayer_state_path": paths.multiplayer_state_path,
            "action": "assign_server_role",
            "assignment_id": "role.lobby.social.owner.host",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": "account.host",
            "role": "owner"
        }),
    )
    .unwrap();
}

fn write_trust_ledger(
    legacy_path: &Path,
    aster_dir: &Path,
) -> crate::content_trust::ContentTrustAsterReadback {
    let import = SharedContentImport {
        import_id: "issue1362-trust-import".to_string(),
        content_id: "ugc.map.fragment.ashen-bridge".to_string(),
        origin: SharedContentOrigin::UserGenerated,
        creator_id: "creator-1362".to_string(),
        source_uri: "ugc://creator-1362/ashen-bridge".to_string(),
        declared_purpose: "ecosystem_share".to_string(),
        consent_scopes: vec!["ecosystem_share".to_string()],
        grounding_refs: vec![
            "Aster://issue1362/grounding/source".to_string(),
            "Aster://issue1362/grounding/manifest".to_string(),
        ],
        body: "Grounded shared scenario for project ecosystem validation".to_string(),
        requested_by: "issue1362-fsv".to_string(),
    };
    let policy = ContentTrustPolicy {
        policy_id: "issue1362-policy".to_string(),
        allowed_purposes: vec!["ecosystem_share".to_string()],
        required_consent_scopes: vec!["ecosystem_share".to_string()],
        trusted_sources: Vec::new(),
        min_grounding_refs: 2,
        blocked_terms: vec!["malware_payload".to_string()],
        escalation_terms: vec!["copyright_dispute".to_string()],
        escalation_queue: "trust-safety-review".to_string(),
    };
    let decision = evaluate_content_trust(&import, &policy).unwrap();
    let ledger = build_content_trust_ledger("issue1362-trust-ledger", vec![decision]).unwrap();
    let readback = write_content_trust_ledger(legacy_path, aster_dir, &ledger).unwrap();
    assert!(!legacy_path.exists());
    readback
}

fn write_quick_ledger(
    package_path: &Path,
    legacy_path: &Path,
    aster_dir: &Path,
) -> crate::quick_share::QuickShareAsterReadback {
    let package_bytes = fs::read(package_path).unwrap();
    let package: QuickSharePackage = serde_json::from_slice(&package_bytes).unwrap();
    let writeback = QuickSharePackageWriteback {
        package_path: package_path.display().to_string(),
        bytes: package_bytes.len(),
        sha256: sha256_hex(&package_bytes),
        host_uri: package.host_uri.clone(),
    };
    let client = load_quick_share_package(package_path, LITD_RUNTIME_ID, "issue1362-fsv").unwrap();
    let ledger =
        build_quick_share_ledger("issue1362-quick-ledger", &package, &writeback, &client).unwrap();
    let readback = write_quick_share_ledger(legacy_path, aster_dir, &ledger).unwrap();
    assert!(!legacy_path.exists());
    readback
}

fn stale_json_path_edge(paths: &FixturePaths) -> Value {
    let stale_path = paths.root.join("edge-stale-social.json");
    fs::write(&stale_path, b"{\"schema\":\"old-json-side-store\"}").unwrap();
    let before = file_state(&stale_path);
    let before_hash = sha256_hex(&fs::read(&stale_path).unwrap());
    let mut args = create_account_args(
        paths,
        "account.edge",
        "player.edge",
        "profile.edge",
        "edge",
        &["local_first"],
    );
    args["social_state_path"] = json!(stale_path);
    let error = call_mcp_project_tool(SOCIAL_GRAPH_TOOL, args)
        .unwrap_err()
        .report();
    let after_hash = sha256_hex(&fs::read(&stale_path).unwrap());
    let after = file_state(&stale_path);
    assert_eq!(before_hash, after_hash);
    let value = json!({ "before": before, "after": after, "error": error_json(error) });
    println!("ISSUE1362_EDGE stale_json_path {}", value);
    value
}

fn empty_social_tags_edge(paths: &FixturePaths) -> Value {
    let before = social_summary(paths);
    let error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        create_account_args(
            paths,
            "account.empty",
            "player.empty",
            "profile.empty",
            "empty",
            &[],
        ),
    )
    .unwrap_err()
    .report();
    let after = social_summary(paths);
    assert_eq!(before, after);
    let value = json!({ "before": before, "after": after, "error": error_json(error) });
    println!("ISSUE1362_EDGE empty_social_tags {}", value);
    value
}

fn invalid_quick_ledger_id_edge(
    paths: &FixturePaths,
    valid_readback: &crate::quick_share::QuickShareAsterReadback,
) -> Value {
    let edge_aster = paths.root.join("edge-quick-share-aster");
    let before = file_state(&edge_aster);
    let mut ledger: QuickShareLedger = serde_json::from_slice(&valid_readback.bytes).unwrap();
    ledger.ledger_id = "bad\nledger".to_string();
    let error = write_quick_share_ledger(&paths.root.join("edge-quick.json"), &edge_aster, &ledger)
        .unwrap_err()
        .report();
    let after = file_state(&edge_aster);
    assert!(!edge_aster.exists());
    let value = json!({ "before": before, "after": after, "error": error_json(error) });
    println!("ISSUE1362_EDGE invalid_quick_ledger_id {}", value);
    value
}

fn source_state(
    paths: &FixturePaths,
    moderation_path: &Path,
    quick_aster: &Path,
    trust_aster: &Path,
) -> Value {
    json!({
        "project": file_state(&paths.project_path),
        "project_state": file_state(&paths.state_path),
        "catalog": file_state(&paths.catalog_path),
        "delivery": file_state(&paths.delivery_manifest_path),
        "install": file_state(&paths.install_manifest_path),
        "hub": file_state(&paths.hub_state_path),
        "multiplayer": file_state(&paths.multiplayer_state_path),
        "social": file_state(&paths.social_state_path),
        "moderation": file_state(moderation_path),
        "quick_share": file_state(quick_aster),
        "content_trust": file_state(trust_aster),
    })
}

fn assert_no_legacy_files(paths: &[&PathBuf]) {
    for path in paths {
        assert!(
            !path.is_file(),
            "{} must not be a legacy JSON file",
            path.display()
        );
    }
}

fn legacy_file_states(paths: &[PathBuf]) -> Value {
    Value::Array(paths.iter().map(|path| file_state(path)).collect())
}

fn physical_summary(stores: &[(&str, PathBuf)]) -> Value {
    let mut total_files = 0usize;
    let mut rows = Vec::new();
    for (label, path) in stores {
        let files = physical_files(path);
        total_files += files.len();
        rows.push(json!({
            "label": label,
            "path": path,
            "file_count": files.len(),
            "files": files,
        }));
    }
    json!({ "total_files": total_files, "stores": rows })
}

fn physical_files(root: &Path) -> Vec<String> {
    if root.is_file() {
        return vec![
            root.file_name()
                .unwrap_or(root.as_os_str())
                .to_string_lossy()
                .replace('\\', "/"),
        ];
    }
    let mut files = Vec::new();
    if root.exists() {
        collect_files(root, root, &mut files);
    }
    files.sort();
    files
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            files.push(
                path.strip_prefix(root)
                    .unwrap()
                    .display()
                    .to_string()
                    .replace('\\', "/"),
            );
        }
    }
}

fn file_state(path: &Path) -> Value {
    let metadata = fs::metadata(path).ok();
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "is_dir": path.is_dir(),
        "is_file": path.is_file(),
        "len": metadata.as_ref().map(|metadata| metadata.len()),
        "sha256": metadata
            .as_ref()
            .filter(|metadata| metadata.is_file())
            .map(|_| sha256_hex(&fs::read(path).unwrap())),
    })
}

fn error_json(report: litd_engine::diagnostics::ErrorReport) -> Value {
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}
