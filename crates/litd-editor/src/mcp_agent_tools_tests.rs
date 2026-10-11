use std::{fs, path::Path};

use serde_json::{Value, json};

use super::*;

#[test]
fn issue860_author_agent_no_code_tool_wires_prompt_model_object_visual_and_ward() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue860_no_code_agent_authoring", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("agent-state.json");
    let ledger_path = root.join("author-ledger.json");
    let before = state(&state_path, &ledger_path);
    println!("issue860 before {before}");

    let authored = call_mcp_agent_tool(
        AUTHOR_AGENT_TOOL,
        sample_author_agent(&state_path, &ledger_path, "issue860-author"),
    )
    .unwrap();
    assert_eq!(authored.tool_name, AUTHOR_AGENT_TOOL);
    assert_eq!(authored.result["authored_agent"]["model_tier"], "tiny");
    assert_eq!(
        authored.result["authored_agent"]["model_id"],
        "model.local.tiny_agent"
    );
    assert_eq!(authored.result["authored_agent"]["ward_auto_wired"], true);

    let read = call_mcp_agent_tool(READ_AGENTS_TOOL, json!({ "state_path": state_path })).unwrap();
    let agent_state = &read.state_readback.as_ref().unwrap().state;
    assert_eq!(agent_state.bindings[0].object_id, "npc.friendly_warden");
    assert_eq!(
        agent_state.bindings[0].deterministic_order_sink,
        "orders.friendly_warden"
    );
    assert_eq!(
        agent_state.prompt_models[0].prompt,
        "Guard the lane, greet travelers, and act only through deterministic game orders."
    );
    assert_eq!(
        agent_state.prompt_models[0].model_id,
        "model.local.tiny_agent"
    );
    assert_eq!(
        agent_state.ward_envelopes[0].behavior.allowed_intents,
        ["intent.attack", "intent.chase"]
    );
    assert!(
        agent_state.ward_envelopes[0]
            .behavior
            .grounding_refs
            .contains(&"vfx.agent_brain_binding".to_string())
    );
    let after = state(&state_path, &ledger_path);
    println!("issue860 after {after}");
    assert_eq!(after["binding_count"], 1);
    assert_eq!(after["prompt_model_count"], 1);
    assert_eq!(after["ward_envelope_count"], 1);
    assert_eq!(after["ledger_exists"], true);
}

#[test]
fn issue860_author_agent_edges_fail_closed_without_state_mutation() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue860_no_code_agent_authoring", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("agent-state.json");
    let ledger_path = root.join("author-ledger.json");

    let mut bad_model = sample_author_agent(&state_path, &ledger_path, "issue860-bad-model");
    bad_model["model_tier"] = json!("huge");
    let before_bad_model = state(&state_path, &ledger_path);
    let bad_model_error = call_mcp_agent_tool(AUTHOR_AGENT_TOOL, bad_model).unwrap_err();
    let after_bad_model = state(&state_path, &ledger_path);
    assert_eq!(before_bad_model, after_bad_model);
    assert!(
        bad_model_error
            .to_string()
            .contains("unsupported model tier")
    );

    let mut bad_preset = sample_author_agent(&state_path, &ledger_path, "issue860-bad-preset");
    bad_preset["behavior_preset"] = json!("teleport_assassin");
    let before_bad_preset = state(&state_path, &ledger_path);
    let bad_preset_error = call_mcp_agent_tool(AUTHOR_AGENT_TOOL, bad_preset).unwrap_err();
    let after_bad_preset = state(&state_path, &ledger_path);
    assert_eq!(before_bad_preset, after_bad_preset);
    assert!(
        bad_preset_error
            .to_string()
            .contains("unsupported behavior preset")
    );

    call_mcp_agent_tool(
        AUTHOR_AGENT_TOOL,
        sample_author_agent(&state_path, &ledger_path, "issue860-seed"),
    )
    .unwrap();
    let before_duplicate = state(&state_path, &ledger_path);
    let duplicate = call_mcp_agent_tool(
        AUTHOR_AGENT_TOOL,
        sample_author_agent(&state_path, &ledger_path, "issue860-duplicate"),
    )
    .unwrap_err();
    let after_duplicate = state(&state_path, &ledger_path);
    assert_eq!(before_duplicate, after_duplicate);
    assert!(duplicate.to_string().contains("already authored"));
}

#[test]
fn issue924_agent_mcp_tools_bind_prompt_ward_and_read_real_state() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue924_mcp_agent_tools", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("agent-state.json");
    let bind_ledger = root.join("bind-ledger.json");
    let prompt_ledger = root.join("prompt-ledger.json");
    let ward_ledger = root.join("ward-ledger.json");
    let before = state(&state_path, &bind_ledger);
    println!("issue924 before {before}");

    let bind = call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": bind_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-bind-ledger",
            "request_id": "issue924-bind",
            "requested_by": "litd-editor-issue924-test",
            "binding": sample_binding()
        }),
    )
    .unwrap();
    assert_eq!(bind.tool_name, BIND_AGENT_TOOL);
    let prompt = call_mcp_agent_tool(
        SET_PROMPT_MODEL_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": prompt_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-prompt-ledger",
            "request_id": "issue924-prompt",
            "requested_by": "litd-editor-issue924-test",
            "prompt_model": sample_prompt_model()
        }),
    )
    .unwrap();
    assert_eq!(prompt.tool_name, SET_PROMPT_MODEL_TOOL);
    let ward = call_mcp_agent_tool(
        SET_WARD_ENVELOPE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ward_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-ward-ledger",
            "request_id": "issue924-ward",
            "requested_by": "litd-editor-issue924-test",
            "envelope": sample_ward_envelope()
        }),
    )
    .unwrap();
    assert_eq!(ward.tool_name, SET_WARD_ENVELOPE_TOOL);

    let read = call_mcp_agent_tool(READ_AGENTS_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.result["state_readback"]["binding_count"], 1);
    assert_eq!(read.result["state_readback"]["prompt_model_count"], 1);
    assert_eq!(read.result["state_readback"]["ward_envelope_count"], 1);
    let after = state(&state_path, &bind_ledger);
    println!("issue924 after {after}");
    assert_eq!(after["binding_count"], 1);
    assert_eq!(after["prompt_model_count"], 1);
    assert_eq!(after["ward_envelope_count"], 1);
}

#[test]
fn issue924_agent_mcp_tools_edges_fail_closed_without_writing() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue924_mcp_agent_tools", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("agent-state.json");
    let edge_ledger = root.join("edge-ledger.json");

    let before_missing_agent = state(&state_path, &edge_ledger);
    let missing_agent = call_mcp_agent_tool(
        SET_PROMPT_MODEL_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-missing-agent",
            "request_id": "issue924-missing-agent",
            "requested_by": "litd-editor-issue924-test",
            "prompt_model": sample_prompt_model()
        }),
    )
    .unwrap_err();
    let after_missing_agent = state(&state_path, &edge_ledger);
    println!(
        "issue924 missing_agent before={before_missing_agent} after={after_missing_agent} error={missing_agent}"
    );
    assert_eq!(before_missing_agent, after_missing_agent);
    assert!(missing_agent.to_string().contains("missing bound agent"));

    seed_agent_state(&root, &state_path);
    let before_teleport = state(&state_path, &edge_ledger);
    let teleport = call_mcp_agent_tool(
        SET_WARD_ENVELOPE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-teleport",
            "request_id": "issue924-teleport",
            "requested_by": "litd-editor-issue924-test",
            "envelope": bad_teleport_envelope()
        }),
    )
    .unwrap_err();
    let after_teleport = state(&state_path, &edge_ledger);
    println!("issue924 teleport before={before_teleport} after={after_teleport} error={teleport}");
    assert_eq!(before_teleport, after_teleport);
    assert!(teleport.to_string().contains("teleport"));

    let before_duplicate = state(&state_path, &edge_ledger);
    let duplicate = call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-duplicate",
            "request_id": "issue924-duplicate",
            "requested_by": "litd-editor-issue924-test",
            "binding": sample_binding()
        }),
    )
    .unwrap_err();
    let after_duplicate = state(&state_path, &edge_ledger);
    println!(
        "issue924 duplicate before={before_duplicate} after={after_duplicate} error={duplicate}"
    );
    assert_eq!(before_duplicate, after_duplicate);
    assert!(duplicate.to_string().contains("already bound"));
}

fn seed_agent_state(root: &Path, state_path: &Path) {
    call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("seed-bind-ledger.json"),
            "state_id": "issue924-agent-state",
            "ledger_id": "issue924-seed-bind",
            "request_id": "issue924-seed-bind",
            "requested_by": "litd-editor-issue924-test",
            "binding": sample_binding()
        }),
    )
    .unwrap();
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

fn sample_author_agent(state_path: &Path, ledger_path: &Path, request_id: &str) -> Value {
    json!({
        "state_path": state_path,
        "ledger_path": ledger_path,
        "state_id": "issue860-agent-state",
        "ledger_id": format!("{request_id}-ledger"),
        "request_id": request_id,
        "requested_by": "noncoder.fsv",
        "agent_id": "agent.friendly_warden",
        "object_id": "npc.friendly_warden",
        "display_name": "Friendly Warden",
        "prompt": "Guard the lane, greet travelers, and act only through deterministic game orders.",
        "model_tier": "tiny",
        "behavior_preset": "guardian_lane",
        "personality": "curious",
        "region_id": "region.lane",
        "activation": {
            "mode": "manual",
            "signal_refs": [],
            "cooldown_ticks": 1
        },
        "visual": {
            "character_ref": "character.friendly_warden",
            "visual_ref": "vfx.agent_brain_binding",
            "particle_ref": "particle.agent_spark"
        }
    })
}

fn bad_teleport_envelope() -> Value {
    let mut value = sample_ward_envelope();
    value["physics"]["allow_teleport"] = json!(true);
    value
}

fn state(state_path: &Path, ledger_path: &Path) -> Value {
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

fn reset_root(root: &Path) {
    // Agent MCP handlers now confine every tool-supplied path within LITD_EDITOR_PROJECT_ROOT
    // (#1518). Fixtures write under the workspace `target/fsv/` tree, which lives inside the
    // workspace root, so pointing the confinement root at the workspace admits every fixture
    // while the rejection path is proven separately by the confinement FSV harness.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
