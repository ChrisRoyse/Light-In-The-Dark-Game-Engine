use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
};

#[test]
fn issue1319_navmesh_bake_source_truth_fsv() {
    let run = begin_issue_run("issue1319_navmesh_bake").unwrap();
    let navmesh_path = run.run_dir().join("ashen-reach-navmesh-aster");
    let before = file_state(&navmesh_path);

    let input = known_input_3x3();
    let mesh = bake_navmesh(&input).unwrap();
    assert_eq!(mesh.terrain_cell_count, 9);
    assert_eq!(mesh.walkable_cell_count, 7);
    assert_eq!(mesh.blocked_cell_count, 2);
    assert!(mesh.boundary_matches_input);

    let readback = write_navmesh_aster(&navmesh_path, &mesh).unwrap();
    let disk_mesh = load_navmesh_aster(&navmesh_path).unwrap().navmesh;
    assert_eq!(disk_mesh, mesh);
    assert_eq!(readback.fnv1a64, mesh.fnv1a64);
    assert_eq!(load_navmesh_aster(&navmesh_path).unwrap().navmesh, mesh);

    let unchanged_rebake = rebake_navmesh(&input, Some(&mesh)).unwrap();
    assert!(!unchanged_rebake.changed);
    let mut edited_input = input.clone();
    edited_input
        .static_blockers
        .push(blocker("destructible.closed", 0, 2, 0, 2));
    let changed_rebake = rebake_navmesh(&edited_input, Some(&mesh)).unwrap();
    assert!(changed_rebake.changed);
    assert_eq!(changed_rebake.navmesh.walkable_cell_count, 6);

    let max_grid = max_grid_edge(run.run_dir());
    let empty_edge = failing_edge(
        "empty",
        NavBakeInput::new(1024, Vec::new(), Vec::new()),
        run.run_dir(),
    );
    let duplicate_edge = failing_edge("duplicate_coord", duplicate_coord_input(), run.run_dir());
    let schema_edge = failing_edge("invalid_schema", invalid_schema_input(), run.run_dir());
    let blocker_edge = failing_edge("blocker_outside", blocker_outside_input(), run.run_dir());

    let after = file_state(&navmesh_path);
    let report = json!({
        "schema_version": 1,
        "issue": 1319,
        "source_of_truth": {
            "navmesh_aster_dir": navmesh_path,
            "navmesh_bytes": readback.bytes,
            "navmesh_sha256": readback.sha256,
            "navmesh_fnv1a64": readback.fnv1a64,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "happy_path": {
            "before": before,
            "after": after,
            "expected": {
                "terrain_cell_count": 9,
                "walkable_cell_count": 7,
                "blocked_cell_count": 2,
                "bounds": {"min_x": 0, "max_x": 2, "min_z": 0, "max_z": 2}
            },
            "actual": {
                "terrain_cell_count": disk_mesh.terrain_cell_count,
                "walkable_cell_count": disk_mesh.walkable_cell_count,
                "blocked_cell_count": disk_mesh.blocked_cell_count,
                "bounds": disk_mesh.bounds,
                "fnv1a64": disk_mesh.fnv1a64,
            },
            "rebake": {
                "unchanged_changed": unchanged_rebake.changed,
                "edited_changed": changed_rebake.changed,
                "edited_walkable_cell_count": changed_rebake.navmesh.walkable_cell_count,
            }
        },
        "edges": {
            "max_grid": max_grid,
            "empty_input": empty_edge,
            "duplicate_coord": duplicate_edge,
            "invalid_schema": schema_edge,
            "blocker_outside": blocker_edge,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["actual"]["walkable_cell_count"],
        7
    );
    assert_eq!(
        report_readback.json["edges"]["empty_input"]["after"]["exists"],
        false
    );
    println!(
        "ISSUE1319_NAVMESH_FSV_REPORT path={} bytes={} sha256={} navmesh_aster={} navmesh_sha256={} navmesh_fnv1a64={} walkable={} blocked={} max_walkable={} empty_error={} duplicate_error={} schema_error={} blocker_error={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        readback.vault_dir.display(),
        readback.sha256,
        readback.fnv1a64,
        disk_mesh.walkable_cell_count,
        disk_mesh.blocked_cell_count,
        report_readback.json["edges"]["max_grid"]["after"]["walkable_cell_count"],
        report_readback.json["edges"]["empty_input"]["error"]["code"],
        report_readback.json["edges"]["duplicate_coord"]["error"]["code"],
        report_readback.json["edges"]["invalid_schema"]["error"]["code"],
        report_readback.json["edges"]["blocker_outside"]["error"]["code"],
    );
}

fn max_grid_edge(root: &Path) -> Value {
    let path = root.join("edge-max-grid-navmesh-aster");
    let before = file_state(&path);
    let mut cells = Vec::new();
    for z in 0..64 {
        for x in 0..64 {
            cells.push(cell(&format!("terrain_cell_{x}_{z}"), x, z, 2048));
        }
    }
    let input = NavBakeInput::new(1024, cells, vec![blocker("north_wall", 0, 63, 63, 63)]);
    let mesh = bake_navmesh(&input).unwrap();
    let readback = write_navmesh_aster(&path, &mesh).unwrap();
    let disk = load_navmesh_aster(&path).unwrap();
    assert_eq!(disk.navmesh.walkable_cell_count, 4032);
    json!({
        "before": before,
        "after": {
            "exists": path.exists(),
            "is_dir": path.is_dir(),
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "fnv1a64": readback.fnv1a64,
            "terrain_cell_count": disk.navmesh.terrain_cell_count,
            "walkable_cell_count": disk.navmesh.walkable_cell_count,
            "blocked_cell_count": disk.navmesh.blocked_cell_count,
        },
        "expected": {
            "terrain_cell_count": 4096,
            "walkable_cell_count": 4032,
            "blocked_cell_count": 64,
        }
    })
}

fn failing_edge(label: &str, input: NavBakeInput, root: &Path) -> Value {
    let path = root.join(format!("edge-{label}-navmesh-aster"));
    let before = file_state(&path);
    let error = attempt_bake_and_write(&input, &path).unwrap_err();
    let after = file_state(&path);
    assert!(!path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn attempt_bake_and_write(input: &NavBakeInput, path: &Path) -> Result<(), EngineError> {
    let mesh = bake_navmesh(input)?;
    write_navmesh_aster(path, &mesh).map(|_| ())
}

fn known_input_3x3() -> NavBakeInput {
    let mut cells = Vec::new();
    for z in 0..3 {
        for x in 0..3 {
            let mut terrain = cell(
                &format!("terrain_cell_{x}_{z}"),
                x,
                z,
                1024 + (x + z) as i64,
            );
            if x == 2 && z == 0 {
                terrain.flags = NavPathingFlags {
                    walkable: false,
                    buildable: false,
                    flyable: false,
                    blocked: true,
                };
            }
            cells.push(terrain);
        }
    }
    NavBakeInput::new(1024, cells, vec![blocker("forge_footprint", 1, 1, 1, 1)])
}

fn duplicate_coord_input() -> NavBakeInput {
    NavBakeInput::new(
        1024,
        vec![cell("terrain_a", 0, 0, 1000), cell("terrain_b", 0, 0, 1000)],
        Vec::new(),
    )
}

fn invalid_schema_input() -> NavBakeInput {
    let mut input = known_input_3x3();
    input.schema = "litd.navmesh.bake_input.v0".to_string();
    input
}

fn blocker_outside_input() -> NavBakeInput {
    let mut input = known_input_3x3();
    input.static_blockers.push(blocker("outside", 3, 0, 3, 0));
    input
}

fn cell(id: &str, x: i32, z: i32, height_fp: i64) -> NavTerrainCellInput {
    NavTerrainCellInput {
        id: id.to_string(),
        x,
        z,
        height_fp,
        flags: NavPathingFlags::default(),
    }
}

fn blocker(id: &str, min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> NavStaticBlockerInput {
    NavStaticBlockerInput {
        id: id.to_string(),
        min_x,
        min_z,
        max_x,
        max_z,
    }
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
fn issue1319_latest_fsv_manifest_points_at_real_report() {
    let root = issue_root("issue1319_navmesh_bake");
    let manifest_path = root.join("latest-manifest.json");
    if !manifest_path.exists() {
        return;
    }
    let bytes = fs::read(&manifest_path).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let report_path = Path::new(manifest["report_path"].as_str().unwrap());
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}
