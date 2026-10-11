use std::path::PathBuf;

use crate::{
    EngineError,
    animation::{
        AnimationSampleRequest, build_skinning_palette_upload, load_gltf_skeletal_animation_asset,
        sample_skeletal_animation,
    },
    render::{
        shader::ShaderSystemSettings,
        skinning::{SKINNING_VERTEX_ATTRIBUTES, build_skinning_shader_plan},
    },
};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";

#[test]
fn samples_real_stan_sword_slash_and_builds_gpu_palette() {
    let asset = load_stan();
    assert_eq!(asset.clip_count, 18);
    assert_eq!(asset.skin_count, 1);
    assert_eq!(asset.skins[0].joint_node_indices.len(), 43);
    let start = build_skinning_palette_upload(
        &asset,
        &AnimationSampleRequest {
            clip_name: "SwordSlash".to_string(),
            skin_index: 0,
            playback_time_ms: 0,
        },
    )
    .unwrap();
    let mid = build_skinning_palette_upload(
        &asset,
        &AnimationSampleRequest {
            clip_name: "SwordSlash".to_string(),
            skin_index: 0,
            playback_time_ms: 375,
        },
    )
    .unwrap();

    assert_eq!(mid.joint_count, 43);
    assert_eq!(mid.byte_count, 43 * 64);
    assert_eq!(mid.bind_group, 3);
    assert_eq!(mid.binding, 0);
    assert!(mid.matrices.iter().all(|matrix| matrix.is_finite()));
    assert_ne!(start.palette_hash(), mid.palette_hash());
    println!(
        "issue1105_palette source={} clip={} joints={} bytes={} start_hash={} mid_hash={}",
        mid.source_relative_path,
        mid.clip_name,
        mid.joint_count,
        mid.byte_count,
        start.palette_hash(),
        mid.palette_hash()
    );
}

#[test]
fn skinning_shader_contract_validates_palette_storage_binding() {
    let plan = build_skinning_shader_plan(ShaderSystemSettings::default()).unwrap();

    assert_eq!(plan.id, "litd.skinning.vertex");
    assert_eq!(plan.bindings.len(), 1);
    assert_eq!(plan.bindings[0].group, 3);
    assert_eq!(plan.bindings[0].binding, 0);
    assert_eq!(SKINNING_VERTEX_ATTRIBUTES[0].shader_location, 10);
    assert_eq!(SKINNING_VERTEX_ATTRIBUTES[1].shader_location, 11);
    println!(
        "issue1105_shader id={} source_hash={} bindings={}",
        plan.id,
        plan.source_hash,
        plan.bindings.len()
    );
}

#[test]
fn unknown_clip_and_out_of_range_time_fail_loud() {
    let asset = load_stan();
    let unknown = sample_error(
        &asset,
        AnimationSampleRequest {
            clip_name: "missing_attack".to_string(),
            skin_index: 0,
            playback_time_ms: 0,
        },
    );
    assert!(unknown.contains("animation.clip_name"));

    let late = sample_error(
        &asset,
        AnimationSampleRequest {
            clip_name: "SwordSlash".to_string(),
            skin_index: 0,
            playback_time_ms: 751,
        },
    );
    assert!(late.contains("animation.playback_time_ms"));
}

#[test]
fn unskinned_animation_asset_fails_for_skeletal_loader() {
    match load_gltf_skeletal_animation_asset(workspace_path(GATE_GLB), GATE_GLB) {
        Err(EngineError::AnimationInvalidConfig { field, .. }) => {
            assert_eq!(field, "animation.skin_count");
        }
        other => panic!("expected AnimationInvalidConfig, got {other:?}"),
    }
}

fn load_stan() -> crate::animation::SkeletalAnimationAsset {
    load_gltf_skeletal_animation_asset(workspace_path(STAN_GLTF), STAN_GLTF).unwrap()
}

fn sample_error(
    asset: &crate::animation::SkeletalAnimationAsset,
    request: AnimationSampleRequest,
) -> String {
    match sample_skeletal_animation(asset, request) {
        Ok(_) => panic!("expected skeletal sample to fail"),
        Err(error) => error.to_string(),
    }
}

// #1543: RUNTIME vault resolution. `env!("CARGO_MANIFEST_DIR")` is a compile-time literal and
// resolved to a nonexistent path inside linked git worktrees.
fn workspace_path(relative_path: &str) -> PathBuf {
    crate::asset_vault_root::local_asset_vault_path(relative_path).unwrap()
}
