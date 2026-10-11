use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
};

fn issue1320_astar_path_source_truth_fsv() {
    let run = begin_issue_run("issue1320_astar_path").unwrap();
    let navmesh_path = run.run_dir().join("astar-wall-navmesh-aster");
    let path_path = run.run_dir().join("astar-wall-path-aster");
    let before = json!({
        "navmesh": file_state(&navmesh_path),
        "path": file_state(&path_path),
    });

    let navmesh = bake_navmesh(&wall_input()).unwrap();
    let navmesh_readback = write_navmesh_aster(&navmesh_path, &navmesh).unwrap();
    let loaded_navmesh = load_navmesh_aster(&navmesh_path).unwrap().navmesh;
    let query = NavPathQuery::new(coord(0, 0), coord(4, 0), 100);
    let path = find_path_with_budget(&loaded_navmesh, query).unwrap();
    let path_readback = write_nav_path_aster(&path_path, &path).unwrap();
    let disk_path = load_nav_path_aster(&path_path).unwrap().path_result;
    let expected_coords = vec![
        (0, 0),
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 4),
        (2, 4),
        (3, 4),
        (3, 3),
        (3, 2),
        (3, 1),
        (3, 0),
        (4, 0),
    ];
    assert_eq!(path_coords(&disk_path), expected_coords);
    assert_eq!(disk_path.total_cost_fp, 12 * 1024);
    assert_eq!(path_readback.path_result, path);

    let start_goal = start_goal_edge(&loaded_navmesh, run.run_dir());
    let max_grid = max_grid_edge(run.run_dir());
    let empty_navmesh = failing_path_edge(
        "empty_navmesh",
        empty_navmesh(),
        NavPathQuery::new(coord(0, 0), coord(0, 0), 1),
        run.run_dir(),
    );
    let unreachable = failing_path_edge(
        "unreachable",
        bake_navmesh(&barrier_input()).unwrap(),
        NavPathQuery::new(coord(0, 1), coord(2, 1), 32),
        run.run_dir(),
    );
    let out_of_bounds = failing_path_edge(
        "out_of_bounds",
        loaded_navmesh.clone(),
        NavPathQuery::new(coord(-1, 0), coord(4, 0), 32),
        run.run_dir(),
    );
    let budget = failing_path_edge(
        "budget",
        loaded_navmesh.clone(),
        NavPathQuery::new(coord(0, 0), coord(4, 0), 2),
        run.run_dir(),
    );

    let after = json!({
        "navmesh": file_state(&navmesh_path),
        "path": file_state(&path_path),
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1320,
        "source_of_truth": {
            "navmesh_aster_dir": navmesh_path,
            "navmesh_sha256": navmesh_readback.sha256,
            "navmesh_fnv1a64": navmesh_readback.fnv1a64,
            "path_aster_dir": path_path,
            "path_sha256": path_readback.sha256,
            "path_fnv1a64": path_readback.fnv1a64,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "happy_path": {
            "before": before,
            "after": after,
            "expected": {
                "coords": expected_coords,
                "total_cost_fp": 12288,
                "node_count": 13,
            },
            "actual": {
                "coords": path_coords(&disk_path),
                "total_cost_fp": disk_path.total_cost_fp,
                "node_count": disk_path.nodes.len(),
                "expanded_nodes": disk_path.expanded_nodes,
                "frontier_peak": disk_path.frontier_peak,
                "fnv1a64": disk_path.fnv1a64,
            }
        },
        "edges": {
            "start_equals_goal": start_goal,
            "max_grid": max_grid,
            "empty_navmesh": empty_navmesh,
            "unreachable": unreachable,
            "out_of_bounds": out_of_bounds,
            "budget": budget,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["actual"]["node_count"],
        13
    );
    assert_eq!(
        report_readback.json["edges"]["unreachable"]["error"]["code"],
        "LITD-NAV-PATH-NOT-FOUND"
    );
    println!(
        "ISSUE1320_ASTAR_FSV_REPORT path={} bytes={} sha256={} path_aster={} path_sha256={} path_fnv1a64={} node_count={} total_cost_fp={} start_goal_cost={} max_nodes={} empty_code={} unreachable_code={} out_of_bounds_code={} budget_code={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        path_readback.vault_dir.display(),
        path_readback.sha256,
        path_readback.fnv1a64,
        disk_path.nodes.len(),
        disk_path.total_cost_fp,
        report_readback.json["edges"]["start_equals_goal"]["after"]["total_cost_fp"],
        report_readback.json["edges"]["max_grid"]["after"]["node_count"],
        report_readback.json["edges"]["empty_navmesh"]["error"]["code"],
        report_readback.json["edges"]["unreachable"]["error"]["code"],
        report_readback.json["edges"]["out_of_bounds"]["error"]["code"],
        report_readback.json["edges"]["budget"]["error"]["code"],
    );
}

fn start_goal_edge(navmesh: &BakedNavMesh, root: &Path) -> Value {
    let path = root.join("edge-start-goal-path-aster");
    let before = file_state(&path);
    let result = find_path(navmesh, coord(1, 0), coord(1, 0)).unwrap();
    let readback = write_nav_path_aster(&path, &result).unwrap();
    let disk = load_nav_path_aster(&path).unwrap().path_result;
    assert_eq!(disk.nodes.len(), 1);
    assert_eq!(disk.total_cost_fp, 0);
    json!({
        "before": before,
        "after": {
            "exists": path.exists(),
            "is_dir": path.is_dir(),
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "fnv1a64": readback.fnv1a64,
            "node_count": disk.nodes.len(),
            "total_cost_fp": disk.total_cost_fp,
        }
    })
}

fn max_grid_edge(root: &Path) -> Value {
    let path = root.join("edge-max-grid-path-aster");
    let before = file_state(&path);
    let navmesh = bake_navmesh(&open_grid_input(32, 32)).unwrap();
    let query = NavPathQuery::new(coord(0, 0), coord(31, 31), 2048);
    let result = find_path_with_budget(&navmesh, query).unwrap();
    let readback = write_nav_path_aster(&path, &result).unwrap();
    let disk = load_nav_path_aster(&path).unwrap().path_result;
    assert_eq!(disk.nodes.len(), 63);
    assert_eq!(disk.total_cost_fp, 62 * 1024);
    json!({
        "before": before,
        "after": {
            "exists": path.exists(),
            "is_dir": path.is_dir(),
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "fnv1a64": readback.fnv1a64,
            "node_count": disk.nodes.len(),
            "total_cost_fp": disk.total_cost_fp,
            "first": disk.nodes.first(),
            "last": disk.nodes.last(),
        },
        "expected": {
            "node_count": 63,
            "total_cost_fp": 63488,
        }
    })
}

fn failing_path_edge(
    label: &str,
    navmesh: BakedNavMesh,
    query: NavPathQuery,
    root: &Path,
) -> Value {
    let path = root.join(format!("edge-{label}-path-aster"));
    let before = file_state(&path);
    let error = attempt_path_and_write(&navmesh, query, &path).unwrap_err();
    let after = file_state(&path);
    assert!(!path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn attempt_path_and_write(
    navmesh: &BakedNavMesh,
    query: NavPathQuery,
    path: &Path,
) -> Result<(), EngineError> {
    let result = find_path_with_budget(navmesh, query)?;
    write_nav_path_aster(path, &result).map(|_| ())
}

fn wall_input() -> NavBakeInput {
    let cells = rectangular_cells(5, 5);
    NavBakeInput::new(1024, cells, vec![blocker("stone_wall", 2, 0, 2, 3)])
}

fn barrier_input() -> NavBakeInput {
    let cells = rectangular_cells(3, 3);
    NavBakeInput::new(1024, cells, vec![blocker("closed_barrier", 1, 0, 1, 2)])
}

fn open_grid_input(width: i32, depth: i32) -> NavBakeInput {
    NavBakeInput::new(1024, rectangular_cells(width, depth), Vec::new())
}

fn rectangular_cells(width: i32, depth: i32) -> Vec<NavTerrainCellInput> {
    let mut cells = Vec::new();
    for z in 0..depth {
        for x in 0..width {
            cells.push(NavTerrainCellInput {
                id: format!("terrain_cell_{x}_{z}"),
                x,
                z,
                height_fp: 0,
                flags: NavPathingFlags::default(),
            });
        }
    }
    cells
}

fn empty_navmesh() -> BakedNavMesh {
    BakedNavMesh {
        schema: NAVMESH_SCHEMA.to_string(),
        source_input_schema: NAV_BAKE_INPUT_SCHEMA.to_string(),
        cell_size_fp: 1024,
        bounds: BakedNavBounds {
            min_x: 0,
            max_x: 0,
            min_z: 0,
            max_z: 0,
            width: 0,
            depth: 0,
        },
        terrain_cell_count: 0,
        static_blocker_count: 0,
        walkable_cell_count: 0,
        blocked_cell_count: 0,
        boundary_matches_input: false,
        cells: Vec::new(),
        fnv1a64: String::new(),
    }
}

fn coord(x: i32, z: i32) -> NavCellCoord {
    NavCellCoord::new(x, z)
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

fn path_coords(path: &NavPath) -> Vec<(i32, i32)> {
    path.nodes.iter().map(|node| (node.x, node.z)).collect()
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
fn issue1320_latest_fsv_manifest_points_at_real_report() {
    issue1320_astar_path_source_truth_fsv();
    let root = issue_root("issue1320_astar_path");
    let manifest_path = root.join("latest-manifest.json");
    let bytes = fs::read(&manifest_path).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let report_path = Path::new(manifest["report_path"].as_str().unwrap());
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}

#[test]
fn issue1486_navindex_verifies_once_and_preserves_determinism_fsv() {
    let run = begin_issue_run("issue1486_navindex").unwrap();
    // An open 8x8 grid: every cell walkable, so many equal-cost paths exist and the A*
    // tie-break (formerly cell-id String, now integer rank) is exercised hard — exactly
    // where a wrong ordering would diverge.
    let navmesh = bake_navmesh(&open_grid_input(8, 8)).unwrap();
    let index = NavIndex::build(&navmesh).unwrap();

    let queries = [
        NavPathQuery::new(coord(0, 0), coord(7, 7), 512),
        NavPathQuery::new(coord(0, 0), coord(7, 0), 512),
        NavPathQuery::new(coord(7, 7), coord(0, 0), 512),
        NavPathQuery::new(coord(3, 3), coord(5, 1), 512),
    ];

    // (b) Determinism + prebuilt-index correctness: the prebuilt-index query returns a
    // byte-identical NavPath to the one-off (index-per-call) query, and repeating either
    // is byte-identical. This is the #800 determinism gate — same navmesh + query ->
    // identical order output before and after the optimisation.
    for query in queries {
        let via_index = find_path_with_index(&navmesh, &index, query).unwrap();
        let via_budget = find_path_with_budget(&navmesh, query).unwrap();
        let via_budget_again = find_path_with_budget(&navmesh, query).unwrap();
        assert_eq!(
            via_index, via_budget,
            "prebuilt-index path must equal the one-off path"
        );
        assert_eq!(
            via_budget, via_budget_again,
            "repeated query must be byte-identical"
        );
    }

    // (a) Verify + index happen ONCE (at NavIndex::build), NOT per query. Corrupt the
    // navmesh's stored hash after building the index: the prebuilt-index query still
    // succeeds (it never re-verifies), while a fresh find_path_with_budget rebuilds the
    // index -> re-verifies -> fails loud on the corrupted hash. Behavioral proof that the
    // per-query hot path skips the O(cells) integrity re-hash + coordinate re-index.
    let mut corrupted = navmesh.clone();
    corrupted.fnv1a64 = "deadbeefdeadbeef".to_string();
    let query = queries[0];
    let index_on_corrupt = find_path_with_index(&corrupted, &index, query);
    let budget_err = find_path_with_budget(&corrupted, query)
        .expect_err("find_path_with_budget rebuilds+verifies -> must reject the corrupt hash");
    let build_err = NavIndex::build(&corrupted)
        .expect_err("NavIndex::build must verify and reject the corrupt navmesh");
    assert!(
        index_on_corrupt.is_ok(),
        "find_path_with_index must NOT re-verify (it uses the prebuilt index): {index_on_corrupt:?}"
    );

    let report = json!({
        "schema_version": 1,
        "issue": 1486,
        "finding": "navmesh verified+indexed once (not per query); A* output byte-identical",
        "navmesh_cells": navmesh.cells.len(),
        "queries_checked": queries.len(),
        "determinism": "prebuilt-index == one-off == repeated (byte-identical NavPath)",
        "verify_once_proof": {
            "find_path_with_index_ok_on_corrupt_navmesh": index_on_corrupt.is_ok(),
            "find_path_with_budget_rejects_corrupt": error_report(&budget_err),
            "navindex_build_rejects_corrupt": error_report(&build_err),
        },
    });
    let readback = run.write_report(&report).unwrap();
    println!(
        "ISSUE1486_NAVINDEX_FSV path={} bytes={} sha256={} cells={} queries={} index_ok_on_corrupt={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        navmesh.cells.len(),
        queries.len(),
        index_on_corrupt.is_ok(),
    );
}
