use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::{
    EngineError,
    ai::{
        AI_CONTROLLER_RUN_SCHEMA, AiControllerAction, AiControllerActionBindings,
        AiControllerAgentInput, BEHAVIOR_TREE_SCHEMA, BehaviorNode, BehaviorNodeKind, BehaviorTree,
        PerceptionAgentInput, PerceptionEntityInput, PerceptionRunReport,
        load_ai_controller_run_json, lower_ai_controller_orders, sense_perception,
        snapshot_from_sim, write_ai_controller_run_json,
    },
    fsv_artifacts::begin_issue_run,
    math::{Fixed, FixedVec3},
    nav::{
        BakedNavMesh, NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput,
        bake_navmesh,
    },
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::SimOrderQueue,
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

#[test]
fn issue1324_bt_nav_controller_manual_fsv() {
    let run = begin_issue_run("issue1324_bt_nav_controller").unwrap();
    let root = run.run_dir().to_path_buf();
    let navmesh = line_navmesh(3);
    let (mut sim, mob, player) = duel_sim(0, 2);
    let initial_snapshot = SimSnapshot::capture(&sim);
    let before = sim_state(&sim, mob, player);

    let input0 = controller_input("mob.alpha", 1324, mob, NavCellCoord::new(0, 0), 25);
    let perception0 = perception_for(&sim, mob, player, NavCellCoord::new(0, 0), 2);
    let report0 = lower_ai_controller_orders(&navmesh, &perception0, &[input0]).unwrap();
    assert_eq!(report0.schema, AI_CONTROLLER_RUN_SCHEMA);
    assert_eq!(report0.decisions[0].action, AiControllerAction::Chase);
    let rb0 = write_ai_controller_run_json(root.join("controller-tick0-aster"), &report0).unwrap();
    let disk0 = load_ai_controller_run_json(&rb0.path).unwrap();
    assert_eq!(disk0.report, report0);

    let mut queue = SimOrderQueue::<8>::default();
    for order in &disk0.report.orders {
        queue.enqueue(*order).unwrap();
    }
    queue.step(&mut sim).unwrap();
    assert_eq!(sim.body_state(mob).unwrap().position, v(1, 0, 0));

    let input1 = controller_input("mob.alpha", 1324, mob, NavCellCoord::new(1, 0), 25);
    let perception1 = perception_for(&sim, mob, player, NavCellCoord::new(1, 0), 2);
    let report1 = lower_ai_controller_orders(&navmesh, &perception1, &[input1]).unwrap();
    assert_eq!(report1.decisions[0].action, AiControllerAction::Attack);
    let rb1 = write_ai_controller_run_json(root.join("controller-tick1-aster"), &report1).unwrap();
    let disk1 = load_ai_controller_run_json(&rb1.path).unwrap();
    assert_eq!(disk1.report, report1);
    for order in &disk1.report.orders {
        queue.enqueue(*order).unwrap();
    }
    queue.step(&mut sim).unwrap();

    let final_snapshot = SimSnapshot::capture(&sim);
    let final_snapshot_path = root.join("final-sim-snapshot.ron");
    fs::write(
        &final_snapshot_path,
        final_snapshot.to_ron_string().unwrap(),
    )
    .unwrap();
    let final_snapshot_disk =
        SimSnapshot::from_ron_str(&fs::read_to_string(&final_snapshot_path).unwrap()).unwrap();
    assert_eq!(final_snapshot_disk, final_snapshot);
    assert_eq!(final_snapshot_disk.attack_events.len(), 1);
    assert_eq!(final_snapshot_disk.attack_events[0].target, player);

    let mut all_orders = disk0.report.orders.clone();
    all_orders.extend(disk1.report.orders.iter().copied());
    let replay_record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot,
        orders: all_orders,
        run_ticks: 2,
        expected_final_hash: sim.state_hash(),
    };
    let replay_path = root.join("controller-replay.ron");
    fs::write(&replay_path, replay_record.to_ron_string().unwrap()).unwrap();
    let replay_disk =
        SimReplayRecord::from_ron_str(&fs::read_to_string(&replay_path).unwrap()).unwrap();
    let replay_outcome = replay_disk.replay::<4, 16>().unwrap();
    assert_eq!(replay_outcome.final_hash, sim.state_hash());
    assert_eq!(replay_outcome.final_snapshot.attack_events.len(), 1);

    let edges = vec![
        empty_agents_edge(&navmesh, &perception0, &root),
        missing_perception_edge(&navmesh, &perception0, mob, &root),
        invalid_damage_edge(&navmesh, &perception1, mob, &root),
        max_agents_edge(&root),
    ];
    let manual_report = json!({
        "issue": 1324,
        "source_of_truth": {
            "controller_tick0": rb0.path.display().to_string(),
            "controller_tick1": rb1.path.display().to_string(),
            "final_sim_snapshot": final_snapshot_path.display().to_string(),
            "replay_record": replay_path.display().to_string(),
            "latest_manifest": run.latest_manifest_path().display().to_string()
        },
        "happy_path": {
            "before": before,
            "after": sim_state(&sim, mob, player),
            "tick0": decision_summary(&disk0.report),
            "tick1": decision_summary(&disk1.report),
            "final_snapshot_readback": {
                "state_hash": final_snapshot_disk.state_hash,
                "attack_events": final_snapshot_disk.attack_events,
                "mob_position": final_snapshot_disk
                    .bodies
                    .iter()
                    .find(|body| body.id == mob)
                    .map(|body| body.position),
                "player_body": player.index()
            },
            "replay_readback": {
                "final_hash": replay_outcome.final_hash,
                "final_tick": replay_outcome.final_tick,
                "orders_applied": replay_outcome.orders_applied
            }
        },
        "edge_cases": edges
    });
    let fsv = run.write_report(&manual_report).unwrap();
    println!(
        "ISSUE1324_CONTROLLER_FSV report={} sha256={} tick0={} tick1={} final_snapshot={} replay={} final_hash={} attack_events={} edges={}",
        fsv.path.display(),
        fsv.sha256,
        rb0.path.display(),
        rb1.path.display(),
        final_snapshot_path.display(),
        replay_path.display(),
        sim.state_hash(),
        sim.attack_event_count(),
        manual_report["edge_cases"],
    );
}

fn duel_sim(mob_x: i32, player_x: i32) -> (FixedTickSim<4>, SimBodyId, SimBodyId) {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(1).unwrap(), 1324).unwrap();
    let mob = sim
        .spawn_body(v(mob_x, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    let player = sim
        .spawn_body(v(player_x, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    (sim, mob, player)
}

fn line_navmesh(width: i32) -> BakedNavMesh {
    let cells = (0..width)
        .map(|x| NavTerrainCellInput {
            id: format!("cell.{x:02}"),
            x,
            z: 0,
            height_fp: 0,
            flags: NavPathingFlags::default(),
        })
        .collect::<Vec<_>>();
    bake_navmesh(&NavBakeInput::new(Fixed::ONE.raw(), cells, Vec::new())).unwrap()
}

fn controller_input(
    id: &str,
    source_id: u32,
    body_id: SimBodyId,
    nav_cell: NavCellCoord,
    damage_fp: i64,
) -> AiControllerAgentInput {
    AiControllerAgentInput {
        id: id.to_string(),
        source_id,
        body_id,
        nav_cell,
        behavior_tree: combat_tree(),
        path_max_expansions: 64,
        attack_damage_fp: damage_fp,
        action_bindings: AiControllerActionBindings {
            attack: "attack".to_string(),
            chase: "chase".to_string(),
            patrol: "patrol".to_string(),
        },
    }
}

fn combat_tree() -> BehaviorTree {
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: vec![
            node(
                "root",
                BehaviorNodeKind::Selector,
                &["attack_seq", "chase", "patrol"],
                None,
            ),
            node(
                "attack_seq",
                BehaviorNodeKind::Sequence,
                &["in_range", "los", "attack"],
                None,
            ),
            node(
                "in_range",
                BehaviorNodeKind::Condition,
                &[],
                Some("in_attack_range"),
            ),
            node(
                "los",
                BehaviorNodeKind::Condition,
                &[],
                Some("line_of_sight"),
            ),
            node("attack", BehaviorNodeKind::Action, &[], Some("attack")),
            node("chase", BehaviorNodeKind::Action, &[], Some("chase")),
            node("patrol", BehaviorNodeKind::Action, &[], Some("patrol")),
        ],
    }
}

fn node(
    id: &str,
    kind: BehaviorNodeKind,
    children: &[&str],
    binding: Option<&str>,
) -> BehaviorNode {
    BehaviorNode {
        id: id.to_string(),
        kind,
        children: children.iter().map(|child| (*child).to_string()).collect(),
        binding: binding.map(str::to_string),
    }
}

fn perception_for(
    sim: &FixedTickSim<4>,
    mob: SimBodyId,
    player: SimBodyId,
    mob_cell: NavCellCoord,
    player_x: i32,
) -> PerceptionRunReport {
    let snapshot = snapshot_from_sim(
        sim,
        &[PerceptionAgentInput {
            id: "mob.alpha".to_string(),
            body_id: mob,
            faction: "mob".to_string(),
            nav_cell: mob_cell,
            sight_range_fp: Fixed::from_i32(8).raw(),
            attack_range_fp: Fixed::from_i32(1).raw(),
            threat_range_fp: Fixed::from_i32(8).raw(),
        }],
        &[PerceptionEntityInput {
            id: "player.one".to_string(),
            body_id: player,
            faction: "player".to_string(),
            nav_cell: NavCellCoord::new(player_x, 0),
            alive: true,
        }],
        &[],
    )
    .unwrap();
    sense_perception(&snapshot, sim.state_hash()).unwrap()
}

fn empty_agents_edge(
    navmesh: &BakedNavMesh,
    perception: &PerceptionRunReport,
    root: &Path,
) -> Value {
    let path = root.join("edge-empty-agents-aster");
    let before = file_state(&path);
    let result = lower_ai_controller_orders(navmesh, perception, &[]);
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"empty_agents","before":before,"after":file_state(&path),"error_code":error_code(result)})
}

fn missing_perception_edge(
    navmesh: &BakedNavMesh,
    perception: &PerceptionRunReport,
    mob: SimBodyId,
    root: &Path,
) -> Value {
    let path = root.join("edge-missing-perception-aster");
    let before = file_state(&path);
    let input = controller_input("mob.missing", 1325, mob, NavCellCoord::new(0, 0), 25);
    let result = lower_ai_controller_orders(navmesh, perception, &[input]);
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"missing_perception","before":before,"after":file_state(&path),"error_code":error_code(result)})
}

fn invalid_damage_edge(
    navmesh: &BakedNavMesh,
    perception: &PerceptionRunReport,
    mob: SimBodyId,
    root: &Path,
) -> Value {
    let path = root.join("edge-invalid-damage-aster");
    let before = file_state(&path);
    let input = controller_input("mob.alpha", 1326, mob, NavCellCoord::new(1, 0), 0);
    let result = lower_ai_controller_orders(navmesh, perception, &[input]);
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"invalid_damage","before":before,"after":file_state(&path),"error_code":error_code(result)})
}

fn max_agents_edge(root: &Path) -> Value {
    let path = root.join("edge-max-agents-controller-aster");
    let before = file_state(&path);
    let navmesh = line_navmesh(34);
    let mut sim = FixedTickSim::<64>::new(SimConfig::new(1).unwrap(), 44).unwrap();
    let mut agent_inputs = Vec::new();
    let mut perception_agents = Vec::new();
    for index in 0..32 {
        let body = sim
            .spawn_body(v(index, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
            .unwrap();
        let id = format!("mob.{index:02}");
        agent_inputs.push(controller_input(
            &id,
            2000 + index as u32,
            body,
            NavCellCoord::new(index, 0),
            10,
        ));
        perception_agents.push(PerceptionAgentInput {
            id,
            body_id: body,
            faction: "mob".to_string(),
            nav_cell: NavCellCoord::new(index, 0),
            sight_range_fp: Fixed::from_i32(40).raw(),
            attack_range_fp: Fixed::from_i32(1).raw(),
            threat_range_fp: Fixed::from_i32(40).raw(),
        });
    }
    let player = sim
        .spawn_body(v(33, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    let snapshot = snapshot_from_sim(
        &sim,
        &perception_agents,
        &[PerceptionEntityInput {
            id: "player.max".to_string(),
            body_id: player,
            faction: "player".to_string(),
            nav_cell: NavCellCoord::new(33, 0),
            alive: true,
        }],
        &[],
    )
    .unwrap();
    let perception = sense_perception(&snapshot, sim.state_hash()).unwrap();
    let report = lower_ai_controller_orders(&navmesh, &perception, &agent_inputs).unwrap();
    let readback = write_ai_controller_run_json(&path, &report).unwrap();
    assert_eq!(readback.report.order_count, 32);
    json!({
        "label":"max_agents",
        "before":before,
        "after":file_state(&path),
        "agent_count":readback.report.agent_count,
        "order_count":readback.report.order_count,
        "sha256":readback.sha256,
        "fnv1a64":readback.fnv1a64
    })
}

fn sim_state(sim: &FixedTickSim<4>, mob: SimBodyId, player: SimBodyId) -> Value {
    json!({
        "tick": sim.tick(),
        "state_hash": sim.state_hash(),
        "attack_event_count": sim.attack_event_count(),
        "mob": sim.body_state(mob),
        "player": sim.body_state(player),
    })
}

fn decision_summary(report: &crate::ai::AiControllerRunReport) -> Value {
    json!({
        "fnv1a64": report.fnv1a64,
        "order_count": report.order_count,
        "actions": report.decisions.iter().map(|decision| decision.action).collect::<Vec<_>>(),
        "order_fingerprints": report
            .decisions
            .iter()
            .flat_map(|decision| decision.order_fingerprints.clone())
            .collect::<Vec<_>>()
    })
}

fn file_state(path: &Path) -> Value {
    json!({
        "file_exists": path.exists(),
        "file_bytes": fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0),
    })
}

fn assert_error_code<T>(result: &Result<T, EngineError>, expected: &str) {
    match result {
        Err(error) => assert_eq!(error.code(), expected),
        Ok(_) => panic!("expected {expected}, got Ok"),
    }
}

fn error_code<T>(result: Result<T, EngineError>) -> String {
    match result {
        Err(error) => error.code().to_string(),
        Ok(_) => "OK".to_string(),
    }
}
