use std::path::Path;

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
};

fn issue1321_local_avoidance_source_truth_fsv() {
    let run = begin_issue_run("issue1321_local_avoidance").unwrap();
    let resolution_path = run.run_dir().join("shared-goal-avoidance-aster");
    let before = file_state(&resolution_path);

    let navmesh = bake_navmesh(&line_input(8)).unwrap();
    let agents = shared_goal_agents(&navmesh).unwrap();
    let resolution = resolve_local_avoidance(&navmesh, 77, &agents).unwrap();
    let repeat = resolve_local_avoidance(&navmesh, 77, &agents).unwrap();
    assert_eq!(resolution, repeat);
    assert_eq!(resolution.state_fnv1a64, repeat.state_fnv1a64);
    assert!(resolution.final_unique);
    assert_eq!(assigned_coords(&resolution), vec![(0, 0), (1, 0), (3, 0)]);
    assert_eq!(resolution.agents[2].velocity_x_fp, 1024);

    let readback = write_avoidance_aster(&resolution_path, &resolution).unwrap();
    let disk_resolution = load_avoidance_aster(&resolution_path).unwrap().resolution;
    assert_eq!(disk_resolution, resolution);

    let max_agents = max_agents_edge(run.run_dir());
    let empty_agents = failing_edge("empty_agents", navmesh.clone(), Vec::new(), run.run_dir());
    let duplicate_start = failing_edge(
        "duplicate_start",
        navmesh.clone(),
        duplicate_start_agents(&navmesh).unwrap(),
        run.run_dir(),
    );
    let path_hash_mismatch = failing_edge(
        "path_hash_mismatch",
        navmesh.clone(),
        path_hash_mismatch_agents().unwrap(),
        run.run_dir(),
    );
    let current_not_in_path = failing_edge(
        "current_not_in_path",
        navmesh.clone(),
        current_not_in_path_agents(&navmesh).unwrap(),
        run.run_dir(),
    );

    let after = file_state(&resolution_path);
    let report = json!({
        "schema_version": 1,
        "issue": 1321,
        "source_of_truth": {
            "avoidance_aster_dir": resolution_path,
            "avoidance_sha256": readback.sha256,
            "avoidance_state_fnv1a64": readback.state_fnv1a64,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "happy_path": {
            "before": before,
            "after": after,
            "expected": {
                "agent_count": 3,
                "assigned": ["0,0", "1,0", "3,0"],
                "moved_agent": "unit.c",
                "repeat_hash_equal": true,
            },
            "actual": {
                "agent_count": disk_resolution.agent_count,
                "assigned": assigned_labels(&disk_resolution),
                "velocities": velocity_labels(&disk_resolution),
                "final_unique": disk_resolution.final_unique,
                "state_fnv1a64": disk_resolution.state_fnv1a64,
                "repeat_state_fnv1a64": repeat.state_fnv1a64,
                "repeat_hash_equal": disk_resolution.state_fnv1a64 == repeat.state_fnv1a64,
            }
        },
        "edges": {
            "max_agents": max_agents,
            "empty_agents": empty_agents,
            "duplicate_start": duplicate_start,
            "path_hash_mismatch": path_hash_mismatch,
            "current_not_in_path": current_not_in_path,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["actual"]["assigned"],
        json!(["0,0", "1,0", "3,0"])
    );
    assert_eq!(
        report_readback.json["edges"]["duplicate_start"]["error"]["code"],
        "LITD-NAV-INVALID-CONFIG"
    );
    println!(
        "ISSUE1321_AVOIDANCE_FSV_REPORT path={} bytes={} sha256={} avoidance_aster={} avoidance_sha256={} state_fnv1a64={} assigned={:?} max_agents={} max_unique={} empty_code={} duplicate_code={} mismatch_code={} current_code={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        readback.vault_dir.display(),
        readback.sha256,
        readback.state_fnv1a64,
        assigned_labels(&disk_resolution),
        report_readback.json["edges"]["max_agents"]["after"]["agent_count"],
        report_readback.json["edges"]["max_agents"]["after"]["final_unique"],
        report_readback.json["edges"]["empty_agents"]["error"]["code"],
        report_readback.json["edges"]["duplicate_start"]["error"]["code"],
        report_readback.json["edges"]["path_hash_mismatch"]["error"]["code"],
        report_readback.json["edges"]["current_not_in_path"]["error"]["code"],
    );
}

fn max_agents_edge(root: &Path) -> Value {
    let path = root.join("edge-max-agents-avoidance-aster");
    let before = file_state(&path);
    let navmesh = bake_navmesh(&line_input(128)).unwrap();
    let mut agents = Vec::new();
    for index in 0..64 {
        let from = coord(index, 0);
        let path = find_path(&navmesh, from, coord(127, 0)).unwrap();
        agents.push(agent(&format!("unit.{index:02}"), from, path));
    }
    let resolution = resolve_local_avoidance(&navmesh, 88, &agents).unwrap();
    let readback = write_avoidance_aster(&path, &resolution).unwrap();
    let disk = load_avoidance_aster(&path).unwrap().resolution;
    assert!(disk.final_unique);
    assert_eq!(disk.agent_count, 64);
    json!({
        "before": before,
        "after": {
            "exists": path.exists(),
            "is_dir": path.is_dir(),
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "state_fnv1a64": readback.state_fnv1a64,
            "agent_count": disk.agent_count,
            "final_unique": disk.final_unique,
            "moved_count": disk.agents.iter().filter(|agent| agent.moved).count(),
        },
        "expected": {
            "agent_count": 64,
            "final_unique": true,
        }
    })
}

fn failing_edge(
    label: &str,
    navmesh: BakedNavMesh,
    agents: Vec<NavAvoidanceAgent>,
    root: &Path,
) -> Value {
    let path = root.join(format!("edge-{label}-avoidance-aster"));
    let before = file_state(&path);
    let error = attempt_resolve_and_write(&navmesh, &agents, &path).unwrap_err();
    let after = file_state(&path);
    assert!(!path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn attempt_resolve_and_write(
    navmesh: &BakedNavMesh,
    agents: &[NavAvoidanceAgent],
    path: &Path,
) -> Result<(), EngineError> {
    let resolution = resolve_local_avoidance(navmesh, 99, agents)?;
    write_avoidance_aster(path, &resolution).map(|_| ())
}

fn shared_goal_agents(navmesh: &BakedNavMesh) -> Result<Vec<NavAvoidanceAgent>, EngineError> {
    Ok(vec![
        agent(
            "unit.a",
            coord(0, 0),
            find_path(navmesh, coord(0, 0), coord(7, 0))?,
        ),
        agent(
            "unit.b",
            coord(1, 0),
            find_path(navmesh, coord(1, 0), coord(7, 0))?,
        ),
        agent(
            "unit.c",
            coord(2, 0),
            find_path(navmesh, coord(2, 0), coord(7, 0))?,
        ),
    ])
}

fn duplicate_start_agents(navmesh: &BakedNavMesh) -> Result<Vec<NavAvoidanceAgent>, EngineError> {
    Ok(vec![
        agent(
            "unit.a",
            coord(0, 0),
            find_path(navmesh, coord(0, 0), coord(7, 0))?,
        ),
        agent(
            "unit.b",
            coord(0, 0),
            find_path(navmesh, coord(0, 0), coord(7, 0))?,
        ),
    ])
}

fn path_hash_mismatch_agents() -> Result<Vec<NavAvoidanceAgent>, EngineError> {
    let other = bake_navmesh(&line_input(9)).unwrap();
    Ok(vec![agent(
        "unit.bad_hash",
        coord(0, 0),
        find_path(&other, coord(0, 0), coord(8, 0))?,
    )])
}

fn current_not_in_path_agents(
    navmesh: &BakedNavMesh,
) -> Result<Vec<NavAvoidanceAgent>, EngineError> {
    Ok(vec![agent(
        "unit.off_path",
        coord(0, 0),
        find_path(navmesh, coord(1, 0), coord(7, 0))?,
    )])
}

fn line_input(width: i32) -> NavBakeInput {
    let mut cells = Vec::new();
    for x in 0..width {
        cells.push(NavTerrainCellInput {
            id: format!("terrain_cell_{x}_0"),
            x,
            z: 0,
            height_fp: 0,
            flags: NavPathingFlags::default(),
        });
    }
    NavBakeInput::new(1024, cells, Vec::new())
}

fn agent(id: &str, current: NavCellCoord, path: NavPath) -> NavAvoidanceAgent {
    NavAvoidanceAgent {
        id: id.to_string(),
        current,
        path,
    }
}

fn coord(x: i32, z: i32) -> NavCellCoord {
    NavCellCoord::new(x, z)
}

fn assigned_coords(resolution: &NavAvoidanceResolution) -> Vec<(i32, i32)> {
    resolution
        .agents
        .iter()
        .map(|agent| (agent.assigned.x, agent.assigned.z))
        .collect()
}

fn assigned_labels(resolution: &NavAvoidanceResolution) -> Vec<String> {
    resolution
        .agents
        .iter()
        .map(|agent| format!("{},{}", agent.assigned.x, agent.assigned.z))
        .collect()
}

fn velocity_labels(resolution: &NavAvoidanceResolution) -> Vec<String> {
    resolution
        .agents
        .iter()
        .map(|agent| {
            format!(
                "{}:{},{}",
                agent.id, agent.velocity_x_fp, agent.velocity_z_fp
            )
        })
        .collect()
}

fn file_state(path: &Path) -> Value {
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "is_dir": path.is_dir(),
        "is_file": path.is_file(),
    })
}

fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}

#[test]
fn issue1321_latest_fsv_manifest_points_at_real_report() {
    issue1321_local_avoidance_source_truth_fsv();
    let root = issue_root("issue1321_local_avoidance");
    let manifest_path = root.join("latest-manifest.json");
    let bytes = std::fs::read(&manifest_path).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let report_path = Path::new(manifest["report_path"].as_str().unwrap());
    let report_bytes = std::fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}
