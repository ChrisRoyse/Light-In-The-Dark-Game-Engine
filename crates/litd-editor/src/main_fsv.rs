use std::{env, fs, io, path::Path, path::PathBuf};

use litd_engine::{
    app::{AppFrameStats, LifecycleReport},
    input::InputStats,
    render::{FrameStats, RendererInfo, editor_world::EditorWorldRenderEvidence},
};
use tracing::warn;

use crate::{main_build_identity::BuildIdentity, main_lifecycle::EditorLifecycleEvidence};

#[cfg(not(test))]
use crate::main_editor_ui::{EditorShellRuntimeEvidence, format_shell_stats};

#[cfg(test)]
#[path = "main_fsv_test_shell.rs"]
mod main_fsv_test_shell;

#[cfg(test)]
use main_fsv_test_shell::{EditorShellRuntimeEvidence, format_shell_stats};

#[derive(Debug, Clone, Copy)]
pub struct InputFsvSnapshot {
    pub stats: InputStats,
    pub camera_forward_down: bool,
    pub confirm_down: bool,
}

pub struct EditorFsvSnapshot<'a> {
    pub info: &'a RendererInfo,
    pub render_stats: &'a FrameStats,
    pub app_stats: &'a AppFrameStats,
    pub input: InputFsvSnapshot,
    pub shell: EditorShellRuntimeEvidence,
    pub world: &'a EditorWorldRenderEvidence,
}

// #1485 finding 3: the per-frame snapshot no longer owns the ~25-heap-string
// `EditorWorldRenderEvidence`; it is rebuilt on demand at snapshot cadence via `borrowed(world)`.
#[derive(Debug, Clone)]
pub struct OwnedEditorFsvSnapshot {
    pub info: RendererInfo,
    pub render_stats: FrameStats,
    pub app_stats: AppFrameStats,
    pub input: InputFsvSnapshot,
    pub shell: EditorShellRuntimeEvidence,
}

impl OwnedEditorFsvSnapshot {
    pub fn borrowed<'a>(&'a self, world: &'a EditorWorldRenderEvidence) -> EditorFsvSnapshot<'a> {
        EditorFsvSnapshot {
            info: &self.info,
            render_stats: &self.render_stats,
            app_stats: &self.app_stats,
            input: self.input,
            shell: self.shell.clone(),
            world,
        }
    }
}

#[derive(Default)]
pub struct FsvConfig {
    pub target_frames: Option<u64>,
    pub stats_path: Option<PathBuf>,
    pub stats_interval_frames: Option<u64>,
    pub last_periodic_stats_frame: u64,
}

impl FsvConfig {
    pub fn from_env() -> Self {
        let target_frames =
            env::var("LITD_FSV_FRAMES")
                .ok()
                .and_then(|value| match value.parse::<u64>() {
                    Ok(frames) => Some(frames),
                    Err(err) => {
                        warn!(value, error = %err, "ignoring invalid LITD_FSV_FRAMES");
                        None
                    }
                });
        let stats_path = env::var("LITD_FSV_STATS").ok().map(PathBuf::from);
        let stats_interval_frames =
            env::var("LITD_FSV_STATS_EVERY_FRAMES")
                .ok()
                .and_then(|value| match value.parse::<u64>() {
                    Ok(0) => {
                        warn!("ignoring zero LITD_FSV_STATS_EVERY_FRAMES");
                        None
                    }
                    Ok(frames) => Some(frames),
                    Err(err) => {
                        warn!(value, error = %err, "ignoring invalid LITD_FSV_STATS_EVERY_FRAMES");
                        None
                    }
                });
        Self {
            target_frames,
            stats_path,
            stats_interval_frames,
            last_periodic_stats_frame: 0,
        }
    }
}

pub fn write_fsv_stats(
    path: &Path,
    window_title: &str,
    build: &BuildIdentity,
    snapshot: &EditorFsvSnapshot<'_>,
    lifecycle: &LifecycleReport,
    editor_lifecycle: &EditorLifecycleEvidence,
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let shell_stats = format_shell_stats(&snapshot.shell);
    fs::write(
        path,
        format!(
            "window_title={window_title}\n\
             package_version={}\n\
             build_git_commit={}\n\
             build_git_dirty={}\n\
             build_profile={}\n\
             build_target={}\n\
             build_timestamp_unix_seconds={}\n\
             executable_path={}\n\
             executable_sha256={}\n\
             repo_root={}\n\
             repo_head_commit={}\n\
             repo_head_matches_build={}\n\
             adapter={}\n\
             backend={}\n\
             device_type={}\n\
             surface_format={}\n\
             present_mode={}\n\
             width={}\n\
             height={}\n\
             frames_presented={}\n\
             render_graph_passes={}\n\
             draw_calls={}\n\
             allocations={}\n\
             budget_max_draw_calls={}\n\
             budget_max_allocations={}\n\
             budget_ok={}\n\
             clear_color={:?}\n\
             world_renderer={}\n\
             world_visual_preset_contract={}\n\
             world_scene_name={}\n\
             world_scene_source_id={}\n\
             world_scene_source_path={}\n\
             world_scene_dependency_count={}\n\
             world_scene_material_count={}\n\
             world_scene_texture_count={}\n\
             world_scene_image_count={}\n\
             world_scene_is_blank={}\n\
             world_blank_grid_visible={}\n\
             world_base_color_texture_source_path={}\n\
             world_base_color_texture_width={}\n\
             world_base_color_texture_height={}\n\
             world_base_color_texture_bytes={}\n\
             world_normal_texture_source_path={}\n\
             world_normal_texture_width={}\n\
             world_normal_texture_height={}\n\
             world_normal_texture_bytes={}\n\
             world_metallic_roughness_texture_source_path={}\n\
             world_metallic_roughness_texture_width={}\n\
             world_metallic_roughness_texture_height={}\n\
             world_metallic_roughness_texture_bytes={}\n\
             world_shader_label={}\n\
             world_post_shader_label={}\n\
             world_hdr_format={}\n\
             world_additive_blend_mode={}\n\
             world_atmosphere_model={}\n\
             world_atmosphere_profile={}\n\
             world_bloom_passes={}\n\
             world_bloom_intensity={}\n\
             world_exposure={}\n\
             world_color_grade={}\n\
             world_background_model={}\n\
             world_background_profile={}\n\
             world_depth_format={}\n\
             world_viewport_rect={}\n\
             world_vertex_count={}\n\
             world_index_count={}\n\
             world_triangle_count={}\n\
             world_mesh_primitives={}\n\
             world_layer_state={}\n\
             world_layer_count={}\n\
             world_hidden_layer_count={}\n\
             world_terrain_visible={}\n\
             world_foliage_visible={}\n\
             world_doodads_visible={}\n\
             world_units_visible={}\n\
             world_water_visible={}\n\
             world_blight_visible={}\n\
             world_shadows_visible={}\n\
             world_lighting_visible={}\n\
             world_weather_visible={}\n\
             world_sky_visible={}\n\
             world_fog_effects_visible={}\n\
             world_grid_visible={}\n\
             world_camera_bounds_visible={}\n\
             world_pathing_ground_visible={}\n\
             world_pathing_naval_visible={}\n\
             world_regions_visible={}\n\
             world_camera_objects_visible={}\n\
             world_info_display_visible={}\n\
             world_letterbox_mode={}\n\
             world_lock_to_game_camera={}\n\
             world_foliage_index_count={}\n\
             world_foliage_template_index_count={}\n\
             world_foliage_instance_count={}\n\
             world_foliage_instanced_draw_calls={}\n\
             world_foliage_distance_fade={}\n\
             world_visible_index_count={}\n\
             world_visible_triangle_count={}\n\
             world_hidden_index_count={}\n\
             world_hidden_triangle_count={}\n\
             world_draw_calls={}\n\
             app_frame_index={}\n\
             app_variable_delta_ns={}\n\
             app_fixed_delta_ns={}\n\
             app_fixed_steps={}\n\
             app_dropped_fixed_steps={}\n\
             app_accumulated_time_ns={}\n\
             app_target_frame_ns={}\n\
             app_sleep_hint_ns={}\n\
             frame_module_fixed_updates={}\n\
             frame_module_updates={}\n\
             input_frame_index={}\n\
             input_action_count={}\n\
             input_binding_count={}\n\
             input_active_binding_count={}\n\
             input_events_seen={}\n\
             input_cursor_x={}\n\
             input_cursor_y={}\n\
             input_camera_forward_down={}\n\
             input_confirm_down={}\n\
             module_count={}\n\
             module_started={}\n\
             module_currently_started={}\n\
             module_ever_started={}\n\
             module_clean_shutdown={}\n\
             module_start_calls={}\n\
             module_fixed_update_calls={}\n\
             module_update_calls={}\n\
             module_shutdown_calls={}\n\
             editor_lifecycle_state={}\n\
             editor_last_exit_reason={}\n\
             editor_close_requested_count={}\n\
             editor_loop_exiting_seen={}\n\
             editor_shutdown_started_count={}\n\
             editor_shutdown_finished_count={}\n\
             editor_process_return_seen={}\n\
             editor_process_exit_code={}\n\
             {}",
            build.package_version,
            build.build_git_commit,
            build.build_git_dirty,
            build.build_profile,
            build.build_target,
            build.build_timestamp_unix_seconds,
            build.executable_path.display(),
            build.executable_sha256,
            build
                .repo_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not_found".to_string()),
            build.repo_head_commit.as_deref().unwrap_or("not_found"),
            build
                .repo_head_matches_build
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            snapshot.info.adapter.name,
            snapshot.info.adapter.backend,
            snapshot.info.adapter.device_type,
            snapshot.info.surface_format,
            snapshot.info.present_mode,
            snapshot.render_stats.width,
            snapshot.render_stats.height,
            snapshot.render_stats.frame_index,
            snapshot.render_stats.render_graph_passes,
            snapshot.render_stats.draw_calls,
            snapshot.render_stats.allocations,
            snapshot.render_stats.budget.max_draw_calls,
            snapshot.render_stats.budget.max_allocations,
            snapshot.render_stats.budget_ok,
            snapshot.render_stats.clear_color,
            snapshot.world.renderer_kind,
            snapshot.world.visual_preset_contract,
            snapshot.world.scene_name,
            snapshot.world.scene_source_id,
            snapshot.world.scene_source_path,
            snapshot.world.scene_dependency_count,
            snapshot.world.scene_material_count,
            snapshot.world.scene_texture_count,
            snapshot.world.scene_image_count,
            snapshot.world.scene_is_blank,
            snapshot.world.blank_grid_visible,
            snapshot.world.base_color_texture_source_path,
            snapshot.world.base_color_texture_width,
            snapshot.world.base_color_texture_height,
            snapshot.world.base_color_texture_bytes,
            snapshot.world.normal_texture_source_path,
            snapshot.world.normal_texture_width,
            snapshot.world.normal_texture_height,
            snapshot.world.normal_texture_bytes,
            snapshot.world.metallic_roughness_texture_source_path,
            snapshot.world.metallic_roughness_texture_width,
            snapshot.world.metallic_roughness_texture_height,
            snapshot.world.metallic_roughness_texture_bytes,
            snapshot.world.shader_label,
            snapshot.world.post_shader_label,
            snapshot.world.hdr_format,
            snapshot.world.additive_blend_mode,
            snapshot.world.atmosphere_model,
            snapshot.world.atmosphere_profile,
            snapshot.world.bloom_passes,
            snapshot.world.bloom_intensity,
            snapshot.world.exposure,
            snapshot.world.color_grade,
            snapshot.world.background_model,
            snapshot.world.background_profile,
            snapshot.world.depth_format,
            snapshot.world.viewport_rect,
            snapshot.world.vertex_count,
            snapshot.world.index_count,
            snapshot.world.triangle_count,
            snapshot.world.mesh_primitives,
            snapshot.world.layer_state,
            snapshot.world.layer_count,
            snapshot.world.hidden_layer_count,
            snapshot.world.terrain_visible,
            snapshot.world.foliage_visible,
            snapshot.world.doodads_visible,
            snapshot.world.units_visible,
            snapshot.world.water_visible,
            snapshot.world.blight_visible,
            snapshot.world.shadows_visible,
            snapshot.world.lighting_visible,
            snapshot.world.weather_visible,
            snapshot.world.sky_visible,
            snapshot.world.fog_effects_visible,
            snapshot.world.grid_visible,
            snapshot.world.camera_bounds_visible,
            snapshot.world.pathing_ground_visible,
            snapshot.world.pathing_naval_visible,
            snapshot.world.regions_visible,
            snapshot.world.camera_objects_visible,
            snapshot.world.info_display_visible,
            snapshot.world.letterbox_mode,
            snapshot.world.lock_to_game_camera,
            snapshot.world.foliage_index_count,
            snapshot.world.foliage_template_index_count,
            snapshot.world.foliage_instance_count,
            snapshot.world.foliage_instanced_draw_calls,
            snapshot.world.foliage_distance_fade,
            snapshot.world.visible_index_count,
            snapshot.world.visible_triangle_count,
            snapshot.world.hidden_index_count,
            snapshot.world.hidden_triangle_count,
            snapshot.world.draw_calls,
            snapshot.app_stats.frame_index,
            snapshot.app_stats.variable_delta_ns,
            snapshot.app_stats.fixed_delta_ns,
            snapshot.app_stats.fixed_steps,
            snapshot.app_stats.dropped_fixed_steps,
            snapshot.app_stats.accumulated_time_ns,
            snapshot.app_stats.target_frame_ns,
            snapshot.app_stats.sleep_hint_ns,
            snapshot.app_stats.module_fixed_updates,
            snapshot.app_stats.module_updates,
            snapshot.input.stats.frame_index,
            snapshot.input.stats.action_count,
            snapshot.input.stats.binding_count,
            snapshot.input.stats.active_binding_count,
            snapshot.input.stats.events_seen,
            snapshot
                .input
                .stats
                .cursor_position
                .map(|position| format!("{:.1}", position.0))
                .unwrap_or_default(),
            snapshot
                .input
                .stats
                .cursor_position
                .map(|position| format!("{:.1}", position.1))
                .unwrap_or_default(),
            snapshot.input.camera_forward_down,
            snapshot.input.confirm_down,
            lifecycle.module_count,
            lifecycle.started,
            lifecycle.started,
            lifecycle.module_start_calls > 0,
            lifecycle.clean_shutdown,
            lifecycle.module_start_calls,
            lifecycle.module_fixed_update_calls,
            lifecycle.module_update_calls,
            lifecycle.module_shutdown_calls,
            editor_lifecycle.state,
            editor_lifecycle.last_exit_reason,
            editor_lifecycle.close_requested_count,
            editor_lifecycle.loop_exiting_seen,
            editor_lifecycle.shutdown_started_count,
            editor_lifecycle.shutdown_finished_count,
            editor_lifecycle.process_return_seen,
            editor_lifecycle.process_exit_code,
            shell_stats,
        ),
    )
}

pub fn write_fsv_lifecycle_stats(
    path: &Path,
    window_title: &str,
    build: &BuildIdentity,
    lifecycle: &LifecycleReport,
    editor_lifecycle: &EditorLifecycleEvidence,
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!(
            "window_title={window_title}\n\
             fsv_stats_kind=editor_lifecycle_without_frame_snapshot\n\
             fsv_snapshot_available=false\n\
             fsv_snapshot_missing_reason=no_rendered_frame_snapshot\n\
             package_version={}\n\
             build_git_commit={}\n\
             build_git_dirty={}\n\
             build_profile={}\n\
             build_target={}\n\
             build_timestamp_unix_seconds={}\n\
             executable_path={}\n\
             executable_sha256={}\n\
             repo_root={}\n\
             repo_head_commit={}\n\
             repo_head_matches_build={}\n\
             module_count={}\n\
             module_started={}\n\
             module_currently_started={}\n\
             module_ever_started={}\n\
             module_clean_shutdown={}\n\
             module_start_calls={}\n\
             module_fixed_update_calls={}\n\
             module_update_calls={}\n\
             module_shutdown_calls={}\n\
             editor_lifecycle_state={}\n\
             editor_last_exit_reason={}\n\
             editor_close_requested_count={}\n\
             editor_loop_exiting_seen={}\n\
             editor_shutdown_started_count={}\n\
             editor_shutdown_finished_count={}\n\
             editor_process_return_seen={}\n\
             editor_process_exit_code={}\n",
            build.package_version,
            build.build_git_commit,
            build.build_git_dirty,
            build.build_profile,
            build.build_target,
            build.build_timestamp_unix_seconds,
            build.executable_path.display(),
            build.executable_sha256,
            build
                .repo_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not_found".to_string()),
            build.repo_head_commit.as_deref().unwrap_or("not_found"),
            build
                .repo_head_matches_build
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            lifecycle.module_count,
            lifecycle.started,
            lifecycle.started,
            lifecycle.module_start_calls > 0,
            lifecycle.clean_shutdown,
            lifecycle.module_start_calls,
            lifecycle.module_fixed_update_calls,
            lifecycle.module_update_calls,
            lifecycle.module_shutdown_calls,
            editor_lifecycle.state,
            editor_lifecycle.last_exit_reason,
            editor_lifecycle.close_requested_count,
            editor_lifecycle.loop_exiting_seen,
            editor_lifecycle.shutdown_started_count,
            editor_lifecycle.shutdown_finished_count,
            editor_lifecycle.process_return_seen,
            editor_lifecycle.process_exit_code,
        ),
    )
}
