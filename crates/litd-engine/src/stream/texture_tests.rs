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

fn issue1330_texture_mip_streaming_source_truth_fsv() {
    let run = begin_issue_run("issue1330_texture_mip_streaming").unwrap();
    let texture_dir = run.artifact_dir("texture-mip-source").unwrap();
    let texture_id = "issue1330.texture.known";
    let before = dir_state(&texture_dir);
    let written = write_texture_mip_product(&texture_dir, texture_id, &[0, 1, 2, 3]);
    let config = TextureMipStreamConfig::new(vec![
        TextureMipDistanceBand::new(50_000, 0),
        TextureMipDistanceBand::new(200_000, 1),
        TextureMipDistanceBand::new(500_000, 2),
    ]);
    let request = TextureMipStreamLoadRequest::new(&written.manifest_path, config).unwrap();
    let mut runtime = RuntimeTextureMipStream::load_background(request)
        .finish()
        .unwrap();
    let source = runtime.source_readback().clone();
    assert!(source.loaded_on_background_thread);
    assert_eq!(source.level_count, 4);

    let near = apply_distance(&mut runtime, 10_000);
    assert_eq!(near.update.after.resident_mips, vec![0, 1, 2, 3]);
    let far = apply_distance(&mut runtime, 400_000);
    assert_eq!(far.update.after.resident_mips, vec![2, 3]);
    assert!(far.update.after.resident_vram_bytes < near.update.after.resident_vram_bytes);

    let single_mip = single_mip_edge(run.artifact_dir("edge-single-mip").unwrap());
    let missing_mip = missing_mip_edge(run.artifact_dir("edge-missing-mip").unwrap());
    let max_distance = apply_distance(&mut runtime, 500_000);
    let beyond_max = beyond_max_edge(&runtime);
    let all_expected_observed = source.loaded_on_background_thread
        && near.update.after.resident_vram_bytes == 5_440
        && far.update.after.resident_vram_bytes == 320
        && far.update.after.resident_vram_bytes < near.update.after.resident_vram_bytes
        && near.physical_bytes_match
        && far.physical_bytes_match
        && single_mip["update"]["after"]["resident_mips"] == json!([0])
        && missing_mip["error"]["code"] == "LITD-ASSET-STREAM-INVALID-CONFIG"
        && max_distance.update.after.resident_mips == vec![2, 3]
        && beyond_max["error"]["code"] == "LITD-ASSET-STREAM-INVALID-CONFIG";
    assert!(all_expected_observed);

    let report = json!({
        "schema_version": 1,
        "issue": 1330,
        "source_of_truth": {
            "texture_dir": texture_dir,
            "manifest_path": written.manifest_path,
            "manifest_sha256": written.manifest_sha256,
            "stream_schema": TEXTURE_MIP_STREAM_SCHEMA,
            "product_schema": TEXTURE_MIP_PRODUCT_SCHEMA,
            "latest_manifest_path": run.latest_manifest_path(),
            "mip_files": written.mip_files,
        },
        "research": {
            "unity_mipmap_streaming": "https://docs.unity3d.com/6000.4/Documentation/Manual/TextureStreaming.html",
            "unreal_texture_streaming": "https://dev.epicgames.com/documentation/unreal-engine/texture-streaming-overview-for-unreal-engine?lang=en-US",
            "ktx2_mip_level_array": "https://github.khronos.org/KTX-Specification/ktxspec.v2.html",
        },
        "happy_path": {
            "before": { "texture_dir": before, "state": TextureMipStreamReadback {
                resident_mips: Vec::new(),
                highest_resident_mip: None,
                resident_vram_bytes: 0,
            }},
            "after": {
                "texture_dir": dir_state(&texture_dir),
                "source": source,
                "near": near,
                "far": far,
            },
            "expected": {
                "near_resident_mips": [0, 1, 2, 3],
                "near_vram_bytes": 5440,
                "far_resident_mips": [2, 3],
                "far_vram_bytes": 320,
                "vram_drops_when_far": true,
                "all_loads_background_thread": true,
            },
            "write_readback": written,
        },
        "edges": {
            "single_mip": single_mip,
            "missing_mip": missing_mip,
            "max_distance": max_distance,
            "beyond_max": beyond_max,
        },
        "spinoff_issue": {
            "number": 1347,
            "reason": "texture_import currently writes only base-level BC7 artifacts while claiming mipmaps_generated"
        },
        "all_expected_observed": all_expected_observed,
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["after"]["near"]["update"]["after"]["resident_vram_bytes"],
        5_440
    );
    assert_eq!(
        report_readback.json["happy_path"]["after"]["far"]["update"]["after"]["resident_vram_bytes"],
        320
    );
    assert_eq!(report_readback.json["all_expected_observed"], true);
    println!(
        "ISSUE1330_TEXTURE_MIP_STREAM_FSV report={} report_sha256={} manifest={} manifest_sha256={} levels={} total_mip_bytes={} near_mips={:?} near_bytes={} far_mips={:?} far_bytes={} single_mips={} missing_code={} max_mips={:?} beyond_code={}",
        report_readback.path.display(),
        report_readback.sha256,
        written.manifest_path.display(),
        written.manifest_sha256,
        report_readback.json["happy_path"]["after"]["source"]["level_count"],
        report_readback.json["happy_path"]["after"]["source"]["total_mip_bytes"],
        near.update.after.resident_mips,
        near.update.after.resident_vram_bytes,
        far.update.after.resident_mips,
        far.update.after.resident_vram_bytes,
        report_readback.json["edges"]["single_mip"]["update"]["after"]["resident_mips"],
        report_readback.json["edges"]["missing_mip"]["error"]["code"],
        max_distance.update.after.resident_mips,
        report_readback.json["edges"]["beyond_max"]["error"]["code"],
    );
}

#[test]
fn issue1330_latest_fsv_manifest_points_at_real_report() {
    issue1330_texture_mip_streaming_source_truth_fsv();
    let root = issue_root("issue1330_texture_mip_streaming");
    let manifest_path = root.join("latest-manifest.json");
    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).expect("manifest json");
    let report_path = manifest["report_path"].as_str().expect("report path");
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}

#[derive(Debug, Clone, Serialize)]
struct TextureMipProductWrite {
    manifest_path: PathBuf,
    manifest_bytes: u64,
    manifest_sha256: String,
    mip_files: Vec<FileState>,
}

#[derive(Debug, Clone, Serialize)]
struct AppliedMipEvidence {
    distance_millimeters: u64,
    before_state: TextureMipStreamReadback,
    before_files: Vec<FileState>,
    plan: TextureMipResidencyPlan,
    receipt: TextureMipResidencyReadback,
    loaded_bytes: u64,
    after_files: Vec<FileState>,
    update: TextureMipStreamUpdate,
    physical_bytes_match: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FileState {
    path: String,
    bytes: u64,
    sha256: String,
}

fn apply_distance(runtime: &mut RuntimeTextureMipStream, distance: u64) -> AppliedMipEvidence {
    let before_state = runtime.state_readback();
    let plan = runtime.plan_residency(distance).unwrap();
    let before_files = plan
        .levels
        .iter()
        .map(|level| file_state(&level.path))
        .collect::<Vec<_>>();
    let receipt = runtime
        .load_residency_background(plan.clone())
        .finish()
        .unwrap();
    assert!(receipt.readback.loaded_on_background_thread);
    let readback = receipt.readback.clone();
    let loaded_bytes = receipt
        .resident_levels
        .iter()
        .map(|level| level.bytes.len() as u64)
        .sum();
    let after_files = plan
        .levels
        .iter()
        .map(|level| file_state(&level.path))
        .collect::<Vec<_>>();
    let physical_bytes_match = readback.levels.iter().all(|level| {
        let state = file_state(&level.path);
        state.bytes == level.bytes_read && state.sha256 == level.verified_sha256
    });
    let update = runtime.apply_residency(receipt).unwrap();
    AppliedMipEvidence {
        distance_millimeters: distance,
        before_state,
        before_files,
        plan,
        receipt: readback,
        loaded_bytes,
        after_files,
        update,
        physical_bytes_match,
    }
}

fn single_mip_edge(root: PathBuf) -> Value {
    let written = write_texture_mip_product(&root, "issue1330.texture.single", &[0]);
    let request = TextureMipStreamLoadRequest::new(
        &written.manifest_path,
        TextureMipStreamConfig::new(vec![TextureMipDistanceBand::new(1_000_000, 0)]),
    )
    .unwrap();
    let mut runtime = RuntimeTextureMipStream::load_background(request)
        .finish()
        .unwrap();
    let update = apply_distance(&mut runtime, 900_000);
    assert_eq!(update.update.after.resident_mips, vec![0]);
    json!({ "write_readback": written, "source": runtime.source_readback(), "update": update.update })
}

fn missing_mip_edge(root: PathBuf) -> Value {
    let written = write_texture_mip_product(&root, "issue1330.texture.missing", &[0, 2, 3]);
    let before = dir_state(&root);
    let request = TextureMipStreamLoadRequest::new(
        &written.manifest_path,
        TextureMipStreamConfig::new(vec![
            TextureMipDistanceBand::new(50_000, 0),
            TextureMipDistanceBand::new(200_000, 1),
            TextureMipDistanceBand::new(500_000, 2),
        ]),
    )
    .unwrap();
    let error = RuntimeTextureMipStream::load_background(request)
        .finish()
        .unwrap_err();
    assert_eq!(error.code(), "LITD-ASSET-STREAM-INVALID-CONFIG");
    json!({
        "before": before,
        "after": dir_state(&root),
        "write_readback": written,
        "error": error_report(&error),
    })
}

fn beyond_max_edge(runtime: &RuntimeTextureMipStream) -> Value {
    let before = runtime.state_readback();
    let error = runtime.plan_residency(500_001).unwrap_err();
    assert_eq!(error.code(), "LITD-ASSET-STREAM-INVALID-CONFIG");
    json!({
        "before": before,
        "after": runtime.state_readback(),
        "error": error_report(&error),
    })
}

fn write_texture_mip_product(
    root: &Path,
    texture_id: &str,
    levels: &[u32],
) -> TextureMipProductWrite {
    let mip_dir = root.join("mips");
    fs::create_dir_all(&mip_dir).unwrap();
    let mut mip_files = Vec::with_capacity(levels.len());
    let mut level_json = Vec::with_capacity(levels.len());
    for level_index in levels {
        let width = mip_size(64, *level_index);
        let height = mip_size(64, *level_index);
        let bytes = deterministic_mip_bytes(*level_index, bc7_bytes(width, height));
        let path = mip_dir.join(format!("mip{level_index}.bc7"));
        fs::write(&path, &bytes).unwrap();
        let state = file_state(&path);
        assert_eq!(state.bytes, bytes.len() as u64);
        mip_files.push(state.clone());
        level_json.push(json!({
            "level_index": level_index,
            "width": width,
            "height": height,
            "path": state.path,
            "bytes": state.bytes,
            "sha256": state.sha256,
        }));
    }
    let product = json!({
        "schema": TEXTURE_MIP_PRODUCT_SCHEMA,
        "texture_id": texture_id,
        "source_relative_path": "synthetic/issue1330-known.ktx2",
        "source_format": "ktx2",
        "gpu_format": "BC7_RGBA",
        "width": 64,
        "height": 64,
        "levels": level_json,
    });
    let manifest_path = root.join("texture-mip-product.json");
    let bytes = serde_json::to_vec_pretty(&product).unwrap();
    fs::write(&manifest_path, &bytes).unwrap();
    let readback = fs::read(&manifest_path).unwrap();
    assert_eq!(readback, bytes);
    TextureMipProductWrite {
        manifest_path,
        manifest_bytes: readback.len() as u64,
        manifest_sha256: sha256_hex(&readback),
        mip_files,
    }
}

fn deterministic_mip_bytes(level_index: u32, byte_count: u64) -> Vec<u8> {
    (0..byte_count)
        .map(|offset| ((offset + (level_index as u64 * 31)) % 251) as u8)
        .collect()
}

fn bc7_bytes(width: u32, height: u32) -> u64 {
    (width.div_ceil(4) as u64) * (height.div_ceil(4) as u64) * 16
}

fn mip_size(size: u32, level_index: u32) -> u32 {
    (size >> level_index).max(1)
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
