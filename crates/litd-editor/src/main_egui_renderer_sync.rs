//! Surface / DPI / chrome-zoom synchronization methods for the egui editor renderer (#1466
//! split of `main_egui_renderer.rs`).
//!
//! A continuation `impl EguiEditorRenderer` holding the per-frame surface acquisition and the
//! window-size / egui-scale / chrome-zoom reconciliation helpers. Nested (not a sibling) so it
//! can reach `EguiEditorRenderer`'s private fields; the methods are `pub(super)` so the main
//! `render_frame` / `resize` path calls them unchanged.

use tracing::info;
use winit::dpi::PhysicalSize;

use litd_engine::EngineError;

use super::EguiEditorRenderer;
use crate::main_editor_ui::{logical_surface_size_from_physical, validate_pixels_per_point};
use crate::main_egui_scale::{drawable_size, editor_chrome_zoom_factor_for_window};

impl EguiEditorRenderer {
    pub(super) fn render_graph_passes(&self) -> u32 {
        self.world_renderer.render_graph_passes() + 1
    }

    pub(super) fn acquire_surface_texture(&mut self) -> Result<wgpu::SurfaceTexture, EngineError> {
        match self.surface.get_current_texture() {
            Ok(texture) => Ok(texture),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(self.context.device(), &self.config);
                Ok(self.surface.get_current_texture()?)
            }
            Err(err) => Err(err.into()),
        }
    }

    pub(super) fn sync_window_size(&mut self) -> Result<(), EngineError> {
        let requested_size = self.window.inner_size();
        let Some(size) = drawable_size(requested_size) else {
            return Err(EngineError::InvalidSurfaceSize {
                width: requested_size.width,
                height: requested_size.height,
            });
        };
        let chrome_zoom_factor = self.sync_editor_chrome_zoom(size)?;
        let pixels_per_point =
            super::target_pixels_per_point(self.window.as_ref(), chrome_zoom_factor)?;
        if self.config.width != size.width
            || self.config.height != size.height
            || (self.egui_pixels_per_point - pixels_per_point).abs() > f32::EPSILON
        {
            self.resize(size)?;
        }
        Ok(())
    }

    pub(super) fn sync_egui_scale(&mut self, pixels_per_point: f32) -> Result<(), EngineError> {
        validate_pixels_per_point(pixels_per_point)?;
        if (self.egui_pixels_per_point - pixels_per_point).abs() <= f32::EPSILON {
            return Ok(());
        }
        self.egui_pixels_per_point = pixels_per_point;
        let (logical_width, logical_height) = logical_surface_size_from_physical(
            self.config.width,
            self.config.height,
            pixels_per_point,
        )?;
        self.shell.rebuild_for_size(logical_width, logical_height)?;
        self.log_shell_surface_evidence("scale_change", self.egui_chrome_zoom_factor)
    }

    pub(super) fn sync_editor_chrome_zoom(
        &mut self,
        size: PhysicalSize<u32>,
    ) -> Result<f32, EngineError> {
        let chrome_zoom_factor = editor_chrome_zoom_factor_for_window(
            size,
            self.window.scale_factor(),
            self.accessibility_text_scale,
        )?;
        let previous = self.egui_chrome_zoom_factor;
        if (previous - chrome_zoom_factor).abs() > f32::EPSILON {
            self.egui_ctx.set_zoom_factor(chrome_zoom_factor);
            self.egui_chrome_zoom_factor = chrome_zoom_factor;
            info!(
                previous_chrome_zoom_factor = previous,
                chrome_zoom_factor,
                physical_width = size.width,
                physical_height = size.height,
                "editor chrome zoom adjusted for current native window size"
            );
        }
        Ok(chrome_zoom_factor)
    }

    pub(super) fn sync_accessibility_text_scale(&mut self) -> Result<(), EngineError> {
        let requested = self.shell.accessibility_text_scale();
        if (requested - self.accessibility_text_scale).abs() <= f32::EPSILON {
            return Ok(());
        }
        let mut next = self.editor_preferences.clone();
        next.text_scale = requested;
        let readback = self.preferences_store.save_preferences(&next)?;
        let previous = self.accessibility_text_scale;
        self.editor_preferences = next;
        self.accessibility_text_scale = requested;
        let chrome_zoom_factor = self.sync_editor_chrome_zoom(self.window.inner_size())?;
        self.window.request_redraw();
        info!(
            previous_text_scale = previous,
            text_scale = requested,
            chrome_zoom_factor,
            preferences_path = %readback.path,
            preferences_fingerprint = %readback.fingerprint,
            "editor accessibility text scale persisted and applied"
        );
        Ok(())
    }

    pub(super) fn log_shell_surface_evidence(
        &self,
        event: &'static str,
        chrome_zoom_factor: f32,
    ) -> Result<(), EngineError> {
        let evidence = self.shell.evidence_for_render_target(
            self.config.width,
            self.config.height,
            self.egui_pixels_per_point,
        )?;
        info!(
            event,
            shell_chrome_zoom_factor = chrome_zoom_factor,
            shell_logical_surface_size = %evidence.logical_surface_size,
            shell_physical_surface_size = %evidence.physical_surface_size,
            shell_pixels_per_point = evidence.pixels_per_point,
            shell_viewport_rect = %evidence.viewport_rect,
            shell_viewport_rect_physical = %evidence.viewport_rect_physical,
            shell_dock_panels_physical = %evidence.dock_panels_physical,
            shell_minimap_rect_physical = %evidence.minimap_rect_physical,
            shell_status_rect_physical = %evidence.status_rect_physical,
            "editor shell surface evidence"
        );
        Ok(())
    }
}
