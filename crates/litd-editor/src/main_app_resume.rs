//! Editor `ApplicationHandler::resumed` window + renderer boot (#1878/#1790 early title).

use std::sync::Arc;

use litd_engine::diagnostics::log_engine_error;
use tracing::{error, info};
use winit::{dpi::LogicalSize, event_loop::ActiveEventLoop, window::Window};

use crate::main_egui_renderer::EguiEditorRenderer;
use crate::main_egui_window::{MIN_EDITOR_LOGICAL_HEIGHT, MIN_EDITOR_LOGICAL_WIDTH};
use crate::main_lifecycle::EditorExitReason;

use super::{EditorApp, WINDOW_TITLE};

impl EditorApp {
    /// Create the titled window, show it immediately, then load the renderer/world profile.
    pub(super) fn handle_resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(
                MIN_EDITOR_LOGICAL_WIDTH,
                MIN_EDITOR_LOGICAL_HEIGHT,
            ))
            .with_visible(false);
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                error!(error = %err, "failed to create editor window");
                self.fail_and_exit(event_loop, EditorExitReason::WindowCreateFailed);
                return;
            }
        };
        let visible_window = window.clone();
        // #1878/#1790: show titled window BEFORE heavy world-profile load so Synapse/FSV
        // and operators can bind HWND + title during composition (no silent multi-minute
        // invisible hang). Title is already WINDOW_TITLE on create.
        visible_window.set_visible(true);
        visible_window.request_redraw();
        info!(
            window_title = WINDOW_TITLE,
            "editor window shown before renderer readiness (world profile load may follow)"
        );

        match pollster::block_on(EguiEditorRenderer::new(window)) {
            Ok(renderer) => {
                let info = renderer.info();
                let shell_evidence = match renderer.shell_evidence() {
                    Ok(evidence) => evidence,
                    Err(err) => {
                        log_engine_error("editor_shell_evidence", &err);
                        self.fail_and_exit(event_loop, EditorExitReason::ShellEvidenceFailed);
                        return;
                    }
                };
                info!(
                    adapter = %info.adapter.name,
                    backend = %info.adapter.backend,
                    device_type = %info.adapter.device_type,
                    surface_format = %info.surface_format,
                    present_mode = %info.present_mode,
                    width = info.width,
                    height = info.height,
                    chrome_zoom_factor = renderer.chrome_zoom_factor(),
                    render_graph_passes = info.render_graph_passes,
                    clear_color = ?info.clear_color,
                    max_draw_calls = info.budget.max_draw_calls,
                    max_allocations = info.budget.max_allocations,
                    shell_frontend = shell_evidence.frontend_kind,
                    shell_layout_units = shell_evidence.layout_units,
                    shell_logical_surface_size = %shell_evidence.logical_surface_size,
                    shell_physical_surface_size = %shell_evidence.physical_surface_size,
                    shell_pixels_per_point = shell_evidence.pixels_per_point,
                    shell_active_mode = ?shell_evidence.active_mode,
                    shell_active_inspector = %shell_evidence.active_inspector_title,
                    "renderer initialized"
                );
                if !self.start_runtime(event_loop) {
                    return;
                }
                info!("editor window remains visible after renderer readiness");
                renderer.request_redraw();
                self.renderer = Some(renderer);
            }
            Err(err) => {
                log_engine_error("editor_renderer_init", &err);
                self.fail_and_exit(event_loop, EditorExitReason::RendererInitFailed);
            }
        }
    }
}
