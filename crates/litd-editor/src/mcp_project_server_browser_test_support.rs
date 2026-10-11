use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::{
    mcp_project_social_test_support::{
        FixturePaths, accept_friendship, create_friend, create_host,
        prepare_multiplayer_fixture_for_issue, send_friendship,
    },
    mcp_project_tools::{
        MODERATION_SAFETY_TOOL, MULTIPLAYER_SESSION_TOOL, SOCIAL_GRAPH_TOOL, call_mcp_project_tool,
        read_publish_catalog, read_server_browser_state,
    },
};

pub(crate) struct BrowserPaths {
    pub(crate) fsv_run: litd_engine::fsv_artifacts::FsvRun,
    pub(crate) root: PathBuf,
    pub(crate) state_path: PathBuf,
    pub(crate) ledger_path: PathBuf,
    pub(crate) browser_state_path: PathBuf,
    pub(crate) moderation_state_path: PathBuf,
    pub(crate) catalog_path: PathBuf,
    pub(crate) hub_state_path: PathBuf,
    pub(crate) install_manifest_path: PathBuf,
    pub(crate) multiplayer_state_path: PathBuf,
    pub(crate) social_state_path: PathBuf,
}

pub(crate) fn prepare_browser_fixture(
    label: &str,
    start_servers: bool,
    invite: bool,
) -> BrowserPaths {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let base = prepare_multiplayer_fixture_for_issue("issue1042_server_browser", label);
    create_host(&base);
    create_friend(&base);
    send_friendship(&base);
    accept_friendship(&base);
    host_private_lobby(&base);
    let paths = BrowserPaths::from(base);
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        owner_role_args(&paths, "lobby.social"),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        owner_role_args(&paths, "lobby.private"),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        policy_args(&paths, "lobby.social", 650, "standard"),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        policy_args(&paths, "lobby.private", 900, "hardcore"),
    )
    .unwrap();
    if start_servers {
        start_server(&paths, "server.social", "lobby.social", 19700);
        start_server(&paths, "server.private", "lobby.private", 19710);
    }
    if invite {
        call_mcp_project_tool(
            SOCIAL_GRAPH_TOOL,
            private_invite_args(&paths, "send_lobby_invite"),
        )
        .unwrap();
        call_mcp_project_tool(
            SOCIAL_GRAPH_TOOL,
            private_invite_args(&paths, "accept_lobby_invite"),
        )
        .unwrap();
        call_mcp_project_tool(SOCIAL_GRAPH_TOOL, join_connection_args(&paths)).unwrap();
    }
    paths
}

impl From<FixturePaths> for BrowserPaths {
    fn from(base: FixturePaths) -> Self {
        Self {
            fsv_run: base.fsv_run,
            root: base.root.clone(),
            state_path: base.state_path,
            ledger_path: base.root.join("server-browser-ledger.json"),
            browser_state_path: base.root.join("server-browser-state.json"),
            moderation_state_path: base.root.join("moderation-safety-state.json"),
            catalog_path: base.catalog_path,
            hub_state_path: base.hub_state_path,
            install_manifest_path: base.install_manifest_path,
            multiplayer_state_path: base.multiplayer_state_path,
            social_state_path: base.social_state_path,
        }
    }
}

pub(crate) fn access_rule_args(paths: &BrowserPaths) -> Value {
    moderation_args(
        paths,
        json!({
            "request_id": "issue1042-allow-host",
            "action": "set_access_rule",
            "rule_id": "rule.lobby.social.allow.account.host",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": "account.host",
            "access_rule_kind": "allow",
            "resolution_note": "Allowlist gate proves public browser listing fail-closed"
        }),
    )
}

pub(crate) fn publish_args(
    paths: &BrowserPaths,
    listing_id: &str,
    lobby_id: &str,
    visibility: &str,
    strictness: u16,
) -> Value {
    browser_args(
        paths,
        json!({
            "action": "publish_listing",
            "request_id": format!("issue1042-publish-{listing_id}"),
            "listing_id": listing_id,
            "lobby_id": lobby_id,
            "actor_account_id": "account.host",
            "visibility": visibility,
            "server_name": format!("Ashen Reach {lobby_id}"),
            "ruleset_summary": format!("RP strictness {strictness} public/private browser proof")
        }),
    )
}

pub(crate) fn query_args(
    paths: &BrowserPaths,
    query_id: &str,
    include_private: bool,
    viewer: Option<&str>,
    sort: &str,
) -> Value {
    let mut value = browser_args(
        paths,
        json!({
            "action": "query_listings",
            "request_id": format!("issue1042-{query_id}"),
            "query_id": query_id,
            "include_private": include_private,
            "required_tags": ["dark_fantasy"],
            "min_open_slots": 1,
            "sort": sort
        }),
    );
    if let Some(viewer) = viewer {
        value["viewer_account_id"] = json!(viewer);
    }
    value
}

pub(crate) fn browser_summary(paths: &BrowserPaths) -> Value {
    let state = read_server_browser_state(&paths.browser_state_path).ok();
    json!({
        "browser_state_exists": paths.browser_state_path.exists(),
        "browser_state_bytes": paths.browser_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "browser_state_sha256": fs::read(&paths.browser_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "listing_count": state.as_ref().map(|s| s.state.listing_count).unwrap_or(0),
        "public_listing_count": state.as_ref().map(|s| s.state.public_listing_count).unwrap_or(0),
        "private_listing_count": state.as_ref().map(|s| s.state.private_listing_count).unwrap_or(0),
        "query_count": state.as_ref().map(|s| s.state.query_count).unwrap_or(0),
        "listing_ids": state.as_ref().map(|s| s.state.listings.iter().map(|listing| listing.listing_id.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "query_ids": state.as_ref().map(|s| s.state.queries.iter().map(|query| query.query_id.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "result_counts": state.as_ref().map(|s| s.state.queries.iter().map(|query| query.result_count).collect::<Vec<_>>()).unwrap_or_default(),
        "hidden_private_counts": state.as_ref().map(|s| s.state.queries.iter().map(|query| query.hidden_private_count).collect::<Vec<_>>()).unwrap_or_default()
    })
}

pub(crate) fn write_report(paths: &BrowserPaths, label: &str, details: Value) {
    let report = json!({
        "issue": 1042,
        "label": label,
        "source_of_truth": "Calyx Aster server-browser, multiplayer, moderation, and social state rows",
        "server_browser_state_path": paths.browser_state_path,
        "server_browser_state_sha256": fs::read(&paths.browser_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "details": details
    });
    paths
        .fsv_run
        .write_json_file(&paths.root.join("manual-fsv-report.json"), &report)
        .unwrap();
}

fn host_private_lobby(paths: &FixturePaths) {
    let content_address = content_address(&paths.catalog_path);
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        json!({
            "request_id": "issue1042-host-private",
            "requested_by": "litd-editor-issue1042-test",
            "state_path": paths.state_path,
            "state_id": "issue1160-project-state",
            "ledger_path": paths.root.join("multiplayer-ledger.json"),
            "ledger_id": "issue1160-multiplayer-ledger",
            "hub_state_path": paths.hub_state_path,
            "install_manifest_path": paths.install_manifest_path,
            "multiplayer_state_path": paths.multiplayer_state_path,
            "multiplayer_state_id": "issue1160-multiplayer-state",
            "action": "host_lobby",
            "lobby_id": "lobby.private",
            "content_address": content_address,
            "host_player_id": "player.host",
            "max_players": 4,
            "endpoint": endpoint("lobby.private", 19710)
        }),
    )
    .unwrap();
}

fn start_server(paths: &BrowserPaths, server_id: &str, lobby_id: &str, port: u16) {
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        json!({
            "request_id": format!("issue1042-start-{server_id}"),
            "requested_by": "litd-editor-issue1042-test",
            "state_path": paths.state_path,
            "state_id": "issue1160-project-state",
            "ledger_path": paths.root.join("multiplayer-ledger.json"),
            "ledger_id": "issue1160-multiplayer-ledger",
            "hub_state_path": paths.hub_state_path,
            "install_manifest_path": paths.install_manifest_path,
            "multiplayer_state_path": paths.multiplayer_state_path,
            "multiplayer_state_id": "issue1160-multiplayer-state",
            "action": "start_authoritative_server",
            "server_id": server_id,
            "lobby_id": lobby_id,
            "content_address": content_address(&paths.catalog_path),
            "host_player_id": "player.host",
            "max_players": 4,
            "endpoint": endpoint(lobby_id, port),
            "deterministic_tick_rate_hz": 64,
            "simulation_probe_ticks": 2,
            "calyx_agent_count": 4
        }),
    )
    .unwrap();
}

fn endpoint(lobby_id: &str, port: u16) -> Value {
    json!({
        "transport": "loopback",
        "host": "127.0.0.1",
        "port": port,
        "path": format!("/lobby/{lobby_id}")
    })
}

fn owner_role_args(paths: &BrowserPaths, lobby_id: &str) -> Value {
    moderation_args(
        paths,
        json!({
            "request_id": format!("issue1042-role-{lobby_id}"),
            "action": "assign_server_role",
            "assignment_id": format!("role.{lobby_id}.owner.host"),
            "lobby_id": lobby_id,
            "actor_account_id": "account.host",
            "account_id": "account.host",
            "role": "owner"
        }),
    )
}

fn policy_args(paths: &BrowserPaths, lobby_id: &str, strictness: u16, mode: &str) -> Value {
    moderation_args(
        paths,
        json!({
            "request_id": format!("issue1042-policy-{lobby_id}"),
            "action": "set_roleplay_policy",
            "policy_id": format!("policy.{lobby_id}.{mode}"),
            "lobby_id": lobby_id,
            "actor_account_id": "account.host",
            "roleplay_policy_mode": mode,
            "strictness_milli": strictness,
            "reward_multiplier_milli": 1000,
            "punishment_multiplier_milli": 1000,
            "min_evidence_refs": 1
        }),
    )
}

fn moderation_args(paths: &BrowserPaths, mut extra: Value) -> Value {
    let map = extra.as_object_mut().unwrap();
    map.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1042-test"),
    );
    map.insert("state_path".to_string(), json!(paths.state_path));
    map.insert("state_id".to_string(), json!("issue1160-project-state"));
    map.insert(
        "ledger_path".to_string(),
        json!(paths.root.join("moderation-ledger.json")),
    );
    map.insert(
        "ledger_id".to_string(),
        json!("issue1042-moderation-ledger"),
    );
    map.insert(
        "moderation_state_path".to_string(),
        json!(paths.moderation_state_path),
    );
    map.insert(
        "moderation_state_id".to_string(),
        json!("issue1042-moderation-state"),
    );
    map.insert(
        "social_state_path".to_string(),
        json!(paths.social_state_path),
    );
    map.insert("catalog_path".to_string(), json!(paths.catalog_path));
    map.insert(
        "multiplayer_state_path".to_string(),
        json!(paths.multiplayer_state_path),
    );
    extra
}

fn browser_args(paths: &BrowserPaths, mut extra: Value) -> Value {
    let map = extra.as_object_mut().unwrap();
    map.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1042-test"),
    );
    map.insert("state_path".to_string(), json!(paths.state_path));
    map.insert("state_id".to_string(), json!("issue1160-project-state"));
    map.insert("ledger_path".to_string(), json!(paths.ledger_path));
    map.insert("ledger_id".to_string(), json!("issue1042-browser-ledger"));
    map.insert(
        "server_browser_state_path".to_string(),
        json!(paths.browser_state_path),
    );
    map.insert(
        "server_browser_state_id".to_string(),
        json!("issue1042-browser-state"),
    );
    map.insert(
        "multiplayer_state_path".to_string(),
        json!(paths.multiplayer_state_path),
    );
    map.insert(
        "moderation_state_path".to_string(),
        json!(paths.moderation_state_path),
    );
    map.insert(
        "social_state_path".to_string(),
        json!(paths.social_state_path),
    );
    extra
}

fn private_invite_args(paths: &BrowserPaths, action: &str) -> Value {
    let account_id = if action == "accept_lobby_invite" {
        "account.friend"
    } else {
        "account.host"
    };
    json!({
        "request_id": format!("issue1042-private-invite-{action}"),
        "requested_by": "litd-editor-issue1042-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("social-ledger.json"),
        "ledger_id": "issue1160-social-ledger",
        "social_state_path": paths.social_state_path,
        "social_state_id": "issue1160-social-state",
        "multiplayer_state_path": paths.multiplayer_state_path,
        "action": action,
        "account_id": account_id,
        "target_account_id": "account.friend",
        "invite_id": "invite.private.001",
        "lobby_id": "lobby.private"
    })
}

fn join_connection_args(paths: &BrowserPaths) -> Value {
    json!({
        "request_id": "issue1042-join-private",
        "requested_by": "litd-editor-issue1042-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("social-ledger.json"),
        "ledger_id": "issue1160-social-ledger",
        "social_state_path": paths.social_state_path,
        "social_state_id": "issue1160-social-state",
        "multiplayer_state_path": paths.multiplayer_state_path,
        "action": "create_join_connection",
        "connection_id": "join.private.friend",
        "account_id": "account.friend",
        "lobby_id": "lobby.private",
        "join_connection_mode": "invite_code",
        "connect_host": "127.0.0.1",
        "connect_port": 19710,
        "connect_path": "/lobby/lobby.private",
        "invite_id": "invite.private.001"
    })
}

fn content_address(catalog_path: &Path) -> String {
    read_publish_catalog(catalog_path).unwrap().catalog.entries[0]
        .content_address
        .clone()
}
