use std::fs;

use litd_editor::{
    agent_object_binding::{lower_bound_agent_orders, read_agent_object_binding_ledger},
    mcp_agent_tools::{READ_AGENTS_TOOL, call_mcp_agent_tool},
};
use litd_engine::{
    ai::load_ai_controller_run_json,
    fsv_artifacts::begin_issue_run,
    sim_order::SimOrderQueue,
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};
use litd_hash::sha256_hex;
use serde_json::json;

#[path = "support/issue788_agent_replay_support.rs"]
mod issue788_agent_replay_support;
#[path = "support/issue788_agent_replay_world.rs"]
mod issue788_agent_replay_world;
use issue788_agent_replay_support::{
    controller_summary, edge_agent_death, edge_illegal_order, edge_no_decision,
    edge_replay_hash_mismatch, edge_tampered_binding_ledger, instantiate_world, nav_cell,
    navmesh_from_world, perception_for, replay_json, run_args, sim_state, step_json, unit_by_role,
    write_agent_state,
};
use issue788_agent_replay_world::write_world_source;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue788_agent_replay")?;
    let root = run.run_dir();
    let world_path = run.artifact_path("world-source-of-truth.json")?;
    let world_readback = write_world_source(&world_path, run.run_id())?;
    let world = &world_readback.world;
    let agent_unit = unit_by_role(world, "agent_body")?;
    let target_unit = unit_by_role(world, "target")?;

    let state_path = root.join("agent-state-aster");
    let mcp_writes = write_agent_state(
        &state_path,
        root,
        world,
        &["intent.attack", "intent.chase"],
        &[],
    )?;
    let state_read = call_mcp_agent_tool(READ_AGENTS_TOOL, json!({ "state_path": state_path }))?;

    let navmesh = navmesh_from_world(world);
    let (mut sim, agent_body, target_body) = instantiate_world(world)?;
    let initial_snapshot = SimSnapshot::capture(&sim);
    let initial_snapshot_path = run.artifact_path("initial-sim-snapshot.ron")?;
    fs::write(&initial_snapshot_path, initial_snapshot.to_ron_string()?)?;
    let initial_snapshot_disk =
        SimSnapshot::from_ron_str(&fs::read_to_string(&initial_snapshot_path)?)?;

    let before_state = sim_state(&sim, agent_body, target_body);
    let args0 = run_args(
        root,
        &state_path,
        "tick0",
        world,
        agent_body,
        nav_cell(agent_unit.nav_cell),
    );
    let perception0 = perception_for(
        world,
        &sim,
        agent_body,
        target_body,
        nav_cell(agent_unit.nav_cell),
        nav_cell(target_unit.nav_cell),
        world.sight_range,
    )?;
    let tick0_sim_tick_before = sim.tick();
    let binding0 = lower_bound_agent_orders(&args0, &navmesh, &perception0, &sim)?;
    let binding0_readback = read_agent_object_binding_ledger(&args0.ledger_path)?;
    let controller0 = load_ai_controller_run_json(&args0.controller_run_path)?;

    let mut queue = SimOrderQueue::<8>::default();
    for order in &controller0.report.orders {
        queue.enqueue(*order)?;
    }
    let step0 = queue.step(&mut sim)?;
    let after_move = sim_state(&sim, agent_body, target_body);

    let args1 = run_args(
        root,
        &state_path,
        "tick1",
        world,
        agent_body,
        nav_cell([1, 0]),
    );
    let perception1 = perception_for(
        world,
        &sim,
        agent_body,
        target_body,
        nav_cell([1, 0]),
        nav_cell(target_unit.nav_cell),
        world.sight_range,
    )?;
    let tick1_sim_tick_before = sim.tick();
    let binding1 = lower_bound_agent_orders(&args1, &navmesh, &perception1, &sim)?;
    let binding1_readback = read_agent_object_binding_ledger(&args1.ledger_path)?;
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
    let order_stream = json!({
        "schema": "litd.editor.issue788.lowered_order_stream.v1",
        "issue": 788,
        "source_of_truth": "controller run readbacks plus SimOrderQueue enqueue readback",
        "queue_capacity": queue.capacity(),
        "applied_total": queue.applied_total(),
        "pending_orders": queue.pending_orders(),
        "orders": all_orders,
        "order_fingerprints": all_orders
            .iter()
            .map(|order| format!("{:016X}", order.fingerprint()))
            .collect::<Vec<_>>()
    });
    let order_stream_readback =
        run.write_json_artifact("lowered-order-stream.json", &order_stream)?;

    let replay_record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: initial_snapshot_disk,
        orders: all_orders,
        run_ticks: 2,
        expected_final_hash: sim.state_hash(),
    };
    let replay_path = run.artifact_path("agent-world-replay.ron")?;
    let replay_text = replay_record.to_ron_string()?;
    fs::write(&replay_path, &replay_text)?;
    let replay_bytes = fs::read(&replay_path)?;
    let replay_disk = SimReplayRecord::from_ron_str(&String::from_utf8(replay_bytes.clone())?)?;
    let replay_outcome = replay_disk.replay::<4, 8>()?;

    let edges = vec![
        edge_no_decision(root, &state_path, world)?,
        edge_illegal_order(root, world)?,
        edge_agent_death(root, &state_path, world)?,
        edge_tampered_binding_ledger(root, &binding0.ledger),
        edge_replay_hash_mismatch(&replay_record),
    ];
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let move_executed = sim
        .body_state(agent_body)
        .is_some_and(|body| body.position == litd_engine::math::FixedVec3::from_i32s(1, 0, 0));
    let attack_executed = sim.attack_event_count() == 1
        && final_snapshot_disk.attack_events.len() == 1
        && final_snapshot_disk.attack_events[0].target == target_body;
    let replay_match = replay_outcome.final_hash == sim.state_hash();
    let final_snapshot_equal = replay_outcome.final_snapshot == final_snapshot_disk;
    let controller_outside_tick = controller0.report.tick == tick0_sim_tick_before
        && controller1.report.tick == tick1_sim_tick_before
        && step0.tick == tick0_sim_tick_before + 1
        && step1.tick == tick1_sim_tick_before + 1;
    let binding_ledger_readback_match =
        binding0 == binding0_readback && binding1 == binding1_readback;
    let grounded_decisions = binding0.ledger.decisions[0].deterministic_order_sink
        == world.agent.deterministic_order_sink
        && binding1.ledger.decisions[0].deterministic_order_sink
            == world.agent.deterministic_order_sink
        && binding0.ledger.decisions[0].source_id == world.agent.source_id
        && binding1.ledger.decisions[0].source_id == world.agent.source_id
        && binding0.ledger.decisions[0].body_id == agent_body
        && binding1.ledger.decisions[0].body_id == agent_body
        && binding0.ledger.decisions[0].intent_id == "intent.chase"
        && binding1.ledger.decisions[0].intent_id == "intent.attack";
    let accepted = move_executed
        && attack_executed
        && replay_match
        && final_snapshot_equal
        && controller_outside_tick
        && binding_ledger_readback_match
        && grounded_decisions
        && all_edges_matched;
    if !accepted {
        return Err("issue788 AI-agent replay acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.editor.issue788.ai_agent_replay_fsv.v1",
        "issue": 788,
        "source_of_truth": {
            "world": world_path.display().to_string(),
            "mcp_agent_state": state_path.display().to_string(),
            "binding_ledger_tick0": args0.ledger_path.display().to_string(),
            "binding_ledger_tick1": args1.ledger_path.display().to_string(),
            "controller_tick0": args0.controller_run_path.display().to_string(),
            "controller_tick1": args1.controller_run_path.display().to_string(),
            "order_stream": order_stream_readback.path.display().to_string(),
            "initial_snapshot": initial_snapshot_path.display().to_string(),
            "final_snapshot": final_snapshot_path.display().to_string(),
            "replay_record": replay_path.display().to_string(),
            "latest_manifest": run.latest_manifest_path().display().to_string(),
            "manual_gate": "manual Synapse/browser readback must inspect the persisted world, MCP state, binding ledgers, order stream, replay record, final hash, and edge errors"
        },
        "research_basis": [
            "Command pattern stores gameplay actions as serializable commands/orders.",
            "Deterministic lockstep proves behavior by replaying the same fixed-tick input stream."
        ],
        "world_readback": world_readback,
        "mcp_tool_writes": mcp_writes,
        "mcp_state_readback": state_read,
        "happy_path": {
            "before": before_state,
            "after_move": after_move,
            "after_attack": sim_state(&sim, agent_body, target_body),
            "tick0_binding_ledger": binding0,
            "tick1_binding_ledger": binding1,
            "tick0_binding_ledger_readback": binding0_readback,
            "tick1_binding_ledger_readback": binding1_readback,
            "tick0_controller": controller_summary(&controller0.report),
            "tick1_controller": controller_summary(&controller1.report),
            "step0": step_json(step0),
            "step1": step_json(step1),
            "order_stream_readback": {
                "path": order_stream_readback.path.display().to_string(),
                "sha256": order_stream_readback.sha256,
                "orders": order_stream_readback.json["orders"].as_array().map_or(0, Vec::len)
            },
            "final_snapshot_readback": {
                "path": final_snapshot_path.display().to_string(),
                "sha256": sha256_hex(&fs::read(&final_snapshot_path)?),
                "state_hash": final_snapshot_disk.state_hash,
                "attack_events": final_snapshot_disk.attack_events
            },
            "replay_readback": {
                "path": replay_path.display().to_string(),
                "sha256": sha256_hex(&replay_bytes),
                "outcome": replay_json(&replay_outcome)
            }
        },
        "acceptance": {
            "accepted": accepted,
            "move_executed": move_executed,
            "attack_executed": attack_executed,
            "replay_match": replay_match,
            "final_snapshot_equal": final_snapshot_equal,
            "controller_outside_tick": controller_outside_tick,
            "binding_ledger_readback_match": binding_ledger_readback_match,
            "grounded_decisions": grounded_decisions,
            "all_edges_matched": all_edges_matched,
            "tick0_order_count": controller0.report.order_count,
            "tick1_order_count": controller1.report.order_count,
            "final_state_hash": sim.state_hash(),
            "replay_final_hash": replay_outcome.final_hash
        },
        "edges": edges,
        "sufficiency_argument": "This #788 proof reads a world JSON source of truth, binds one real agent object through the MCP state row, lowers chase and attack decisions into SimOrderQueue, persists the complete lowered order stream, and replays from the captured initial snapshot to the identical final snapshot hash. The edges cover no-decision, Ward-forbidden lowering, stale/dead body binding, tampered decision ledger, and replay hash mismatch."
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE788_AGENT_REPLAY_FSV report={} sha256={} world_sha256={} order_stream_sha256={} replay_sha256={} final_hash={} replay_hash={} edges={} grounded={}",
        readback.path.display(),
        readback.sha256,
        world_readback.sha256,
        order_stream_readback.sha256,
        sha256_hex(&replay_bytes),
        sim.state_hash(),
        replay_outcome.final_hash,
        edges.len(),
        grounded_decisions
    );
    Ok(())
}
