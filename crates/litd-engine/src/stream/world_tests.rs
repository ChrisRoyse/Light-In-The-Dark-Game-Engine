use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
    math::Vec3,
};

use super::super::world_store::{
    WORLD_ASTER_COLLECTION, WORLD_ASTER_MANIFEST_COLUMN, WorldRecordChunkManifest,
    chunk_column_name, open_aster, row_key,
};
use super::*;

fn issue1328_world_chunk_streaming_source_truth_fsv() {
    let run = begin_issue_run("issue1328_world_chunk_streaming").unwrap();
    let happy_dir = run.artifact_dir("happy-aster").unwrap();
    let happy_doc = "calyx://world/issue1328-happy";
    let happy_before = dir_state(&happy_dir);
    let happy_write = write_test_world_to_aster(&happy_dir, happy_doc, 64, 64, None);
    let config = WorldChunkStreamConfig {
        spatial_chunk_tiles: 16,
        load_radius_chunks: 1,
        unload_radius_chunks: 1,
    };
    let job = WorldStreamer::load_background(WorldChunkStreamLoadRequest::new(
        &happy_dir, happy_doc, config,
    ));
    let mut streamer = job.finish().unwrap();
    let source = streamer.source_readback();
    assert!(source.loaded_on_background_thread);
    assert!(source.storage_chunk_count > 1);
    let empty_state = streamer.state_readback();
    let center = streamer
        .update_anchor(WorldStreamAnchor {
            position: Vec3::new(33.0, 0.0, 33.0),
        })
        .unwrap();
    assert_eq!(center.after.active_chunks, expected_square(2, 2, 1, 4, 4));
    let corner = streamer
        .update_anchor(WorldStreamAnchor {
            position: Vec3::new(1.0, 0.0, 1.0),
        })
        .unwrap();
    assert_eq!(corner.after.active_chunks, expected_square(0, 0, 1, 4, 4));
    assert!(corner.after.resident_bytes < center.after.resident_bytes);
    assert!(corner.after.resident_cell_count < center.after.resident_cell_count);

    let single_edge = single_chunk_edge(run.artifact_dir("edge-single-aster").unwrap());
    let boundary_edge = boundary_straddle_edge(run.artifact_dir("edge-boundary-aster").unwrap());
    let missing_edge = missing_chunk_edge(run.artifact_dir("edge-missing-aster").unwrap());
    let report = json!({
        "schema_version": 1,
        "issue": 1328,
        "source_of_truth": {
            "aster_dir": happy_dir,
            "storage_schema": WORLD_ASTER_CHUNK_SCHEMA,
            "stream_schema": WORLD_STREAM_SCHEMA,
            "source_sha256": source.source_sha256,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "research": {
            "unreal_world_partition": "https://dev.epicgames.com/documentation/unreal-engine/world-partition-in-unreal-engine?lang=en-US",
            "esengine_streaming_system": "https://esengine.cn/en/modules/world-streaming/streaming-system/"
        },
        "happy_path": {
            "before": {
                "aster_dir": happy_before,
                "active_state": empty_state,
            },
            "after": {
                "aster_dir": dir_state(&happy_dir),
                "source": source,
                "center_update": center,
                "corner_update": corner,
            },
            "expected": {
                "storage_chunk_count_gt_one": true,
                "source_cell_count": 4096,
                "center_active_chunks": expected_square(2, 2, 1, 4, 4),
                "corner_active_chunks": expected_square(0, 0, 1, 4, 4),
                "corner_memory_drops": true,
            },
            "write_readback": happy_write,
        },
        "edges": {
            "single_chunk": single_edge,
            "boundary_straddle": boundary_edge,
            "missing_chunk": missing_edge,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["after"]["source"]["source_cell_count"],
        4096
    );
    assert_eq!(
        report_readback.json["edges"]["missing_chunk"]["error"]["code"],
        "LITD-STREAM-PERSISTENCE-FAILED"
    );
    println!(
        "ISSUE1328_WORLD_STREAM_FSV report={} report_sha256={} aster_dir={} storage_chunks={} graph_rows={} source_sha256={} center_active={} corner_active={} center_bytes={} corner_bytes={} single_active={} boundary_before={:?} boundary_after={:?} missing_code={}",
        report_readback.path.display(),
        report_readback.sha256,
        happy_dir.display(),
        report_readback.json["happy_path"]["after"]["source"]["storage_chunk_count"],
        report_readback.json["happy_path"]["after"]["source"]["graph_rows"],
        report_readback.json["happy_path"]["after"]["source"]["source_sha256"],
        report_readback.json["happy_path"]["after"]["center_update"]["after"]["active_chunks"]
            .as_array()
            .unwrap()
            .len(),
        report_readback.json["happy_path"]["after"]["corner_update"]["after"]["active_chunks"]
            .as_array()
            .unwrap()
            .len(),
        report_readback.json["happy_path"]["after"]["center_update"]["after"]["resident_bytes"],
        report_readback.json["happy_path"]["after"]["corner_update"]["after"]["resident_bytes"],
        report_readback.json["edges"]["single_chunk"]["after"]["active_chunks"]
            .as_array()
            .unwrap()
            .len(),
        report_readback.json["edges"]["boundary_straddle"]["before"]["after"]["active_chunks"],
        report_readback.json["edges"]["boundary_straddle"]["after"]["after"]["active_chunks"],
        report_readback.json["edges"]["missing_chunk"]["error"]["code"],
    );
}

#[test]
fn issue1328_latest_fsv_manifest_points_at_real_report() {
    issue1328_world_chunk_streaming_source_truth_fsv();
    let root = issue_root("issue1328_world_chunk_streaming");
    let manifest_path = root.join("latest-manifest.json");
    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).expect("manifest json");
    let report_path = manifest["report_path"].as_str().expect("report path");
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}

#[derive(Debug, Clone, serde::Serialize)]
struct TestAsterWrite {
    dir: PathBuf,
    document_id: String,
    bytes: usize,
    sha256: String,
    manifest_bytes: usize,
    chunk_count: usize,
    graph_rows: usize,
    omitted_chunk: Option<usize>,
}

fn single_chunk_edge(aster_dir: PathBuf) -> Value {
    let doc = "calyx://world/issue1328-single";
    let before = dir_state(&aster_dir);
    let write = write_test_world_to_aster(&aster_dir, doc, 8, 8, None);
    let mut streamer = WorldStreamer::from_aster(WorldChunkStreamLoadRequest::new(
        &aster_dir,
        doc,
        WorldChunkStreamConfig {
            spatial_chunk_tiles: 16,
            load_radius_chunks: 2,
            unload_radius_chunks: 2,
        },
    ))
    .unwrap();
    let after = streamer
        .update_anchor(WorldStreamAnchor {
            position: Vec3::new(0.0, 0.0, 0.0),
        })
        .unwrap()
        .after;
    assert_eq!(after.active_chunks, vec![WorldChunkCoord { x: 0, z: 0 }]);
    json!({ "before": before, "after": after, "write_readback": write })
}

fn boundary_straddle_edge(aster_dir: PathBuf) -> Value {
    let doc = "calyx://world/issue1328-boundary";
    let write = write_test_world_to_aster(&aster_dir, doc, 32, 16, None);
    let mut streamer = WorldStreamer::from_aster(WorldChunkStreamLoadRequest::new(
        &aster_dir,
        doc,
        WorldChunkStreamConfig {
            spatial_chunk_tiles: 16,
            load_radius_chunks: 0,
            unload_radius_chunks: 0,
        },
    ))
    .unwrap();
    let before = streamer
        .update_anchor(WorldStreamAnchor {
            position: Vec3::new(15.9, 0.0, 1.0),
        })
        .unwrap();
    let after = streamer
        .update_anchor(WorldStreamAnchor {
            position: Vec3::new(16.0, 0.0, 1.0),
        })
        .unwrap();
    assert_eq!(
        before.after.active_chunks,
        vec![WorldChunkCoord { x: 0, z: 0 }]
    );
    assert_eq!(
        after.after.active_chunks,
        vec![WorldChunkCoord { x: 1, z: 0 }]
    );
    json!({ "write_readback": write, "before": before, "after": after })
}

fn missing_chunk_edge(aster_dir: PathBuf) -> Value {
    let doc = "calyx://world/issue1328-missing";
    let before = dir_state(&aster_dir);
    let write = write_test_world_to_aster(&aster_dir, doc, 64, 64, Some(1));
    let error = WorldStreamer::from_aster(WorldChunkStreamLoadRequest::new(
        &aster_dir,
        doc,
        WorldChunkStreamConfig::default(),
    ))
    .unwrap_err();
    assert_eq!(error.code(), "LITD-STREAM-PERSISTENCE-FAILED");
    json!({
        "before": before,
        "after": dir_state(&aster_dir),
        "write_readback": write,
        "error": error_report(&error),
    })
}

fn write_test_world_to_aster(
    dir: &Path,
    document_id: &str,
    width: u32,
    height: u32,
    omitted_chunk: Option<usize>,
) -> TestAsterWrite {
    fs::create_dir_all(dir).unwrap();
    let bytes = world_record_bytes(document_id, width, height);
    let sha256 = sha256_hex(&bytes);
    let chunks = bytes.chunks(256 * 1024).map(Vec::from).collect::<Vec<_>>();
    let manifest = WorldRecordChunkManifest {
        schema: WORLD_ASTER_CHUNK_SCHEMA.to_string(),
        calyx_document_id: document_id.to_string(),
        document_fingerprint: fingerprint(document_id, width, height),
        total_bytes: bytes.len(),
        sha256: sha256.clone(),
        chunk_size: 256 * 1024,
        chunk_count: chunks.len(),
        chunk_columns: (0..chunks.len())
            .map(|index| chunk_column_name(&sha256, index))
            .collect(),
    };
    let vault = open_aster(dir).unwrap();
    let column = PlainColumn::new(&vault, WORLD_ASTER_COLLECTION).unwrap();
    let row_key = row_key(document_id).unwrap();
    for (index, chunk) in chunks.iter().enumerate() {
        if omitted_chunk == Some(index) {
            continue;
        }
        column
            .put(&row_key, manifest.chunk_columns[index].as_bytes(), chunk)
            .unwrap();
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    let commit = column
        .put(&row_key, WORLD_ASTER_MANIFEST_COLUMN, &manifest_bytes)
        .unwrap();
    let manifest_readback = column
        .get(commit.seq, &row_key, WORLD_ASTER_MANIFEST_COLUMN)
        .unwrap()
        .unwrap();
    assert_eq!(manifest_readback, manifest_bytes);
    let graph_rows = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .unwrap()
        .len();
    TestAsterWrite {
        dir: dir.to_path_buf(),
        document_id: document_id.to_string(),
        bytes: bytes.len(),
        sha256,
        manifest_bytes: manifest_readback.len(),
        chunk_count: chunks.len(),
        graph_rows,
        omitted_chunk,
    }
}

fn world_record_bytes(document_id: &str, width: u32, height: u32) -> Vec<u8> {
    let mut cells = BTreeMap::new();
    for z in 0..height {
        for x in 0..width {
            let id = format!("terrain_cell_{x}_{z}");
            cells.insert(
                id.clone(),
                json!({
                    "id": id,
                    "tile_x": x as i32,
                    "tile_z": z as i32,
                    "tile_id": "tile.ashen_grass.default",
                    "cliff_type": "cliff.natural",
                    "cliff_level": 0,
                    "height": ((x + z) % 7) as f32 * 0.25,
                }),
            );
        }
    }
    let properties = json!({
        "display_name": "Issue 1328 Stream World",
        "description": "Synthetic known world for real Aster streaming FSV",
        "width_tiles": width,
        "height_tiles": height,
        "tileset_id": "tileset.ashen_grass",
        "initial_tile_id": "tile.ashen_grass.default",
        "initial_cliff_type": "cliff.natural",
        "initial_cliff_level": 0,
        "water_level": -2,
        "random_height_seed": 1328,
        "playable_bounds": {
            "min_tile_x": 0,
            "min_tile_z": 0,
            "max_tile_x_exclusive": width as i32,
            "max_tile_z_exclusive": height as i32,
        },
        "loading_screen_asset_ref": "docs/readme/ED01-editor-shell.png",
        "preview_asset_ref": "docs/readme/ED11-world-view.png",
    });
    serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "source_issue": 1210,
        "record_kind": "litd.editor.world_record",
        "capability_operation_id": "editor.world.create",
        "readback_tool_name": "litd.editor.world.readback",
        "spec": {
            "calyx_document_id": document_id,
            "revision_id": "issue1328-stream-rev-1",
            "properties": properties,
        },
        "document": {
            "calyx_document_id": document_id,
            "revision_id": "issue1328-stream-rev-1",
            "terrain_cells": cells,
        },
        "document_fingerprint": fingerprint(document_id, width, height),
        "terrain_cell_count": (width * height) as usize,
        "default_tile_count": (width * height) as usize,
        "playable_region_id": "playable_bounds",
        "summary": format!("issue1328 stream world {width}x{height}"),
    }))
    .unwrap()
}

fn expected_square(
    center_x: i32,
    center_z: i32,
    radius: i32,
    width_chunks: i32,
    height_chunks: i32,
) -> Vec<WorldChunkCoord> {
    let mut coords = Vec::new();
    for z in 0..height_chunks {
        for x in 0..width_chunks {
            if (x - center_x).abs().max((z - center_z).abs()) <= radius {
                coords.push(WorldChunkCoord { x, z });
            }
        }
    }
    coords.sort();
    coords
}

fn fingerprint(document_id: &str, width: u32, height: u32) -> String {
    format!("issue1328:{document_id}:{width}x{height}")
}

fn dir_state(path: &Path) -> Value {
    let file_count = fs::read_dir(path)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0);
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "entries": file_count,
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
