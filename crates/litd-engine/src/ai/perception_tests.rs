use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::{
    EngineError,
    ai::*,
    fsv_artifacts::begin_issue_run,
    math::{Fixed, FixedVec3},
    nav::NavCellCoord,
    sim::{FixedTickSim, SimBodyId, SimConfig},
};

#[test]
fn issue1323_blackboard_perception_manual_fsv() {
    let run = begin_issue_run("issue1323_blackboard_perception").unwrap();
    let root = run.run_dir().to_path_buf();
    let sim = known_world();
    let before_hash = sim.state_hash();
    let snapshot = known_snapshot(&sim);
    let after_snapshot_hash = sim.state_hash();
    assert_eq!(before_hash, after_snapshot_hash);
    let report = sense_perception(&snapshot, sim.state_hash()).unwrap();
    let perception_path = root.join("guard-perception-run-aster");
    let readback = write_perception_run_json(&perception_path, &report).unwrap();
    let disk = load_perception_run_json(&readback.path).unwrap();
    assert_eq!(disk.report, report);
    let guard = disk
        .report
        .agents
        .iter()
        .find(|agent| agent.agent_id == "guard.alpha")
        .unwrap();
    assert_eq!(guard.nearest_enemy_id.as_deref(), Some("enemy.near"));
    assert_eq!(
        guard.nearest_visible_enemy_id.as_deref(),
        Some("enemy.near")
    );
    assert_eq!(guard.enemies_in_attack_range, vec!["enemy.near"]);
    assert_eq!(
        guard.enemies_in_sight_range,
        vec!["enemy.far", "enemy.near", "enemy.occluded"]
    );
    assert_eq!(
        guard.enemies_with_line_of_sight,
        vec!["enemy.far", "enemy.near"]
    );
    assert_eq!(
        guard.threat_enemy_ids,
        vec!["enemy.far", "enemy.near", "enemy.occluded"]
    );
    assert_eq!(
        guard.blackboard.leaf_state.conditions.get("sees_enemy"),
        Some(&BehaviorStatus::Success)
    );
    assert_eq!(sim.state_hash(), before_hash);

    let edges = vec![
        empty_agents_edge(&sim, &root),
        max_entities_edge(&root),
        missing_body_edge(&sim, &root),
        invalid_range_edge(&sim, &root),
        oversized_los_edge(&sim, &root),
        tampered_blackboard_edge(&sim, &root),
    ];
    let manual_report = json!({
        "schema": "litd.manual_fsv.issue1323.v1",
        "issue": 1323,
        "source_of_truth": {
            "sim_snapshot": "FixedTickSim body_states plus state_hash read before and after sensing",
            "perception_report": readback.path.display().to_string(),
            "latest_manifest": run.latest_manifest_path().display().to_string()
        },
        "happy_path": {
            "before": {
                "sim_state_hash": before_hash,
                "body_count": sim.body_count(),
                "perception_file_exists": false
            },
            "after": {
                "sim_state_hash": sim.state_hash(),
                "sim_state_unchanged": report.sim_state_unchanged,
                "perception_file_exists": readback.path.exists(),
                "report_sha256": readback.sha256,
                "nearest_enemy_id": guard.nearest_enemy_id,
                "nearest_visible_enemy_id": guard.nearest_visible_enemy_id,
                "enemies_in_attack_range": guard.enemies_in_attack_range,
                "enemies_with_line_of_sight": guard.enemies_with_line_of_sight,
                "threat_enemy_ids": guard.threat_enemy_ids,
                "blackboard_fnv1a64": guard.blackboard.fnv1a64
            }
        },
        "edge_cases": edges,
    });
    let fsv = run.write_report(&manual_report).unwrap();
    println!(
        "ISSUE1323_PERCEPTION_FSV report={} sha256={} perception_json={} perception_sha256={} before_hash={} after_hash={} nearest={:?} visible={:?} edges={}",
        fsv.path.display(),
        fsv.sha256,
        readback.path.display(),
        readback.sha256,
        before_hash,
        sim.state_hash(),
        guard.nearest_enemy_id,
        guard.nearest_visible_enemy_id,
        manual_report["edge_cases"],
    );
}

fn known_world() -> FixedTickSim<128> {
    let mut sim = FixedTickSim::new(SimConfig::new(20).unwrap(), 13).unwrap();
    for position in [
        FixedVec3::from_i32s(0, 0, 0),
        FixedVec3::from_i32s(1, 0, 0),
        FixedVec3::from_i32s(4, 0, 0),
        FixedVec3::from_i32s(0, 0, 4),
        FixedVec3::from_i32s(0, 0, 1),
    ] {
        sim.spawn_body(position, FixedVec3::ZERO, FixedVec3::ZERO)
            .unwrap();
    }
    sim
}

fn known_snapshot(sim: &FixedTickSim<128>) -> PerceptionSnapshot {
    snapshot_from_sim(
        sim,
        &[guard_input()],
        &[
            entity("guard.self", 0, "guard", 0, 0, true),
            entity("enemy.near", 1, "player", 1, 0, true),
            entity("enemy.far", 2, "player", 4, 0, true),
            entity("enemy.occluded", 3, "player", 0, 4, true),
            entity("guard.ally", 4, "guard", 0, 1, true),
        ],
        &[PerceptionBlocker {
            id: "pillar.mid".to_string(),
            min: NavCellCoord::new(0, 2),
            max: NavCellCoord::new(0, 2),
        }],
    )
    .unwrap()
}

fn guard_input() -> PerceptionAgentInput {
    PerceptionAgentInput {
        id: "guard.alpha".to_string(),
        body_id: SimBodyId::from_raw(0),
        faction: "guard".to_string(),
        nav_cell: NavCellCoord::new(0, 0),
        sight_range_fp: Fixed::from_i32(5).raw(),
        attack_range_fp: Fixed::from_i32(1).raw(),
        threat_range_fp: Fixed::from_i32(5).raw(),
    }
}

fn entity(
    id: &str,
    body: u32,
    faction: &str,
    x: i32,
    z: i32,
    alive: bool,
) -> PerceptionEntityInput {
    PerceptionEntityInput {
        id: id.to_string(),
        body_id: SimBodyId::from_raw(body),
        faction: faction.to_string(),
        nav_cell: NavCellCoord::new(x, z),
        alive,
    }
}

fn empty_agents_edge(sim: &FixedTickSim<128>, root: &Path) -> Value {
    let path = root.join("edge-empty-agents-aster");
    let before = edge_state(sim, &path);
    let result = snapshot_from_sim(
        sim,
        &[],
        &[entity("enemy.near", 1, "player", 1, 0, true)],
        &[],
    );
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"empty_agents","before":before,"after":edge_state(sim, &path),"error_code":error_code(result)})
}

fn max_entities_edge(root: &Path) -> Value {
    let max_sim = max_world();
    let path = root.join("edge-max-entities-aster");
    let before = edge_state(&max_sim, &path);
    let mut entities = vec![entity("guard.self", 0, "guard", 0, 0, true)];
    for index in 1..=63 {
        entities.push(entity(
            &format!("enemy.{index:02}"),
            index,
            "player",
            1,
            0,
            true,
        ));
    }
    let snapshot = snapshot_from_sim(&max_sim, &[guard_input()], &entities, &[]).unwrap();
    let report = sense_perception(&snapshot, max_sim.state_hash()).unwrap();
    let readback = write_perception_run_json(&path, &report).unwrap();
    assert_eq!(
        readback.report.agents[0].nearest_enemy_id.as_deref(),
        Some("enemy.01")
    );
    json!({
        "label":"max_entities",
        "before":before,
        "after":edge_state(&max_sim, &path),
        "agent_count": readback.report.agent_count,
        "entity_count": readback.report.entity_count,
        "nearest_enemy_id": readback.report.agents[0].nearest_enemy_id,
        "sha256": readback.sha256
    })
}

fn max_world() -> FixedTickSim<128> {
    let mut sim = FixedTickSim::new(SimConfig::new(20).unwrap(), 77).unwrap();
    sim.spawn_body(
        FixedVec3::from_i32s(0, 0, 0),
        FixedVec3::ZERO,
        FixedVec3::ZERO,
    )
    .unwrap();
    for _ in 1..=63 {
        sim.spawn_body(
            FixedVec3::from_i32s(1, 0, 0),
            FixedVec3::ZERO,
            FixedVec3::ZERO,
        )
        .unwrap();
    }
    sim
}

fn missing_body_edge(sim: &FixedTickSim<128>, root: &Path) -> Value {
    let path = root.join("edge-missing-body-aster");
    let before = edge_state(sim, &path);
    let result = snapshot_from_sim(
        sim,
        &[guard_input()],
        &[entity("enemy.missing", 99, "player", 9, 9, true)],
        &[],
    );
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"missing_body","before":before,"after":edge_state(sim, &path),"error_code":error_code(result)})
}

fn invalid_range_edge(sim: &FixedTickSim<128>, root: &Path) -> Value {
    let path = root.join("edge-invalid-range-aster");
    let before = edge_state(sim, &path);
    let mut agent = guard_input();
    agent.attack_range_fp = Fixed::from_i32(6).raw();
    let result = snapshot_from_sim(
        sim,
        &[agent],
        &[entity("enemy.near", 1, "player", 1, 0, true)],
        &[],
    );
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"invalid_range","before":before,"after":edge_state(sim, &path),"error_code":error_code(result)})
}

fn oversized_los_edge(sim: &FixedTickSim<128>, root: &Path) -> Value {
    let path = root.join("edge-oversized-los-aster");
    let before = edge_state(sim, &path);
    let snapshot = snapshot_from_sim(
        sim,
        &[guard_input()],
        &[entity("enemy.near", 1, "player", 5000, 0, true)],
        &[],
    )
    .unwrap();
    let result = sense_perception(&snapshot, sim.state_hash());
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"oversized_los","before":before,"after":edge_state(sim, &path),"error_code":error_code(result)})
}

fn tampered_blackboard_edge(sim: &FixedTickSim<128>, root: &Path) -> Value {
    let path = root.join("edge-tampered-blackboard-aster");
    let before = edge_state(sim, &path);
    let snapshot = known_snapshot(sim);
    let mut report = sense_perception(&snapshot, sim.state_hash()).unwrap();
    let guard = &mut report.agents[0];
    guard.blackboard.values.insert(
        "perception.has_enemy".to_string(),
        BlackboardValue::Bool(false),
    );
    guard.blackboard.fnv1a64 = compute_blackboard_fnv1a64(&guard.blackboard);
    report.fnv1a64 = compute_perception_run_fnv1a64(&report);
    let result = write_perception_run_json(&path, &report);
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    json!({"label":"tampered_blackboard","before":before,"after":edge_state(sim, &path),"error_code":error_code(result)})
}

fn edge_state(sim: &FixedTickSim<128>, path: &Path) -> Value {
    json!({
        "sim_state_hash": sim.state_hash(),
        "file_exists": path.exists(),
        "file_bytes": fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0),
    })
}

fn assert_error_code<T>(result: &Result<T, EngineError>, expected: &str) {
    match result {
        Err(error) => assert_eq!(error.report().code, expected),
        Ok(_) => panic!("expected {expected}"),
    }
}

fn error_code<T>(result: Result<T, EngineError>) -> String {
    match result {
        Err(error) => error.report().code.to_string(),
        Ok(_) => panic!("expected error"),
    }
}

#[test]
fn issue1486_perception_blocker_grid_area_cap_fails_loud() {
    let sim = known_world();

    // A blocker AABB spanning 100x100 = 10,000 cells exceeds MAX_BLOCKER_CELLS (4096):
    // rasterising it into the occupancy grid would allocate an unbounded region, so the
    // grid build must fail loud rather than OOM (#1486 #3). The snapshot itself is valid
    // (min <= max), so this surfaces only when sense_perception builds the grid.
    let huge_blocker = PerceptionBlocker {
        id: "wall.pathological".to_string(),
        min: NavCellCoord::new(0, 0),
        max: NavCellCoord::new(99, 99),
    };
    let snapshot = snapshot_from_sim(
        &sim,
        &[guard_input()],
        &[entity("enemy.near", 1, "player", 1, 0, true)],
        std::slice::from_ref(&huge_blocker),
    )
    .unwrap();
    let result = sense_perception(&snapshot, sim.state_hash());
    assert_error_code(&result, "LITD-AI-INVALID-CONFIG");
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("perception.blocker.area"),
        "expected the blocker-area cap error, got: {error}"
    );

    // A normal single-cell blocker still rasterises and senses fine.
    let ok_blocker = PerceptionBlocker {
        id: "pillar".to_string(),
        min: NavCellCoord::new(2, 2),
        max: NavCellCoord::new(2, 2),
    };
    let ok_snapshot = snapshot_from_sim(
        &sim,
        &[guard_input()],
        &[entity("enemy.near", 1, "player", 1, 0, true)],
        std::slice::from_ref(&ok_blocker),
    )
    .unwrap();
    sense_perception(&ok_snapshot, sim.state_hash())
        .expect("a small blocker must build the occupancy grid and sense without error");

    println!("ISSUE1486_LOS_GRID area_cap_rejects_10000_cell_blocker=true small_blocker_ok=true");
}
