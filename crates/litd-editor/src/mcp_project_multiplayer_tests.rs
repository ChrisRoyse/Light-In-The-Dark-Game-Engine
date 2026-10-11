use litd_hash::sha256_hex;
use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        DELIVERY_LITDGAME_TOOL, HUB_LAUNCHER_TOOL, MULTIPLAYER_SESSION_TOOL, NEW_OPEN_SAVE_TOOL,
        PUBLISH_LITDGAME_TOOL, ProjectLifecycleAction, call_mcp_project_tool,
        read_hub_launcher_state, read_litdgame_install_manifest, read_mcp_project_state,
        read_multiplayer_session_state, read_publish_catalog,
    },
};
use litd_engine::math::{Transform, Vec3};

const PNG_1X1_RGBA: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 167, 197, 129, 218, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
fn issue1159_multiplayer_host_join_party_matchmake_fsv() {
    let paths = prepare_hub_fixture("unit");
    let content_address = published_content_address(&paths.catalog_path);

    let before = multiplayer_summary(&paths);
    println!(
        "issue1159 multiplayer before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(
            &paths,
            &content_address,
            "lobby.ashen_reach",
            4,
            "127.0.0.1",
        ),
    )
    .unwrap();
    call_mcp_project_tool(MULTIPLAYER_SESSION_TOOL, party_args(&paths)).unwrap();
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        join_args(&paths, "lobby.ashen_reach", "player.guest"),
    )
    .unwrap();
    call_mcp_project_tool(MULTIPLAYER_SESSION_TOOL, matchmake_party_args(&paths)).unwrap();

    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).unwrap();
    let hub = read_hub_launcher_state(&paths.hub_state_path).unwrap();
    let install = read_litdgame_install_manifest(&paths.install_manifest_path).unwrap();
    let lobby = &multiplayer.state.lobbies[0];
    let ticket = &multiplayer.state.tickets[0];
    let installed_path = PathBuf::from(&lobby.installed_package_path);
    let installed_bytes = fs::read(&installed_path).unwrap();

    assert_eq!(multiplayer.state.lobby_count, 1);
    assert_eq!(multiplayer.state.party_count, 1);
    assert_eq!(multiplayer.state.join_count, 1);
    assert_eq!(multiplayer.state.ticket_count, 1);
    assert_eq!(lobby.member_ids.len(), 4);
    assert_eq!(
        lobby.member_ids,
        vec![
            "player.host",
            "player.guest",
            "player.friend",
            "player.ally"
        ]
    );
    assert_eq!(format!("{:?}", lobby.status), "Full");
    assert_eq!(ticket.matched_lobby_id, lobby.lobby_id);
    assert_eq!(ticket.member_ids, vec!["player.friend", "player.ally"]);
    assert_eq!(sha256_hex(&installed_bytes), lobby.package_sha256);
    assert_eq!(
        install.manifest.records[0].installed_package_path,
        installed_path
    );
    assert_eq!(hub.state.cards[0].content_address, lobby.content_address);
    assert!(lobby.runtime_lock_verified);
    assert!(lobby.playable_on_litd_runtime);

    let after = multiplayer_summary(&paths);
    println!(
        "issue1159 multiplayer after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1159_multiplayer_edges_fail_closed_without_state_drift() {
    let paths = prepare_hub_fixture("edges");
    let content_address = published_content_address(&paths.catalog_path);

    let endpoint_before = multiplayer_summary(&paths);
    let endpoint_error = call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(&paths, &content_address, "lobby.bad_endpoint", 2, "0.0.0.0"),
    )
    .unwrap_err()
    .to_string();
    let endpoint_after = multiplayer_summary(&paths);
    assert_eq!(endpoint_before, endpoint_after);
    println!(
        "issue1159 edge invalid_endpoint before={} after={} error={}",
        endpoint_before, endpoint_after, endpoint_error
    );

    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(&paths, &content_address, "lobby.capacity", 3, "127.0.0.1"),
    )
    .unwrap();

    let duplicate_before = multiplayer_summary(&paths);
    let duplicate_error = call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        join_args(&paths, "lobby.capacity", "player.host"),
    )
    .unwrap_err()
    .to_string();
    let duplicate_after = multiplayer_summary(&paths);
    assert_eq!(duplicate_before, duplicate_after);
    println!(
        "issue1159 edge duplicate_join before={} after={} error={}",
        duplicate_before, duplicate_after, duplicate_error
    );

    call_mcp_project_tool(MULTIPLAYER_SESSION_TOOL, party_args(&paths)).unwrap();
    let tag_before = multiplayer_summary(&paths);
    let tag_error = call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        matchmake_with_tags(&paths, &["nonexistent_tag"]),
    )
    .unwrap_err()
    .to_string();
    let tag_after = multiplayer_summary(&paths);
    assert_eq!(tag_before, tag_after);
    println!(
        "issue1159 edge no_matching_tag before={} after={} error={}",
        tag_before, tag_after, tag_error
    );

    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        join_args(&paths, "lobby.capacity", "player.guest"),
    )
    .unwrap();
    let capacity_before = multiplayer_summary(&paths);
    let capacity_error =
        call_mcp_project_tool(MULTIPLAYER_SESSION_TOOL, matchmake_party_args(&paths))
            .unwrap_err()
            .to_string();
    let capacity_after = multiplayer_summary(&paths);
    assert_eq!(capacity_before, capacity_after);
    println!(
        "issue1159 edge over_capacity before={} after={} error={}",
        capacity_before, capacity_after, capacity_error
    );
}

pub(super) fn prepare_hub_fixture(label: &str) -> FixturePaths {
    // Confine delivery paths to the workspace root (#1487); target/ is within it.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let root = test_root(label);
    clean_dir(&root);
    let paths = FixturePaths {
        root: root.clone(),
        state_path: root.join("project-state.json"),
        project_path: root.join("ashen-reach.litdworld.json"),
        package_path: root.join("ashen-reach.litdgame.json"),
        catalog_path: root.join("publish-catalog.json"),
        thumbnail_path: root.join("thumbnail.png"),
        delivery_manifest_path: root.join("delivery-store.json"),
        blob_root_path: root.join("content-store"),
        install_manifest_path: root.join("install-manifest.json"),
        install_root_path: root.join("installed"),
        hub_state_path: root.join("hub-launcher-state.json"),
        multiplayer_state_path: root.join("multiplayer-state.json"),
    };
    fs::write(&paths.thumbnail_path, PNG_1X1_RGBA).unwrap();
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(
            &paths.state_path,
            &paths.root,
            &paths.project_path,
            fixture_document(),
        ),
    )
    .unwrap();
    call_mcp_project_tool(PUBLISH_LITDGAME_TOOL, publish_args(&paths)).unwrap();
    let content_address = published_content_address(&paths.catalog_path);
    call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address),
    )
    .unwrap();
    call_mcp_project_tool(HUB_LAUNCHER_TOOL, hub_args(&paths)).unwrap();
    paths
}

pub(super) fn host_args(
    paths: &FixturePaths,
    content_address: &str,
    lobby_id: &str,
    max_players: usize,
    host: &str,
) -> Value {
    json!({
        "request_id": format!("issue1159-host-{lobby_id}"),
        "requested_by": "litd-editor-issue1159-test",
        "state_path": paths.state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1159-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1159-multiplayer-state",
        "action": "host_lobby",
        "lobby_id": lobby_id,
        "content_address": content_address,
        "host_player_id": "player.host",
        "max_players": max_players,
        "endpoint": {
            "transport": "loopback",
            "host": host,
            "port": 19690,
            "path": format!("/lobby/{lobby_id}")
        }
    })
}

pub(super) fn join_args(paths: &FixturePaths, lobby_id: &str, player_id: &str) -> Value {
    json!({
        "request_id": format!("issue1159-join-{player_id}"),
        "requested_by": "litd-editor-issue1159-test",
        "state_path": paths.state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1159-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1159-multiplayer-state",
        "action": "join_lobby",
        "lobby_id": lobby_id,
        "player_id": player_id,
        "joining_package_path": read_litdgame_install_manifest(&paths.install_manifest_path).unwrap().manifest.records[0].installed_package_path
    })
}

fn party_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1159-party",
        "requested_by": "litd-editor-issue1159-test",
        "state_path": paths.state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1159-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1159-multiplayer-state",
        "action": "create_party",
        "party_id": "party.dawnward",
        "leader_player_id": "player.friend",
        "member_ids": ["player.friend", "player.ally"],
        "max_players": 4
    })
}

fn matchmake_party_args(paths: &FixturePaths) -> Value {
    matchmake_with_tags(paths, &["dark_fantasy", "local_first"])
}

fn matchmake_with_tags(paths: &FixturePaths, required_tags: &[&str]) -> Value {
    json!({
        "request_id": format!("issue1159-matchmake-{}", required_tags.join("-")),
        "requested_by": "litd-editor-issue1159-test",
        "state_path": paths.state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1159-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1159-multiplayer-state",
        "action": "matchmake",
        "ticket_id": format!("ticket.dawnward.{}", required_tags.join(".")),
        "queue_id": "queue.local_first.casual",
        "party_id": "party.dawnward",
        "required_tags": required_tags
    })
}

fn hub_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1159-hub",
        "requested_by": "litd-editor-issue1159-test",
        "state_path": paths.state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": paths.root.join("hub-ledger.json"),
        "ledger_id": "issue1159-hub-ledger",
        "catalog_path": paths.catalog_path,
        "install_manifest_path": paths.install_manifest_path,
        "hub_state_path": paths.hub_state_path,
        "hub_id": "issue1159-local-hub",
        "query": "Multiplayer",
        "required_tags": ["singleplayer"],
        "installed_only": true,
        "sort": "title_asc"
    })
}

fn delivery_args(paths: &FixturePaths, content_address: &str) -> Value {
    json!({
        "request_id": "issue1159-deliver",
        "requested_by": "litd-editor-issue1159-test",
        "state_path": paths.state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": paths.root.join("delivery-ledger.json"),
        "ledger_id": "issue1159-delivery-ledger",
        "catalog_path": paths.catalog_path,
        "delivery_manifest_path": paths.delivery_manifest_path,
        "delivery_store_id": "issue1159-local-store",
        "blob_root_path": paths.blob_root_path,
        "install_manifest_path": paths.install_manifest_path,
        "install_id": "issue1159-install",
        "install_root_path": paths.install_root_path,
        "content_address": content_address,
        "delivered_by": "agent.delivery.store",
        "installed_by": "agent.delivery.install"
    })
}

fn save_args(
    state_path: &Path,
    root: &Path,
    project_path: &Path,
    document: EditorDocument,
) -> Value {
    json!({
        "request_id": "issue1159-save",
        "requested_by": "litd-editor-issue1159-test",
        "state_path": state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": root.join("project-ledger.json"),
        "ledger_id": "issue1159-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue1159",
        "title": "Ashen Reach Issue 1159",
        "version": "1.2.3",
        "tags": ["dark_fantasy", "multiplayer", "local_first"],
        "project_path": project_path,
        "snapshot_label": "multiplayer-ready",
        "document": document
    })
}

fn publish_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1159-publish",
        "requested_by": "litd-editor-issue1159-test",
        "state_path": paths.state_path,
        "state_id": "issue1159-project-state",
        "ledger_path": paths.root.join("publish-ledger.json"),
        "ledger_id": "issue1159-publish-ledger",
        "project_path": paths.project_path,
        "package_path": paths.package_path,
        "catalog_path": paths.catalog_path,
        "share_id": "issue1159-share",
        "created_by": "agent.project.publisher",
        "host_port": 19590,
        "max_package_bytes": 262144,
        "title": "Ashen Reach Multiplayer",
        "version": "1.2.3",
        "summary": "Known deterministic multiplayer fixture",
        "tags": ["dark_fantasy", "singleplayer", "local_first"],
        "thumbnail_path": paths.thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1159-project", "issue1159-rev-1").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_multiplayer_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(10.0, 0.0, -12.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_multiplayer_hero", &record).unwrap();
    document
}

pub(super) fn multiplayer_summary(paths: &FixturePaths) -> Value {
    let state = read_mcp_project_state(&paths.state_path).ok();
    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).ok();
    let installed_path = multiplayer
        .as_ref()
        .and_then(|readback| readback.state.lobbies.first())
        .map(|lobby| PathBuf::from(&lobby.installed_package_path))
        .unwrap_or_else(|| paths.install_root_path.join("missing"));
    json!({
        "state_exists": paths.state_path.exists(),
        "state_update_count": state.as_ref().map(|readback| readback.state.update_count).unwrap_or(0),
        "multiplayer_state_exists": paths.multiplayer_state_path.exists(),
        "multiplayer_state_bytes": paths.multiplayer_state_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "multiplayer_state_sha256": fs::read(&paths.multiplayer_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "lobby_count": multiplayer.as_ref().map(|readback| readback.state.lobby_count).unwrap_or(0),
        "party_count": multiplayer.as_ref().map(|readback| readback.state.party_count).unwrap_or(0),
        "join_count": multiplayer.as_ref().map(|readback| readback.state.join_count).unwrap_or(0),
        "ticket_count": multiplayer.as_ref().map(|readback| readback.state.ticket_count).unwrap_or(0),
        "members": multiplayer.as_ref().and_then(|readback| readback.state.lobbies.first()).map(|lobby| lobby.member_ids.clone()).unwrap_or_default(),
        "installed_exists": installed_path.exists(),
        "installed_bytes": installed_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "installed_sha256": fs::read(&installed_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default()
    })
}

pub(super) fn published_content_address(catalog_path: &Path) -> String {
    read_publish_catalog(catalog_path).unwrap().catalog.entries[0]
        .content_address
        .clone()
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1159_multiplayer_lobbies", label)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}

pub(super) struct FixturePaths {
    pub(super) root: PathBuf,
    state_path: PathBuf,
    project_path: PathBuf,
    package_path: PathBuf,
    pub(super) catalog_path: PathBuf,
    thumbnail_path: PathBuf,
    delivery_manifest_path: PathBuf,
    blob_root_path: PathBuf,
    pub(super) install_manifest_path: PathBuf,
    install_root_path: PathBuf,
    hub_state_path: PathBuf,
    pub(super) multiplayer_state_path: PathBuf,
}
