use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    nav::{
        NavAvoidanceAgent, NavBakeInput, NavCellCoord, NavPath, NavPathQuery, NavPathingFlags,
        NavStaticBlockerInput, NavTerrainCellInput, bake_navmesh, find_path, find_path_with_budget,
        load_avoidance_aster, load_nav_path_aster, load_navmesh_aster, resolve_local_avoidance,
        write_avoidance_aster, write_nav_path_aster, write_navmesh_aster,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    asset_scene_foliage_brush::{FoliageBrushStore, apply_foliage_brush_state},
    asset_scene_foliage_config::{load_foliage_config, seed_default_foliage_config},
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties, EditorWorldRecord,
        create_editor_world_record, parse_world_record, read_world_record, write_world_record,
    },
};

#[test]
fn issue1359_terrain_world_nav_foliage_lives_in_calyx_aster_fsv() {
    let run = begin_issue_run("issue1359_terrain_world_nav_foliage_calyx").unwrap();
    let world_aster = run.run_dir().join("world-aster");
    let navmesh_aster = run.run_dir().join("navmesh-aster");
    let nav_path_aster = run.run_dir().join("nav-path-aster");
    let avoidance_aster = run.run_dir().join("avoidance-aster");
    let foliage_config_aster = run.run_dir().join("foliage-config-aster");
    let foliage_brush_aster = run.run_dir().join("foliage-brush-aster");
    let before = json!({
        "world": file_state(&world_aster),
        "navmesh": file_state(&navmesh_aster),
        "nav_path": file_state(&nav_path_aster),
        "avoidance": file_state(&avoidance_aster),
        "foliage_config": file_state(&foliage_config_aster),
        "foliage_brush": file_state(&foliage_brush_aster),
    });

    let world_spec = known_world_spec();
    let world_record = create_editor_world_record(world_spec.clone()).unwrap();
    let world_write = write_world_record(&world_aster, &world_record).unwrap();
    let world_read = read_world_record(&world_aster, &world_spec.calyx_document_id)
        .unwrap()
        .unwrap();
    let world_disk = parse_world_record(&world_read.bytes).unwrap();
    assert_eq!(world_write.bytes, world_read.bytes);
    assert_eq!(world_disk.terrain_cell_count, 24);
    assert_eq!(world_disk.default_tile_count, 24);

    let nav_input = nav_input_from_world(&world_disk);
    let navmesh = bake_navmesh(&nav_input).unwrap();
    let navmesh_write = write_navmesh_aster(&navmesh_aster, &navmesh).unwrap();
    let navmesh_disk = load_navmesh_aster(&navmesh_aster).unwrap().navmesh;
    assert_eq!(navmesh_disk, navmesh);
    assert_eq!(navmesh_disk.walkable_cell_count, 23);

    let path_query = NavPathQuery::new(coord(0, 0), coord(5, 3), 128);
    let path = find_path_with_budget(&navmesh_disk, path_query).unwrap();
    let path_write = write_nav_path_aster(&nav_path_aster, &path).unwrap();
    let path_disk = load_nav_path_aster(&nav_path_aster).unwrap().path_result;
    assert_eq!(path_disk, path);
    assert_eq!(path_coords(&path_disk).first(), Some(&(0, 0)));
    assert_eq!(path_coords(&path_disk).last(), Some(&(5, 3)));

    let avoidance_agents = vec![
        agent(
            "unit.alpha",
            coord(0, 0),
            find_path(&navmesh_disk, coord(0, 0), coord(5, 3)).unwrap(),
        ),
        agent(
            "unit.beta",
            coord(0, 1),
            find_path(&navmesh_disk, coord(0, 1), coord(5, 3)).unwrap(),
        ),
    ];
    let avoidance = resolve_local_avoidance(&navmesh_disk, 1359, &avoidance_agents).unwrap();
    let avoidance_write = write_avoidance_aster(&avoidance_aster, &avoidance).unwrap();
    let avoidance_disk = load_avoidance_aster(&avoidance_aster).unwrap().resolution;
    assert_eq!(avoidance_disk, avoidance);

    let config_seed = seed_default_foliage_config(&foliage_config_aster).unwrap();
    let foliage_config = load_foliage_config(&foliage_config_aster).unwrap();
    assert_eq!(foliage_config.prototypes.len(), 2);
    assert_eq!(foliage_config.patches.len(), 18);

    let brush_store = FoliageBrushStore::new(foliage_brush_aster.clone());
    let brush_added = brush_store
        .add_density_brush(&foliage_config, [0.0, -24.0], 8.0, 50.0)
        .unwrap();
    let brush_erased = brush_store.erase_density_brush([0.0, -24.0], 9.0).unwrap();
    let effective_foliage =
        apply_foliage_brush_state(foliage_config.clone(), &brush_erased).unwrap();
    assert_eq!(brush_added.stamps.len(), 1);
    assert_eq!(brush_erased.stamps.len(), 2);
    assert_eq!(effective_foliage.brush.added_patch_count, 1);
    assert_eq!(effective_foliage.brush.erased_patch_count, 1);

    let edges = json!({
        "file_path_rejected_as_aster_vault": file_path_rejection_edge(run.run_dir(), &path_disk),
        "missing_foliage_config_row_fails_closed": missing_config_edge(run.run_dir()),
        "invalid_world_bounds_do_not_persist": invalid_world_bounds_edge(run.run_dir()),
    });
    let after = json!({
        "world": file_state(&world_aster),
        "navmesh": file_state(&navmesh_aster),
        "nav_path": file_state(&nav_path_aster),
        "avoidance": file_state(&avoidance_aster),
        "foliage_config": file_state(&foliage_config_aster),
        "foliage_brush": file_state(&foliage_brush_aster),
    });
    let physical_aster = physical_aster_summary(&[
        ("world", &world_aster),
        ("navmesh", &navmesh_aster),
        ("nav_path", &nav_path_aster),
        ("avoidance", &avoidance_aster),
        ("foliage_config", &foliage_config_aster),
        ("foliage_brush", &foliage_brush_aster),
    ]);
    let legacy_json_outputs = legacy_json_output_paths(run.run_dir());
    assert!(legacy_json_outputs.is_empty(), "{legacy_json_outputs:?}");

    let report = json!({
        "schema_version": 1,
        "issue": 1359,
        "source_of_truth": {
            "store": "Calyx Aster Graph/Base column families",
            "latest_manifest_path": run.latest_manifest_path(),
            "aster_dirs": {
                "world": world_aster,
                "navmesh": navmesh_aster,
                "nav_path": nav_path_aster,
                "avoidance": avoidance_aster,
                "foliage_config": foliage_config_aster,
                "foliage_brush": foliage_brush_aster,
            },
            "physical_aster_files": physical_aster,
            "legacy_json_output_paths": legacy_json_outputs,
        },
        "happy_path": {
            "before": before,
            "after": after,
            "world": {
                "expected": {"terrain_cell_count": 24, "default_tile_count": 24},
                "actual": {
                    "seq": world_read.seq,
                    "graph_rows": world_read.graph_rows,
                    "chunk_count": world_read.chunk_count,
                    "sha256": world_read.sha256,
                    "terrain_cell_count": world_disk.terrain_cell_count,
                    "default_tile_count": world_disk.default_tile_count,
                    "fingerprint": world_disk.document_fingerprint,
                }
            },
            "navmesh": {
                "expected": {"terrain_cell_count": 24, "walkable_cell_count": 23, "blocked_cell_count": 1},
                "actual": {
                    "seq": navmesh_write.seq,
                    "graph_rows": navmesh_write.graph_rows,
                    "bytes": navmesh_write.bytes,
                    "sha256": navmesh_write.sha256,
                    "fnv1a64": navmesh_write.fnv1a64,
                    "terrain_cell_count": navmesh_disk.terrain_cell_count,
                    "walkable_cell_count": navmesh_disk.walkable_cell_count,
                    "blocked_cell_count": navmesh_disk.blocked_cell_count,
                }
            },
            "nav_path": {
                "actual": {
                    "seq": path_write.seq,
                    "graph_rows": path_write.graph_rows,
                    "bytes": path_write.bytes,
                    "sha256": path_write.sha256,
                    "fnv1a64": path_write.fnv1a64,
                    "coords": path_coords(&path_disk),
                    "total_cost_fp": path_disk.total_cost_fp,
                }
            },
            "avoidance": {
                "actual": {
                    "seq": avoidance_write.seq,
                    "graph_rows": avoidance_write.graph_rows,
                    "bytes": avoidance_write.bytes,
                    "sha256": avoidance_write.sha256,
                    "state_fnv1a64": avoidance_write.state_fnv1a64,
                    "assigned": assigned_labels(&avoidance_disk),
                    "final_unique": avoidance_disk.final_unique,
                }
            },
            "foliage": {
                "config": {
                    "seq": config_seed.seq,
                    "graph_rows": config_seed.graph_rows,
                    "bytes": config_seed.byte_len,
                    "sha256": config_seed.sha256,
                    "source": foliage_config.source_relative_path,
                    "prototype_count": foliage_config.prototypes.len(),
                    "patch_count": foliage_config.patches.len(),
                },
                "brush": {
                    "source": brush_erased.source_relative_path,
                    "sha256": brush_erased.source_sha256,
                    "stamp_count": brush_erased.stamps.len(),
                    "added_patch_count": effective_foliage.brush.added_patch_count,
                    "erased_patch_count": effective_foliage.brush.erased_patch_count,
                }
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["world"]["actual"]["terrain_cell_count"],
        24
    );
    assert_eq!(
        report_readback.json["source_of_truth"]["legacy_json_output_paths"],
        json!([])
    );
    println!(
        "ISSUE1359_CALYX_FSV_REPORT path={} bytes={} sha256={} world_seq={} world_graph_rows={} navmesh_sha256={} path_nodes={} avoidance_agents={} foliage_config_sha256={} brush_stamps={} physical_files={} edge_file_code={} edge_missing_config={} edge_invalid_world={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        world_read.seq,
        world_read.graph_rows,
        navmesh_write.sha256,
        path_disk.nodes.len(),
        avoidance_disk.agent_count,
        config_seed.sha256,
        brush_erased.stamps.len(),
        report_readback.json["source_of_truth"]["physical_aster_files"]["total_files"],
        report_readback.json["edges"]["file_path_rejected_as_aster_vault"]["error"]["code"],
        report_readback.json["edges"]["missing_foliage_config_row_fails_closed"]["error"]["code"],
        report_readback.json["edges"]["invalid_world_bounds_do_not_persist"]["error"]["code"],
    );
}

fn known_world_spec() -> EditorWorldCreationSpec {
    EditorWorldCreationSpec {
        calyx_document_id: "calyx://world/issue1359-calyx-state".to_string(),
        revision_id: "issue1359-calyx-state-rev-1".to_string(),
        properties: EditorWorldProperties {
            display_name: "Issue 1359 Calyx State World".to_string(),
            description: "Known terrain/world/nav/foliage migration FSV world".to_string(),
            width_tiles: 6,
            height_tiles: 4,
            random_height_seed: Some(1359),
            playable_bounds: EditorPlayableBounds {
                min_tile_x: 0,
                min_tile_z: 0,
                max_tile_x_exclusive: 6,
                max_tile_z_exclusive: 4,
            },
            ..EditorWorldProperties::default()
        },
    }
}

fn nav_input_from_world(record: &EditorWorldRecord) -> NavBakeInput {
    let mut cells = record
        .document
        .terrain_cells
        .values()
        .map(|cell| NavTerrainCellInput {
            id: cell.id.clone(),
            x: cell.tile_x,
            z: cell.tile_z,
            height_fp: (cell.height * 1024.0).round() as i64,
            flags: NavPathingFlags::default(),
        })
        .collect::<Vec<_>>();
    cells.sort_by_key(|cell| (cell.z, cell.x));
    NavBakeInput::new(1024, cells, vec![blocker("issue1359_rock", 2, 1, 2, 1)])
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

fn path_coords(path: &NavPath) -> Vec<(i32, i32)> {
    path.nodes.iter().map(|node| (node.x, node.z)).collect()
}

fn assigned_labels(resolution: &litd_engine::nav::NavAvoidanceResolution) -> Vec<String> {
    resolution
        .agents
        .iter()
        .map(|agent| format!("{}:{},{}", agent.id, agent.assigned.x, agent.assigned.z))
        .collect()
}

fn file_path_rejection_edge(root: &Path, path_result: &NavPath) -> Value {
    let edge_path = root.join("edge-file-target.txt");
    if edge_path.exists() {
        fs::remove_file(&edge_path).unwrap();
    }
    let before = file_state(&edge_path);
    fs::write(&edge_path, b"not an aster vault directory").unwrap();
    let before_write_hash = sha256_hex(&fs::read(&edge_path).unwrap());
    let error = write_nav_path_aster(&edge_path, path_result).unwrap_err();
    let after = file_state(&edge_path);
    let after_write_hash = sha256_hex(&fs::read(&edge_path).unwrap());
    assert_eq!(before_write_hash, after_write_hash);
    json!({
        "before": before,
        "after": after,
        "file_hash_before_write": before_write_hash,
        "file_hash_after_write": after_write_hash,
        "error": error_report(&error),
    })
}

fn missing_config_edge(root: &Path) -> Value {
    let missing_path = root.join("edge-missing-config-aster");
    if missing_path.exists() {
        fs::remove_dir_all(&missing_path).unwrap();
    }
    let before = file_state(&missing_path);
    let error = load_foliage_config(&missing_path).unwrap_err();
    let after = file_state(&missing_path);
    assert!(!missing_path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn invalid_world_bounds_edge(root: &Path) -> Value {
    let aster_dir = root.join("edge-invalid-world-aster");
    if aster_dir.exists() {
        fs::remove_dir_all(&aster_dir).unwrap();
    }
    let before = file_state(&aster_dir);
    let mut spec = known_world_spec();
    spec.calyx_document_id = "calyx://world/issue1359-invalid-bounds".to_string();
    spec.properties.playable_bounds.max_tile_x_exclusive = 99;
    let error = match create_editor_world_record(spec) {
        Ok(record) => write_world_record(&aster_dir, &record).unwrap_err(),
        Err(error) => error,
    };
    let after = file_state(&aster_dir);
    assert!(!aster_dir.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn physical_aster_summary(stores: &[(&str, &PathBuf)]) -> Value {
    let mut total_files = 0usize;
    let mut store_reports = Vec::new();
    for (label, path) in stores {
        let files = physical_files(path);
        total_files += files.len();
        store_reports.push(json!({
            "label": label,
            "path": path,
            "file_count": files.len(),
            "files": files,
        }));
    }
    json!({
        "total_files": total_files,
        "stores": store_reports,
    })
}

fn physical_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if !root.exists() {
        return files;
    }
    collect_files(root, root, &mut files);
    files.sort();
    files
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            files.push(relative_path(root, &path));
        }
    }
}

fn legacy_json_output_paths(root: &Path) -> Vec<String> {
    let legacy_names = [
        "world.json",
        "navmesh.json",
        "path.json",
        "avoidance.json",
        "foliage-config.json",
        "foliage-brush-state.json",
        "brush.json",
    ];
    let mut paths = Vec::new();
    collect_legacy_json_outputs(root, root, &legacy_names, &mut paths);
    paths.sort();
    paths
}

fn collect_legacy_json_outputs(
    root: &Path,
    current: &Path,
    legacy_names: &[&str],
    paths: &mut Vec<String>,
) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_legacy_json_outputs(root, &path, legacy_names, paths);
        } else if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| legacy_names.contains(&name))
        {
            paths.push(relative_path(root, &path));
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
        .replace('\\', "/")
}

fn file_state(path: &Path) -> Value {
    let metadata = fs::metadata(path).ok();
    let sha256 = metadata
        .as_ref()
        .filter(|metadata| metadata.is_file())
        .map(|_| sha256_hex(&fs::read(path).unwrap()));
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "is_dir": path.is_dir(),
        "is_file": path.is_file(),
        "len": metadata.as_ref().map(|metadata| metadata.len()),
        "sha256": sha256,
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
