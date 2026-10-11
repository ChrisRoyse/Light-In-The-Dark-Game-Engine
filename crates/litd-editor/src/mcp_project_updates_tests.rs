use std::{fs, path::PathBuf};

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::mcp_project_tools::{
    GAME_UPDATES_TOOL, PUBLISH_LITDGAME_TOOL, call_mcp_project_tool, read_game_updates_state,
    read_litdgame_install_manifest, read_publish_catalog,
};

use crate::mcp_project_social_test_support::{
    accept_friendship, create_friend, create_host, prepare_multiplayer_fixture_for_issue,
    send_friendship,
};

#[test]
fn issue1162_game_updates_version_compatibility_fsv() {
    let paths = prepare_update_fixture("unit");
    let addresses = publish_update_version(&paths);
    let before = updates_summary(&paths);
    println!(
        "issue1162 updates before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(GAME_UPDATES_TOOL, register_args(&paths, &addresses)).unwrap();
    call_mcp_project_tool(
        GAME_UPDATES_TOOL,
        compatibility_args(&paths, "account.host", 3, 2, 5),
    )
    .unwrap();
    call_mcp_project_tool(GAME_UPDATES_TOOL, acknowledge_args(&paths, "account.host")).unwrap();
    call_mcp_project_tool(
        GAME_UPDATES_TOOL,
        compatibility_args(&paths, "account.friend", 1, 2, 7),
    )
    .unwrap();

    let updates = read_game_updates_state(&paths.updates_state_path).unwrap();
    let install = read_litdgame_install_manifest(&paths.install_manifest_path).unwrap();
    let source_install = &install.manifest.records[0];
    let release = &updates.state.releases[0];
    let host = updates
        .state
        .compatibilities
        .iter()
        .find(|record| record.account_id == "account.host")
        .unwrap();
    let friend = updates
        .state
        .compatibilities
        .iter()
        .find(|record| record.account_id == "account.friend")
        .unwrap();
    let acknowledged = updates
        .state
        .notifications
        .iter()
        .filter(|record| format!("{:?}", record.status) == "Acknowledged")
        .count();
    let pending = updates
        .state
        .notifications
        .iter()
        .filter(|record| format!("{:?}", record.status) == "Pending")
        .count();

    assert_eq!(updates.state.channel_count, 1);
    assert_eq!(updates.state.release_count, 1);
    assert_eq!(updates.state.compatibility_count, 2);
    assert_eq!(updates.state.notification_count, 2);
    assert_eq!(release.version, "1.2.4");
    assert_eq!(release.from_content_address, addresses.from_content_address);
    assert_eq!(release.to_content_address, addresses.to_content_address);
    assert_eq!(
        source_install.content_address,
        addresses.from_content_address
    );
    assert_eq!(
        source_install.installed_package_sha256,
        addresses.from_sha256
    );
    assert_eq!(release.package_sha256, addresses.to_sha256);
    assert!(host.compatible);
    assert!(!friend.compatible);
    assert!(
        friend
            .reasons
            .iter()
            .any(|reason| reason.contains("client_protocol"))
    );
    assert!(
        friend
            .reasons
            .iter()
            .any(|reason| reason.contains("save_schema_version"))
    );
    assert_eq!(acknowledged, 1);
    assert_eq!(pending, 1);

    let after = updates_summary(&paths);
    println!(
        "issue1162 updates after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1162_game_update_edges_fail_closed_without_state_drift() {
    let paths = prepare_update_fixture("edges");
    let addresses = publish_update_version(&paths);

    let unknown_before = updates_summary(&paths);
    let mut unknown = register_args(&paths, &addresses);
    unknown["to_content_address"] =
        json!("litdgame.sha256.0000000000000000000000000000000000000000000000000000000000000000");
    let unknown_error = call_mcp_project_tool(GAME_UPDATES_TOOL, unknown)
        .unwrap_err()
        .to_string();
    let unknown_after = updates_summary(&paths);
    assert_eq!(unknown_before, unknown_after);
    println!(
        "issue1162 edge unknown_target before={} after={} error={}",
        unknown_before, unknown_after, unknown_error
    );

    let same_before = updates_summary(&paths);
    let mut same = register_args(&paths, &addresses);
    same["to_content_address"] = json!(addresses.from_content_address.clone());
    let same_error = call_mcp_project_tool(GAME_UPDATES_TOOL, same)
        .unwrap_err()
        .to_string();
    let same_after = updates_summary(&paths);
    assert_eq!(same_before, same_after);
    println!(
        "issue1162 edge same_content before={} after={} error={}",
        same_before, same_after, same_error
    );

    call_mcp_project_tool(GAME_UPDATES_TOOL, register_args(&paths, &addresses)).unwrap();

    let ack_before = updates_summary(&paths);
    let ack_error = call_mcp_project_tool(
        GAME_UPDATES_TOOL,
        acknowledge_args(&paths, "account.friend"),
    )
    .unwrap_err()
    .to_string();
    let ack_after = updates_summary(&paths);
    assert_eq!(ack_before, ack_after);
    println!(
        "issue1162 edge ack_without_compat before={} after={} error={}",
        ack_before, ack_after, ack_error
    );

    let duplicate_before = updates_summary(&paths);
    let duplicate_error =
        call_mcp_project_tool(GAME_UPDATES_TOOL, register_args(&paths, &addresses))
            .unwrap_err()
            .to_string();
    let duplicate_after = updates_summary(&paths);
    assert_eq!(duplicate_before, duplicate_after);
    println!(
        "issue1162 edge duplicate_release before={} after={} error={}",
        duplicate_before, duplicate_after, duplicate_error
    );
}

struct UpdatePaths {
    root: PathBuf,
    state_path: PathBuf,
    ledger_path: PathBuf,
    updates_state_path: PathBuf,
    catalog_path: PathBuf,
    install_manifest_path: PathBuf,
    social_state_path: PathBuf,
    project_path: PathBuf,
    thumbnail_path: PathBuf,
}

struct UpdateAddresses {
    from_content_address: String,
    to_content_address: String,
    from_sha256: String,
    to_sha256: String,
}

fn prepare_update_fixture(label: &str) -> UpdatePaths {
    let social_paths = prepare_multiplayer_fixture_for_issue("issue1162_game_updates", label);
    create_host(&social_paths);
    create_friend(&social_paths);
    send_friendship(&social_paths);
    accept_friendship(&social_paths);
    UpdatePaths {
        root: social_paths.root.clone(),
        state_path: social_paths.state_path,
        ledger_path: social_paths.root.join("updates-ledger.json"),
        updates_state_path: social_paths.root.join("game-updates-state.json"),
        catalog_path: social_paths.catalog_path,
        install_manifest_path: social_paths.install_manifest_path,
        social_state_path: social_paths.social_state_path,
        project_path: social_paths.project_path,
        thumbnail_path: social_paths.thumbnail_path,
    }
}

fn publish_update_version(paths: &UpdatePaths) -> UpdateAddresses {
    let before = read_publish_catalog(&paths.catalog_path)
        .unwrap()
        .catalog
        .entries[0]
        .clone();
    call_mcp_project_tool(
        PUBLISH_LITDGAME_TOOL,
        json!({
            "request_id": "issue1162-publish-update",
            "requested_by": "litd-editor-issue1162-test",
            "state_path": paths.state_path,
            "state_id": "issue1160-project-state",
            "ledger_path": paths.root.join("publish-update-ledger.json"),
            "ledger_id": "issue1162-publish-update-ledger",
            "project_path": paths.project_path,
            "package_path": paths.root.join("ashen-reach-1.2.4.litdgame.json"),
            "catalog_path": paths.catalog_path,
            "share_id": "issue1162-share-v124",
            "created_by": "agent.project.publisher",
            "host_port": 19624,
            "max_package_bytes": 262144,
            "title": "Ashen Reach Social",
            "version": "1.2.4",
            "summary": "Known deterministic game update fixture",
            "tags": ["dark_fantasy", "singleplayer", "local_first"],
            "thumbnail_path": paths.thumbnail_path,
            "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
        }),
    )
    .unwrap();
    let catalog = read_publish_catalog(&paths.catalog_path).unwrap().catalog;
    let update = catalog
        .entries
        .iter()
        .find(|entry| entry.version == "1.2.4")
        .unwrap();
    UpdateAddresses {
        from_content_address: before.content_address,
        to_content_address: update.content_address.clone(),
        from_sha256: before.package_sha256,
        to_sha256: update.package_sha256.clone(),
    }
}

fn register_args(paths: &UpdatePaths, addresses: &UpdateAddresses) -> Value {
    updates_args(
        paths,
        json!({
            "action": "register_release",
            "release_id": "release.ashen_reach.1.2.4",
            "channel_id": "channel.stable",
            "from_content_address": addresses.from_content_address,
            "to_content_address": addresses.to_content_address,
            "min_client_protocol": 3,
            "min_server_protocol": 2,
            "min_save_schema": 4,
            "max_save_schema": 6,
            "release_notes": "Known deterministic update from 1.2.3 to 1.2.4"
        }),
    )
}

fn compatibility_args(
    paths: &UpdatePaths,
    account_id: &str,
    client: u32,
    server: u32,
    save: u32,
) -> Value {
    updates_args(
        paths,
        json!({
            "action": "evaluate_compatibility",
            "release_id": "release.ashen_reach.1.2.4",
            "account_id": account_id,
            "client_protocol": client,
            "server_protocol": server,
            "save_schema_version": save
        }),
    )
}

fn acknowledge_args(paths: &UpdatePaths, account_id: &str) -> Value {
    updates_args(
        paths,
        json!({
            "action": "acknowledge_notification",
            "account_id": account_id,
            "notification_id": format!("update.release.ashen_reach.1.2.4.{account_id}")
        }),
    )
}

fn updates_args(paths: &UpdatePaths, fields: Value) -> Value {
    let mut value = json!({
        "request_id": "issue1162-updates",
        "requested_by": "litd-editor-issue1162-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.ledger_path,
        "ledger_id": "issue1162-updates-ledger",
        "updates_state_path": paths.updates_state_path,
        "updates_state_id": "issue1162-updates-state",
        "catalog_path": paths.catalog_path,
        "install_manifest_path": paths.install_manifest_path,
        "social_state_path": paths.social_state_path
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

fn updates_summary(paths: &UpdatePaths) -> Value {
    let state = read_game_updates_state(&paths.updates_state_path).ok();
    json!({
        "updates_state_exists": paths.updates_state_path.exists(),
        "updates_state_bytes": paths.updates_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "updates_state_sha256": fs::read(&paths.updates_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "channel_count": state.as_ref().map(|s| s.state.channel_count).unwrap_or(0),
        "release_count": state.as_ref().map(|s| s.state.release_count).unwrap_or(0),
        "compatibility_count": state.as_ref().map(|s| s.state.compatibility_count).unwrap_or(0),
        "notification_count": state.as_ref().map(|s| s.state.notification_count).unwrap_or(0),
        "acknowledged_count": state.as_ref().map(|s| s.state.notifications.iter().filter(|n| format!("{:?}", n.status) == "Acknowledged").count()).unwrap_or(0),
        "pending_count": state.as_ref().map(|s| s.state.notifications.iter().filter(|n| format!("{:?}", n.status) == "Pending").count()).unwrap_or(0),
        "compatible_count": state.as_ref().map(|s| s.state.compatibilities.iter().filter(|c| c.compatible).count()).unwrap_or(0)
    })
}
