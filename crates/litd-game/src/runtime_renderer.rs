use std::{sync::Arc, time::Instant};

use litd_engine::{
    EngineError,
    determinism_boundary::{HotPathPhase, enter_hot_path},
    game_package::RuntimePackageReadback,
    gpu::{GpuContext, engine_instance_descriptor},
    input::{
        ACTION_CAMERA_BACK, ACTION_CAMERA_FORWARD, ACTION_CAMERA_LEFT, ACTION_CAMERA_RIGHT,
        ACTION_CAMERA_YAW_LEFT, ACTION_CAMERA_YAW_RIGHT, ACTION_CAMERA_ZOOM_IN,
        ACTION_CAMERA_ZOOM_OUT, ACTION_ORBIT, InputState,
    },
    math::Vec2,
    render::{
        FrameBudget, FrameStats, RendererInfo, SurfaceRendererInfo,
        camera::{CameraControlInput, EdgeScrollInput, RtsCamera},
        editor_world::{
            EditorViewportRect, EditorWorldCameraState, EditorWorldLayerVisibility,
            EditorWorldRenderEvidence, EditorWorldRenderer, PrepareFrameMetricsArgs,
            build_package_world_scene,
        },
    },
};
use winit::{dpi::PhysicalSize, window::Window};

use crate::runtime_host_module::RuntimeCounters;
use crate::runtime_state_text::RuntimeTextState;

#[path = "runtime_renderer_support.rs"]
mod runtime_renderer_support;
use runtime_renderer_support::{
    axis_from_actions, build_runtime_rts_camera, color_array, drawable_size,
};

#[path = "runtime_renderer_text.rs"]
mod runtime_renderer_text;
use runtime_renderer_text::GameRuntimeTextRenderer;

#[path = "runtime_renderer_replay_fog.rs"]
mod runtime_renderer_replay_fog;

/// Frame dt used when applying RTS camera input (host targets ~60 Hz redraw).
const CAMERA_DT_SECONDS: f32 = 1.0 / 60.0;
/// WC3-style edge band in CSS/logical-ish pixels (#1892 / #36).
const EDGE_SCROLL_MARGIN_PX: f32 = 24.0;

const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.018,
    g: 0.026,
    b: 0.040,
    a: 1.0,
};

pub struct GameRuntimeRenderer {
    window: Arc<Window>,
    package_id: String,
    surface: wgpu::Surface<'static>,
    context: GpuContext,
    config: wgpu::SurfaceConfiguration,
    world_renderer: EditorWorldRenderer,
    runtime_text: GameRuntimeTextRenderer,
    frame_index: u64,
    budget: FrameBudget,
    /// #877 RTS camera (pan/zoom/yaw). Presentation-only; not sim state.
    rts_camera: RtsCamera,
    /// Previous cursor for middle-mouse drag pan (#1892).
    last_cursor_px: Option<Vec2>,
    /// Internal (pre-upscale) render resolution derived from the graphics config's
    /// FSR upscale settings (#1741). When upscale is enabled and non-native this is
    /// smaller than the surface; when disabled it equals the surface size. The live
    /// frame graph does not yet host the reconstruction pass (no motion vectors from
    /// the world renderer), so this is currently reported for state readback and is
    /// the value a future offscreen render pass will render the world at.
    internal_render_size: (u32, u32),
    // #1485 finding 5 (game-side residual): adapter report + surface-format/present-mode strings
    // change only on (re)configure, never per frame. Cache them so `info()` (called every frame,
    // folded into the FSV snapshot) never re-clones the adapter nor re-runs `format!("{:?}", …)`.
    surface_info: SurfaceRendererInfo,
    /// When true, wrap each frame's GPU work in a validation error scope and
    /// block on `pop_error_scope`. This is a diagnostics aid (`--gpu-validation`)
    /// only: the release frame path must not block on a GPU roundtrip per frame
    /// (#1460). Construction-time validation always runs regardless.
    gpu_validation_each_frame: bool,
    replay_fog_cells: Vec<litd_engine::render::editor_world::EditorWorldFogCell>,
    last_replay_fog_key: Option<(u64, u32)>,
}

impl GameRuntimeRenderer {
    pub async fn new(
        window: Arc<Window>,
        package: &RuntimePackageReadback,
        quality: Option<litd_engine::performance_quality::GraphicsQualitySettings>,
        gpu_validation_each_frame: bool,
    ) -> Result<Self, EngineError> {
        // Prefer the live inner size; fall back to a drawable default if Windows
        // reports 0x0 before the first non-zero resize (same class as surface suspend).
        let size = drawable_size(window.inner_size()).unwrap_or(PhysicalSize::new(1280, 720));
        let instance = wgpu::Instance::new(&engine_instance_descriptor());
        let surface = instance.create_surface(window.clone())?;
        let context = GpuContext::new_for_surface(instance, &surface).await?;
        let config = surface
            .get_default_config(context.adapter(), size.width, size.height)
            .ok_or(EngineError::NoSurfaceConfig {
                width: size.width,
                height: size.height,
            })?;
        surface.configure(context.device(), &config);
        // #1891: package map file -> scene. Fail closed — never ED11 for package play.
        let package_scene = build_package_world_scene(package)?;
        context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let world_renderer = EditorWorldRenderer::new_with_scene(
            context.device(),
            context.queue(),
            config.format,
            size.width,
            size.height,
            package_scene.scene,
        )?;
        let runtime_text =
            GameRuntimeTextRenderer::new(context.device(), context.queue(), config.format)?;
        if let Some(error) = context.device().pop_error_scope().await {
            return Err(EngineError::EditorWorldRendererInvalidConfig {
                field: "runtime_world.wgpu_validation".to_string(),
                reason: error.to_string(),
            });
        }
        let budget = quality
            .map(|quality| quality.frame_budget())
            .unwrap_or_default();
        // #1741: read the FSR upscale settings from the graphics config and derive
        // the internal render resolution the world should render at. This is the
        // real runtime consumer of `GraphicsQualitySettings::upscale`; a config with
        // an invalid upscale knob fails loud here at construction rather than
        // silently rendering at native. Absent a quality config, render native.
        let internal_render_size = match quality {
            Some(quality) => quality.internal_render_dimensions()?,
            None => (config.width, config.height),
        };
        let rts_camera = build_runtime_rts_camera(
            config.width,
            config.height,
            package_scene.map_center,
            package_scene.map_half_extent,
        )?;
        tracing::info!(
            surface_width = config.width,
            surface_height = config.height,
            internal_render_width = internal_render_size.0,
            internal_render_height = internal_render_size.1,
            upscale_enabled = quality.map(|q| q.upscale.enabled).unwrap_or(false),
            camera_distance = rts_camera.state.distance,
            camera_yaw_deg = rts_camera.state.yaw_rad.to_degrees(),
            camera_pitch_deg = rts_camera.state.pitch_rad.to_degrees(),
            package_document_id = %package_scene.document_id,
            terrain_cell_count = package_scene.terrain_cell_count,
            marker_count = package_scene.marker_count,
            "runtime renderer package map + RTS camera ready"
        );
        let surface_info = SurfaceRendererInfo::from_surface(&context, &config);
        Ok(Self {
            window,
            package_id: package.package_id.clone(),
            surface,
            context,
            config,
            world_renderer,
            runtime_text,
            frame_index: 0,
            budget,
            rts_camera,
            last_cursor_px: None,
            gpu_validation_each_frame,
            surface_info,
            internal_render_size,
            replay_fog_cells: Vec::new(),
            last_replay_fog_key: None,
        })
    }

    pub fn info(&self) -> RendererInfo {
        RendererInfo {
            gpu_context_id: self.context.context_id(),
            // #1485 finding 5: read the cached, surface-derived allocations instead of re-cloning
            // the adapter report and re-running `format!("{:?}", …)` every frame.
            adapter: self.surface_info.adapter.clone(),
            surface_format: self.surface_info.surface_format.clone(),
            present_mode: self.surface_info.present_mode.clone(),
            width: self.config.width,
            height: self.config.height,
            // #1741: the config-derived internal (pre-upscale) render resolution,
            // surfaced into the persisted runtime state for FSV readback.
            internal_render_width: self.internal_render_size.0,
            internal_render_height: self.internal_render_size.1,
            render_graph_passes: self.world_renderer.render_graph_passes(),
            clear_color: color_array(CLEAR_COLOR),
            budget: self.budget,
        }
    }

    pub fn render_frame(
        &mut self,
        input: &mut InputState,
        counters: &RuntimeCounters,
    ) -> Result<(FrameStats, RendererInfo), EngineError> {
        let _hot_path = enter_hot_path(HotPathPhase::RenderFrame);
        self.sync_window_size()?;
        // Do not acquire/present while Windows has the surface at 0x0.
        if drawable_size(self.window.inner_size()).is_none() {
            return Ok((
                FrameStats {
                    frame_index: self.frame_index,
                    gpu_context_id: self.context.context_id(),
                    width: self.config.width,
                    height: self.config.height,
                    render_graph_passes: self.world_renderer.render_graph_passes(),
                    draw_calls: 0,
                    allocations: 0,
                    frame_ns: 0,
                    clear_color: color_array(CLEAR_COLOR),
                    budget: self.budget,
                    budget_ok: true,
                },
                self.info(),
            ));
        }
        self.update_camera_from_input(input)?;
        let started = Instant::now();
        let alloc_start = litd_engine::frame_alloc::allocation_count();
        let viewport = EditorViewportRect::new(0, 0, self.config.width, self.config.height)?;
        let camera = self.camera();
        let layers = EditorWorldLayerVisibility::default();
        // #1485 finding 3: the per-frame loop only needs `draw_calls`; the allocation-heavy
        // EditorWorldRenderEvidence is rebuilt on demand at heartbeat/transition cadence via
        // `frame_evidence()` (below), NOT every frame.
        let metrics = self
            .world_renderer
            .prepare_frame_metrics(PrepareFrameMetricsArgs {
                device: self.context.device(),
                queue: self.context.queue(),
                surface_width: self.config.width,
                surface_height: self.config.height,
                viewport,
                camera,
                layers,
            })?;
        if self.gpu_validation_each_frame {
            self.context
                .device()
                .push_error_scope(wgpu::ErrorFilter::Validation);
        }
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("litd-game-runtime-frame-encoder"),
                });
        let surface_texture = self.acquire_surface_texture()?;
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.world_renderer
            .render(&mut encoder, &view, viewport, CLEAR_COLOR, layers);
        let runtime_text_snapshot = counters.runtime_text_snapshot()?;
        let runtime_text = self.runtime_text.render(
            self.context.queue(),
            &mut encoder,
            &view,
            [self.config.width, self.config.height],
            camera,
            &runtime_text_snapshot,
        )?;
        tracing::debug!(
            session_tick = runtime_text.session_tick,
            labels = runtime_text.label_count,
            damage_numbers = runtime_text.damage_number_count,
            rendered_text = runtime_text.rendered_count,
            off_screen_text = runtime_text.off_screen_count,
            glyph_quads = runtime_text.submitted_glyph_count,
            text_vertices = runtime_text.submitted_vertex_count,
            plan_fnv1a64 = ?runtime_text.plan_fnv1a64,
            "runtime HUD text composed"
        );
        let text_draw_calls = u32::from(runtime_text.submitted_vertex_count > 0);
        self.context.queue().submit([encoder.finish()]);
        // Per-frame GPU validation is a blocking device roundtrip; keep it behind
        // the `--gpu-validation` diagnostics flag so the release frame path never
        // stalls on it (#1460). Construction-time validation still runs in `new`.
        if self.gpu_validation_each_frame
            && let Some(error) = pollster::block_on(self.context.device().pop_error_scope())
        {
            return Err(EngineError::EditorWorldRendererInvalidConfig {
                field: "runtime_world.frame_wgpu_validation".to_string(),
                reason: error.to_string(),
            });
        }
        self.window.pre_present_notify();
        surface_texture.present();

        self.frame_index += 1;
        let draw_calls = metrics.draw_calls.saturating_add(text_draw_calls);
        // #1532: REAL per-frame heap-allocation count via the global CountingAllocator.
        let allocations = litd_engine::frame_alloc::frame_delta(alloc_start);
        tracing::debug!(frame_allocations = allocations, draw_calls, "frame budget");
        let stats = FrameStats {
            frame_index: self.frame_index,
            gpu_context_id: self.context.context_id(),
            width: self.config.width,
            height: self.config.height,
            render_graph_passes: self.world_renderer.render_graph_passes(),
            draw_calls,
            allocations,
            frame_ns: started.elapsed().as_nanos(),
            clear_color: color_array(CLEAR_COLOR),
            budget: self.budget,
            budget_ok: self.budget.is_within(draw_calls, allocations),
        };
        Ok((stats, self.info()))
    }

    /// #1485 finding 3: rebuild the full world-render evidence on demand (at heartbeat/transition
    /// cadence), from the last `prepare_frame_metrics` inputs + current renderer state. `None`
    /// before the first frame. This replaces the per-frame evidence that `render_frame` used to
    /// return and the host stashed every frame.
    pub fn frame_evidence(&self) -> Option<EditorWorldRenderEvidence> {
        self.world_renderer.last_frame_evidence()
    }

    pub fn runtime_text_state(&self) -> &RuntimeTextState {
        self.runtime_text.state()
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), EngineError> {
        // Windows emits Resized(0x0) on minimize / DPI / show-hide transitions.
        // Match the editor surface-suspend path: keep the last drawable config and
        // do not fail-closed the whole runtime (visible skirmish was dying ~12s in).
        let Some(size) = drawable_size(size) else {
            tracing::info!(
                code = "LITD-RUNTIME-SURFACE-SUSPENDED",
                width = size.width,
                height = size.height,
                "runtime surface suspended until a non-zero resize arrives"
            );
            return Ok(());
        };
        if self.config.width == size.width && self.config.height == size.height {
            return Ok(());
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(self.context.device(), &self.config);
        // Keep the cached surface-derived info in lockstep with every reconfigure (#1485 f5).
        self.surface_info = SurfaceRendererInfo::from_surface(&self.context, &self.config);
        self.rts_camera.config.viewport_width = size.width;
        self.rts_camera.config.viewport_height = size.height;
        Ok(())
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn camera(&self) -> EditorWorldCameraState {
        // EditorWorldCameraState uses negative pitch for look-down; RtsCamera uses positive.
        EditorWorldCameraState {
            target_world: self.rts_camera.state.target,
            distance: self.rts_camera.state.distance,
            yaw_degrees: self.rts_camera.state.yaw_rad.to_degrees(),
            pitch_degrees: -self.rts_camera.state.pitch_rad.to_degrees(),
        }
    }

    pub fn frame_duration_hint() -> std::time::Duration {
        std::time::Duration::from_millis(16)
    }

    fn acquire_surface_texture(&mut self) -> Result<wgpu::SurfaceTexture, EngineError> {
        match self.surface.get_current_texture() {
            Ok(texture) => Ok(texture),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(self.context.device(), &self.config);
                Ok(self.surface.get_current_texture()?)
            }
            Err(err) => Err(err.into()),
        }
    }

    fn sync_window_size(&mut self) -> Result<(), EngineError> {
        let size = self.window.inner_size();
        // Zero-size windows are suspended; leave the last drawable surface config.
        if drawable_size(size).is_none() {
            return Ok(());
        }
        if self.config.width != size.width || self.config.height != size.height {
            self.resize(size)?;
        }
        Ok(())
    }

    fn update_camera_from_input(&mut self, input: &mut InputState) -> Result<(), EngineError> {
        // Optional arrow-key pan (no WASD — #1892).
        let mut pan_y = axis_from_actions(input, ACTION_CAMERA_FORWARD, ACTION_CAMERA_BACK);
        let mut pan_x = axis_from_actions(input, ACTION_CAMERA_RIGHT, ACTION_CAMERA_LEFT);
        let rotate = axis_from_actions(input, ACTION_CAMERA_YAW_RIGHT, ACTION_CAMERA_YAW_LEFT);
        let key_zoom = axis_from_actions(input, ACTION_CAMERA_ZOOM_IN, ACTION_CAMERA_ZOOM_OUT);
        let zoom = key_zoom + input.take_scroll_y();

        // Middle-mouse drag pan (ACTION_ORBIT bound to MMB in rts_play_defaults).
        let cursor = input
            .stats()
            .cursor_position
            .map(|(x, y)| Vec2::new(x as f32, y as f32));
        if input.action_state(ACTION_ORBIT).down
            && let (Some(cur), Some(prev)) = (cursor, self.last_cursor_px)
        {
            let delta = cur - prev;
            // Drag left -> look right (invert like WC3 middle-pan grab).
            pan_x += (-delta.x / 48.0).clamp(-2.0, 2.0);
            pan_y += (delta.y / 48.0).clamp(-2.0, 2.0);
        }
        self.last_cursor_px = cursor;

        self.rts_camera.apply_input(
            CameraControlInput {
                pan_axis: Vec2::new(pan_x, pan_y),
                zoom_axis: zoom,
                rotate_axis: rotate,
                pitch_axis: 0.0,
            },
            CAMERA_DT_SECONDS,
        )?;

        // Edge-scroll when cursor is inside the window (#36/#1892).
        if let Some(cur) = cursor {
            let w = self.config.width as f32;
            let h = self.config.height as f32;
            if cur.x >= 0.0 && cur.y >= 0.0 && cur.x <= w && cur.y <= h {
                let margin = EDGE_SCROLL_MARGIN_PX.min(w.min(h) * 0.2);
                if margin > 1.0 {
                    // Ignore edge-scroll errors at exact margins; only apply when valid.
                    if let Ok(()) = self.rts_camera.apply_edge_scroll(
                        EdgeScrollInput {
                            cursor_px: cur,
                            margin_px: margin,
                        },
                        CAMERA_DT_SECONDS,
                    ) {
                        // applied
                    }
                }
            }
        }
        Ok(())
    }
}
