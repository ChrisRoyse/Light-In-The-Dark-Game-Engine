use std::{error::Error, fs, path::PathBuf};

use litd_editor::mcp_project_tools::read_moderation_safety_state;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let fixture_path = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: issue1467_gui_moderation_readback <gui-moderation-env.json> [screenshot.png] [hwnd] [pid]")?;
    let screenshot_path = args.next().map(PathBuf::from);
    let window_hwnd = args.next();
    let process_id = args.next();

    let fixture: Value = serde_json::from_slice(&fs::read(&fixture_path)?)?;
    let run_dir = fixture["run_dir"]
        .as_str()
        .ok_or("fixture run_dir missing")?;
    let env = fixture["env"].as_object().ok_or("fixture env missing")?;
    let moderation_state_path = env
        .get("LITD_GUI_MODERATION_MODERATION_STATE_PATH")
        .and_then(Value::as_str)
        .ok_or("fixture moderation state path missing")?;
    let expected_report_id = fixture["expected_report_id"]
        .as_str()
        .ok_or("fixture expected_report_id missing")?;

    let moderation_readback = read_moderation_safety_state(PathBuf::from(moderation_state_path))?;
    let report = moderation_readback
        .state
        .reports
        .iter()
        .find(|report| report.report_id == expected_report_id)
        .ok_or("expected GUI moderation report was not persisted")?;

    let screenshot = match screenshot_path {
        Some(path) => {
            let bytes = fs::read(&path)?;
            json!({
                "path": path,
                "bytes": bytes.len(),
                "sha256": sha256_hex(&bytes)
            })
        }
        None => Value::Null,
    };

    let summary = json!({
        "issue": 1467,
        "source": "manual Synapse GUI command invocation with persisted moderation readback",
        "fixture_path": fixture_path,
        "run_dir": run_dir,
        "window_hwnd": window_hwnd,
        "process_id": process_id,
        "expected_report_id": expected_report_id,
        "report_id": report.report_id,
        "report_count": moderation_readback.state.report_count,
        "open_report_count": moderation_readback.state.open_report_count,
        "status": format!("{:?}", report.status),
        "reason": format!("{:?}", report.reason),
        "lobby_id": report.lobby_id,
        "reporter_account_id": report.reporter_account_id,
        "reported_account_id": report.reported_account_id,
        "content_address": report.content_address,
        "moderation_state_path": moderation_readback.path,
        "moderation_state_sha256": moderation_readback.sha256,
        "screenshot": screenshot
    });

    let output_path = PathBuf::from(run_dir).join("gui-moderation-readback.json");
    fs::write(&output_path, serde_json::to_vec_pretty(&summary)?)?;
    let output_bytes = fs::read(&output_path)?;
    println!("GUI_MODERATION_READBACK={}", output_path.display());
    println!(
        "GUI_MODERATION_READBACK_SHA256={}",
        sha256_hex(&output_bytes)
    );
    Ok(())
}
