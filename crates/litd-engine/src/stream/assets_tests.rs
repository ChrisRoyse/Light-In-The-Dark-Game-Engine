use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
};

use super::*;

fn issue1329_asset_lod_streaming_source_truth_fsv() {
    let run = begin_issue_run("issue1329_asset_lod_streaming").unwrap();
    let asset_dir = run.artifact_dir("asset-lod-source").unwrap();
    let product_id = "issue1329.mesh.known";
    let before = dir_state(&asset_dir);
    let written = write_lod_product(&asset_dir, product_id);
    let config = AssetLodStreamConfig::new(vec![
        AssetLodDistanceBand::new(0, 50_000),
        AssetLodDistanceBand::new(1, 200_000),
        AssetLodDistanceBand::new(2, 500_000),
    ]);
    let request = AssetLodStreamLoadRequest::new(&written.manifest_path, config.clone()).unwrap();
    let runtime = RuntimeAssetLod::load_background(request).finish().unwrap();
    let source = runtime.source_readback().clone();
    assert!(source.loaded_on_background_thread);
    assert_eq!(source.level_count, 3);

    let happy = load_placements(&runtime, &[(0, 0), (25_000, 0), (125_000, 1), (400_000, 2)]);
    let selected_lods = happy
        .iter()
        .map(|placement| placement.readback.selection.level_index)
        .collect::<Vec<_>>();
    assert_eq!(selected_lods, vec![0, 0, 1, 2]);
    assert!(
        happy
            .iter()
            .all(|placement| placement.readback.loaded_on_background_thread)
    );

    let missing_lod = missing_lod_edge(&written.manifest_path);
    let zero_distance = zero_distance_edge(&runtime);
    let max_distance = max_distance_edge(&runtime);
    let beyond_max = beyond_max_edge(&runtime);
    let all_expected_observed = selected_lods == vec![0, 0, 1, 2]
        && source.loaded_on_background_thread
        && happy.iter().all(|placement| placement.physical_bytes_match)
        && missing_lod["error"]["code"] == "LITD-ASSET-STREAM-INVALID-CONFIG"
        && zero_distance["readback"]["selection"]["level_index"] == 0
        && max_distance["readback"]["selection"]["level_index"] == 2
        && beyond_max["error"]["code"] == "LITD-ASSET-STREAM-INVALID-CONFIG";
    assert!(all_expected_observed);

    let report = json!({
        "schema_version": 1,
        "issue": 1329,
        "source_of_truth": {
            "asset_dir": asset_dir,
            "manifest_path": written.manifest_path,
            "manifest_sha256": written.manifest_sha256,
            "stream_schema": ASSET_LOD_STREAM_SCHEMA,
            "latest_manifest_path": run.latest_manifest_path(),
            "lod_files": written.level_files,
        },
        "research": {
            "unreal_async_asset_loading": "https://dev.epicgames.com/documentation/unreal-engine/asynchronous-asset-loading-in-unreal-engine?lang=en-US",
            "unity_lod_group": "https://docs.unity3d.com/6000.4/Documentation/Manual/class-LODGroup.html",
            "bevy_asset_server": "https://docs.rs/bevy/latest/bevy/asset/struct.AssetServer.html",
        },
        "happy_path": {
            "before": {
                "asset_dir": before,
            },
            "after": {
                "asset_dir": dir_state(&asset_dir),
                "source": source,
                "placements": happy,
            },
            "expected": {
                "distances_millimeters": [0, 25000, 125000, 400000],
                "selected_lod_indices": [0, 0, 1, 2],
                "all_loads_background_thread": true,
                "all_loaded_bytes_match_disk": true,
            },
            "write_readback": written,
        },
        "edges": {
            "missing_lod": missing_lod,
            "zero_distance": zero_distance,
            "max_distance": max_distance,
            "beyond_max": beyond_max,
        },
        "all_expected_observed": all_expected_observed,
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["after"]["placements"][2]["readback"]["selection"]["level_index"],
        1
    );
    assert_eq!(
        report_readback.json["edges"]["missing_lod"]["error"]["code"],
        "LITD-ASSET-STREAM-INVALID-CONFIG"
    );
    assert_eq!(report_readback.json["all_expected_observed"], true);
    println!(
        "ISSUE1329_ASSET_LOD_STREAM_FSV report={} report_sha256={} manifest={} manifest_sha256={} levels={} total_index_bytes={} selected_lods={:?} all_background={} zero_lod={} max_lod={} missing_code={} beyond_code={}",
        report_readback.path.display(),
        report_readback.sha256,
        written.manifest_path.display(),
        written.manifest_sha256,
        report_readback.json["happy_path"]["after"]["source"]["level_count"],
        report_readback.json["happy_path"]["after"]["source"]["total_index_buffer_bytes"],
        selected_lods,
        happy
            .iter()
            .all(|placement| placement.readback.loaded_on_background_thread),
        report_readback.json["edges"]["zero_distance"]["readback"]["selection"]["level_index"],
        report_readback.json["edges"]["max_distance"]["readback"]["selection"]["level_index"],
        report_readback.json["edges"]["missing_lod"]["error"]["code"],
        report_readback.json["edges"]["beyond_max"]["error"]["code"],
    );
}

#[test]
fn issue1329_latest_fsv_manifest_points_at_real_report() {
    issue1329_asset_lod_streaming_source_truth_fsv();
    let root = issue_root("issue1329_asset_lod_streaming");
    let manifest_path = root.join("latest-manifest.json");
    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).expect("manifest json");
    let report_path = manifest["report_path"].as_str().expect("report path");
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}

#[derive(Debug, Clone, Serialize)]
struct TestLodProductWrite {
    manifest_path: PathBuf,
    manifest_bytes: u64,
    manifest_sha256: String,
    level_files: Vec<FileState>,
}

#[derive(Debug, Clone, Serialize)]
struct PlacementEvidence {
    distance_millimeters: u64,
    expected_lod_index: usize,
    before_file: FileState,
    readback: AssetLodLoadReadback,
    after_file: FileState,
    physical_bytes_match: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FileState {
    path: String,
    bytes: u64,
    sha256: String,
}

fn load_placements(runtime: &RuntimeAssetLod, cases: &[(u64, usize)]) -> Vec<PlacementEvidence> {
    cases
        .iter()
        .map(|(distance, expected_lod_index)| {
            let selection = runtime.select_lod(*distance).unwrap();
            assert_eq!(selection.level_index, *expected_lod_index);
            let before_file = file_state(&selection.index_buffer_path);
            let receipt = runtime
                .load_selected_background(selection.clone())
                .finish()
                .unwrap();
            let disk = fs::read(&selection.index_buffer_path).unwrap();
            assert_eq!(receipt.bytes.as_ref(), disk.as_slice());
            let after_file = file_state(&selection.index_buffer_path);
            assert_eq!(after_file.sha256, receipt.readback.verified_sha256);
            let readback = receipt.readback.clone();
            PlacementEvidence {
                distance_millimeters: *distance,
                expected_lod_index: *expected_lod_index,
                before_file,
                readback,
                after_file,
                physical_bytes_match: receipt.bytes.as_ref() == disk.as_slice(),
            }
        })
        .collect()
}

fn missing_lod_edge(manifest_path: &Path) -> Value {
    let bad_config = AssetLodStreamConfig::new(vec![
        AssetLodDistanceBand::new(0, 50_000),
        AssetLodDistanceBand::new(2, 200_000),
        AssetLodDistanceBand::new(3, 500_000),
    ]);
    let before = file_state(manifest_path);
    let request = AssetLodStreamLoadRequest::new(manifest_path, bad_config).unwrap();
    let error = RuntimeAssetLod::load_background(request)
        .finish()
        .unwrap_err();
    assert_eq!(error.code(), "LITD-ASSET-STREAM-INVALID-CONFIG");
    json!({
        "before": before,
        "after": file_state(manifest_path),
        "error": error_report(&error),
    })
}

fn zero_distance_edge(runtime: &RuntimeAssetLod) -> Value {
    let selection = runtime.select_lod(0).unwrap();
    assert_eq!(selection.level_index, 0);
    let before = file_state(&selection.index_buffer_path);
    let receipt = runtime
        .load_selected_background(selection)
        .finish()
        .unwrap();
    assert!(receipt.readback.loaded_on_background_thread);
    let readback = receipt.readback.clone();
    json!({
        "before": before,
        "readback": readback.clone(),
        "after": file_state(&readback.selection.index_buffer_path),
    })
}

fn max_distance_edge(runtime: &RuntimeAssetLod) -> Value {
    let selection = runtime.select_lod(500_000).unwrap();
    assert_eq!(selection.level_index, 2);
    let before = file_state(&selection.index_buffer_path);
    let receipt = runtime
        .load_selected_background(selection)
        .finish()
        .unwrap();
    assert!(receipt.readback.loaded_on_background_thread);
    let readback = receipt.readback.clone();
    json!({
        "before": before,
        "readback": readback.clone(),
        "after": file_state(&readback.selection.index_buffer_path),
    })
}

fn beyond_max_edge(runtime: &RuntimeAssetLod) -> Value {
    let before = runtime.source_readback().clone();
    let error = runtime.select_lod(500_001).unwrap_err();
    assert_eq!(error.code(), "LITD-ASSET-STREAM-INVALID-CONFIG");
    json!({
        "before": before,
        "after": runtime.source_readback(),
        "error": error_report(&error),
    })
}

fn write_lod_product(root: &Path, product_id: &str) -> TestLodProductWrite {
    let lod_dir = root.join("lod");
    fs::create_dir_all(&lod_dir).unwrap();
    let source_indices = vec![0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4];
    let lod0 = write_index_buffer(&lod_dir.join("lod0.indices.u32le"), &source_indices);
    let lod1 = write_index_buffer(&lod_dir.join("lod1.indices.u32le"), &source_indices[..6]);
    let lod2 = write_index_buffer(&lod_dir.join("lod2.indices.u32le"), &source_indices[..3]);
    let level_files = vec![lod0.clone(), lod1.clone(), lod2.clone()];
    let levels = level_files
        .iter()
        .enumerate()
        .map(|(level_index, state)| {
            let output_index_count = (state.bytes / 4) as usize;
            let target_index_ratio = match level_index {
                0 => 1.0,
                1 => 0.5,
                _ => 0.25,
            };
            json!({
                "level_index": level_index,
                "level_id": format!("{product_id}.lod{level_index}"),
                "target_index_ratio": target_index_ratio,
                "target_index_count": output_index_count,
                "output_index_count": output_index_count,
                "output_triangle_count": output_index_count / 3,
                "result_error": 0.0,
                "index_buffer_path": state.path,
                "index_buffer_bytes": state.bytes,
                "index_buffer_sha256": state.sha256,
            })
        })
        .collect::<Vec<_>>();
    let product = json!({
        "schema": "litd.asset_lod_product.v1",
        "builder_id": "litd.asset.meshopt_lod_builder",
        "builder_fingerprint": "meshopt-0.6.2:index-cache-overdraw-simplify",
        "product_id": product_id,
        "source_relative_path": "synthetic/issue1329-known.mesh",
        "source_sha256": sha256_hex(b"issue1329-source-mesh"),
        "source_vertex_count": 8,
        "source_index_count": source_indices.len(),
        "source_triangle_count": source_indices.len() / 3,
        "source_mesh_primitives": 1,
        "levels": levels,
    });
    let manifest_path = root.join("asset-lod-product.json");
    let bytes = serde_json::to_vec_pretty(&product).unwrap();
    fs::write(&manifest_path, &bytes).unwrap();
    let readback = fs::read(&manifest_path).unwrap();
    assert_eq!(readback, bytes);
    TestLodProductWrite {
        manifest_path,
        manifest_bytes: readback.len() as u64,
        manifest_sha256: sha256_hex(&readback),
        level_files,
    }
}

fn write_index_buffer(path: &Path, indices: &[u32]) -> FileState {
    let bytes = indices
        .iter()
        .flat_map(|index| index.to_le_bytes())
        .collect::<Vec<_>>();
    fs::write(path, &bytes).unwrap();
    let readback = fs::read(path).unwrap();
    assert_eq!(readback, bytes);
    file_state(path)
}

fn dir_state(root: &Path) -> Vec<FileState> {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

fn collect_files(root: &Path, files: &mut Vec<FileState>) {
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(file_state(&path));
        }
    }
}

fn file_state(path: &Path) -> FileState {
    let bytes = fs::read(path).unwrap();
    FileState {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
    }
}

fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
        "source_error": report.source_error,
    })
}
