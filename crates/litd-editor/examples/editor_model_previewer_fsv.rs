use litd_engine::fsv_artifacts::env_path;
use std::{fs, io, path::Path};

use litd_editor::{
    asset_previewer::{
        ModelPreviewCamera, ModelPreviewRequest, ModelPreviewRuntime,
        load_model_previewer_from_record,
    },
    asset_scene::DEFAULT_EDITOR_WORLD_GLTF,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    history::{stable_fingerprint, to_stable_ron},
};

const ANIMATED_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const REAL_AUDIO: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = env_path(
        "LITD_EDITOR_MODEL_PREVIEWER_FSV_ROOT",
        "tmp/issue1274-model-previewer",
    );
    let runtime_path = root.join("model_previewer_runtime.ron");
    let stats_path = root.join("stats.txt");
    let catalog = AssetVaultCatalog::load_default()?;
    let animated = real_record(&catalog, ANIMATED_GLB)?;
    let static_record = real_record(&catalog, DEFAULT_EDITOR_WORLD_GLTF)?;
    let audio = real_record(&catalog, REAL_AUDIO)?;

    let source_before = source_truth_state(&catalog, &runtime_path)?;
    let runtime = load_model_previewer_from_record(
        &animated,
        ModelPreviewRequest {
            selected_animation: None,
            playback_time_ms: 0,
            camera: ModelPreviewCamera {
                yaw_degrees: 60.0,
                pitch_degrees: -24.0,
                distance: 96.0,
            },
        },
    )?;
    write_text(&runtime_path, &to_stable_ron(&runtime)?)?;
    let runtime_readback_text = fs::read_to_string(&runtime_path)?;
    let runtime_readback: ModelPreviewRuntime = ron::from_str(&runtime_readback_text)?;
    if runtime_readback != runtime {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "model previewer runtime readback did not match the written runtime",
        )
        .into());
    }
    let runtime_fingerprint = stable_fingerprint(&runtime)?;
    let source_after_happy = source_truth_state(&catalog, &runtime_path)?;

    let static_edge = edge_static_no_animation(&catalog, &runtime_path, &static_record)?;
    let missing_edge = edge_missing_asset(&catalog, &runtime_path, &animated, &root)?;
    let unknown_animation_edge = edge_unknown_animation(&catalog, &runtime_path, &animated)?;
    let playback_unsupported_edge =
        edge_animation_playback_unsupported(&catalog, &runtime_path, &animated)?;
    let non_mesh_edge = edge_non_mesh(&catalog, &runtime_path, &audio)?;
    let source_final = source_truth_state(&catalog, &runtime_path)?;

    write_text(
        &stats_path,
        &format!(
            "issue=1274\n\
             source_of_truth=local-asset-vault manifests plus physical GLB/glTF/audio files and {}\n\
             runtime_file={}\n\
             runtime_fingerprint={}\n\
             runtime_readback_matches={}\n\
             source_before={}\n\
             source_after_happy={}\n\
             source_final={}\n\
             animated_source={}\n\
             animated_sha256={}\n\
             animated_verified_sha256={}\n\
             animated_renderer_path={}\n\
             animated_uses_viewport_render_path={}\n\
             animated_mesh_count={}\n\
             animated_node_count={}\n\
             animated_gltf_import_verified={}\n\
             animated_material_texture_sets={}\n\
             animated_joint_count={}\n\
             animated_skinned_vertices={}\n\
             animated_weighted_vertices={}\n\
             animated_morph_targets={}\n\
             animated_animation_channels={}\n\
             animated_animation_samplers={}\n\
             animated_pose_applied_to_scene_buffers={}\n\
             animated_clip_count={}\n\
             animated_selected_animation={:?}\n\
             animated_playback_state={:?}\n\
             animated_playback_time_ms={}\n\
             animated_first_clip_duration_ms={}\n\
             animated_vertices={}\n\
             animated_indices={}\n\
             animated_triangles={}\n\
             animated_bounds_min={:?}\n\
             animated_bounds_max={:?}\n\
             runtime_readback_contains_gate={}\n\
             runtime_readback_contains_open={}\n\
             runtime_readback_contains_selected_none={}\n\
             runtime_readback_contains_renderer={}\n\
             edge_static_no_animation_error={}\n\
             edge_static_before_state={}\n\
             edge_static_action={}\n\
             edge_static_after_state={}\n\
             edge_static_source_unchanged={}\n\
             edge_missing_asset_error={}\n\
             edge_missing_before_state={}\n\
             edge_missing_action={}\n\
             edge_missing_after_state={}\n\
             edge_missing_source_unchanged={}\n\
             edge_unknown_animation_error={}\n\
             edge_unknown_animation_before_state={}\n\
             edge_unknown_animation_action={}\n\
             edge_unknown_animation_after_state={}\n\
             edge_unknown_animation_source_unchanged={}\n\
             edge_animation_playback_error={}\n\
             edge_animation_playback_before_state={}\n\
             edge_animation_playback_action={}\n\
             edge_animation_playback_after_state={}\n\
             edge_animation_playback_source_unchanged={}\n\
             edge_non_mesh_error={}\n\
             edge_non_mesh_before_state={}\n\
             edge_non_mesh_action={}\n\
             edge_non_mesh_after_state={}\n\
             edge_non_mesh_source_unchanged={}\n",
            runtime_path.display(),
            runtime_path.display(),
            runtime_fingerprint,
            runtime_readback == runtime,
            source_before,
            source_after_happy,
            source_final,
            runtime.source_relative_path,
            runtime.source_sha256,
            runtime.verified_file_sha256,
            runtime.renderer_path,
            runtime.uses_viewport_render_path,
            runtime.mesh_count,
            runtime.node_count,
            runtime.gltf_import_verified,
            runtime.unique_material_texture_set_count,
            runtime.joint_count,
            runtime.skinned_vertex_count,
            runtime.weighted_vertex_count,
            runtime.morph_target_count,
            runtime.animation_channel_count,
            runtime.animation_sampler_count,
            runtime.animation_pose_applied_to_scene_buffers,
            runtime.animation_clips.len(),
            runtime.selected_animation,
            runtime.playback_state,
            runtime.playback_time_ms,
            runtime.animation_clips[0].duration_ms,
            runtime.vertex_count,
            runtime.index_count,
            runtime.triangle_count,
            runtime.bounds_min,
            runtime.bounds_max,
            runtime_readback_text.contains(ANIMATED_GLB),
            runtime_readback_text.contains("name: \"open\""),
            runtime_readback_text.contains("selected_animation: None"),
            runtime_readback_text.contains("renderer_path: \"EditorWorldSceneBuffers\""),
            static_edge.error_code,
            static_edge.before_state,
            static_edge.action,
            static_edge.after_state,
            static_edge.source_unchanged(),
            missing_edge.error_code,
            missing_edge.before_state,
            missing_edge.action,
            missing_edge.after_state,
            missing_edge.source_unchanged(),
            unknown_animation_edge.error_code,
            unknown_animation_edge.before_state,
            unknown_animation_edge.action,
            unknown_animation_edge.after_state,
            unknown_animation_edge.source_unchanged(),
            playback_unsupported_edge.error_code,
            playback_unsupported_edge.before_state,
            playback_unsupported_edge.action,
            playback_unsupported_edge.after_state,
            playback_unsupported_edge.source_unchanged(),
            non_mesh_edge.error_code,
            non_mesh_edge.before_state,
            non_mesh_edge.action,
            non_mesh_edge.after_state,
            non_mesh_edge.source_unchanged(),
        ),
    )?;
    println!(
        "wrote model previewer FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

#[derive(Debug)]
struct EdgeAudit {
    error_code: String,
    before_state: String,
    action: String,
    after_state: String,
}

impl EdgeAudit {
    fn source_unchanged(&self) -> bool {
        self.before_state == self.after_state
    }
}

fn edge_static_no_animation(
    catalog: &AssetVaultCatalog,
    runtime_path: &Path,
    record: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, runtime_path)?;
    let runtime = load_model_previewer_from_record(record, ModelPreviewRequest::default())?;
    if !runtime.animation_clips.is_empty() || runtime.selected_animation.is_some() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "static preview had clips").into());
    }
    let after_state = source_truth_state(catalog, runtime_path)?;
    Ok(EdgeAudit {
        error_code: "STATIC_OK".to_string(),
        before_state,
        action: format!("load_static_preview={}", record.relative_path),
        after_state,
    })
}

fn edge_missing_asset(
    catalog: &AssetVaultCatalog,
    runtime_path: &Path,
    record: &AssetVaultRecord,
    root: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let mut missing = record.clone();
    missing.absolute_path = root.join("missing.glb");
    let before_state = source_truth_state(catalog, runtime_path)?;
    let error_code = validation_error_code(
        load_model_previewer_from_record(&missing, ModelPreviewRequest::default()).map(|_| ()),
        "missing model unexpectedly loaded",
    )?;
    let after_state = source_truth_state(catalog, runtime_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("load_missing_preview={}", missing.absolute_path.display()),
        after_state,
    })
}

fn edge_unknown_animation(
    catalog: &AssetVaultCatalog,
    runtime_path: &Path,
    record: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, runtime_path)?;
    let error_code = validation_error_code(
        load_model_previewer_from_record(
            record,
            ModelPreviewRequest {
                selected_animation: Some("attack_missing".to_string()),
                playback_time_ms: 0,
                camera: ModelPreviewCamera::default(),
            },
        )
        .map(|_| ()),
        "missing animation unexpectedly loaded",
    )?;
    let after_state = source_truth_state(catalog, runtime_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: "select_animation=attack_missing".to_string(),
        after_state,
    })
}

fn edge_animation_playback_unsupported(
    catalog: &AssetVaultCatalog,
    runtime_path: &Path,
    record: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, runtime_path)?;
    let error_code = validation_error_code(
        load_model_previewer_from_record(
            record,
            ModelPreviewRequest {
                selected_animation: Some("open".to_string()),
                playback_time_ms: 0,
                camera: ModelPreviewCamera::default(),
            },
        )
        .map(|_| ()),
        "animation playback unexpectedly loaded without pose application",
    )?;
    let after_state = source_truth_state(catalog, runtime_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: "select_animation=open".to_string(),
        after_state,
    })
}

fn edge_non_mesh(
    catalog: &AssetVaultCatalog,
    runtime_path: &Path,
    record: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(catalog, runtime_path)?;
    let error_code = validation_error_code(
        load_model_previewer_from_record(record, ModelPreviewRequest::default()).map(|_| ()),
        "non-mesh record unexpectedly loaded",
    )?;
    let after_state = source_truth_state(catalog, runtime_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("load_non_mesh_preview={}", record.relative_path),
        after_state,
    })
}

fn source_truth_state(
    catalog: &AssetVaultCatalog,
    runtime_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let coverage = catalog.coverage_summary()?;
    let animated = real_record(catalog, ANIMATED_GLB)?;
    let static_record = real_record(catalog, DEFAULT_EDITOR_WORLD_GLTF)?;
    let audio = real_record(catalog, REAL_AUDIO)?;
    Ok(format!(
        "records={};usable={};record_types={};asset_types={};licenses={};animated={}:{}:{};static={}:{}:{};audio={}:{}:{};runtime={}:{}",
        catalog.record_count(),
        catalog.usable_record_count(),
        coverage.record_types,
        coverage.asset_types,
        coverage.licenses,
        animated.bytes_on_disk,
        animated.sha256,
        file_sha256(&animated.absolute_path)?,
        static_record.bytes_on_disk,
        static_record.sha256,
        file_sha256(&static_record.absolute_path)?,
        audio.bytes_on_disk,
        audio.sha256,
        file_sha256(&audio.absolute_path)?,
        runtime_path.exists(),
        if runtime_path.exists() {
            file_sha256(runtime_path)?
        } else {
            "absent".to_string()
        },
    ))
}

fn real_record(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("missing real vault record {relative_path}"),
            )
            .into()
        })
}

fn validation_error_code(
    result: Result<(), litd_engine::EngineError>,
    message: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    match result {
        Ok(()) => Err(io::Error::new(io::ErrorKind::InvalidData, message).into()),
        Err(error) => Ok(error.code().to_string()),
    }
}

fn file_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
