use litd_engine::fsv_artifacts::env_path;
use std::fs;

use litd_engine::calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes};
use serde_json::json;

const RUNTIME_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCC";
const RUNTIME_STATE_VAULT_PURPOSE: &[u8] = b"litd-game-runtime-state-issue1363";
const RUNTIME_STATE_COLLECTION: &str = "litd_game_runtime_host";
const RUNTIME_STATE_ROW: &[u8] = b"runtime/host_state/latest";
const RUNTIME_STATE_COLUMN: &[u8] = b"state_json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let vault_dir = env_path(
        "LITD_ISSUE1418_RUNTIME_STATE_VAULT",
        "target/fsv/issue1418_determinism_boundary/runtime-state-aster",
    );
    let report_path = env_path(
        "LITD_ISSUE1418_RUNTIME_STATE_REPORT",
        "target/fsv/issue1418_determinism_boundary/runtime-state-readback.json",
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
    let state: serde_json::Value = serde_json::from_slice(&readback.bytes)?;
    let boundary = state
        .get("determinism_boundary")
        .ok_or("runtime state missing determinism_boundary")?;
    let render_frame_recorded = boundary["render_frame_entries"].as_u64().unwrap_or(0) >= 1;
    let hot_path_calyx_access_zero = boundary["hot_path_calyx_access_total"] == 0;
    let runtime_exited = state["status"] == "exited";
    let runtime_started_once = state["counters"]["runtime_start_calls"] == 1;
    let runtime_shutdown_once = state["counters"]["runtime_shutdown_calls"] == 1;
    let renderer_recorded = state["renderer"]["gpu_context_id"].as_u64().unwrap_or(0) >= 1
        && state["renderer"]["width"].as_u64().unwrap_or(0) >= 1
        && state["renderer"]["height"].as_u64().unwrap_or(0) >= 1;
    let rendered_world_nonblank = state["world"]["scene_is_blank"] == false
        && state["world"]["visible_triangle_count"]
            .as_u64()
            .unwrap_or(0)
            >= 1;
    let report = json!({
        "schema_version": 1,
        "issue": 1418,
        "source_of_truth": "Aster Graph CF plain-column row written by litd-game runtime_state",
        "vault_dir": vault_dir,
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "cell_key": readback.cell_key,
        "index_key": readback.index_key,
        "state_sha256": readback.sha256,
        "status": state["status"],
        "package_id": state["package_id"],
        "app_frame": state["app_frame"],
        "render_frame": state["render_frame"],
        "renderer": state["renderer"],
        "world": state["world"],
        "counters": state["counters"],
        "determinism_boundary": boundary,
        "assertions": {
            "render_frame_recorded": render_frame_recorded,
            "hot_path_calyx_access_zero": hot_path_calyx_access_zero,
            "runtime_exited": runtime_exited,
            "runtime_started_once": runtime_started_once,
            "runtime_shutdown_once": runtime_shutdown_once,
            "renderer_recorded": renderer_recorded,
            "rendered_world_nonblank": rendered_world_nonblank,
        }
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
    let persisted_report: serde_json::Value = serde_json::from_slice(&persisted)?;
    for field in [
        "render_frame_recorded",
        "hot_path_calyx_access_zero",
        "runtime_exited",
        "runtime_started_once",
        "runtime_shutdown_once",
        "renderer_recorded",
        "rendered_world_nonblank",
    ] {
        if persisted_report["assertions"][field] != true {
            return Err(format!(
                "runtime state persisted assertion {field} was false: {}",
                persisted_report["assertions"]
            )
            .into());
        }
    }
    println!(
        "ISSUE1418_RUNTIME_STATE_READBACK report={} sha256={} state_sha256={} render_entries={} hot_path_calyx={}",
        report_path.display(),
        litd_hash::sha256_hex(&persisted),
        readback.sha256,
        boundary["render_frame_entries"].as_u64().unwrap_or(0),
        boundary["hot_path_calyx_access_total"]
            .as_u64()
            .unwrap_or(0),
    );
    Ok(())
}
