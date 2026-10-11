use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        DELIVERY_LITDGAME_TOOL, HUB_LAUNCHER_TOOL, MULTIPLAYER_SESSION_TOOL, NEW_OPEN_SAVE_TOOL,
        PUBLISH_LITDGAME_TOOL, ProjectLifecycleAction, SOCIAL_GRAPH_TOOL, call_mcp_project_tool,
        read_publish_catalog,
    },
};
use litd_engine::math::{Transform, Vec3};
use serde_json::{Value, json};

const PNG_1X1_RGBA: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 167, 197, 129, 218, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

struct FixturePaths {
    root: PathBuf,
    state_path: PathBuf,
    project_path: PathBuf,
    package_path: PathBuf,
    catalog_path: PathBuf,
    thumbnail_path: PathBuf,
    delivery_manifest_path: PathBuf,
    blob_root_path: PathBuf,
    install_manifest_path: PathBuf,
    install_root_path: PathBuf,
    hub_state_path: PathBuf,
    multiplayer_state_path: PathBuf,
    social_state_path: PathBuf,
    moderation_state_path: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    let paths = prepare_fixture()?;
    let content_address = read_publish_catalog(&paths.catalog_path)?.catalog.entries[0]
        .content_address
        .clone();
    let env = json!({
        "issue": 1467,
        "source": "real project/publish/delivery/hub/multiplayer/social MCP fixture for live GUI moderation command",
        "run_dir": paths.root,
        "content_address": content_address,
        "expected_report_id": "report.gui.issue1467",
        "env": {
            "LITD_EDITOR_SHELL_STATE": paths.root.join("editor-shell-state-aster"),
            "LITD_FSV_STATS": paths.root.join("gui-editor-stats.txt"),
            "LITD_LOG_FILE": paths.root.join("gui-editor.log"),
            "LITD_GUI_MODERATION_STATE_PATH": paths.state_path,
            "LITD_GUI_MODERATION_LEDGER_PATH": paths.root.join("gui-moderation-ledger.json"),
            "LITD_GUI_MODERATION_MODERATION_STATE_PATH": paths.moderation_state_path,
            "LITD_GUI_MODERATION_SOCIAL_STATE_PATH": paths.social_state_path,
            "LITD_GUI_MODERATION_CATALOG_PATH": paths.catalog_path,
            "LITD_GUI_MODERATION_MULTIPLAYER_STATE_PATH": paths.multiplayer_state_path,
            "LITD_GUI_MODERATION_CONTENT_ADDRESS": content_address,
            "LITD_GUI_MODERATION_REPORT_ID": "report.gui.issue1467",
            "LITD_GUI_MODERATION_REPORTER_ACCOUNT_ID": "account.host",
            "LITD_GUI_MODERATION_REPORTED_ACCOUNT_ID": "account.friend",
            "LITD_GUI_MODERATION_LOBBY_ID": "lobby.social",
            "LITD_GUI_MODERATION_REASON": "harassment",
            "LITD_GUI_MODERATION_SEVERITY": "4",
            "LITD_GUI_MODERATION_DESCRIPTION": "Manual GUI moderation report for issue1467"
        }
    });
    let env_path = paths.root.join("gui-moderation-env.json");
    fs::write(&env_path, serde_json::to_vec_pretty(&env)?)?;
    let ps1_path = paths.root.join("gui-moderation-env.ps1");
    fs::write(&ps1_path, powershell_env(&env)?)?;
    println!("GUI_MODERATION_FIXTURE={}", env_path.display());
    println!("GUI_MODERATION_ENV_PS1={}", ps1_path.display());
    println!("GUI_MODERATION_RUN={}", paths.root.display());
    Ok(())
}

fn prepare_fixture() -> Result<FixturePaths, Box<dyn Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::current_dir()?
        .join("target")
        .join("fsv")
        .join("issue1467_mcp_gui_parity")
        .join("runs")
        .join(format!("gui-moderation-{nanos}-{}", std::process::id()));
    fs::create_dir_all(&root)?;
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
        social_state_path: root.join("social-graph-state.json"),
        moderation_state_path: root.join("moderation-safety-state.json"),
    };
    fs::write(&paths.thumbnail_path, PNG_1X1_RGBA)?;
    call_mcp_project_tool(NEW_OPEN_SAVE_TOOL, save_args(&paths)?)?;
    call_mcp_project_tool(PUBLISH_LITDGAME_TOOL, publish_args(&paths))?;
    let content_address = read_publish_catalog(&paths.catalog_path)?.catalog.entries[0]
        .content_address
        .clone();
    call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address),
    )?;
    call_mcp_project_tool(HUB_LAUNCHER_TOOL, hub_args(&paths))?;
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(&paths, &content_address),
    )?;
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        create_account_args(
            &paths,
            "account.host",
            "player.host",
            "profile.host",
            "host",
        ),
    )?;
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        create_account_args(
            &paths,
            "account.friend",
            "player.friend",
            "profile.friend",
            "friend",
        ),
    )?;
    Ok(paths)
}

fn save_args(paths: &FixturePaths) -> Result<Value, Box<dyn Error>> {
    Ok(json!({
        "request_id": "issue1467-gui-save",
        "requested_by": "issue1467-fixture",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("project-ledger.json"),
        "ledger_id": "issue1467-gui-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue1467_gui",
        "title": "Ashen Reach Issue 1467 GUI",
        "version": "1.2.3",
        "tags": ["dark_fantasy", "social", "local_first"],
        "project_path": paths.project_path,
        "snapshot_label": "gui-moderation-ready",
        "document": fixture_document()?
    }))
}

fn publish_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1467-gui-publish",
        "requested_by": "issue1467-fixture",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("publish-ledger.json"),
        "ledger_id": "issue1467-gui-publish-ledger",
        "project_path": paths.project_path,
        "package_path": paths.package_path,
        "catalog_path": paths.catalog_path,
        "share_id": "issue1467-gui-share",
        "created_by": "agent.project.publisher",
        "host_port": 19670,
        "max_package_bytes": 262144,
        "title": "Ashen Reach GUI Moderation",
        "version": "1.2.3",
        "summary": "Known deterministic GUI moderation fixture",
        "tags": ["dark_fantasy", "singleplayer", "local_first"],
        "thumbnail_path": paths.thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn delivery_args(paths: &FixturePaths, content_address: &str) -> Value {
    json!({
        "request_id": "issue1467-gui-deliver",
        "requested_by": "issue1467-fixture",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("delivery-ledger.json"),
        "ledger_id": "issue1467-gui-delivery-ledger",
        "catalog_path": paths.catalog_path,
        "delivery_manifest_path": paths.delivery_manifest_path,
        "delivery_store_id": "issue1467-local-store",
        "blob_root_path": paths.blob_root_path,
        "install_manifest_path": paths.install_manifest_path,
        "install_id": "issue1467-install",
        "install_root_path": paths.install_root_path,
        "content_address": content_address,
        "delivered_by": "agent.delivery.store",
        "installed_by": "agent.delivery.install"
    })
}

fn hub_args(paths: &FixturePaths) -> Value {
    json!({
        "request_id": "issue1467-gui-hub",
        "requested_by": "issue1467-fixture",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("hub-ledger.json"),
        "ledger_id": "issue1467-gui-hub-ledger",
        "catalog_path": paths.catalog_path,
        "install_manifest_path": paths.install_manifest_path,
        "hub_state_path": paths.hub_state_path,
        "hub_id": "issue1467-local-hub",
        "query": "Moderation",
        "required_tags": ["singleplayer"],
        "installed_only": true,
        "sort": "title_asc"
    })
}

fn host_args(paths: &FixturePaths, content_address: &str) -> Value {
    json!({
        "request_id": "issue1467-gui-host",
        "requested_by": "issue1467-fixture",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1467-gui-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1467-gui-multiplayer-state",
        "action": "host_lobby",
        "lobby_id": "lobby.social",
        "content_address": content_address,
        "host_player_id": "player.host",
        "max_players": 4,
        "endpoint": {
            "transport": "loopback",
            "host": "127.0.0.1",
            "port": 19767,
            "path": "/lobby/lobby.social"
        }
    })
}

fn create_account_args(
    paths: &FixturePaths,
    account: &str,
    player: &str,
    profile: &str,
    handle: &str,
) -> Value {
    json!({
        "request_id": format!("issue1467-gui-social-{handle}"),
        "requested_by": "issue1467-fixture",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("social-ledger.json"),
        "ledger_id": "issue1467-gui-social-ledger",
        "social_state_path": paths.social_state_path,
        "social_state_id": "issue1467-gui-social-state",
        "multiplayer_state_path": paths.multiplayer_state_path,
        "action": "create_account_profile",
        "account_id": account,
        "player_id": player,
        "profile_id": profile,
        "handle": handle,
        "display_name": format!("{handle} Display"),
        "bio": "Known deterministic GUI moderation fixture",
        "tags": ["local_first", "social"]
    })
}

fn fixture_document() -> Result<EditorDocument, Box<dyn Error>> {
    let mut document = EditorDocument::new("calyx://world/issue1467-gui", "issue1467-gui-rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(EditorObject::new(
        "unit_gui_moderation_hero",
        "Unit",
        Some("world".to_string()),
        Transform::from_translation(Vec3::new(12.0, 0.0, -14.0)),
    ))?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or("missing default fixture model record")?;
    assign_vault_model_to_unit(&mut document, "unit_gui_moderation_hero", &record)?;
    Ok(document)
}

fn powershell_env(env: &Value) -> Result<String, Box<dyn Error>> {
    let vars = env["env"].as_object().ok_or("env object missing")?;
    let mut lines = vec!["$ErrorActionPreference = 'Stop'".to_string()];
    for (name, value) in vars {
        let value = value
            .as_str()
            .ok_or("all GUI moderation env values must be strings")?;
        lines.push(format!("$env:{name} = '{}'", escape_ps(value)));
    }
    lines.push("cargo run -p litd-editor --bin litd-editor".to_string());
    Ok(lines.join("\r\n"))
}

fn escape_ps(value: &str) -> String {
    value.replace('\'', "''")
}
