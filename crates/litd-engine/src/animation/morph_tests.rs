use std::{fs, path::PathBuf};

use crate::{
    EngineError,
    animation::{
        MorphSampleRequest, build_morph_weight_upload, load_gltf_morph_target_animation_asset,
        sample_morph_target_weights,
    },
    fsv_artifacts::begin_issue_run,
    render::{morphing::build_morphing_shader_plan, shader::ShaderSystemSettings},
};

const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";

#[test]
fn samples_real_gltf_morph_weight_animation_and_uploads_weights() {
    let (path, relative) = write_minimal_morph_gltf("happy");
    let asset = load_gltf_morph_target_animation_asset(&path, &relative).unwrap();

    assert_eq!(asset.mesh_count, 1);
    assert_eq!(asset.clips.len(), 1);
    assert_eq!(asset.meshes[0].target_count, 1);
    assert_eq!(asset.meshes[0].primitives[0].position_delta_count, 3);
    let request = MorphSampleRequest {
        clip_name: "Smile".to_string(),
        target_node_index: 0,
        playback_time_ms: 500,
    };
    let sample = sample_morph_target_weights(&asset, request.clone()).unwrap();
    let upload = build_morph_weight_upload(&asset, &request).unwrap();

    assert_eq!(sample.weights, vec![0.5]);
    assert_eq!(upload.target_count, 1);
    assert_eq!(upload.byte_count, 4);
    assert_eq!(upload.bind_group, 4);
    assert_eq!(upload.binding, 0);
    assert_eq!(upload.shader_id, "litd.morphing.vertex");
    println!(
        "issue1106_morph source={} clip={} target_count={} weight={:.3} bytes={} hash={}",
        upload.source_relative_path,
        upload.clip_name,
        upload.target_count,
        upload.weights[0],
        upload.byte_count,
        upload.weight_hash()
    );
}

#[test]
fn morphing_shader_contract_validates_weight_and_delta_bindings() {
    let plan = build_morphing_shader_plan(ShaderSystemSettings::default()).unwrap();

    assert_eq!(plan.id, "litd.morphing.vertex");
    assert_eq!(plan.bindings.len(), 2);
    assert_eq!(plan.bindings[0].group, 4);
    assert_eq!(plan.bindings[0].binding, 0);
    assert_eq!(plan.bindings[1].binding, 1);
    println!(
        "issue1106_shader id={} source_hash={} bindings={}",
        plan.id,
        plan.source_hash,
        plan.bindings.len()
    );
}

#[test]
fn no_morph_targets_fail_loud() {
    match load_gltf_morph_target_animation_asset(workspace_path(GATE_GLB), GATE_GLB) {
        Err(EngineError::AnimationInvalidConfig { field, .. }) => {
            assert_eq!(field, "animation.morph.meshes");
        }
        other => panic!("expected AnimationInvalidConfig, got {other:?}"),
    }
}

#[test]
fn unknown_morph_clip_and_time_bounds_fail_loud() {
    let (path, relative) = write_minimal_morph_gltf("edges");
    let asset = load_gltf_morph_target_animation_asset(&path, &relative).unwrap();
    let unknown = sample_error(
        &asset,
        MorphSampleRequest {
            clip_name: "MissingSmile".to_string(),
            target_node_index: 0,
            playback_time_ms: 0,
        },
    );
    assert!(unknown.contains("animation.morph.clip_name"));

    let late = sample_error(
        &asset,
        MorphSampleRequest {
            clip_name: "Smile".to_string(),
            target_node_index: 0,
            playback_time_ms: 1001,
        },
    );
    assert!(late.contains("animation.morph.playback_time_ms"));
}

fn sample_error(
    asset: &crate::animation::MorphTargetAnimationAsset,
    request: MorphSampleRequest,
) -> String {
    match sample_morph_target_weights(asset, request) {
        Ok(_) => panic!("expected morph sample to fail"),
        Err(error) => error.to_string(),
    }
}

fn write_minimal_morph_gltf(label: &str) -> (PathBuf, String) {
    let root = begin_issue_run("issue1106_morph_animation")
        .unwrap()
        .artifact_dir(label)
        .unwrap();
    let mut bytes = Vec::new();
    let position_offset = push_f32s(&mut bytes, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let morph_offset = push_f32s(&mut bytes, &[0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.0]);
    let time_offset = push_f32s(&mut bytes, &[0.0, 1.0]);
    let weight_offset = push_f32s(&mut bytes, &[0.0, 1.0]);
    let bin_path = root.join("morph.bin");
    fs::write(&bin_path, &bytes).unwrap();
    let gltf = format!(
        r#"{{
  "asset": {{"version": "2.0", "generator": "litd-issue1106-test"}},
  "buffers": [{{"uri": "morph.bin", "byteLength": {byte_len}}}],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": {position_offset}, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": {morph_offset}, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": {time_offset}, "byteLength": 8}},
    {{"buffer": 0, "byteOffset": {weight_offset}, "byteLength": 8}}
  ],
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]}},
    {{"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [0, 0.25, 0]}},
    {{"bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1]}},
    {{"bufferView": 3, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1]}}
  ],
  "meshes": [{{"name": "FacePanel", "weights": [0.0], "primitives": [{{"attributes": {{"POSITION": 0}}, "targets": [{{"POSITION": 1}}]}}]}}],
  "nodes": [{{"name": "FaceNode", "mesh": 0}}],
  "scenes": [{{"nodes": [0]}}],
  "scene": 0,
  "animations": [{{"name": "Smile", "samplers": [{{"input": 2, "output": 3, "interpolation": "LINEAR"}}], "channels": [{{"sampler": 0, "target": {{"node": 0, "path": "weights"}}}}]}}]
}}"#,
        byte_len = bytes.len(),
    );
    let gltf_path = root.join("morph.gltf");
    fs::write(&gltf_path, gltf).unwrap();
    (gltf_path, format!("issue1106/morph-{label}.gltf"))
}

fn push_f32s(bytes: &mut Vec<u8>, values: &[f32]) -> usize {
    let offset = bytes.len();
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    offset
}

// #1543: RUNTIME vault resolution; see `crate::asset_vault_root`.
fn workspace_path(relative_path: &str) -> PathBuf {
    crate::asset_vault_root::local_asset_vault_path(relative_path).unwrap()
}
