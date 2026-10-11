use litd_engine::fsv_artifacts::env_path;
use std::fs;

use litd_engine::calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes};
use serde_json::{Value, json};

const RUNTIME_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCC";
const RUNTIME_STATE_VAULT_PURPOSE: &[u8] = b"litd-game-runtime-state-issue1363";
const RUNTIME_STATE_COLLECTION: &str = "litd_game_runtime_host";
const RUNTIME_STATE_ROW: &[u8] = b"runtime/host_state/latest";
const RUNTIME_STATE_COLUMN: &[u8] = b"state_json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let vault_dir = env_path(
        "LITD_ISSUE1153_RUNTIME_STATE_VAULT",
        "target/fsv/issue1153_player_min_spec/player-runtime-fsv-20260705/runtime-state-aster",
    );
    let report_path = env_path(
        "LITD_ISSUE1153_RUNTIME_STATE_REPORT",
        "target/fsv/issue1153_player_min_spec/player-runtime-fsv-20260705/runtime-state-readback.json",
    );
    let readback = read_calyx_json_bytes(&CalyxJsonStoreSpec {
        vault_dir: &vault_dir,
        vault_id: RUNTIME_STATE_VAULT_ID,
        purpose: RUNTIME_STATE_VAULT_PURPOSE,
        collection: RUNTIME_STATE_COLLECTION,
        row_key: RUNTIME_STATE_ROW,
        column: RUNTIME_STATE_COLUMN,
    })?
    .ok_or_else(|| format!("runtime state row missing in {}", vault_dir.display()))?;
    let state: Value = serde_json::from_slice(&readback.bytes)?;
    let quality = state
        .get("quality")
        .ok_or("runtime state missing quality settings")?;
    let render_frame = state
        .get("render_frame")
        .ok_or("runtime state missing render_frame")?;
    let renderer = state
        .get("renderer")
        .ok_or("runtime state missing renderer")?;
    let boundary = state
        .get("determinism_boundary")
        .ok_or("runtime state missing determinism_boundary")?;
    let assertions = json!({
        "runtime_exited": state["status"] == "exited",
        "quality_low": quality["tier"] == "low",
        "target_1080p": quality["target_width"] == 1920 && quality["target_height"] == 1080,
        "render_scale_50": quality["render_scale_percent"] == 50,
        "target_60fps": quality["target_fps"] == 60,
        "surface_1080p": renderer["width"] == 1920 && renderer["height"] == 1080,
        "frame_1080p": render_frame["width"] == 1920 && render_frame["height"] == 1080,
        "low_draw_budget": render_frame["budget_max_draw_calls"] == 512,
        "draw_budget_ok": render_frame["budget_ok"] == true,
        "hot_path_calyx_access_zero": boundary["hot_path_calyx_access_total"] == 0,
    });
    for (name, value) in assertions.as_object().unwrap() {
        if value != true {
            return Err(format!("runtime state assertion failed: {name}={value}").into());
        }
    }
    let report = json!({
        "schema": "litd.engine.issue1153.player_runtime_state.v1",
        "issue": 1153,
        "source_of_truth": "Aster Graph CF row written by litd-game --quality low runtime_state",
        "vault_dir": vault_dir,
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "cell_key": readback.cell_key,
        "index_key": readback.index_key,
        "state_sha256": readback.sha256,
        "status": state["status"],
        "package_id": state["package_id"],
        "app_frame": state["app_frame"],
        "render_frame": render_frame,
        "renderer": renderer,
        "quality": quality,
        "world": state["world"],
        "determinism_boundary": boundary,
        "assertions": assertions,
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&report_path, &bytes)?;
    let persisted = fs::read(&report_path)?;
    if persisted != bytes {
        return Err("runtime state readback report changed after write".into());
    }
    println!(
        "ISSUE1153_RUNTIME_STATE_READBACK report={} sha256={} state_sha256={} frame={} draw_calls={} budget={} surface={}x{}",
        report_path.display(),
        litd_hash::sha256_hex(&persisted),
        readback.sha256,
        render_frame["frame_index"].as_u64().unwrap_or(0),
        render_frame["draw_calls"].as_u64().unwrap_or(0),
        render_frame["budget_max_draw_calls"].as_u64().unwrap_or(0),
        renderer["width"].as_u64().unwrap_or(0),
        renderer["height"].as_u64().unwrap_or(0),
    );
    Ok(())
}
