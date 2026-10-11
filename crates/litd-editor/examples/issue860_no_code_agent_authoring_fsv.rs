use std::fs;

use litd_editor::{
    agent_object_binding::lower_bound_agent_orders, mcp_agent_tools::call_mcp_agent_tool,
};
use litd_engine::{
    ai::load_ai_controller_run_json,
    fsv_artifacts::begin_issue_run,
    nav::NavCellCoord,
    sim_order::SimOrderQueue,
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};
use litd_hash::sha256_hex;
use serde_json::json;

#[path = "support/issue860_no_code_agent_authoring_support.rs"]
mod issue860_no_code_agent_authoring_support;
use issue860_no_code_agent_authoring_support::{
    author_request, call_author_agent, controller_summary, duel_sim, edge_report, line_navmesh,
    manual_activation, perception_for, request_file_readback, run_args,
    signal_activation_without_refs, sim_state, state_summary, step_json, v, write_noncoder_request,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue860_no_code_agent_authoring")?;
    let root = run.run_dir();
    let state_path = root.join("agent-state-aster");
    let author_ledger_path = root.join("author-agent-ledger-aster");
    let request_path = run.artifact_path("noncoder-author-agent-request.json")?;
    let request = write_noncoder_request(&request_path, root, &state_path, &author_ledger_path)?;
    let before_author = state_summary(&state_path);
    let authored = call_author_agent(request.clone())?;
    let after_author = state_summary(&state_path);
    let state_read = call_mcp_agent_tool(
        litd_editor::mcp_agent_tools::READ_AGENTS_TOOL,
        json!({ "state_path": state_path }),
    )?;

    let navmesh = line_navmesh();
    let (mut sim, agent_body, target_body) = duel_sim()?;
    let initial_snapshot = SimSnapshot::capture(&sim);
    let before_activation = sim_state(&sim, agent_body, target_body);

    let args0 = run_args(
        root,
        &state_path,
        "tick0",
        agent_body,
        NavCellCoord::new(0, 0),
    );
    let binding0 = lower_bound_agent_orders(
        &args0,
        &navmesh,
        &perception_for(&sim, agent_body, target_body, NavCellCoord::new(0, 0))?,
        &sim,
    )?;
    let controller0 = load_ai_controller_run_json(&args0.controller_run_path)?;
    let mut queue = SimOrderQueue::<8>::default();
    for order in &controller0.report.orders {
        queue.enqueue(*order)?;
    }
    let step0 = queue.step(&mut sim)?;
    let after_chase = sim_state(&sim, agent_body, target_body);

    let args1 = run_args(
        root,
        &state_path,
        "tick1",
        agent_body,
        NavCellCoord::new(1, 0),
    );
    let binding1 = lower_bound_agent_orders(
        &args1,
        &navmesh,
        &perception_for(&sim, agent_body, target_body, NavCellCoord::new(1, 0))?,
        &sim,
    )?;
    let controller1 = load_ai_controller_run_json(&args1.controller_run_path)?;
    for order in &controller1.report.orders {
        queue.enqueue(*order)?;
    }
    let step1 = queue.step(&mut sim)?;
    let final_snapshot = SimSnapshot::capture(&sim);
    let final_snapshot_path = run.artifact_path("final-sim-snapshot.ron")?;
    fs::write(&final_snapshot_path, final_snapshot.to_ron_string()?)?;
    let final_snapshot_disk =
        SimSnapshot::from_ron_str(&fs::read_to_string(&final_snapshot_path)?)?;

    let mut all_orders = controller0.report.orders.clone();
    all_orders.extend(controller1.report.orders.iter().copied());
    let replay_record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot,
        orders: all_orders,
        run_ticks: 2,
        expected_final_hash: sim.state_hash(),
    };
    let replay_path = run.artifact_path("no-code-agent-activation-replay.ron")?;
    let replay_text = replay_record.to_ron_string()?;
    fs::write(&replay_path, &replay_text)?;
    let replay_bytes = fs::read(&replay_path)?;
    let replay_outcome = SimReplayRecord::from_ron_str(&String::from_utf8(replay_bytes.clone())?)?
        .replay::<4, 8>()?;

    let edges = vec![
        edge_bad_model(root)?,
        edge_bad_preset(root)?,
        edge_bad_activation(root)?,
        edge_empty_prompt(root)?,
        edge_duplicate_object(root)?,
    ];
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let prompt_persisted = state_read
        .state_readback
        .as_ref()
        .is_some_and(|readback| readback.state.prompt_models[0].prompt == request["prompt"]);
    let model_persisted = state_read.state_readback.as_ref().is_some_and(|readback| {
        readback.state.prompt_models[0].model_id == "model.local.tiny_agent"
    });
    let ward_auto_wired = state_read.state_readback.as_ref().is_some_and(|readback| {
        readback.state.ward_envelopes[0]
            .visual
            .allowed_effect_ids
            .contains(&"vfx.agent_brain_binding".to_string())
            && readback.state.ward_envelopes[0]
                .behavior
                .grounding_refs
                .contains(&"particle.agent_spark".to_string())
    });
    let binding_persisted = state_read.state_readback.as_ref().is_some_and(|readback| {
        readback.state.bindings[0].object_id == "npc.friendly_warden"
            && readback.state.bindings[0].deterministic_order_sink == "orders.friendly_warden"
    });
    let activation_in_game = sim
        .body_state(agent_body)
        .is_some_and(|body| body.position == v(1, 0, 0))
        && sim.attack_event_count() == 1;
    let replay_match = replay_outcome.final_hash == sim.state_hash();
    let accepted = prompt_persisted
        && model_persisted
        && ward_auto_wired
        && binding_persisted
        && activation_in_game
        && replay_match
        && all_edges_matched;
    if !accepted {
        return Err("issue860 no-code agent authoring acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.editor.issue860.no_code_agent_authoring_fsv.v1",
        "issue": 860,
        "source_of_truth": {
            "noncoder_request": request_path.display().to_string(),
            "mcp_agent_state": state_path.display().to_string(),
            "author_ledger": author_ledger_path.display().to_string(),
            "binding_ledger_tick0": args0.ledger_path.display().to_string(),
            "binding_ledger_tick1": args1.ledger_path.display().to_string(),
            "controller_tick0": args0.controller_run_path.display().to_string(),
            "controller_tick1": args1.controller_run_path.display().to_string(),
            "final_snapshot": final_snapshot_path.display().to_string(),
            "replay_record": replay_path.display().to_string(),
            "manual_gate": "manual Synapse/browser readback must inspect the persisted non-coder request, MCP state, ledgers, activation orders, replay hash, and edge failures"
        },
        "readme_refs_inspected": [
            "docs/readme/ED03-object-editor.png",
            "docs/readme/ED04-trigger-editor.png",
            "docs/readme/ED05-ai-coauthor.png",
            "docs/readme/S17-agent-binding.png",
            "docs/readme/03-mcp-agent.png"
        ],
        "research_basis": [
            "Unity visual scripting frames no-code authoring as high-level behavior design instead of code.",
            "Convai's action-system model separates prompt interpretation from registered in-scene actions and objects.",
            "Warcraft III editor discussions emphasize simple object/trigger workflows over complex editor surfaces."
        ],
        "authoring": {
            "request_readback": request_file_readback(&request_path)?,
            "before_author": before_author,
            "author_tool_output": authored,
            "after_author": after_author,
            "state_readback": state_read
        },
        "activation": {
            "before": before_activation,
            "after_chase": after_chase,
            "after_attack": sim_state(&sim, agent_body, target_body),
            "tick0_binding_ledger": binding0,
            "tick1_binding_ledger": binding1,
            "tick0_controller": controller_summary(&controller0.report),
            "tick1_controller": controller_summary(&controller1.report),
            "step0": step_json(step0),
            "step1": step_json(step1),
            "final_snapshot_hash": final_snapshot_disk.state_hash,
            "replay": {
                "path": replay_path.display().to_string(),
                "sha256": sha256_hex(&replay_bytes),
                "final_hash": replay_outcome.final_hash,
                "orders_applied": replay_outcome.orders_applied,
                "final_tick": replay_outcome.final_tick
            }
        },
        "acceptance": {
            "accepted": accepted,
            "prompt_persisted": prompt_persisted,
            "model_persisted": model_persisted,
            "ward_auto_wired": ward_auto_wired,
            "binding_persisted": binding_persisted,
            "activation_in_game": activation_in_game,
            "replay_match": replay_match,
            "all_edges_matched": all_edges_matched,
            "final_state_hash": sim.state_hash(),
            "replay_final_hash": replay_outcome.final_hash,
            "tick0_order_count": controller0.report.order_count,
            "tick1_order_count": controller1.report.order_count
        },
        "edges": edges,
        "sufficiency_argument": "One synthetic non-coder request is sufficient because it covers every #860 field transition: prompt text, model tier selection, character/object binding, visual and particle refs, automatic Ward wiring, deterministic activation orders, and replay. The five edges cover unsupported model, unsupported preset, invalid activation, empty prompt, and duplicate object binding without state mutation."
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE860_NO_CODE_AGENT_AUTHORING_FSV report={} sha256={} final_hash={} replay_hash={} edges={} accepted={}",
        readback.path.display(),
        readback.sha256,
        sim.state_hash(),
        replay_outcome.final_hash,
        edges.len(),
        accepted
    );
    Ok(())
}

fn edge_bad_model(root: &std::path::Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let state = root.join("edge-bad-model-state");
    let ledger = root.join("edge-bad-model-ledger");
    let before = state_summary(&state);
    let result = call_author_agent(author_request(
        &state,
        &ledger,
        "edge-bad-model",
        "huge",
        "guardian_lane",
        "Guard the lane.",
        manual_activation(),
    ));
    let after = state_summary(&state);
    Ok(edge_report(
        "bad_model_tier",
        before,
        result,
        after,
        "unsupported model tier",
    ))
}

fn edge_bad_preset(
    root: &std::path::Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let state = root.join("edge-bad-preset-state");
    let ledger = root.join("edge-bad-preset-ledger");
    let before = state_summary(&state);
    let result = call_author_agent(author_request(
        &state,
        &ledger,
        "edge-bad-preset",
        "tiny",
        "teleport_assassin",
        "Guard the lane.",
        manual_activation(),
    ));
    let after = state_summary(&state);
    Ok(edge_report(
        "bad_behavior_preset",
        before,
        result,
        after,
        "unsupported behavior preset",
    ))
}

fn edge_bad_activation(
    root: &std::path::Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let state = root.join("edge-bad-activation-state");
    let ledger = root.join("edge-bad-activation-ledger");
    let before = state_summary(&state);
    let result = call_author_agent(author_request(
        &state,
        &ledger,
        "edge-bad-activation",
        "tiny",
        "guardian_lane",
        "Guard the lane.",
        signal_activation_without_refs(),
    ));
    let after = state_summary(&state);
    Ok(edge_report(
        "bad_activation",
        before,
        result,
        after,
        "signal activation requires",
    ))
}

fn edge_empty_prompt(
    root: &std::path::Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let state = root.join("edge-empty-prompt-state");
    let ledger = root.join("edge-empty-prompt-ledger");
    let before = state_summary(&state);
    let result = call_author_agent(author_request(
        &state,
        &ledger,
        "edge-empty-prompt",
        "tiny",
        "guardian_lane",
        "",
        manual_activation(),
    ));
    let after = state_summary(&state);
    Ok(edge_report(
        "empty_prompt",
        before,
        result,
        after,
        "prompt must",
    ))
}

fn edge_duplicate_object(
    root: &std::path::Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let state = root.join("edge-duplicate-state");
    let ledger = root.join("edge-duplicate-ledger");
    call_author_agent(author_request(
        &state,
        &ledger,
        "edge-duplicate-seed",
        "tiny",
        "guardian_lane",
        "Guard the lane.",
        manual_activation(),
    ))?;
    let before = state_summary(&state);
    let mut duplicate = author_request(
        &state,
        &ledger,
        "edge-duplicate",
        "tiny",
        "guardian_lane",
        "Guard the lane again.",
        manual_activation(),
    );
    duplicate["agent_id"] = json!("agent.second_warden");
    duplicate["display_name"] = json!("Second Warden");
    let result = call_author_agent(duplicate);
    let after = state_summary(&state);
    Ok(edge_report(
        "duplicate_object",
        before,
        result,
        after,
        "already has an authored agent",
    ))
}
