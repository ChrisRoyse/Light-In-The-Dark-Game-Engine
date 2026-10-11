use std::fs;

use litd_editor::mcp_agent_tools::{
    BIND_AGENT_TOOL, READ_AGENTS_TOOL, SET_PROMPT_MODEL_TOOL, SET_WARD_ENVELOPE_TOOL,
    call_mcp_agent_tool, read_mcp_agent_state,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue924Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_AGENT_TOOLS_FSV_ROOT",
        "issue924_mcp_agent_tools",
        "manual",
    )?;

    let state_path = root.join("agent-state.json");
    let bind_ledger = root.join("bind-ledger.json");
    let prompt_ledger = root.join("prompt-ledger.json");
    let ward_ledger = root.join("ward-ledger.json");
    let report_path = root.join("mcp-agent-tools-fsv-readback.json");
    let before = state(&state_path, &bind_ledger);

    let bind = call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": bind_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-bind-ledger",
            "request_id": "issue924-bind",
            "requested_by": "litd-editor-issue924-fsv",
            "binding": sample_binding()
        }),
    )?;
    let prompt = call_mcp_agent_tool(
        SET_PROMPT_MODEL_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": prompt_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-prompt-ledger",
            "request_id": "issue924-prompt",
            "requested_by": "litd-editor-issue924-fsv",
            "prompt_model": sample_prompt_model()
        }),
    )?;
    let ward = call_mcp_agent_tool(
        SET_WARD_ENVELOPE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ward_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-ward-ledger",
            "request_id": "issue924-ward",
            "requested_by": "litd-editor-issue924-fsv",
            "envelope": sample_ward_envelope()
        }),
    )?;
    let read = call_mcp_agent_tool(READ_AGENTS_TOOL, json!({ "state_path": state_path }))?;
    // The state/ledger paths are Aster vault DIRECTORIES (CalyxJsonStoreSpec.vault_dir), so a
    // flat fs::read of them fails with PermissionDenied on Windows (#1525). The store already
    // computes the sha256 of the real persisted+re-read row bytes and returns it in each
    // readback — that IS the physical source-of-truth hash, so use it directly.
    let state_sha256 = read
        .state_readback
        .as_ref()
        .ok_or("READ_AGENTS must return a state readback")?
        .sha256
        .clone();
    let bind_ledger_sha256 = bind
        .ledger_readback
        .as_ref()
        .ok_or("BIND_AGENT must return a ledger readback")?
        .sha256
        .clone();
    let prompt_ledger_sha256 = prompt
        .ledger_readback
        .as_ref()
        .ok_or("SET_PROMPT_MODEL must return a ledger readback")?
        .sha256
        .clone();
    let ward_ledger_sha256 = ward
        .ledger_readback
        .as_ref()
        .ok_or("SET_WARD_ENVELOPE must return a ledger readback")?
        .sha256
        .clone();
    let after = state(&state_path, &bind_ledger);
    let edges = vec![
        edge_missing_agent(&root)?,
        edge_teleport_envelope(&root, &state_path)?,
        edge_duplicate_binding(&root, &state_path)?,
        edge_signal_without_ref(&root, &state_path)?,
    ];
    let fsv = Issue924Fsv {
        issue: 924,
        source_of_truth: json!({
            "agent_state": state_path,
            "bind_ledger": bind_ledger,
            "prompt_ledger": prompt_ledger,
            "ward_ledger": ward_ledger
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "bind_agent": bind,
            "set_prompt_model": prompt,
            "set_ward_envelope": ward,
            "read_agents": read
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    if read_json::<Issue924Fsv>(&report_path)? != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue924 mcp_agent_tools_fsv report={} report_sha256={} state_sha256={} bind_ledger_sha256={} prompt_ledger_sha256={} ward_ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        state_sha256,
        bind_ledger_sha256,
        prompt_ledger_sha256,
        ward_ledger_sha256,
    );
    Ok(())
}

fn edge_missing_agent(root: &std::path::Path) -> Result<Value, Box<dyn std::error::Error>> {
    let state_path = root.join("edge-missing-agent-state.json");
    let ledger = root.join("edge-missing-agent-ledger.json");
    let before = state(&state_path, &ledger);
    let error = call_mcp_agent_tool(
        SET_PROMPT_MODEL_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue924-edge-state",
            "ledger_id": "issue924-edge-missing-agent",
            "request_id": "issue924-edge-missing-agent",
            "requested_by": "litd-editor-issue924-fsv",
            "prompt_model": sample_prompt_model()
        }),
    )
    .unwrap_err();
    edge_report(
        "missing_bound_agent",
        before,
        state(&state_path, &ledger),
        error,
    )
}

fn edge_teleport_envelope(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-teleport-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_agent_tool(
        SET_WARD_ENVELOPE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-edge-teleport",
            "request_id": "issue924-edge-teleport",
            "requested_by": "litd-editor-issue924-fsv",
            "envelope": bad_teleport_envelope()
        }),
    )
    .unwrap_err();
    edge_report(
        "teleport_outside_ward",
        before,
        state(state_path, &ledger),
        error,
    )
}

fn edge_duplicate_binding(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-duplicate-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-edge-duplicate",
            "request_id": "issue924-edge-duplicate",
            "requested_by": "litd-editor-issue924-fsv",
            "binding": sample_binding()
        }),
    )
    .unwrap_err();
    edge_report(
        "duplicate_agent_binding",
        before,
        state(state_path, &ledger),
        error,
    )
}

fn edge_signal_without_ref(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-signal-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-edge-signal",
            "request_id": "issue924-edge-signal",
            "requested_by": "litd-editor-issue924-fsv",
            "binding": bad_signal_binding()
        }),
    )
    .unwrap_err();
    edge_report(
        "signal_without_ref",
        before,
        state(state_path, &ledger),
        error,
    )
}

fn edge_report(
    name: &str,
    before: Value,
    after: Value,
    error: litd_engine::EngineError,
) -> Result<Value, Box<dyn std::error::Error>> {
    if before != after {
        return Err(format!("edge {name} changed source-of-truth state").into());
    }
    Ok(json!({
        "name": name,
        "state_unchanged": true,
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn sample_binding() -> Value {
    json!({
        "binding_id": "binding.guardian_ember",
        "agent_id": "agent.guardian_ember",
        "object_id": "npc.guardian_ember",
        "display_name": "Guardian Ember",
        "deterministic_order_sink": "orders.guardian_ember",
        "activation": {
            "mode": "signal",
            "signal_refs": ["signal.region_entry.watchtower"],
            "cooldown_ticks": 30
        },
        "enabled": true
    })
}

fn sample_prompt_model() -> Value {
    json!({
        "agent_id": "agent.guardian_ember",
        "prompt": "Protect the watchtower, greet trusted travelers, and lower every action to patrol or dialog orders.",
        "model_id": "model.tiny_guardian_7b",
        "personality": "curious_guardian",
        "memory_scope_refs": ["memory.watchtower", "lore.ember_oath"]
    })
}

fn sample_ward_envelope() -> Value {
    json!({
        "agent_id": "agent.guardian_ember",
        "physics": {
            "max_move_units_per_tick": 320,
            "allowed_region_ids": ["region.watchtower", "region.market_gate"],
            "allow_teleport": false
        },
        "visual": {
            "allowed_effect_ids": ["vfx.aura_guardian", "vfx.dialog_glow"],
            "max_scale_milli": 1200,
            "max_emissive_milli": 850
        },
        "behavior": {
            "allowed_intents": ["intent.patrol", "intent.dialog", "intent.alert_guard"],
            "forbidden_intents": ["intent.attack_civilian"],
            "grounding_refs": ["ward.guardian_ember", "calyx.anchor.watchtower"]
        }
    })
}

fn bad_teleport_envelope() -> Value {
    let mut value = sample_ward_envelope();
    value["physics"]["allow_teleport"] = json!(true);
    value
}

fn bad_signal_binding() -> Value {
    let mut value = sample_binding();
    value["binding_id"] = json!("binding.bad_signal");
    value["agent_id"] = json!("agent.bad_signal");
    value["object_id"] = json!("npc.bad_signal");
    value["activation"]["signal_refs"] = json!([]);
    value
}

fn state(state_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
    let readback = if state_path.exists() {
        Some(read_mcp_agent_state(state_path).unwrap())
    } else {
        None
    };
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "binding_count": readback.as_ref().map(|readback| readback.binding_count).unwrap_or(0),
        "prompt_model_count": readback.as_ref().map(|readback| readback.prompt_model_count).unwrap_or(0),
        "ward_envelope_count": readback.as_ref().map(|readback| readback.ward_envelope_count).unwrap_or(0),
        "enabled_binding_count": readback.as_ref().map(|readback| readback.enabled_binding_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    let bytes = serde_json::to_vec_pretty(value)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &std::path::Path) -> Result<T, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}
