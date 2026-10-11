use super::*;

pub struct PrepareFrameArgs<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub surface_width: u32,
    pub surface_height: u32,
    pub viewport: EditorViewportRect,
    pub camera: EditorWorldCameraState,
    pub layers: EditorWorldLayerVisibility,
}

pub struct PrepareFrameMetricsArgs<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub surface_width: u32,
    pub surface_height: u32,
    pub viewport: EditorViewportRect,
    pub camera: EditorWorldCameraState,
    pub layers: EditorWorldLayerVisibility,
}

/// #1485 finding 3: the cheap, all-`Copy` per-frame result the shipping frame loop consumes (game
/// uses `draw_calls`) so the steady-state frame does NOT build the ~25 heap strings of
/// [`EditorWorldRenderEvidence`] — that is rebuilt on demand via `last_frame_evidence`.
#[derive(Debug, Clone, Copy)]
pub struct EditorWorldFrameMetrics {
    pub renderer_kind: &'static str,
    pub draw_calls: u32,
    pub visible_index_count: u32,
    pub visible_triangle_count: u32,
    pub triangle_count: u32,
    pub viewport: EditorViewportRect,
}
