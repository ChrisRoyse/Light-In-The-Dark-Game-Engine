use litd_hash::sha256_hex;
use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        DELIVERY_LITDGAME_TOOL, HUB_LAUNCHER_TOOL, MULTIPLAYER_SESSION_TOOL, NEW_OPEN_SAVE_TOOL,
        PUBLISH_LITDGAME_TOOL, ProjectLifecycleAction, SOCIAL_GRAPH_TOOL, call_mcp_project_tool,
        read_publish_catalog, read_social_graph_state,
    },
};
use litd_engine::{
    fsv_artifacts::{FsvRun, begin_issue_run},
    math::{Transform, Vec3},
};

const PNG_1X1_RGBA: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 167, 197, 129, 218, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

pub(crate) struct FixturePaths {
    pub(crate) fsv_run: FsvRun,
    pub(crate) root: PathBuf,
    pub(crate) state_path: PathBuf,
    pub(crate) project_path: PathBuf,
    pub(crate) package_path: PathBuf,
    pub(crate) catalog_path: PathBuf,
    pub(crate) thumbnail_path: PathBuf,
    pub(crate) delivery_manifest_path: PathBuf,
    pub(crate) blob_root_path: PathBuf,
    pub(crate) install_manifest_path: PathBuf,
    pub(crate) install_root_path: PathBuf,
    pub(crate) hub_state_path: PathBuf,
    pub(crate) multiplayer_state_path: PathBuf,
    pub(crate) social_state_path: PathBuf,
}

pub(crate) fn prepare_multiplayer_fixture(label: &str) -> FixturePaths {
    prepare_multiplayer_fixture_for_issue("issue1160_social_graph", label)
}

pub(crate) fn prepare_multiplayer_fixture_for_issue(issue_root: &str, label: &str) -> FixturePaths {
    let fsv_run = begin_issue_run(issue_root).unwrap();
    let root = fsv_run.artifact_dir(label).unwrap();
    prepare_multiplayer_fixture_at(
        root,
        fsv_run,
        matches!(
            issue_root,
            "issue1038_authoritative_server" | "issue1042_server_browser"
        ),
    )
}

fn prepare_multiplayer_fixture_at(
    root: PathBuf,
    fsv_run: FsvRun,
    two_authority_clients: bool,
) -> FixturePaths {
    // Confine delivery paths to the workspace root (#1487); fixtures live under
    // target/fsv which is within it.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    clean_dir(&root);
    let paths = FixturePaths {
        fsv_run,
        root: root.clone(),
        state_path: root.join("project-state.json"),
        project_path: root.join("ashen-reach.litdworld"),
        package_path: root.join("ashen-reach.litdgame.json"),
        catalog_path: root.join("publish-catalog.json"),
        thumbnail_path: root.join("thumbnail.png"),
        delivery_manifest_path: root.join("delivery-store.json"),
        blob_root_path: root.join("content-store"),
        install_manifest_path: root.join("install-manifest.json"),
        install_root_path: root.join("installed"),
        hub_state_path: root.join("hub-launcher-state.json"),
        multiplayer_state_path: root.join("multiplayer-state.json"),
        social_state_path: root.join("social-graph-state.json"),
    };
    fs::write(&paths.thumbnail_path, PNG_1X1_RGBA).unwrap();
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1160-save",
            "requested_by": "litd-editor-issue1160-test",
            "state_path": paths.state_path,
            "state_id": "issue1160-project-state",
            "ledger_path": paths.root.join("project-ledger.json"),
            "ledger_id": "issue1160-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.ashen_reach_issue1160",
            "title": "Ashen Reach Issue 1160",
            "version": "1.2.3",
            "tags": ["dark_fantasy", "social", "local_first"],
            "project_path": paths.project_path,
            "snapshot_label": "social-ready",
            "document": fixture_document(two_authority_clients)
        }),
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
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(&paths, &content_address),
    )
    .unwrap();
    paths
}

pub(crate) fn create_host(paths: &FixturePaths) {
    create_account(paths, "account.host", "player.host", "profile.host", "host");
}

pub(crate) fn create_friend(paths: &FixturePaths) {
    create_account(
        paths,
        "account.friend",
        "player.friend",
        "profile.friend",
        "friend",
    );
}

pub(crate) fn send_friendship(paths: &FixturePaths) {
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        friend_args(
            paths,
            "send_friend_request",
            "account.host",
            "account.friend",
        ),
    )
    .unwrap();
}

pub(crate) fn accept_friendship(paths: &FixturePaths) {
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        friend_args(
            paths,
            "accept_friend_request",
            "account.friend",
            "account.host",
        ),
    )
    .unwrap();
}

pub(crate) fn set_host_presence(paths: &FixturePaths) {
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        presence_args(paths, "account.host", "in_game", Some("lobby.social")),
    )
    .unwrap();
}

pub(crate) fn set_friend_presence(paths: &FixturePaths) {
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        presence_args(paths, "account.friend", "online", None),
    )
    .unwrap();
}

pub(crate) fn send_invite(paths: &FixturePaths) {
    call_mcp_project_tool(SOCIAL_GRAPH_TOOL, invite_args(paths, "send_lobby_invite")).unwrap();
}

pub(crate) fn accept_invite(paths: &FixturePaths) {
    call_mcp_project_tool(SOCIAL_GRAPH_TOOL, invite_args(paths, "accept_lobby_invite")).unwrap();
}

pub(crate) fn create_account_args(
    paths: &FixturePaths,
    account: &str,
    player: &str,
    profile: &str,
    handle: &str,
    tags: &[&str],
) -> Value {
    social_args(
        paths,
        json!({
            "action": "create_account_profile",
            "account_id": account,
            "player_id": player,
            "profile_id": profile,
            "handle": handle,
            "display_name": format!("{} Display", handle),
            "bio": "Known deterministic social graph fixture",
            "tags": tags
        }),
    )
}

pub(crate) fn friend_args(
    paths: &FixturePaths,
    action: &str,
    account: &str,
    target: &str,
) -> Value {
    social_args(
        paths,
        json!({
            "action": action,
            "account_id": account,
            "target_account_id": target
        }),
    )
}

pub(crate) fn presence_args(
    paths: &FixturePaths,
    account: &str,
    status: &str,
    lobby: Option<&str>,
) -> Value {
    social_args(
        paths,
        json!({
            "action": "set_presence",
            "account_id": account,
            "presence_status": status,
            "lobby_id": lobby,
            "rich_presence": format!("{} at {:?}", account, lobby)
        }),
    )
}

pub(crate) fn invite_args(paths: &FixturePaths, action: &str) -> Value {
    let account_id = if action == "accept_lobby_invite" {
        "account.friend"
    } else {
        "account.host"
    };
    social_args(
        paths,
        json!({
            "action": action,
            "account_id": account_id,
            "target_account_id": "account.friend",
            "invite_id": "invite.social.001",
            "lobby_id": "lobby.social"
        }),
    )
}

pub(crate) fn social_summary(paths: &FixturePaths) -> Value {
    let social = read_social_graph_state(&paths.social_state_path).ok();
    json!({
        "social_state_exists": paths.social_state_path.exists(),
        "social_state_bytes": paths.social_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "social_state_sha256": fs::read(&paths.social_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "account_count": social.as_ref().map(|s| s.state.account_count).unwrap_or(0),
        "profile_count": social.as_ref().map(|s| s.state.profile_count).unwrap_or(0),
        "friendship_count": social.as_ref().map(|s| s.state.friendship_count).unwrap_or(0),
        "presence_count": social.as_ref().map(|s| s.state.presence_count).unwrap_or(0),
        "invite_count": social.as_ref().map(|s| s.state.invite_count).unwrap_or(0),
        "invite_status": social.as_ref().and_then(|s| s.state.invites.first()).map(|i| format!("{:?}", i.status)).unwrap_or_default()
    })
}

fn create_account(paths: &FixturePaths, account: &str, player: &str, profile: &str, handle: &str) {
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        create_account_args(
            paths,
            account,
            player,
            profile,
            handle,
            &["local_first", "social"],
        ),
    )
    .unwrap();
}

fn social_args(paths: &FixturePaths, fields: Value) -> Value {
    let mut value = json!({
        "request_id": "issue1160-social",
        "requested_by": "litd-editor-issue1160-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("social-ledger.json"),
        "ledger_id": "issue1160-social-ledger",
        "social_state_path": paths.social_state_path,
        "social_state_id": "issue1160-social-state",
        "multiplayer_state_path": paths.multiplayer_state_path
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

fn publish_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1160-publish",
        "requested_by": "litd-editor-issue1160-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("publish-ledger.json"),
        "ledger_id": "issue1160-publish-ledger",
        "project_path": paths.project_path,
        "package_path": paths.package_path,
        "catalog_path": paths.catalog_path,
        "share_id": "issue1160-share",
        "created_by": "agent.project.publisher",
        "host_port": 19600,
        "max_package_bytes": 262144,
        "title": "Ashen Reach Social",
        "version": "1.2.3",
        "summary": "Known deterministic social graph fixture",
        "tags": ["dark_fantasy", "singleplayer", "local_first"],
        "thumbnail_path": paths.thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn delivery_args(paths: &FixturePaths, content_address: &str) -> Value {
    json!({
        "request_id": "issue1160-deliver",
        "requested_by": "litd-editor-issue1160-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("delivery-ledger.json"),
        "ledger_id": "issue1160-delivery-ledger",
        "catalog_path": paths.catalog_path,
        "delivery_manifest_path": paths.delivery_manifest_path,
        "delivery_store_id": "issue1160-local-store",
        "blob_root_path": paths.blob_root_path,
        "install_manifest_path": paths.install_manifest_path,
        "install_id": "issue1160-install",
        "install_root_path": paths.install_root_path,
        "content_address": content_address,
        "delivered_by": "agent.delivery.store",
        "installed_by": "agent.delivery.install"
    })
}

fn hub_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1160-hub",
        "requested_by": "litd-editor-issue1160-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("hub-ledger.json"),
        "ledger_id": "issue1160-hub-ledger",
        "catalog_path": paths.catalog_path,
        "install_manifest_path": paths.install_manifest_path,
        "hub_state_path": paths.hub_state_path,
        "hub_id": "issue1160-local-hub",
        "query": "Social",
        "required_tags": ["singleplayer"],
        "installed_only": true,
        "sort": "title_asc"
    })
}

fn host_args(paths: &FixturePaths, content_address: &str) -> Value {
    json!({
        "request_id": "issue1160-host",
        "requested_by": "litd-editor-issue1160-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1160-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1160-multiplayer-state",
        "action": "host_lobby",
        "lobby_id": "lobby.social",
        "content_address": content_address,
        "host_player_id": "player.host",
        "max_players": 4,
        "endpoint": {
            "transport": "loopback",
            "host": "127.0.0.1",
            "port": 19700,
            "path": "/lobby/lobby.social"
        }
    })
}

fn fixture_document(two_authority_clients: bool) -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1160-project", "issue1160-rev-1").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "unit_social_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(12.0, 0.0, -14.0)),
        ))
        .unwrap();
    let record = default_unit_model_record();
    assign_vault_model_to_unit(&mut document, "unit_social_hero", &record).unwrap();
    if two_authority_clients {
        document
            .upsert_object(EditorObject::new(
                "unit_authority_client_two",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(16.0, 0.0, -14.0)),
            ))
            .unwrap();
        assign_vault_model_to_unit(&mut document, "unit_authority_client_two", &record).unwrap();
    }
    document
}

fn default_unit_model_record() -> AssetVaultRecord {
    AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .unwrap()
}

fn published_content_address(catalog_path: &Path) -> String {
    read_publish_catalog(catalog_path).unwrap().catalog.entries[0]
        .content_address
        .clone()
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
