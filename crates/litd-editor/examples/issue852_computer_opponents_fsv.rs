use litd_editor::computer_opponents::{ComputerOpponentAuthoringArgs, author_computer_opponents};
use litd_engine::{
    fsv_artifacts::begin_issue_run,
    sim_order::SimOrderQueue,
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};
use serde_json::json;

#[path = "support/issue852_computer_opponents_support.rs"]
mod issue852_computer_opponents_support;
use issue852_computer_opponents_support::{
    controller_summary, duel_sim, edge_dead_opponent, edge_duplicate_player, edge_forbidden_chase,
    line_navmesh, objects, opponents, run_opponent_tick, sim_state, sim_state_after_step,
    step_json, targets, write_replay, write_snapshot,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue852_computer_opponents")?;
    let root = run.run_dir();
    let state_path = root.join("computer-agent-state-aster");
    let opponents = opponents();
    let authoring = author_computer_opponents(&ComputerOpponentAuthoringArgs {
        state_path: state_path.clone(),
        ledger_dir: root.join("authoring-ledgers"),
        state_id: "issue852_computer_state".to_string(),
        requested_by: "issue852_fsv".to_string(),
        opponents: opponents.clone(),
    })?;

    let navmesh = line_navmesh(5);
    let (mut sim, north, south, human) = duel_sim()?;
    let initial_snapshot = SimSnapshot::capture(&sim);
    let before = sim_state(&sim, north, south, human);

    let tick0 = run_opponent_tick(
        root,
        "tick0",
        &state_path,
        &opponents,
        objects(north, south, 0, 4),
        targets(human),
        &navmesh,
        &sim,
    )?;
    let mut queue = SimOrderQueue::<16>::default();
    for order in &tick0.controller.report.orders {
        queue.enqueue(*order)?;
    }
    let step0 = queue.step(&mut sim)?;

    let tick1 = run_opponent_tick(
        root,
        "tick1",
        &state_path,
        &opponents,
        objects(north, south, 1, 3),
        targets(human),
        &navmesh,
        &sim,
    )?;
    for order in &tick1.controller.report.orders {
        queue.enqueue(*order)?;
    }
    let step1 = queue.step(&mut sim)?;
    let final_snapshot = SimSnapshot::capture(&sim);
    let final_snapshot_path = run.artifact_path("computer-opponents-final-snapshot.ron")?;
    let final_snapshot_disk = write_snapshot(&final_snapshot_path, &final_snapshot)?;

    let mut all_orders = tick0.controller.report.orders.clone();
    all_orders.extend(tick1.controller.report.orders.iter().copied());
    let replay = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot,
        orders: all_orders,
        run_ticks: 2,
        expected_final_hash: sim.state_hash(),
    };
    let replay_path = run.artifact_path("computer-opponents-replay.ron")?;
    let replay_disk = write_replay(&replay_path, &replay)?;
    let replay_outcome = replay_disk.replay::<8, 16>()?;

    let edges = vec![
        edge_duplicate_player(root)?,
        edge_forbidden_chase(root, &navmesh)?,
        edge_dead_opponent(
            root,
            &state_path,
            &opponents,
            &navmesh,
            &sim,
            north,
            south,
            human,
        )?,
    ];
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let accepted = authoring.binding_count == 2
        && tick0.controller.report.order_count == 2
        && tick1.controller.report.order_count == 4
        && sim.attack_event_count() == 2
        && final_snapshot_disk.attack_events.len() == 2
        && replay_outcome.final_hash == sim.state_hash()
        && all_edges_matched;
    if !accepted {
        return Err("issue852 computer opponent acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.editor.issue852.computer_opponents_fsv.v1",
        "issue": 852,
        "research": {
            "wc3_ai_editor_pattern": "computer player scripts author attack/build behavior and assign it to computer slots; LitD proof maps this to persisted opponent scripts, Ward envelopes, and deterministic controller orders",
            "sources": [
                "https://www.hiveworkshop.com/threads/how-to-make-an-ai-attack-waves-like-the-ones-in-the-normal-campaigns.352543/",
                "https://www.thehelper.net/threads/tutorial-the-wc3-ai-editor.37178/"
            ]
        },
        "reference_image_reviewed": "docs/readme/S17-agent-binding.png",
        "source_of_truth": {
            "mcp_agent_state": state_path.display().to_string(),
            "controller_tick0": tick0.controller_path.display().to_string(),
            "controller_tick1": tick1.controller_path.display().to_string(),
            "opponent_ledger_tick0": tick0.ledger_path.display().to_string(),
            "opponent_ledger_tick1": tick1.ledger_path.display().to_string(),
            "final_snapshot": final_snapshot_path.display().to_string(),
            "replay": replay_path.display().to_string(),
            "manual_gate": "manual Synapse/browser readback must inspect persisted state, controller rows, opponent ledgers, final snapshot/replay, hashes, and edge errors"
        },
        "authoring": authoring,
        "happy_path": {
            "before": before,
            "after_tick0": sim_state_after_step(step_json(step0), &sim, north, south, human),
            "after_tick1": sim_state_after_step(step_json(step1), &sim, north, south, human),
            "tick0_ledger": tick0.ledger,
            "tick1_ledger": tick1.ledger,
            "tick0_controller": controller_summary(&tick0.controller.report),
            "tick1_controller": controller_summary(&tick1.controller.report),
            "final_snapshot_readback": {
                "state_hash": final_snapshot_disk.state_hash,
                "attack_events": final_snapshot_disk.attack_events,
                "body_count": final_snapshot_disk.bodies.len()
            },
            "replay_readback": {
                "initial_hash": replay_outcome.initial_hash,
                "final_hash": replay_outcome.final_hash,
                "final_tick": replay_outcome.final_tick,
                "orders_applied": replay_outcome.orders_applied
            }
        },
        "acceptance": {
            "authored_computer_players": authoring.binding_count,
            "tick0_order_count": tick0.controller.report.order_count,
            "tick1_order_count": tick1.controller.report.order_count,
            "attack_event_count": sim.attack_event_count(),
            "replay_match": replay_outcome.final_hash == sim.state_hash(),
            "final_state_hash": sim.state_hash(),
            "all_edges_matched": all_edges_matched
        },
        "edges": edges,
        "sufficiency_argument": "The smallest known-I/O proof uses two authored computer-player slots, one human target, two ticks, and one replay. Tick 0 proves per-player scripts lower chase orders; tick 1 proves attack orders and deterministic replay. Edges cover duplicate player IDs, Ward-forbidden chase intent, and a dead runtime opponent object. Larger maps add volume but no distinct #852 state transition."
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE852_COMPUTER_OPPONENTS_FSV report={} sha256={} state_sha={} tick0_ledger_sha={} tick1_ledger_sha={} final_hash={} edges={} replay_match={}",
        readback.path.display(),
        readback.sha256,
        authoring.state_sha256,
        tick0.ledger.sha256,
        tick1.ledger.sha256,
        sim.state_hash(),
        edges.len(),
        replay_outcome.final_hash == sim.state_hash()
    );
    Ok(())
}
