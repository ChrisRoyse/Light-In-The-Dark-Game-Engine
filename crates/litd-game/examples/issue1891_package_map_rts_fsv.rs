//! Full-State-Verification for #1891–#1894: package-map play path + RTS defaults + QWER casts.
//!
//! Source of truth (independent readbacks):
//! 1. `build_package_world_scene` over a sealed package (terrain_cell_count / markers / scene name).
//! 2. Fail-closed host exit when terrain_cells are absent (no ED11).
//! 3. `InputBindings::rts_play_defaults` binding table (no WASD pan; QWER ability slots).
//! 4. Real `litd-game` host process: Aster runtime-state vault row for world.scene_name,
//!    package document_id, and ability_cast counters after `--inject-ability-mask`.
//!
//! Process exits non-zero on any failed assertion (no false-green report write).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use litd_engine::{
    calyx_json_store::{CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes},
    fsv_artifacts::{begin_issue_run, workspace_path},
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageEndpoint,
        RuntimePackageHostMode, compute_runtime_manifest_fingerprint, load_runtime_locked_package,
    },
    input::{
        ACTION_ABILITY_SLOT_0, ACTION_ABILITY_SLOT_1, ACTION_ABILITY_SLOT_2, ACTION_ABILITY_SLOT_3,
        ACTION_CAMERA_FORWARD, ACTION_CAMERA_YAW_LEFT, ACTION_CAMERA_YAW_RIGHT, InputBinding,
        InputBindings,
    },
    render::editor_world::build_package_world_scene,
};
use serde_json::json;
use winit::keyboard::KeyCode;

const RUNTIME_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCC";
const RUNTIME_STATE_VAULT_PURPOSE: &[u8] = b"litd-game-runtime-state-issue1363";
const RUNTIME_STATE_COLLECTION: &str = "litd_game_runtime_host";
const RUNTIME_STATE_ROW: &[u8] = b"runtime/host_state/latest";
const RUNTIME_STATE_COLUMN: &[u8] = b"state_json";
const FRAMES: u32 = 90;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1891_package_map_rts")?;

    // --- #1892/#1893: binding table is the source of truth for RTS defaults ---
    let bindings = InputBindings::rts_play_defaults();
    assert_eq!(
        bindings.bindings_for(ACTION_ABILITY_SLOT_0),
        &[InputBinding::Key(KeyCode::KeyQ)]
    );
    assert_eq!(
        bindings.bindings_for(ACTION_ABILITY_SLOT_1),
        &[InputBinding::Key(KeyCode::KeyW)]
    );
    assert_eq!(
        bindings.bindings_for(ACTION_ABILITY_SLOT_2),
        &[InputBinding::Key(KeyCode::KeyE)]
    );
    assert_eq!(
        bindings.bindings_for(ACTION_ABILITY_SLOT_3),
        &[InputBinding::Key(KeyCode::KeyR)]
    );
    assert!(
        !bindings
            .bindings_for(ACTION_CAMERA_FORWARD)
            .contains(&InputBinding::Key(KeyCode::KeyW)),
        "WASD must not pan in RTS defaults (#1892)"
    );
    assert!(
        !bindings
            .bindings_for(ACTION_CAMERA_YAW_LEFT)
            .contains(&InputBinding::Key(KeyCode::KeyQ)),
        "Q must not yaw camera (#1893)"
    );
    assert!(
        !bindings
            .bindings_for(ACTION_CAMERA_YAW_RIGHT)
            .contains(&InputBinding::Key(KeyCode::KeyE)),
        "E must not yaw camera (#1893)"
    );

    // --- #1891: synthetic package with known terrain + markers ---
    let package_path = run.run_dir().join("issue1891-map.litdgame.json");
    write_map_package(&package_path, "issue1891map", true)?;
    let loaded = load_runtime_locked_package(&package_path, "issue1891_fsv")?;
    let scene = build_package_world_scene(&loaded)?;
    assert_eq!(scene.terrain_cell_count, 4, "synthetic map has 4 cells");
    assert!(
        scene.marker_count >= 1,
        "start marker must contribute to package scene"
    );
    assert_eq!(
        scene.document_id, "calyx://world/issue1891-fsv-map",
        "document_id is package SoT"
    );
    let scene_name = scene.scene.scene_name().to_string();
    assert!(
        scene_name.starts_with("package_map:"),
        "scene must be package_map, got {scene_name}"
    );

    // Edge A: empty terrain fails closed (build + host).
    let no_terrain_path = run.run_dir().join("issue1891-no-terrain.litdgame.json");
    write_map_package(&no_terrain_path, "issue1891noterrain", false)?;
    let no_terrain = load_runtime_locked_package(&no_terrain_path, "issue1891_fsv")?;
    let no_terrain_err = match build_package_world_scene(&no_terrain) {
        Ok(_) => panic!("missing terrain_cells must refuse ED11"),
        Err(err) => err,
    };
    assert!(
        no_terrain_err.to_string().contains("terrain_cells"),
        "fail-closed error must name terrain_cells: {no_terrain_err}"
    );
    let edge_no_terrain = run_host_expect_failure(&no_terrain_path, None, FRAMES)?;
    assert!(
        edge_no_terrain.failed_closed,
        "host must exit non-zero without terrain: {}",
        edge_no_terrain.detail
    );

    // Edge B: host with valid map + runtime state vault.
    let state_path = run.run_dir().join("runtime-state-store");
    run_host(
        &package_path,
        Some(&state_path),
        FRAMES,
        None, // no session on synthetic package — inject still exercises reject path
    )?;
    let state_readback = read_runtime_state_source(&state_path)?;
    let state: serde_json::Value = serde_json::from_slice(&state_readback.bytes)?;
    let status = state
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        status == "exited" || status == "closed",
        "terminal status must persist, got {status:?}"
    );
    let world_scene = state
        .pointer("/world/scene_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(
        world_scene.starts_with("package_map:"),
        "runtime state world.scene_name must be package_map, got {world_scene:?}"
    );
    let scene_blank = state
        .pointer("/world/scene_is_blank")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    assert!(!scene_blank, "package map scene must not be blank");
    let doc_id = state
        .pointer("/document_summary/document_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert_eq!(doc_id, "calyx://world/issue1891-fsv-map");

    // --- #1894: inject Q slot on a real ship package (session present) when available ---
    let ship = find_ship_skirmish_package();
    let mut ability_accepted = 0u64;
    let mut ability_rejected = 0u64;
    let mut ability_last_slot = 255u64;
    let mut ship_scene = String::new();
    let mut ship_terrain = 0u64;
    let mut ship_package_used = String::new();
    if let Some(ship_path) = ship {
        ship_package_used = ship_path.display().to_string();
        let ship_state = run.run_dir().join("runtime-state-ship");
        // bit0 = Q ability slot
        run_host(&ship_path, Some(&ship_state), FRAMES, Some(1))?;
        let ship_rb = read_runtime_state_source(&ship_state)?;
        let ship_state_json: serde_json::Value = serde_json::from_slice(&ship_rb.bytes)?;
        ability_accepted = ship_state_json
            .pointer("/counters/ability_cast_accepted")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        ability_rejected = ship_state_json
            .pointer("/counters/ability_cast_rejected")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        ability_last_slot = ship_state_json
            .pointer("/counters/last_ability_slot")
            .and_then(|v| v.as_u64())
            .unwrap_or(255);
        ship_scene = ship_state_json
            .pointer("/world/scene_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        ship_terrain = ship_state_json
            .pointer("/document_summary/terrain_cell_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(
            ship_scene.starts_with("package_map:"),
            "ship package must render package_map, got {ship_scene}"
        );
        assert_eq!(
            ship_terrain, 256,
            "Ashford/Ridgefork class maps are 16x16 = 256 cells"
        );
        assert_eq!(ability_last_slot, 0, "inject bit0 must record last slot 0");
        assert!(
            ability_accepted + ability_rejected >= 1,
            "ability inject must accept or reject loud, never silent (a={ability_accepted} r={ability_rejected})"
        );
    }

    // Edge C: inject without session still rejects loud on synthetic package.
    let inject_state = run.run_dir().join("runtime-state-inject-nosession");
    run_host(&package_path, Some(&inject_state), FRAMES, Some(1))?;
    let inject_rb = read_runtime_state_source(&inject_state)?;
    let inject_json: serde_json::Value = serde_json::from_slice(&inject_rb.bytes)?;
    let inject_rejected = inject_json
        .pointer("/counters/ability_cast_rejected")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        inject_rejected >= 1,
        "no-session ability inject must reject: {inject_rejected}"
    );

    let report = json!({
        "schema": "litd.issue1891.package_map_rts_fsv.v1",
        "issues": [1891, 1892, 1893, 1894],
        "source_of_truth": {
            "package_path": package_path.display().to_string(),
            "runtime_state_vault": state_path.display().to_string(),
            "runtime_state_aster": {
                "sha256": state_readback.sha256,
                "bytes": state_readback.byte_len,
                "seq": state_readback.seq,
                "collection": state_readback.collection,
                "row_key": state_readback.row_key,
            },
            "scene_name": world_scene,
            "document_id": doc_id,
            "terrain_cell_count": scene.terrain_cell_count,
            "marker_count": scene.marker_count,
        },
        "rts_defaults": {
            "qwer_ability_slots": true,
            "no_wasd_pan": true,
            "no_qe_camera_yaw": true,
        },
        "edges": {
            "no_terrain_failed_closed": edge_no_terrain.failed_closed,
            "no_terrain_detail": edge_no_terrain.detail,
            "no_session_ability_rejected": inject_rejected,
        },
        "ship_package": {
            "path": ship_package_used,
            "scene_name": ship_scene,
            "terrain_cell_count": ship_terrain,
            "ability_cast_accepted": ability_accepted,
            "ability_cast_rejected": ability_rejected,
            "last_ability_slot": ability_last_slot,
        },
        "sufficiency_argument": "4-cell synthetic map proves package_map scene build + fail-closed empty terrain + host Aster world.scene_name. RTS binding table is pure data SoT. Ability path: no-session reject on synthetic + optional ship package with session (inject mask 1) proves accepted|rejected counters. 90 frames is enough for first-frame inject + state heartbeat.",
        "research_sources": [
            "https://www.reddit.com/r/RealTimeStrategy/comments/1e9bypx/what_is_your_preferred_method_of_camera_control/",
            "https://discussions.unity.com/t/rts-style-camera-scrolling/2269"
        ],
        "expected_actuals": { "all_assertions_passed": true }
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1891_PACKAGE_MAP_RTS_FSV report={} sha={} scene={} terrain={} no_terrain_closed={} ability_rejected_nosession={} ship_a={} ship_r={}",
        readback.path.display(),
        readback.sha256,
        world_scene,
        scene.terrain_cell_count,
        edge_no_terrain.failed_closed,
        inject_rejected,
        ability_accepted,
        ability_rejected
    );
    Ok(())
}

struct EdgeResult {
    failed_closed: bool,
    detail: String,
}

fn write_map_package(
    path: &Path,
    share_id: &str,
    with_terrain: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 8490,
        path: format!("/share/{share_id}"),
    };
    let document_fingerprint = "1891ad0c1891ad0c";
    let play_build_fingerprint = "1891beef1891beef";
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let asset_dir = parent.join("assets");
        fs::create_dir_all(&asset_dir)?;
        fs::write(
            asset_dir.join("issue1891.bin"),
            b"issue1891-package-scene-fixture",
        )?;
    }
    let asset_bytes = b"issue1891-package-scene-fixture";
    let assets = vec![litd_engine::game_package::RuntimePackageAssetRef {
        product_id: "litd.asset.issue1891.fixture".to_string(),
        source_relative_path: "assets/issue1891.bin".to_string(),
        source_sha256: litd_hash::sha256_hex(asset_bytes),
        source_bytes: asset_bytes.len() as u64,
    }];
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue1891",
        LITD_RUNTIME_ID,
        &endpoint,
        document_fingerprint,
        play_build_fingerprint,
        &assets,
    )?;
    let mut document = json!({
        "schema": "litd.lowered_document.v1",
        "calyx_document_id": "calyx://world/issue1891-fsv-map",
        "objects": {
            "start_player_one": {
                "kind": "StartLocation",
                "transform": { "translation": [0.5, 0.0, 0.5] }
            }
        }
    });
    if with_terrain {
        document["terrain_cells"] = json!({
            "0_0": { "tile_x": 0, "tile_z": 0, "height": 0.4, "cliff_level": 0 },
            "1_0": { "tile_x": 1, "tile_z": 0, "height": 0.4, "cliff_level": 0 },
            "0_1": { "tile_x": 0, "tile_z": 1, "height": 1.1, "cliff_level": 1 },
            "1_1": { "tile_x": 1, "tile_z": 1, "height": 0.4, "cliff_level": 0 }
        });
    }
    let envelope = json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue1891",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": endpoint.uri(),
        "document": document,
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": assets.len(),
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint
    });
    fs::write(path, serde_json::to_vec_pretty(&envelope)?)?;
    Ok(())
}

fn run_host(
    package_path: &Path,
    state_path: Option<&Path>,
    frames: u32,
    inject_mask: Option<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut command = Command::new(litd_game_bin());
    command.arg("--package").arg(package_path);
    if let Some(state_path) = state_path {
        command.arg("--runtime-state").arg(state_path);
    }
    if let Some(mask) = inject_mask {
        command.arg("--inject-ability-mask").arg(mask.to_string());
    }
    command.arg("--hidden-window");
    command.arg("--max-frames").arg(frames.to_string());
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "litd-game failed: status={:?} stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn run_host_expect_failure(
    package_path: &Path,
    state_path: Option<&Path>,
    frames: u32,
) -> Result<EdgeResult, Box<dyn std::error::Error>> {
    let mut command = Command::new(litd_game_bin());
    command.arg("--package").arg(package_path);
    if let Some(state_path) = state_path {
        command.arg("--runtime-state").arg(state_path);
    }
    command.arg("--hidden-window");
    command.arg("--max-frames").arg(frames.to_string());
    let output = command.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(EdgeResult {
        failed_closed: !output.status.success(),
        detail: format!("stderr={} stdout={}", stderr.trim(), stdout.trim()),
    })
}

fn litd_game_bin() -> PathBuf {
    let mut path = std::env::current_exe().expect("current executable path");
    path.pop();
    if path.file_name().and_then(|name| name.to_str()) == Some("examples") {
        path.pop();
    }
    path.join(format!("litd-game{}", std::env::consts::EXE_SUFFIX))
}

fn read_runtime_state_source(
    path: &Path,
) -> Result<CalyxJsonStoreReadback, Box<dyn std::error::Error>> {
    read_calyx_json_bytes(&runtime_state_spec(path))
        .map_err(|error| format!("read runtime state Aster row: {error}"))?
        .ok_or_else(|| format!("missing runtime state Aster row at {}", path.display()).into())
}

fn runtime_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: RUNTIME_STATE_VAULT_ID,
        purpose: RUNTIME_STATE_VAULT_PURPOSE,
        collection: RUNTIME_STATE_COLLECTION,
        row_key: RUNTIME_STATE_ROW,
        column: RUNTIME_STATE_COLUMN,
    }
}

fn find_ship_skirmish_package() -> Option<PathBuf> {
    let roots = [
        workspace_path("target/fsv/issue1788_m1_full_roster_ship"),
        workspace_path("target/fsv/issue1788_m1_skirmish"),
    ];
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        if let Some(found) = find_named_under(&root, "litd-m1-skirmish.litdgame.json") {
            return Some(found);
        }
    }
    None
}

fn find_named_under(root: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                let modified = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                match &newest {
                    Some((t, _)) if *t >= modified => {}
                    _ => newest = Some((modified, path)),
                }
            }
        }
    }
    newest.map(|(_, p)| p)
}
