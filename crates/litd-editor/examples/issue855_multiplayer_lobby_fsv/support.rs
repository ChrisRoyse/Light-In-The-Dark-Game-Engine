use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        DELIVERY_LITDGAME_TOOL, HUB_LAUNCHER_TOOL, NEW_OPEN_SAVE_TOOL, PUBLISH_LITDGAME_TOOL,
        ProjectLifecycleAction, call_mcp_project_tool, read_multiplayer_session_state,
        read_publish_catalog,
    },
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const PNG_1X1_RGBA: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 167, 197, 129, 218, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

pub(crate) struct Paths {
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
    pub(crate) multiplayer_ledger_path: PathBuf,
}

impl Paths {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
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
            multiplayer_ledger_path: root.join("multiplayer-ledger.json"),
        }
    }
}

pub(crate) fn run_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(workspace_root()
        .join("target")
        .join("fsv")
        .join("issue855_multiplayer_lobby")
        .join("runs")
        .join(format!("run-{stamp}-{}", std::process::id())))
}

pub(crate) fn prepare_fixture(root: &Path) -> Result<Paths, Box<dyn std::error::Error>> {
    let paths = Paths::new(root);
    fs::write(&paths.thumbnail_path, PNG_1X1_RGBA)?;
    call_mcp_project_tool(NEW_OPEN_SAVE_TOOL, save_args(&paths)?)?;
    call_mcp_project_tool(PUBLISH_LITDGAME_TOOL, publish_args(&paths))?;
    let content_address = content_address(&paths.catalog_path)?;
    call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address),
    )?;
    call_mcp_project_tool(HUB_LAUNCHER_TOOL, hub_args(&paths))?;
    Ok(paths)
}

pub(crate) fn summary(paths: &Paths) -> Value {
    let state = read_multiplayer_session_state(&paths.multiplayer_state_path).ok();
    json!({
        "exists": paths.multiplayer_state_path.exists(),
        "bytes": paths.multiplayer_state_path.metadata().map(|m| m.len()).unwrap_or_default(),
        "sha256": fs::read(&paths.multiplayer_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "lobbies": state.as_ref().map(|s| s.state.lobby_count).unwrap_or_default(),
        "joins": state.as_ref().map(|s| s.state.join_count).unwrap_or_default(),
        "servers": state.as_ref().map(|s| s.state.server_count).unwrap_or_default(),
        "running_servers": state.as_ref().map(|s| s.state.running_server_count).unwrap_or_default()
    })
}

pub(crate) fn host_args(
    paths: &Paths,
    content: &str,
    lobby: &str,
    max: usize,
    host: &str,
    team: &str,
) -> Value {
    json!({
        "request_id": format!("issue855-host-{lobby}"),
        "requested_by": "litd-editor-issue855-fsv",
        "state_path": paths.state_path,
        "state_id": "issue855-project-state",
        "ledger_path": paths.multiplayer_ledger_path,
        "ledger_id": "issue855-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue855-multiplayer-state",
        "action": "host_lobby",
        "lobby_id": lobby,
        "content_address": content,
        "host_player_id": "player.host",
        "team_id": team,
        "max_players": max,
        "endpoint": endpoint(host, lobby)
    })
}

pub(crate) fn join_args(paths: &Paths, lobby: &str, player: &str, team: &str) -> Value {
    json!({
        "request_id": format!("issue855-join-{player}"),
        "requested_by": "litd-editor-issue855-fsv",
        "state_path": paths.state_path,
        "state_id": "issue855-project-state",
        "ledger_path": paths.multiplayer_ledger_path,
        "ledger_id": "issue855-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue855-multiplayer-state",
        "action": "join_lobby",
        "lobby_id": lobby,
        "player_id": player,
        "team_id": team
    })
}

pub(crate) fn start_server_args(
    paths: &Paths,
    content: &str,
    server: &str,
    lobby: &str,
    host: &str,
) -> Value {
    json!({
        "request_id": format!("issue855-start-{server}"),
        "requested_by": "litd-editor-issue855-fsv",
        "state_path": paths.state_path,
        "state_id": "issue855-project-state",
        "ledger_path": paths.multiplayer_ledger_path,
        "ledger_id": "issue855-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue855-multiplayer-state",
        "action": "start_authoritative_server",
        "server_id": server,
        "lobby_id": lobby,
        "content_address": content,
        "host_player_id": "player.host",
        "max_players": 2,
        "endpoint": endpoint(host, lobby),
        "deterministic_tick_rate_hz": 64,
        "simulation_probe_ticks": 3,
        "calyx_agent_count": 2
    })
}

pub(crate) fn file_readback(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)?;
    if metadata.is_file() {
        let bytes = fs::read(path)?;
        return Ok(json!({
            "path": path,
            "kind": "file",
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes)
        }));
    }
    if metadata.is_dir() {
        let mut files = Vec::new();
        collect_files(path, path, &mut files)?;
        if files.len() > 64 {
            return Err(format!(
                "{} has {} files, verifier limit is 64",
                path.display(),
                files.len()
            )
            .into());
        }
        files.sort_by(|left, right| left.0.cmp(&right.0));
        let mut total_bytes = 0usize;
        let mut aggregate = String::new();
        let entries = files
            .into_iter()
            .map(|(relative, bytes)| {
                let hash = sha256_hex(&bytes);
                total_bytes += bytes.len();
                aggregate.push_str(&relative);
                aggregate.push_str(&hash);
                json!({ "relative_path": relative, "bytes": bytes.len(), "sha256": hash })
            })
            .collect::<Vec<_>>();
        return Ok(json!({
            "path": path,
            "kind": "directory",
            "file_count": entries.len(),
            "total_bytes": total_bytes,
            "sha256": sha256_hex(aggregate.as_bytes()),
            "files": entries
        }));
    }
    Err(format!("{} is neither file nor directory", path.display()).into())
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(current)? {
        let path = entry?.path();
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(root)?.display().to_string();
            files.push((relative, fs::read(path)?));
        }
    }
    Ok(())
}

pub(crate) fn content_address(catalog: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(read_publish_catalog(catalog)?.catalog.entries[0]
        .content_address
        .clone())
}

fn save_args(paths: &Paths) -> Result<Value, EngineError> {
    Ok(json!({
        "request_id": "issue855-save",
        "requested_by": "litd-editor-issue855-fsv",
        "state_path": paths.state_path,
        "state_id": "issue855-project-state",
        "ledger_path": paths.root.join("project-ledger.json"),
        "ledger_id": "issue855-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue855",
        "title": "Ashen Reach Issue 855",
        "version": "1.0.0",
        "tags": ["dark_fantasy", "multiplayer", "local_first"],
        "project_path": paths.project_path,
        "snapshot_label": "multiplayer-lobby-ready",
        "document": fixture_document()?
    }))
}

fn publish_args(paths: &Paths) -> Value {
    json!({
        "request_id": "issue855-publish",
        "requested_by": "litd-editor-issue855-fsv",
        "state_path": paths.state_path,
        "state_id": "issue855-project-state",
        "ledger_path": paths.root.join("publish-ledger.json"),
        "ledger_id": "issue855-publish-ledger",
        "project_path": paths.project_path,
        "package_path": paths.package_path,
        "catalog_path": paths.catalog_path,
        "share_id": "issue855-share",
        "created_by": "agent.project.publisher",
        "host_port": 19855,
        "max_package_bytes": 262144,
        "title": "Ashen Reach Multiplayer Lobby",
        "version": "1.0.0",
        "summary": "Known deterministic host/join lobby fixture",
        "tags": ["dark_fantasy", "multiplayer", "local_first"],
        "thumbnail_path": paths.thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn delivery_args(paths: &Paths, content_address: &str) -> Value {
    json!({
        "request_id": "issue855-deliver",
        "requested_by": "litd-editor-issue855-fsv",
        "state_path": paths.state_path,
        "state_id": "issue855-project-state",
        "ledger_path": paths.root.join("delivery-ledger.json"),
        "ledger_id": "issue855-delivery-ledger",
        "catalog_path": paths.catalog_path,
        "delivery_manifest_path": paths.delivery_manifest_path,
        "delivery_store_id": "issue855-local-store",
        "blob_root_path": paths.blob_root_path,
        "install_manifest_path": paths.install_manifest_path,
        "install_id": "issue855-install",
        "install_root_path": paths.install_root_path,
        "content_address": content_address,
        "delivered_by": "agent.delivery.store",
        "installed_by": "agent.delivery.install"
    })
}

fn hub_args(paths: &Paths) -> Value {
    json!({
        "request_id": "issue855-hub",
        "requested_by": "litd-editor-issue855-fsv",
        "state_path": paths.state_path,
        "state_id": "issue855-project-state",
        "ledger_path": paths.root.join("hub-ledger.json"),
        "ledger_id": "issue855-hub-ledger",
        "catalog_path": paths.catalog_path,
        "install_manifest_path": paths.install_manifest_path,
        "hub_state_path": paths.hub_state_path,
        "hub_id": "issue855-local-hub",
        "query": "Multiplayer",
        "required_tags": ["multiplayer"],
        "installed_only": true,
        "sort": "title_asc"
    })
}

fn endpoint(host: &str, lobby: &str) -> Value {
    json!({ "transport": "loopback", "host": host, "port": 19856, "path": format!("/lobby/{lobby}") })
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/issue855-lobby", "issue855-rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.issue855.asset".to_string(),
            reason: "missing medieval dungeon Rock1.fbx vault record".to_string(),
        })?;
    for (id, x) in [("unit_host", 10.0), ("unit_guest", 16.0)] {
        document.upsert_object(EditorObject::new(
            id,
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(x, 0.0, -12.0)),
        ))?;
        assign_vault_model_to_unit(&mut document, id, &record)?;
    }
    Ok(document)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor is under workspace crates")
        .to_path_buf()
}
