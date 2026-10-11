use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use litd_engine::{
    EngineError,
    gpu::{GpuContext, engine_instance_descriptor},
    render::{
        FrameBudget, FrameStats, RendererInfo,
        editor_world::{
            EditorWorldFrameMetrics, EditorWorldRenderEvidence, EditorWorldRenderer,
            PrepareFrameMetricsArgs,
        },
    },
};
use tracing::{info, warn};
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::main_editor_ui::{
    EditorShellRuntime, EditorShellRuntimeEvidence, logical_surface_size_from_physical,
    validate_pixels_per_point,
};
use crate::main_egui_minimap::editor_shell_scene_report;
use crate::main_egui_renderer_foliage::reload_editor_world_scene;
use crate::main_egui_renderer_info::SurfaceRendererInfo;
use crate::main_egui_scale::{drawable_size, editor_chrome_zoom_factor_for_window, non_zero_size};
use crate::main_egui_style::apply_editor_style;
use crate::main_egui_surface::{color_array, preferred_egui_surface_format};
use crate::main_egui_window::{corrected_minimum_size, request_minimum_inner_size};
use crate::main_egui_world_profile::load_editor_world_profile_with_foliage_brush_path;

#[path = "main_egui_renderer_sync.rs"]
mod main_egui_renderer_sync;

const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.022,
    g: 0.030,
    b: 0.044,
    a: 1.0,
};
pub struct EguiEditorRenderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    context: GpuContext,
    config: wgpu::SurfaceConfiguration,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: Renderer,
    world_renderer: EditorWorldRenderer,
    shell: EditorShellRuntime,
    preferences_store: litd_editor::editor_preferences::EditorPreferencesStore,
    editor_preferences: litd_editor::editor_preferences::EditorPreferences,
    accessibility_text_scale: f32,
    // #1485 finding 5a: cached surface-derived RendererInfo fields (adapter/format/present_mode).
    // Refreshed only on surface (re)configure; `info()` reads this instead of re-cloning the
    // adapter report and re-running two `format!("{:?}", ...)` every frame.
    surface_info: SurfaceRendererInfo,
    egui_chrome_zoom_factor: f32,
    egui_pixels_per_point: f32,
    budget: FrameBudget,
    frame_index: u64,
    // #1485 finding 1: when false (default), the steady-state frame never pushes a GPU error
    // scope nor blocks draining the device — the CPU proceeds without waiting for all submitted
    // GPU work. `poll(wait_indefinitely)` every frame serialized CPU/GPU and capped FPS at one
    // full pipeline latency. Enabled only as a diagnostics aid via LITD_EDITOR_GPU_VALIDATION,
    // mirroring litd-game's `--gpu-validation` gate (#1460).
    gpu_validation_each_frame: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceResizeState {
    Drawable { width: u32, height: u32 },
    Suspended { width: u32, height: u32 },
}

impl EguiEditorRenderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, EngineError> {
        let size = non_zero_size(window.inner_size())?;
        let instance = wgpu::Instance::new(&engine_instance_descriptor());
        let surface = instance.create_surface(window.clone())?;
        let context = GpuContext::new_for_surface(instance, &surface).await?;
        info!("editor renderer configuring surface");
        let mut config = surface
            .get_default_config(context.adapter(), size.width, size.height)
            .ok_or(EngineError::NoSurfaceConfig {
                width: size.width,
                height: size.height,
            })?;
        config.format = preferred_egui_surface_format(&surface, context.adapter())?;
        surface.configure(context.device(), &config);
        info!(
            width = size.width,
            height = size.height,
            surface_format = ?config.format,
            "editor renderer surface configured"
        );

        let preferences_store =
            litd_editor::editor_preferences::EditorPreferencesStore::from_env()?;
        let preferences_outcome = preferences_store.load_or_default_loud();
        if !preferences_outcome.error.is_empty() {
            warn!(
                error = %preferences_outcome.error,
                "editor preferences unavailable; explicit accessibility defaults selected"
            );
        }
        let editor_preferences = preferences_outcome.preferences;
        let accessibility_text_scale = editor_preferences.text_scale;
        let egui_ctx = egui::Context::default();
        apply_editor_style(&egui_ctx);
        let chrome_zoom_factor = editor_chrome_zoom_factor_for_window(
            size,
            window.scale_factor(),
            accessibility_text_scale,
        )?;
        egui_ctx.set_zoom_factor(chrome_zoom_factor);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(context.device().limits().max_texture_dimension_2d as usize),
        );
        let pixels_per_point = target_pixels_per_point(window.as_ref(), chrome_zoom_factor)?;
        let (logical_width, logical_height) =
            logical_surface_size_from_physical(size.width, size.height, pixels_per_point)?;
        let egui_renderer =
            Renderer::new(context.device(), config.format, RendererOptions::default());
        info!("editor renderer loading world profile");
        let world_profile_started = std::time::Instant::now();
        let foliage_brush_store =
            litd_editor::asset_scene_foliage_brush::FoliageBrushStore::from_env()?;
        let asset_scene =
            load_editor_world_profile_with_foliage_brush_path(Some(foliage_brush_store.path()))?;
        info!(
            elapsed_ms = world_profile_started.elapsed().as_millis() as u64,
            product_id = %asset_scene.report.product_id,
            dependency_count = asset_scene.report.dependency_count,
            vertex_count = asset_scene.report.vertex_count,
            "editor renderer world profile loaded"
        );
        let shell_scene_report = editor_shell_scene_report(&asset_scene.report)?;
        info!("editor renderer initializing shell runtime");
        let mut shell = EditorShellRuntime::new_with_world_scene_report(
            logical_width,
            logical_height,
            &shell_scene_report,
        )?;
        shell.set_accessibility_text_scale(accessibility_text_scale)?;
        info!("editor renderer creating world renderer");
        context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let world_renderer = EditorWorldRenderer::new_with_scene(
            context.device(),
            context.queue(),
            config.format,
            size.width,
            size.height,
            asset_scene.scene,
        )?;
        info!("editor renderer polling world renderer validation");
        let validation = context.device().pop_error_scope();
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| EngineError::EditorWorldRendererInvalidConfig {
                field: "editor_world.wgpu_validation_poll".to_string(),
                reason: format!("device poll failed: {error}"),
            })?;
        if let Some(error) = validation.await {
            return Err(EngineError::EditorWorldRendererInvalidConfig {
                field: "editor_world.wgpu_validation".to_string(),
                reason: error.to_string(),
            });
        }
        info!("editor renderer world renderer validation passed");

        let surface_info = SurfaceRendererInfo::from_surface(&context, &config);
        Ok(Self {
            window,
            surface,
            context,
            config,
            egui_ctx,
            egui_state,
            egui_renderer,
            world_renderer,
            shell,
            preferences_store,
            editor_preferences,
            accessibility_text_scale,
            surface_info,
            egui_chrome_zoom_factor: chrome_zoom_factor,
            egui_pixels_per_point: pixels_per_point,
            budget: FrameBudget::default(),
            frame_index: 0,
            gpu_validation_each_frame:
                crate::main_egui_gpu_validation::gpu_validation_each_frame_from_env(),
        })
    }

    pub fn info(&self) -> RendererInfo {
        RendererInfo {
            gpu_context_id: self.context.context_id(),
            adapter: self.surface_info.adapter.clone(),
            surface_format: self.surface_info.surface_format.clone(),
            present_mode: self.surface_info.present_mode.clone(),
            width: self.config.width,
            height: self.config.height,
            internal_render_width: self.config.width,
            internal_render_height: self.config.height,
            render_graph_passes: self.render_graph_passes(),
            clear_color: color_array(CLEAR_COLOR),
            budget: self.budget,
        }
    }

    pub fn shell_evidence(&self) -> Result<EditorShellRuntimeEvidence, EngineError> {
        self.shell.evidence_for_render_target(
            self.config.width,
            self.config.height,
            self.egui_pixels_per_point,
        )
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        let _ = self.egui_state.on_window_event(&self.window, event);
    }

    pub fn chrome_zoom_factor(&self) -> f32 {
        self.egui_chrome_zoom_factor
    }

    pub fn resize(
        &mut self,
        requested_size: PhysicalSize<u32>,
    ) -> Result<SurfaceResizeState, EngineError> {
        let Some(size) = drawable_size(requested_size) else {
            return Ok(SurfaceResizeState::Suspended {
                width: requested_size.width,
                height: requested_size.height,
            });
        };
        let chrome_zoom_factor = self.sync_editor_chrome_zoom(size)?;
        let pixels_per_point = target_pixels_per_point(self.window.as_ref(), chrome_zoom_factor)?;
        if let Some(corrected_size) = corrected_minimum_size(size, pixels_per_point) {
            request_minimum_inner_size(self.window.as_ref(), size, corrected_size);
            return Ok(SurfaceResizeState::Suspended {
                width: size.width,
                height: size.height,
            });
        }
        if self.config.width == size.width
            && self.config.height == size.height
            && (self.egui_pixels_per_point - pixels_per_point).abs() <= f32::EPSILON
        {
            return Ok(SurfaceResizeState::Drawable {
                width: size.width,
                height: size.height,
            });
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(self.context.device(), &self.config);
        // #1485 finding 5a: refresh the cached surface-derived RendererInfo fields on every
        // surface reconfigure. Width/height only change here (format/present_mode do not), but
        // rebuilding unconditionally keeps the cache correct even if a future edit alters the
        // surface format on resize — conservative over aggressive.
        self.surface_info = SurfaceRendererInfo::from_surface(&self.context, &self.config);
        self.egui_pixels_per_point = pixels_per_point;
        let (logical_width, logical_height) =
            logical_surface_size_from_physical(size.width, size.height, pixels_per_point)?;
        self.shell.rebuild_for_size(logical_width, logical_height)?;
        self.log_shell_surface_evidence("resize", chrome_zoom_factor)?;
        Ok(SurfaceResizeState::Drawable {
            width: size.width,
            height: size.height,
        })
    }

    pub fn render_frame(
        &mut self,
    ) -> Result<
        (
            FrameStats,
            RendererInfo,
            EditorShellRuntimeEvidence,
            EditorWorldFrameMetrics,
        ),
        EngineError,
    > {
        self.sync_window_size()?;
        let started = Instant::now();
        let alloc_start = litd_engine::frame_alloc::allocation_count();
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let egui_ctx = self.egui_ctx.clone();
        let full_output = egui_ctx.run(raw_input, |ctx| {
            self.shell.show(ctx);
        });
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);
        self.sync_accessibility_text_scale()?;
        if self.shell.take_world_scene_reload_requested() {
            self.world_renderer = reload_editor_world_scene(
                &mut self.shell,
                self.context.device(),
                self.context.queue(),
                self.config.format,
                self.config.width,
                self.config.height,
            )?;
        }
        let pixels_per_point = full_output.pixels_per_point;
        self.sync_egui_scale(pixels_per_point)?;
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point,
        };
        let world_viewport = self.shell.viewport_rect_for_surface(
            self.config.width,
            self.config.height,
            pixels_per_point,
        )?;
        let world_camera = self.shell.world_camera_state();
        let world_layers = self.shell.world_layer_visibility();
        // #1485 finding 3: the editor per-frame path builds only the cheap all-`Copy`
        // EditorWorldFrameMetrics; the ~25-heap-string EditorWorldRenderEvidence is rebuilt on
        // demand via `world_frame_evidence()` at FSV snapshot cadence, NOT every frame. Mirrors the
        // shipping game (runtime_renderer::render_frame -> prepare_frame_metrics + frame_evidence).
        let world_metrics = self
            .world_renderer
            .prepare_frame_metrics(PrepareFrameMetricsArgs {
                device: self.context.device(),
                queue: self.context.queue(),
                surface_width: self.config.width,
                surface_height: self.config.height,
                viewport: world_viewport,
                camera: world_camera,
                layers: world_layers,
            })?;
        // #1485 finding 1: only scope-check GPU validation when the diagnostics flag is on; the
        // release frame path never pushes a scope (and never drains the device below).
        if self.gpu_validation_each_frame {
            self.context
                .device()
                .push_error_scope(wgpu::ErrorFilter::Validation);
        }
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("litd-editor-egui-encoder"),
                });
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(
                self.context.device(),
                self.context.queue(),
                *id,
                image_delta,
            );
        }
        let user_commands = self.egui_renderer.update_buffers(
            self.context.device(),
            self.context.queue(),
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        let surface_texture = self.acquire_surface_texture()?;
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.world_renderer.render(
            &mut encoder,
            &view,
            world_viewport,
            CLEAR_COLOR,
            world_layers,
        );
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("litd-editor-egui-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.egui_renderer.render(
                &mut render_pass.forget_lifetime(),
                &paint_jobs,
                &screen_descriptor,
            );
        }
        self.context
            .queue()
            .submit(user_commands.into_iter().chain([encoder.finish()]));
        // #1485 finding 1: the release steady-state frame does NOT block draining the GPU
        // (`poll(wait_indefinitely)` serialized CPU/GPU every frame, capping FPS at one pipeline
        // latency). Only when the diagnostics flag is on do we pop the scope and surface any
        // validation error — matching litd-game's gated path (#1460), which likewise does not
        // force a full device drain.
        if self.gpu_validation_each_frame
            && let Some(error) = pollster::block_on(self.context.device().pop_error_scope())
        {
            return Err(EngineError::EditorWorldRendererInvalidConfig {
                field: "editor_world.frame_wgpu_validation".to_string(),
                reason: error.to_string(),
            });
        }
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
        self.window.pre_present_notify();
        surface_texture.present();

        self.frame_index += 1;
        let draw_calls = world_metrics.draw_calls + u32::from(!paint_jobs.is_empty());
        let allocations = litd_engine::frame_alloc::frame_delta(alloc_start);
        tracing::debug!(frame_allocations = allocations, draw_calls, "frame budget");
        let stats = FrameStats {
            frame_index: self.frame_index,
            gpu_context_id: self.context.context_id(),
            width: self.config.width,
            height: self.config.height,
            render_graph_passes: self.render_graph_passes(),
            draw_calls,
            allocations,
            frame_ns: started.elapsed().as_nanos(),
            clear_color: color_array(CLEAR_COLOR),
            budget: self.budget,
            budget_ok: self.budget.is_within(draw_calls, allocations),
        };
        let shell_evidence = self.shell.evidence_for_render_target(
            self.config.width,
            self.config.height,
            pixels_per_point,
        )?;
        Ok((stats, self.info(), shell_evidence, world_metrics))
    }

    /// #1485 finding 3: rebuild the full [`EditorWorldRenderEvidence`] on demand from the last
    /// prepared frame's inputs plus current renderer state. Called ONLY at FSV snapshot cadence
    /// (periodic/exit), never per frame — so the ~25 heap-string clones + `format!`s are paid at
    /// snapshot cadence, not every frame. `None` before the first rendered frame.
    pub fn world_frame_evidence(&self) -> Option<EditorWorldRenderEvidence> {
        self.world_renderer.last_frame_evidence()
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn surface_resize_state(&self) -> SurfaceResizeState {
        let size = self.window.inner_size();
        match drawable_size(size) {
            Some(size) if corrected_minimum_size(size, self.egui_pixels_per_point).is_none() => {
                SurfaceResizeState::Drawable {
                    width: size.width,
                    height: size.height,
                }
            }
            Some(size) => SurfaceResizeState::Suspended {
                width: size.width,
                height: size.height,
            },
            None => SurfaceResizeState::Suspended {
                width: size.width,
                height: size.height,
            },
        }
    }

    pub fn frame_duration_hint() -> Duration {
        Duration::from_millis(16)
    }
}

fn target_pixels_per_point(window: &Window, chrome_zoom_factor: f32) -> Result<f32, EngineError> {
    validate_pixels_per_point(chrome_zoom_factor)?;
    let pixels_per_point = window.scale_factor() as f32 * chrome_zoom_factor;
    validate_pixels_per_point(pixels_per_point)?;
    Ok(pixels_per_point)
}
