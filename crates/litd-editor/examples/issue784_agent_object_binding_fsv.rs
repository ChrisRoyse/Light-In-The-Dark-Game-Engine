use std::fs;

use litd_editor::{
    agent_object_binding::lower_bound_agent_orders,
    mcp_agent_tools::{READ_AGENTS_TOOL, call_mcp_agent_tool},
};
use litd_engine::{
    ai::load_ai_controller_run_json,
    fsv_artifacts::begin_issue_run,
    nav::NavCellCoord,
    sim_order::SimOrderQueue,
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};
use serde_json::json;

#[path = "support/issue784_agent_object_binding_support.rs"]
mod issue784_agent_object_binding_support;
use issue784_agent_object_binding_support::{
    controller_summary, duel_sim, edge_agent_death, edge_illegal_order, edge_no_decision,
    line_navmesh, perception_for, run_args, sim_state, step_json, v, write_agent_state,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue784_agent_object_binding")?;
    let root = run.run_dir();
    let state_path = root.join("agent-state-aster");
    write_agent_state(&state_path, root, &["intent.attack", "intent.chase"], &[])?;
    let state_read = call_mcp_agent_tool(READ_AGENTS_TOOL, json!({ "state_path": state_path }))?;

    let navmesh = line_navmesh(3);
    let (mut sim, mob, player) = duel_sim(0, 2)?;
    let initial_snapshot = SimSnapshot::capture(&sim);
    let before_state = sim_state(&sim, mob, player);

    let args0 = run_args(root, &state_path, "tick0", mob, NavCellCoord::new(0, 0));
    let perception0 = perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 2, 8)?;
    let tick0_sim_tick_before = sim.tick();
    let binding0 = lower_bound_agent_orders(&args0, &navmesh, &perception0, &sim)?;
    let controller0 = load_ai_controller_run_json(&args0.controller_run_path)?;
    let mut queue = SimOrderQueue::<8>::default();
    for order in &controller0.report.orders {
        queue.enqueue(*order)?;
    }
    let step0 = queue.step(&mut sim)?;
    let after_move = sim_state(&sim, mob, player);

    let args1 = run_args(root, &state_path, "tick1", mob, NavCellCoord::new(1, 0));
    let perception1 = perception_for(&sim, mob, player, NavCellCoord::new(1, 0), 2, 8)?;
    let tick1_sim_tick_before = sim.tick();
    let binding1 = lower_bound_agent_orders(&args1, &navmesh, &perception1, &sim)?;
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
    let replay_path = run.artifact_path("agent-binding-replay.ron")?;
    fs::write(&replay_path, replay_record.to_ron_string()?)?;
    let replay_disk = SimReplayRecord::from_ron_str(&fs::read_to_string(&replay_path)?)?;
    let replay_outcome = replay_disk.replay::<4, 8>()?;

    let edges = vec![
        edge_no_decision(root, &state_path)?,
        edge_illegal_order(root)?,
        edge_agent_death(root, &state_path)?,
    ];
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let move_executed = sim
        .body_state(mob)
        .is_some_and(|body| body.position == v(1, 0, 0));
    let attack_executed = sim.attack_event_count() == 1
        && final_snapshot_disk.attack_events.len() == 1
        && final_snapshot_disk.attack_events[0].target == player;
    let replay_match = replay_outcome.final_hash == sim.state_hash();
    let controller_outside_tick = controller0.report.tick == tick0_sim_tick_before
        && controller1.report.tick == tick1_sim_tick_before
        && step0.tick == tick0_sim_tick_before + 1
        && step1.tick == tick1_sim_tick_before + 1;
    let accepted = move_executed
        && attack_executed
        && replay_match
        && controller_outside_tick
        && all_edges_matched;
    if !accepted {
        return Err("issue784 agent object binding acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.editor.issue784.agent_object_binding_fsv.v1",
        "issue": 784,
        "source_of_truth": {
            "mcp_agent_state": state_path.display().to_string(),
            "binding_ledger_tick0": args0.ledger_path.display().to_string(),
            "binding_ledger_tick1": args1.ledger_path.display().to_string(),
            "controller_tick0": args0.controller_run_path.display().to_string(),
            "controller_tick1": args1.controller_run_path.display().to_string(),
            "final_snapshot": final_snapshot_path.display().to_string(),
            "replay_record": replay_path.display().to_string(),
            "latest_manifest": run.latest_manifest_path().display().to_string(),
            "manual_gate": "manual Synapse/browser readback must inspect persisted MCP state, controller rows, binding ledgers, snapshot/replay files, hashes, and edge errors"
        },
        "mcp_state_readback": state_read,
        "happy_path": {
            "before": before_state,
            "after_move": after_move,
            "after_attack": sim_state(&sim, mob, player),
            "tick0_binding_ledger": binding0,
            "tick1_binding_ledger": binding1,
            "tick0_controller": controller_summary(&controller0.report),
            "tick1_controller": controller_summary(&controller1.report),
            "step0": step_json(step0),
            "step1": step_json(step1),
            "final_snapshot_readback": {
                "state_hash": final_snapshot_disk.state_hash,
                "attack_events": final_snapshot_disk.attack_events,
                "mob_position": final_snapshot_disk
                    .bodies
                    .iter()
                    .find(|body| body.id == mob)
                    .map(|body| body.position)
            },
            "replay_readback": {
                "initial_hash": replay_outcome.initial_hash,
                "final_hash": replay_outcome.final_hash,
                "final_tick": replay_outcome.final_tick,
                "orders_applied": replay_outcome.orders_applied
            }
        },
        "acceptance": {
            "move_executed": move_executed,
            "attack_executed": attack_executed,
            "replay_match": replay_match,
            "controller_outside_tick": controller_outside_tick,
            "all_edges_matched": all_edges_matched,
            "tick0_order_count": controller0.report.order_count,
            "tick1_order_count": controller1.report.order_count,
            "final_state_hash": sim.state_hash(),
            "attack_event_count": sim.attack_event_count()
        },
        "edges": edges,
        "sufficiency_argument": "The smallest known-I/O proof uses one persisted enabled agent binding and one target. Tick 0 proves a move/chase order from the binding into SimOrderQueue, tick 1 proves attack lowering and replay, and the three edge cases cover no decision, Ward-illegal intent, and stale/dead object binding. Larger scenes add volume but no distinct #784 state transition."
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE784_AGENT_OBJECT_BINDING_FSV report={} sha256={} binding0_sha256={} binding1_sha256={} controller0_sha256={} controller1_sha256={} final_hash={} edges={} replay_match={}",
        readback.path.display(),
        readback.sha256,
        binding0.sha256,
        binding1.sha256,
        controller0.sha256,
        controller1.sha256,
        sim.state_hash(),
        edges.len(),
        replay_match
    );
    Ok(())
}
