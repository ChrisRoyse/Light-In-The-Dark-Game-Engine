//! FSV: `LITD_CALYX_AI_ADDON` force-absent vs require (#1790/#1792/#1791).
//!
//! Proves the shipped gate on the REAL content-store entry path:
//! 1. `absent` — author+persist+lower succeeds with pending measurement (no ORT).
//! 2. `require` on an unmaterialized panel vault fails LOUD with
//!    `LITD_CALYX_AI_ADDON_REQUIRED_UNAVAILABLE` (never silent success).
//!
//! Run: cargo run -p litd-engine --example issue1792_ai_addon_gate_fsv --features fsv

use std::error::Error;
use std::sync::{Mutex, OnceLock};

use litd_engine::calyx_ai_addon_gate::{CALYX_AI_ADDON_ENV, CALYX_AI_ADDON_REQUIRED_UNAVAILABLE};
use litd_engine::calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource};
use litd_engine::calyx_content_runtime::open_canonical_engine_content_store;
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_engine::rpg::persist_and_lower_rpg_authored_content;
use litd_hash::sha256_hex;
use serde_json::json;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_ai_addon_env<T>(
    value: Option<&str>,
    body: impl FnOnce() -> Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    let lock = ENV_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| "LITD_CALYX_AI_ADDON env lock poisoned")?;
    // Rust 2024: env mutation is unsafe; serialized behind ENV_LOCK for this FSV.
    let previous = std::env::var_os(CALYX_AI_ADDON_ENV);
    unsafe {
        match value {
            Some(v) => std::env::set_var(CALYX_AI_ADDON_ENV, v),
            None => std::env::remove_var(CALYX_AI_ADDON_ENV),
        }
    }
    let result = body();
    unsafe {
        match previous {
            Some(v) => std::env::set_var(CALYX_AI_ADDON_ENV, v),
            None => std::env::remove_var(CALYX_AI_ADDON_ENV),
        }
    }
    result
}

fn sample_record() -> Result<EngineContentRecord, Box<dyn Error>> {
    let id = "area-ai-addon-gate-fsv";
    let payload = json!({
        "schema": "litd.rpg.area.v1",
        "surface_kind": "area_region",
        "id": id,
        "region_name": "AI Addon Gate Vale",
        "terrain_tag": "ashen_road",
        "acoustics": "muffled_open_ash",
        "description": "FSV terrain row for LITD_CALYX_AI_ADDON gate proof."
    });
    Ok(EngineContentRecord {
        kind: EngineContentKind::Terrain,
        id: id.to_string(),
        display_name: "AI Addon Gate FSV".to_string(),
        source: EngineContentSource {
            path: "editor://areas/ai-addon-gate-fsv".to_string(),
            author: "issue1792_ai_addon_gate_fsv".to_string(),
            source_hash: sha256_hex(&serde_json::to_vec(&payload)?),
        },
        payload,
        anchors: Vec::new(),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1792_ai_addon_gate")?;

    let absent_ok = with_ai_addon_env(Some("absent"), || {
        let store = open_canonical_engine_content_store()?;
        let authored = persist_and_lower_rpg_authored_content(&store, &[sample_record()?])?;
        Ok(authored.row_count == 1 && !authored.entities.is_empty())
    })?;

    let require_err = with_ai_addon_env(Some("require"), || {
        let store = open_canonical_engine_content_store()?;
        match persist_and_lower_rpg_authored_content(&store, &[sample_record()?]) {
            Ok(_) => Ok("SILENT_SUCCESS".to_string()),
            Err(error) => Ok(error.to_string()),
        }
    })?;
    let require_failed_loud = require_err.contains(CALYX_AI_ADDON_REQUIRED_UNAVAILABLE)
        || require_err.contains("ai_addon_require");
    let require_not_silent = require_err != "SILENT_SUCCESS";

    let verdict = absent_ok && require_failed_loud && require_not_silent;
    let report = json!({
        "schema": "litd.fsv.issue1792_ai_addon_gate.v1",
        "issue": 1792,
        "parents": [1790, 1791],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "absent_persist_ok": absent_ok,
        "require_error": require_err,
        "require_failed_loud": require_failed_loud,
        "require_not_silent_success": require_not_silent,
        "env": CALYX_AI_ADDON_ENV,
        "required_unavailable_code": CALYX_AI_ADDON_REQUIRED_UNAVAILABLE,
        "sufficiency_argument": "Force-absent path authors through open_canonical_engine_content_store + persist_and_lower without ORT. Require mode on unmaterialized panel fails with LITD_CALYX_AI_ADDON_REQUIRED_UNAVAILABLE — never silent pending success.",
    });
    let written = run.write_report(&report)?;
    let report_sha = sha256_hex(&std::fs::read(&written.path)?);
    println!(
        "ISSUE1792_AI_ADDON_GATE_FSV verdict={} report={} sha256={} absent_ok={} require_loud={}",
        if verdict { "GREEN" } else { "RED" },
        written.path.display(),
        report_sha,
        absent_ok,
        require_failed_loud,
    );
    if !verdict {
        return Err("ISSUE1792 AI addon gate FSV verdict RED".into());
    }
    Ok(())
}
