use std::{env, fs, path::Path};

use calyx_mcp::{JsonRpcId, JsonRpcRequest, McpServer, tools::register_all};
use litd_editor::{
    history::stable_fingerprint,
    mcp_tools::{
        build_structured_intelligence_mcp_surface_report, call_engine_calyx_mcp_tool,
        default_structured_intelligence_mcp_surface_manifest,
    },
};
use litd_engine::calyx_paths::canonical_gameeditor_calyx_root;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[path = "issue1419_structured_mcp_surface_fsv/support.rs"]
mod support;

use support::{
    CalyxHomeScope, first_base_readback, read_json, summarize_value, vault_state,
    vault_state_for_happy, write_json,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue1419Fsv {
    issue: u32,
    source_of_truth: String,
    manifest: Value,
    report: Value,
    tools_list: Value,
    happy_path: Value,
    edge_cases: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, fsv_root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1419_FSV_ROOT",
        "issue1419_structured_mcp_surface",
        "manual",
    )?;
    let artifact_relative =
        env::var("LITD_ISSUE1419_FSV").unwrap_or_else(|_| "manual-fsv-report.json".to_string());
    let artifact_path = fsv_run.artifact_path(&artifact_relative)?;
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let calyx_home =
        canonical_gameeditor_calyx_root()?.join("tmp/fsv/issue1419-structured-mcp-surface");
    let _ = fs::remove_dir_all(&calyx_home);
    fs::create_dir_all(&calyx_home)?;
    let _home = CalyxHomeScope::set(&calyx_home);

    let manifest = default_structured_intelligence_mcp_surface_manifest()?;
    let report = build_structured_intelligence_mcp_surface_report(&manifest)?;
    let tools_list = tools_list_readback()?;
    let happy_path = happy_path(&fsv_root, &calyx_home)?;
    let edge_cases = vec![
        edge_measure_missing_structured_field(&calyx_home, &happy_path)?,
        edge_ingest_record_and_batch(&calyx_home, &happy_path)?,
        edge_unknown_tool_preserves_state(&calyx_home, &happy_path)?,
    ];

    let fsv = Issue1419Fsv {
        issue: 1419,
        source_of_truth:
            "MCP tools/list, MCP tools/call, and direct durable Aster Base/Assay/Kernel/Guard/Ledger CF byte readback; Synapse tools were unavailable in this session"
                .to_string(),
        manifest: serde_json::to_value(manifest)?,
        report: serde_json::to_value(report)?,
        tools_list,
        happy_path,
        edge_cases,
    };
    write_json(&artifact_path, &fsv)?;
    let readback = read_json::<Issue1419Fsv>(&artifact_path)?;
    if readback != fsv {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let recorded = fsv_run.record_json_file(&artifact_path)?;
    drop(_home);
    fs::remove_dir_all(&calyx_home)?;
    println!(
        "{}",
        json!({
            "issue": 1419,
            "artifact_path": artifact_path,
            "artifact_sha256": recorded.sha256,
            "family_count": fsv.report["family_count"],
            "required_tool_count": fsv.report["required_tool_count"],
            "tools_list_count": fsv.tools_list["tool_count"],
            "base_rows": fsv.happy_path["after_ingest"]["base"]["count"],
            "assay_rows": fsv.happy_path["after_bits"]["assay"]["count"],
            "kernel_rows": fsv.happy_path["after_kernel"]["kernel"]["count"],
            "guard_rows": fsv.happy_path["after_guard"]["guard"]["count"],
            "edge_case_count": fsv.edge_cases.len(),
            "artifact_readback_matches": true,
        })
    );
    Ok(())
}

fn happy_path(fsv_root: &Path, calyx_home: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let vault_name = "issue1419_structured";
    let create = call_ok(
        "calyx.create_vault",
        json!({
            "name": vault_name
        }),
    )?;
    let vault_id = create["vault_id"].as_str().ok_or("missing vault_id")?;
    let vault_path = calyx_home.join("vaults").join(vault_id);
    let before_panel = call_ok("calyx.list_panel", json!({ "vault": vault_name }))?;
    for slot in before_panel["slots"]
        .as_array()
        .ok_or("panel slots missing")?
    {
        if slot["state"] == "active" {
            call_ok(
                "calyx.park_lens",
                json!({
                    "vault": vault_name,
                    "slot": slot["slot"],
                }),
            )?;
        }
    }
    let add_lens = call_ok(
        "calyx.add_lens",
        json!({
            "vault": vault_name,
            "name": "summary.text_embedding",
            "runtime": "algorithmic",
            "modality": "text",
        }),
    )?;
    let panel = call_ok("calyx.list_panel", json!({ "vault": vault_name }))?;
    let active_slot = active_slot(&panel)?;

    let before_measure = vault_state(&vault_path, vault_id, vault_name)?;
    let measured = call_ok(
        "calyx.measure",
        json!({
            "vault": vault_name,
            "input": structured_record_text(0, true),
        }),
    )?;
    let after_measure = vault_state(&vault_path, vault_id, vault_name)?;
    if before_measure["base"]["count"] != after_measure["base"]["count"] {
        return Err("calyx.measure wrote Base CF rows".into());
    }

    let records = (0..60)
        .map(|idx| structured_record_text(idx, idx < 30))
        .collect::<Vec<_>>();
    let ingested = call_ok(
        "calyx.ingest",
        json!({
            "vault": vault_name,
            "batch": records,
        }),
    )?;
    let cx_ids = ingested["results"]
        .as_array()
        .ok_or("ingest results missing")?
        .iter()
        .map(|result| {
            result["cx_id"]
                .as_str()
                .ok_or("ingest result cx_id not string")
                .map(ToString::to_string)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (index, cx_id) in cx_ids.iter().enumerate() {
        call_ok(
            "calyx.anchor",
            json!({
                "vault": vault_name,
                "cx_id": cx_id,
                "kind": "test_pass",
                "value": index < 30,
                "source": "issue1419",
                "confidence": 1.0,
            }),
        )?;
    }
    let after_ingest = vault_state(&vault_path, vault_id, vault_name)?;
    let first_base = first_base_readback(&vault_path, vault_id, vault_name, &cx_ids[0])?;

    let guard_before = call_error(
        "calyx.guard.check",
        json!({
            "vault": vault_name,
            "cx_id": cx_ids[0],
        }),
    )?;
    let before_bits = vault_state(&vault_path, vault_id, vault_name)?;
    let abundance = call_ok("calyx.abundance", json!({ "vault": vault_name }))?;
    let bits = call_ok(
        "calyx.bits",
        json!({
            "vault": vault_name,
            "anchor": "test_pass",
            "explain": true,
        }),
    )?;
    let after_bits = vault_state(&vault_path, vault_id, vault_name)?;
    let kernel = call_ok(
        "calyx.kernel",
        json!({
            "vault": vault_name,
            "anchor": "test_pass",
            "rebuild": true,
        }),
    )?;
    let after_kernel = vault_state(&vault_path, vault_id, vault_name)?;
    let neighbors = call_ok(
        "calyx.neighbors",
        json!({
            "vault": vault_name,
            "cx_id": cx_ids[0],
            "k": 3,
        }),
    )?;
    let answer = call_ok(
        "calyx.kernel_answer",
        json!({
            "vault": vault_name,
            "query": "victory profile",
            "anchor": "test_pass",
        }),
    )?;
    let calibration = fsv_root.join("guard-calibration.jsonl");
    write_calibration(&calibration, active_slot)?;
    let guard_profile = call_ok(
        "calyx.guard.calibrate",
        json!({
            "vault": vault_name,
            "domain": "issue1419",
            "set": calibration,
            "target_far": 0.01,
        }),
    )?;
    let guard_check = call_ok(
        "calyx.guard.check",
        json!({
            "vault": vault_name,
            "cx_id": cx_ids[0],
        }),
    )?;
    let after_guard = vault_state(&vault_path, vault_id, vault_name)?;

    let expected_base_rows = cx_ids.len() as u64;
    if after_ingest["base"]["count"] != expected_base_rows {
        return Err("Base CF row count did not match ingested cx_ids".into());
    }
    if after_bits["assay"]["count"].as_u64().unwrap_or(0) < 1 {
        return Err("bits did not persist an Assay CF row".into());
    }
    if after_kernel["kernel"]["count"].as_u64().unwrap_or(0) < 1 {
        return Err("kernel did not persist a Kernel CF row".into());
    }
    if after_guard["guard"]["count"].as_u64().unwrap_or(0) < 1 {
        return Err("guard calibration did not persist a Guard CF row".into());
    }

    Ok(json!({
        "vault_name": vault_name,
        "vault_id": vault_id,
        "vault_path": vault_path,
        "create": summarize_value(&create),
        "parked_initial_slot_count": before_panel["slots"].as_array().unwrap().len(),
        "add_lens": add_lens,
        "active_slot": active_slot,
        "measure": summarize_value(&measured),
        "before_measure": before_measure,
        "after_measure": after_measure,
        "ingest": {
            "cx_count": cx_ids.len(),
            "result_count": ingested["results"].as_array().map(|items| items.len()),
            "anchor_count": cx_ids.len(),
        },
        "first_base": first_base,
        "after_ingest": after_ingest,
        "guard_before_calibration": guard_before,
        "before_bits": before_bits,
        "abundance": abundance,
        "bits": bits,
        "after_bits": after_bits,
        "kernel": kernel,
        "after_kernel": after_kernel,
        "neighbors": neighbors,
        "kernel_answer": answer,
        "guard_profile": guard_profile,
        "guard_check": guard_check,
        "after_guard": after_guard,
        "actual_expected": {
            "base_rows_match_cx_ids": after_ingest["base"]["count"] == expected_base_rows,
            "measure_base_unchanged": before_measure["base"]["count"] == after_measure["base"]["count"],
            "assay_row_persisted": after_bits["assay"]["count"].as_u64().unwrap_or(0) >= 1,
            "kernel_row_persisted": after_kernel["kernel"]["count"].as_u64().unwrap_or(0) >= 1,
            "guard_row_persisted": after_guard["guard"]["count"].as_u64().unwrap_or(0) >= 1,
        },
    }))
}

fn edge_measure_missing_structured_field(
    calyx_home: &Path,
    happy: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = vault_state_for_happy(calyx_home, happy)?;
    let response = call_error(
        "calyx.measure",
        json!({
            "vault": happy["vault_name"],
            "input": "",
        }),
    )?;
    let after = vault_state_for_happy(calyx_home, happy)?;
    Ok(json!({
        "name": "measure_empty_input",
        "response": response,
        "before": before,
        "after": after,
        "state_unchanged": before == after,
    }))
}

fn edge_ingest_record_and_batch(
    calyx_home: &Path,
    happy: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = vault_state_for_happy(calyx_home, happy)?;
    let response = call_error(
        "calyx.ingest",
        json!({
            "vault": happy["vault_name"],
            "input": structured_record_text(100, true),
            "batch": [structured_record_text(101, false)],
        }),
    )?;
    let after = vault_state_for_happy(calyx_home, happy)?;
    Ok(json!({
        "name": "ingest_input_and_batch_invalid_params",
        "response": response,
        "before": before,
        "after": after,
        "state_unchanged": before == after,
    }))
}

fn edge_unknown_tool_preserves_state(
    calyx_home: &Path,
    happy: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = vault_state_for_happy(calyx_home, happy)?;
    let response = call_error("calyx.structured_ghost", json!({}))?;
    let after = vault_state_for_happy(calyx_home, happy)?;
    Ok(json!({
        "name": "unknown_tool",
        "response": response,
        "before": before,
        "after": after,
        "state_unchanged": before == after,
    }))
}

fn structured_record(index: usize, passed: bool) -> Value {
    let unit_id = format!("unit_{index:03}");
    let summary = if passed {
        format!("victory profile durable frontline unit {index}")
    } else {
        format!("loss profile fragile scout unit {index}")
    };
    json!({
        "entity_type": "game_unit",
        "fields": {
            "unit_id": {"text": unit_id},
            "summary": {"text": summary},
            "hit_points": {"f64": if passed { 120.0 } else { 30.0 }},
            "attack_damage": {"f64": if passed { 16.0 } else { 2.0 }},
            "outcome.anchor_kind": {"text": "test_pass"},
            "outcome.value": {"bool": passed},
            "outcome.source": {"text": "issue1419"},
            "outcome.confidence": {"f64": 1.0},
            "outcome.observed_at": {"u64": 141900},
        }
    })
}

fn structured_record_text(index: usize, passed: bool) -> String {
    let record = structured_record(index, passed);
    serde_json::to_string(&record).expect("structured record is JSON serializable")
}

fn tools_list_readback() -> Result<Value, Box<dyn std::error::Error>> {
    let mut server = McpServer::new();
    register_all(&mut server)?;
    let response = server.dispatch(JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "tools/list".to_string(),
        params: None,
        id: Some(JsonRpcId::Number(1419)),
    });
    let tools = response
        .result
        .as_ref()
        .ok_or("tools/list missing result")?["tools"]
        .as_array()
        .ok_or("tools/list tools missing")?;
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    let expected = litd_editor::mcp_tools::expected_calyx_mcp_tool_names()?;
    Ok(json!({
        "tool_count": tools.len(),
        "names": names,
        "expected_names": expected,
        "expected_match": names == expected,
        "fingerprint": stable_fingerprint(&names)?,
    }))
}

fn call_ok(tool: &str, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let response = call_engine_calyx_mcp_tool(tool, arguments)?;
    if let Some(error) = response.error {
        return Err(format!("{tool} returned error: {error:?}").into());
    }
    tool_payload(response.result.as_ref().ok_or("tool missing result")?)
}

fn call_error(tool: &str, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let response = call_engine_calyx_mcp_tool(tool, arguments)?;
    let error = response
        .error
        .ok_or_else(|| format!("{tool} unexpectedly succeeded"))?;
    Ok(serde_json::to_value(error)?)
}

fn tool_payload(result: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let text = result["content"][0]["text"]
        .as_str()
        .ok_or("tool result text missing")?;
    Ok(serde_json::from_str(text)?)
}

fn active_slot(panel: &Value) -> Result<u16, Box<dyn std::error::Error>> {
    panel["slots"]
        .as_array()
        .ok_or("panel slots missing")?
        .iter()
        .find(|slot| slot["state"] == "active")
        .and_then(|slot| slot["slot"].as_u64())
        .map(|slot| slot as u16)
        .ok_or_else(|| "no active slot".into())
}

fn write_calibration(path: &Path, slot: u16) -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    for _ in 0..50 {
        text.push_str(&format!(r#"{{"slot":{slot},"score":0.99,"class":"good"}}"#));
        text.push('\n');
        text.push_str(&format!(
            r#"{{"slot":{slot},"score":0.10,"class":"injection"}}"#
        ));
        text.push('\n');
    }
    fs::write(path, text)?;
    Ok(())
}
