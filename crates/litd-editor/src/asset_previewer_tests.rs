use std::path::PathBuf;

use super::*;

use crate::asset_scene::DEFAULT_EDITOR_WORLD_GLTF;

const ANIMATED_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const REAL_AUDIO: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

#[test]
fn loads_real_animated_gltf_preview_on_viewport_render_path() {
    let record = real_record(ANIMATED_GLTF);
    let runtime = load_model_previewer_from_record(
        &record,
        ModelPreviewRequest {
            selected_animation: None,
            playback_time_ms: 0,
            camera: ModelPreviewCamera {
                yaw_degrees: 60.0,
                pitch_degrees: -24.0,
                distance: 96.0,
            },
        },
    )
    .unwrap();

    assert_eq!(runtime.source_relative_path, ANIMATED_GLTF);
    assert_eq!(runtime.source_sha256, record.sha256);
    assert_eq!(runtime.verified_file_sha256, record.sha256);
    assert_eq!(runtime.renderer_path, "EditorWorldSceneBuffers");
    assert!(runtime.uses_viewport_render_path);
    assert!(runtime.gltf_import_verified);
    assert_eq!(runtime.playback_state, ModelPreviewPlaybackState::Static);
    assert_eq!(runtime.selected_animation, None);
    assert_eq!(runtime.playback_time_ms, 0);
    assert_eq!(runtime.animation_clips.len(), 18);
    assert!(
        runtime
            .animation_clips
            .iter()
            .any(|clip| clip.name == "SwordSlash")
    );
    assert_eq!(runtime.animation_channel_count, 1548);
    assert_eq!(runtime.animation_sampler_count, 1548);
    assert!(!runtime.animation_pose_applied_to_scene_buffers);
    assert_eq!(runtime.mesh_count, 1);
    assert_eq!(runtime.skin_count, 1);
    assert_eq!(runtime.morph_target_count, 0);
    assert_eq!(runtime.vertex_count, 6_214);
    assert_eq!(runtime.index_count, 17_916);
    assert_eq!(runtime.triangle_count, 5_972);
    assert_eq!(runtime.source_bytes, record.bytes_on_disk);
    println!(
        "preview_animated source={} sha={} renderer={} clips={} selected={:?} sword_slash_present={} pose_applied={} vertices={} indices={} triangles={} bounds_min={:?} bounds_max={:?} camera={:.1}/{:.1}/{:.1}",
        runtime.source_relative_path,
        runtime.verified_file_sha256,
        runtime.renderer_path,
        runtime.animation_clips.len(),
        runtime.selected_animation,
        runtime
            .animation_clips
            .iter()
            .any(|clip| clip.name == "SwordSlash"),
        runtime.animation_pose_applied_to_scene_buffers,
        runtime.vertex_count,
        runtime.index_count,
        runtime.triangle_count,
        runtime.bounds_min,
        runtime.bounds_max,
        runtime.camera.yaw_degrees,
        runtime.camera.pitch_degrees,
        runtime.camera.distance
    );
}

#[test]
fn static_gltf_preview_has_no_animation_and_keeps_viewport_path() {
    let record = real_record(DEFAULT_EDITOR_WORLD_GLTF);
    let runtime =
        load_model_previewer_from_record(&record, ModelPreviewRequest::default()).unwrap();

    assert_eq!(runtime.source_relative_path, DEFAULT_EDITOR_WORLD_GLTF);
    assert_eq!(runtime.verified_file_sha256, record.sha256);
    assert_eq!(runtime.renderer_path, "EditorWorldSceneBuffers");
    assert_eq!(runtime.playback_state, ModelPreviewPlaybackState::Static);
    assert_eq!(runtime.selected_animation, None);
    assert_eq!(runtime.playback_time_ms, 0);
    assert_eq!(runtime.animation_clips.len(), 0);
    assert!(runtime.gltf_import_verified);
    assert_eq!(runtime.unique_material_texture_set_count, 1);
    assert_eq!(runtime.joint_count, 0);
    assert_eq!(runtime.morph_target_count, 0);
    assert_eq!(runtime.vertex_count, 80_238);
    assert_eq!(runtime.triangle_count, 153_472);
    println!(
        "preview_static source={} sha={} renderer={} clips={} vertices={} triangles={} bounds_min={:?} bounds_max={:?}",
        runtime.source_relative_path,
        runtime.verified_file_sha256,
        runtime.renderer_path,
        runtime.animation_clips.len(),
        runtime.vertex_count,
        runtime.triangle_count,
        runtime.bounds_min,
        runtime.bounds_max
    );
}

#[test]
fn missing_preview_asset_fails_before_product_load() {
    let mut record = real_record(ANIMATED_GLTF);
    record.absolute_path = PathBuf::from("tmp/issue1274-model-previewer/missing.glb");
    let error = preview_error(&record, ModelPreviewRequest::default());

    assert!(error.contains("editor.asset_previewer.source.read"));
    println!(
        "preview_edge_missing before_path={} after_error={}",
        record.absolute_path.display(),
        error
    );
}

#[test]
fn non_mesh_record_fails_loud_for_model_previewer() {
    let record = real_record(REAL_AUDIO);
    let error = preview_error(&record, ModelPreviewRequest::default());

    assert!(error.contains("editor.asset_previewer.product.kind"));
    println!(
        "preview_edge_non_mesh source={} sha={} error={}",
        record.relative_path, record.sha256, error
    );
}

#[test]
fn unknown_animation_name_fails_without_changing_source_record() {
    let record = real_record(ANIMATED_GLTF);
    let request = ModelPreviewRequest {
        selected_animation: Some("attack_missing".to_string()),
        playback_time_ms: 0,
        camera: ModelPreviewCamera::default(),
    };
    let error = preview_error(&record, request);

    assert!(error.contains("editor.asset_previewer.animation.selected"));
    assert_eq!(verify_record_hash(&record).unwrap(), record.sha256);
    println!(
        "preview_edge_unknown_animation source={} sha={} error={}",
        record.relative_path, record.sha256, error
    );
}

#[test]
fn animation_clip_selection_fails_until_pose_application_exists() {
    let record = real_record(ANIMATED_GLTF);
    let request = ModelPreviewRequest {
        selected_animation: Some("SwordSlash".to_string()),
        playback_time_ms: 0,
        camera: ModelPreviewCamera::default(),
    };
    let error = preview_error(&record, request);

    assert!(error.contains("editor.asset_previewer.animation.playback"));
    assert_eq!(verify_record_hash(&record).unwrap(), record.sha256);
    println!(
        "preview_edge_animation_pose source={} sha={} error={}",
        record.relative_path, record.sha256, error
    );
}

fn real_record(relative_path: &str) -> AssetVaultRecord {
    AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "{}",
                crate::asset_vault_fixture::missing_fixture(relative_path)
            )
        })
}

fn preview_error(record: &AssetVaultRecord, request: ModelPreviewRequest) -> String {
    match load_model_previewer_from_record(record, request) {
        Ok(_) => panic!("expected previewer load to fail"),
        Err(error) => error.to_string(),
    }
}
