//! FSV: child-process stdio JSON-RPC sidecar to the calyx-mcp addon (#1792 / #1790).
//!
//! Proves the two doctrine states with an independent readback:
//!   1. Addon ABSENT  -> availability probe greys out with a reason code, NEVER errors;
//!      Ward gate off-by-vacuity is stated.
//!   2. Addon PRESENT -> the SAME JSON-RPC surface answers LIVE over stdio: tools/list
//!      matches the in-process McpServer surface field-for-field (parity), and a
//!      read-only tools/call round-trips with its JSON-RPC error shape preserved.
//!
//! Independent verification: the addon is a genuinely separate OS process (spawned
//! calyx-mcp.exe), so tools/list here is a real cross-implementation readback, not the
//! in-process self-comparison the loopback path could only ever do. The verdict artifact
//! is written to target/fsv/ and read back before the run is declared green.
//!
//! Env: LITD_CALYX_MCP_BIN must point at the installed calyx-mcp.exe (the addon). This
//! mirrors production: the base editor resolves the addon the same way.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{env, fs};

use litd_editor_mcp::calyx_sidecar::{
    CALYX_MCP_BIN_ENV, CalyxSidecar, CalyxSidecarConfig, SIDECAR_BINARY_MISSING, SIDECAR_ONLINE,
    probe_calyx_sidecar_availability,
};
use litd_editor_mcp::mcp_tools::default_engine_calyx_mcp_surface_manifest;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addon = env::var_os(CALYX_MCP_BIN_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            format!("{CALYX_MCP_BIN_ENV} must point at the installed calyx-mcp addon binary")
        })?;
    if !addon.is_file() {
        return Err(format!("addon binary not found at {}", addon.display()).into());
    }

    // --- State 1: addon ABSENT -> greyed with reason, no error. ---
    let bad_path = PathBuf::from("Z:/no/such/calyx-mcp-addon-issue1792.exe");
    let absent = probe_calyx_sidecar_availability(&CalyxSidecarConfig::with_binary(&bad_path));
    if absent.available
        || absent.reason_code != SIDECAR_BINARY_MISSING
        || !absent.ward_off_by_vacuity
        || absent.tool_count != 0
    {
        return Err(format!("absent probe did not grey out as expected: {absent:?}").into());
    }

    // --- State 2: addon PRESENT -> live over stdio. ---
    let present = probe_calyx_sidecar_availability(&CalyxSidecarConfig::with_binary(&addon));
    if !present.available || present.reason_code != SIDECAR_ONLINE || present.tool_count == 0 {
        return Err(format!("present probe did not go online: {present:?}").into());
    }

    // Parity: the spawned addon's tools/list equals the in-process surface, field-for-field.
    let mut sidecar = CalyxSidecar::spawn(&CalyxSidecarConfig::with_binary(&addon))?;
    let live_tools = sidecar.tools_list()?;
    let live_map: BTreeMap<String, Value> = live_tools
        .iter()
        .map(|tool| (tool.name.clone(), tool.input_schema.clone()))
        .collect();

    let manifest = default_engine_calyx_mcp_surface_manifest()?;
    let inproc_map: BTreeMap<String, Value> = manifest
        .tools
        .iter()
        .map(|tool| (tool.tool_name.clone(), tool.input_schema.clone()))
        .collect();

    let live_names: Vec<String> = live_map.keys().cloned().collect();
    let inproc_names: Vec<String> = inproc_map.keys().cloned().collect();
    if live_names != inproc_names {
        return Err(format!(
            "tool-name set diverged: live={live_names:?} in-process={inproc_names:?}"
        )
        .into());
    }
    for (name, live_schema) in &live_map {
        let inproc_schema = &inproc_map[name];
        if live_schema != inproc_schema {
            return Err(format!(
                "input-schema diverged for {name}:\n  sidecar={live_schema}\n  in-proc={inproc_schema}"
            )
            .into());
        }
    }
    let live_schema_fp = fingerprint_schema_map(&live_map);
    let inproc_schema_fp = fingerprint_schema_map(&inproc_map);
    if live_schema_fp != inproc_schema_fp {
        return Err(
            "tool input-schema fingerprints diverged between sidecar and in-process".into(),
        );
    }

    // Edge B: live read-only tools/call preserves the JSON-RPC error shape (no side effect).
    let bad_call = sidecar.call_tool(
        "calyx.abundance",
        json!({"vault": "issue1792_absent_vault"}),
    )?;
    let error_preserved = bad_call.error.is_some();
    let bad_call_error_code = bad_call.error.as_ref().map(|e| e.code);
    if !error_preserved {
        return Err(format!(
            "read-only tools/call on a missing vault should return a JSON-RPC error: {bad_call:?}"
        )
        .into());
    }

    // Edge C: a second sequential request on the SAME long-lived process id-correlates and
    // succeeds -- proves framing has no desync across calls (before=1 call, after=2 calls).
    let live_tools_2 = sidecar.tools_list()?;
    if live_tools_2.len() != live_tools.len() {
        return Err("second tools/list on the same process desynced".into());
    }
    drop(sidecar); // clean shutdown: closing stdin gives the addon EOF.

    let addon_bytes = fs::metadata(&addon)?.len();
    let report = json!({
        "schema": "litd.editor_mcp.issue1792.calyx_sidecar_fsv.v1",
        "issue": 1792,
        "parent_issue": 1790,
        "transport": "child_process_stdio_jsonrpc",
        "addon_binary": addon.display().to_string(),
        "addon_binary_bytes": addon_bytes,
        "source_of_truth": {
            "absent": "calyx-mcp sidecar spawn/handshake probe (binary missing)",
            "present": "spawned calyx-mcp child-process tools/list over stdio JSON-RPC",
            "parity": "in-process McpServer surface manifest tool set + input-schema fingerprint",
            "live_error": "read-only tools/call round-trip returning a preserved JSON-RPC error"
        },
        "state_1_absent": {
            "available": absent.available,
            "reason_code": absent.reason_code,
            "reason": absent.reason,
            "ward_off_by_vacuity": absent.ward_off_by_vacuity,
            "greyed_panel_disabled_reason": format!(
                "AI features unavailable: {} ({})", absent.reason, absent.reason_code
            )
        },
        "state_2_present": {
            "available": present.available,
            "reason_code": present.reason_code,
            "tool_count": present.tool_count
        },
        "parity": {
            "tool_count": live_names.len(),
            "tool_names": live_names,
            "sidecar_schema_fingerprint": live_schema_fp,
            "in_process_schema_fingerprint": inproc_schema_fp,
            "identical_surface": live_schema_fp == inproc_schema_fp
        },
        "edge_b_live_error_shape": {
            "tool": "calyx.abundance",
            "jsonrpc_error_preserved": error_preserved,
            "error_code": bad_call_error_code
        },
        "edge_c_sequential_framing": {
            "first_call_tools": live_tools.len(),
            "second_call_tools": live_tools_2.len(),
            "no_desync": live_tools.len() == live_tools_2.len()
        }
    });

    let out_dir = artifact_dir()?;
    fs::create_dir_all(&out_dir)?;
    let report_path = out_dir.join("issue1792-calyx-sidecar-fsv.json");
    let bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &bytes)?;
    let disk: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    if disk != report {
        return Err("report readback mismatch: on-disk artifact != in-memory verdict".into());
    }
    let sha = litd_hash::sha256_hex(&bytes);
    println!(
        "ISSUE1792_CALYX_SIDECAR_FSV verdict=PASS report={} sha256={} tools={} addon_bytes={} absent_reason={} present_reason={}",
        report_path.display(),
        sha,
        live_names.len(),
        addon_bytes,
        absent.reason_code,
        present.reason_code
    );
    Ok(())
}

fn fingerprint_schema_map(map: &BTreeMap<String, Value>) -> String {
    // BTreeMap keys are sorted and serde_json::Value objects serialize with sorted keys, so
    // this canonical serialization is deterministic across the two implementations.
    let canonical: Vec<(String, String)> = map
        .iter()
        .map(|(name, schema)| (name.clone(), schema.to_string()))
        .collect();
    litd_hash::sha256_hex(format!("{canonical:?}").as_bytes())
}

fn artifact_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(root) = env::var_os("LITD_CALYX_SIDECAR_FSV_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let target = match env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target"),
    };
    Ok(target.join("fsv").join("issue1792_calyx_sidecar"))
}
