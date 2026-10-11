use crate::gpu::{AdapterReport, GpuContext};

/// #1485 finding 5: the allocation-heavy, surface-derived fields of `RendererInfo` — the
/// adapter report clone plus the two `format!("{:?}", ...)` strings for the surface format and
/// present mode — change ONLY when the surface is (re)configured, never per frame. They are
/// cached here so a renderer's `info()` (called every frame, then folded into the FSV snapshot)
/// never re-clones the adapter report nor re-runs the Debug formatting each frame.
///
/// The cheap `Copy` fields of `RendererInfo` (gpu_context_id, width, height,
/// render_graph_passes, clear_color, budget) are deliberately NOT cached — `info` reads them
/// live so they can never go stale; only the fields that require heap allocation are memoized.
///
/// Shared by BOTH the editor renderer (`litd-editor`, finding 5a) and the shipping game runtime
/// renderer (`litd-game`, finding 5 game-side residual) so the fix lives in exactly one place and
/// neither steady-state frame path re-runs the surface formatting.
#[derive(Debug, Clone)]
pub struct SurfaceRendererInfo {
    pub adapter: AdapterReport,
    pub surface_format: String,
    pub present_mode: String,
}

impl SurfaceRendererInfo {
    /// Rebuild the cache from the live GPU context and surface configuration. Call this from
    /// EVERY path that (re)configures the surface (initial construction and resize), so the
    /// cached format/present-mode strings can never lag `config`.
    pub fn from_surface(context: &GpuContext, config: &wgpu::SurfaceConfiguration) -> Self {
        Self {
            adapter: context.adapter_report().clone(),
            surface_format: format!("{:?}", config.format),
            present_mode: format!("{:?}", config.present_mode),
        }
    }
}
