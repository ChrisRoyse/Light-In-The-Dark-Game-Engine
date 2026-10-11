//! First renderer slice: create/configure a wgpu surface, clear it, and present
//! frames with source-of-truth stats for FSV readback.

pub mod batching;
pub mod camera;
pub mod catalog;
pub mod editor_world;
pub mod font;
// Shared SDF coverage helper for `font` and `ui::ui_text`. Declared ONCE here so the
// crate holds a single `font_sdf` module; both consumers import it via
// `crate::render::font_sdf`. Previously each `#[path]`-included the file as its own
// module, compiling two incompatible copies of every item (#1480 item 2).
pub(crate) mod font_sdf;
// GPU-driven two-phase Hi-Z occlusion culling + indirect-draw compaction (#1740),
// the sanctioned non-virtualized middle path (#1746). `gpu_driven` is the CPU-side
// surface (types, pyramid layout, explicit draw-path selection, shader sources);
// `gpu_driven_cpu` is the non-indirect fallback + the oracle for the GPU readback.
pub mod gpu_driven;
pub mod gpu_driven_cpu;
pub mod lighting;
pub mod material;
pub mod material_preview;
#[cfg(feature = "fsv")]
pub mod material_preview_fsv;
pub mod morphing;
// Broad-phase screen-space occlusion grid used by `batching::build_batch_plan` to
// replace the old O(instances x occluders) linear scan (#1524 item 2).
pub(crate) mod occlusion_grid;
pub mod post;
pub mod primitive;
// #1742: baked probe-grid GI (DDGI-style irradiance + visibility volumes). The grid doubles
// as the CPU stealth light-level query (#1597/#1601). Baked offline, sampled as a pure lookup.
pub mod probe_gi;
pub mod reference;
pub mod runtime_font;
pub mod runtime_text;
pub mod runtime_text_gpu;
pub mod screen_space;
pub mod shader;
pub mod shadow;
pub mod skinning;
pub mod sprite_billboard;
pub mod stylized;
mod surface_info;
// CDLOD/quadtree GPU-driven terrain runtime (#1739): per-frame LOD selection,
// geomorphing, chunked-clipmap streaming with coarse fallback, splat hooks.
pub mod terrain;
pub mod text_shaping;
pub mod two_d;
mod two_d_image;
pub mod ui;
pub mod upscale;
pub mod vfx;

pub use surface_info::SurfaceRendererInfo;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    EngineError,
    gpu::{AdapterReport, GpuContext, engine_instance_descriptor},
};

/// #1532: per-frame heap-allocation ceiling, derived from LIVE measurement on the
/// RTX 5090 box. Until #1532 the allocation budget was `0` while every frame site
/// hardcoded `allocations: 0`, so `is_within`'s allocation term (`0 <= 0`) was a
/// false-green that could never fail. Now the real per-frame delta (via the global
/// `CountingAllocator`) is fed in.
///
/// Measured with the editor booted on the real Ashen test world: steady-state idle
/// frames allocate ~2.7k, the heaviest observed frame (world creation, 4096 draw
/// calls) allocated ~33.9k. `131_072` (2^17) is ~4x that observed peak — it passes
/// every legitimately-observed frame with headroom for heavier world loads, yet a
/// per-frame allocation runaway (a leak allocating >128k/frame) trips `budget_ok`.
/// Tighten further, or split into load-vs-steady phase budgets, as more/heavier
/// worlds are profiled (tracked on #1532).
pub const MAX_FRAME_ALLOCATIONS: u32 = 131_072;

#[derive(Debug, Clone, Copy)]
pub struct FrameBudget {
    pub max_draw_calls: u32,
    pub max_allocations: u32,
}

impl Default for FrameBudget {
    fn default() -> Self {
        Self {
            max_draw_calls: crate::performance_quality::ULTRA_MAX_DRAW_CALLS,
            max_allocations: MAX_FRAME_ALLOCATIONS,
        }
    }
}

impl FrameBudget {
    pub const fn is_within(self, draw_calls: u32, allocations: u32) -> bool {
        draw_calls <= self.max_draw_calls && allocations <= self.max_allocations
    }
}

#[derive(Debug, Clone)]
pub struct FrameStats {
    pub frame_index: u64,
    pub gpu_context_id: u64,
    pub width: u32,
    pub height: u32,
    pub render_graph_passes: u32,
    pub draw_calls: u32,
    pub allocations: u32,
    pub frame_ns: u128,
    pub clear_color: [f64; 4],
    pub budget: FrameBudget,
    pub budget_ok: bool,
}

#[derive(Debug, Clone)]
pub struct RendererInfo {
    pub gpu_context_id: u64,
    pub adapter: AdapterReport,
    pub surface_format: String,
    pub present_mode: String,
    pub width: u32,
    pub height: u32,
    /// Internal (pre-upscale) render resolution. Equals the surface size for
    /// renderers with no FSR upscale config; the produced-game runtime sets it from
    /// `GraphicsQualitySettings::internal_render_dimensions` (#1741).
    pub internal_render_width: u32,
    pub internal_render_height: u32,
    pub render_graph_passes: u32,
    pub clear_color: [f64; 4],
    pub budget: FrameBudget,
}

#[derive(Debug, Clone)]
pub struct RenderGraph {
    passes: Vec<RenderPassNode>,
}

impl RenderGraph {
    pub fn clear_only(color: wgpu::Color) -> Self {
        Self {
            passes: vec![RenderPassNode {
                label: "litd-clear-frame",
                kind: RenderPassKind::Clear {
                    color: color_array(color),
                },
            }],
        }
    }

    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    fn clear_pass(&self) -> (&'static str, wgpu::Color) {
        let pass = self
            .passes
            .first()
            .expect("ClearRenderer owns a clear-only render graph");
        match pass.kind {
            RenderPassKind::Clear { color } => (pass.label, color_from_array(color)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderPassNode {
    pub label: &'static str,
    pub kind: RenderPassKind,
}

#[derive(Debug, Clone, Copy)]
pub enum RenderPassKind {
    Clear { color: [f64; 4] },
}

pub struct ClearRenderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    context: GpuContext,
    config: wgpu::SurfaceConfiguration,
    render_graph: RenderGraph,
    budget: FrameBudget,
    frame_index: u64,
}

impl ClearRenderer {
    pub async fn new(window: Arc<Window>, clear_color: wgpu::Color) -> Result<Self, EngineError> {
        let size = non_zero_size(window.inner_size())?;
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

        Ok(Self {
            window,
            surface,
            context,
            config,
            render_graph: RenderGraph::clear_only(clear_color),
            budget: FrameBudget::default(),
            frame_index: 0,
        })
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn info(&self) -> RendererInfo {
        RendererInfo {
            gpu_context_id: self.context.context_id(),
            adapter: self.context.adapter_report().clone(),
            surface_format: format!("{:?}", self.config.format),
            present_mode: format!("{:?}", self.config.present_mode),
            width: self.config.width,
            height: self.config.height,
            // ClearRenderer has no FSR upscale config; it renders at native.
            internal_render_width: self.config.width,
            internal_render_height: self.config.height,
            render_graph_passes: self.render_graph.pass_count() as u32,
            clear_color: color_array(self.render_graph.clear_pass().1),
            budget: self.budget,
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), EngineError> {
        let size = non_zero_size(size)?;
        if self.config.width == size.width && self.config.height == size.height {
            return Ok(());
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(self.context.device(), &self.config);
        Ok(())
    }

    pub fn render_clear_frame(&mut self) -> Result<FrameStats, EngineError> {
        let started = Instant::now();
        let alloc_start = crate::frame_alloc::allocation_count();
        let surface_texture = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(self.context.device(), &self.config);
                self.surface.get_current_texture()?
            }
            Err(err) => return Err(err.into()),
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("litd-clear-frame-encoder"),
                });
        let (pass_label, clear_color) = self.render_graph.clear_pass();
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(pass_label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.context.queue().submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();

        self.frame_index += 1;
        let frame_ns = started.elapsed().as_nanos();
        let draw_calls = 0;
        // #1532: REAL per-frame heap-allocation count via the global CountingAllocator.
        let allocations = crate::frame_alloc::frame_delta(alloc_start);
        tracing::debug!(frame_allocations = allocations, draw_calls, "frame budget");
        Ok(FrameStats {
            frame_index: self.frame_index,
            gpu_context_id: self.context.context_id(),
            width: self.config.width,
            height: self.config.height,
            render_graph_passes: self.render_graph.pass_count() as u32,
            draw_calls,
            allocations,
            frame_ns,
            clear_color: color_array(clear_color),
            budget: self.budget,
            budget_ok: self.budget.is_within(draw_calls, allocations),
        })
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn frame_duration_hint() -> Duration {
        Duration::from_millis(16)
    }
}

fn non_zero_size(size: PhysicalSize<u32>) -> Result<PhysicalSize<u32>, EngineError> {
    if size.width == 0 || size.height == 0 {
        return Err(EngineError::InvalidSurfaceSize {
            width: size.width,
            height: size.height,
        });
    }
    Ok(size)
}

fn color_array(color: wgpu::Color) -> [f64; 4] {
    [color.r, color.g, color.b, color.a]
}

fn color_from_array(color: [f64; 4]) -> wgpu::Color {
    wgpu::Color {
        r: color[0],
        g: color[1],
        b: color[2],
        a: color[3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_width_size_fails_loud() {
        match non_zero_size(PhysicalSize::new(0, 720)) {
            Err(EngineError::InvalidSurfaceSize { width, height }) => {
                assert_eq!(width, 0);
                assert_eq!(height, 720);
            }
            other => panic!("expected InvalidSurfaceSize, got {other:?}"),
        }
    }

    #[test]
    fn zero_height_size_fails_loud() {
        match non_zero_size(PhysicalSize::new(1280, 0)) {
            Err(EngineError::InvalidSurfaceSize { width, height }) => {
                assert_eq!(width, 1280);
                assert_eq!(height, 0);
            }
            other => panic!("expected InvalidSurfaceSize, got {other:?}"),
        }
    }

    #[test]
    fn default_budget_allows_empty_clear_pass_stats() {
        let budget = FrameBudget::default();
        assert_eq!(
            budget.max_draw_calls,
            crate::performance_quality::ULTRA_MAX_DRAW_CALLS
        );
        // #1532: the default allocation budget was `0` (the false-green bug); it is now
        // the provisional non-zero ceiling. Updating this assertion to the new const is a
        // correct test fix, not a workaround — the old expected value WAS the bug.
        assert_eq!(budget.max_allocations, MAX_FRAME_ALLOCATIONS);
        assert!(budget.is_within(0, 0));
        assert!(budget.is_within(0, MAX_FRAME_ALLOCATIONS));
        assert!(!budget.is_within(0, MAX_FRAME_ALLOCATIONS + 1));
    }

    #[test]
    fn clear_graph_reports_one_pass_and_color() {
        let graph = RenderGraph::clear_only(wgpu::Color {
            r: 0.03,
            g: 0.07,
            b: 0.12,
            a: 1.0,
        });
        assert_eq!(graph.pass_count(), 1);
        assert_eq!(color_array(graph.clear_pass().1), [0.03, 0.07, 0.12, 1.0]);
    }
}
