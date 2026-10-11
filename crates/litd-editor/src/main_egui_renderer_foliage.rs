use litd_engine::{EngineError, render::editor_world::EditorWorldRenderer};

use crate::main_editor_ui::EditorShellRuntime;
use crate::main_egui_minimap::editor_shell_scene_report;
use crate::main_egui_world_profile::load_editor_world_profile_with_foliage_brush_path;

pub fn reload_editor_world_scene(
    shell: &mut EditorShellRuntime,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    surface_width: u32,
    surface_height: u32,
) -> Result<EditorWorldRenderer, EngineError> {
    let asset_scene =
        load_editor_world_profile_with_foliage_brush_path(Some(shell.foliage_brush_state_path()))?;
    let shell_scene_report = editor_shell_scene_report(&asset_scene.report)?;
    shell.replace_world_scene_report(&shell_scene_report)?;
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let world_renderer = EditorWorldRenderer::new_with_scene(
        device,
        queue,
        surface_format,
        surface_width,
        surface_height,
        asset_scene.scene,
    )?;
    let validation = device.pop_error_scope();
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| EngineError::EditorWorldRendererInvalidConfig {
            field: "editor_world.reload_wgpu_validation_poll".to_string(),
            reason: format!("device poll failed: {error}"),
        })?;
    if let Some(error) = pollster::block_on(validation) {
        return Err(EngineError::EditorWorldRendererInvalidConfig {
            field: "editor_world.reload_wgpu_validation".to_string(),
            reason: error.to_string(),
        });
    }
    Ok(world_renderer)
}
