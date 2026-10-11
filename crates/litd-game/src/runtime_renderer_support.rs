use litd_engine::{
    EngineError,
    input::InputState,
    math::{Aabb, Vec3},
    render::camera::{RtsCamera, RtsCameraConfig, RtsCameraState},
};
use winit::dpi::PhysicalSize;

pub(super) fn axis_from_actions(input: &InputState, positive: &str, negative: &str) -> f32 {
    let mut axis = 0.0;
    if input.action_state(positive).down {
        axis += 1.0;
    }
    if input.action_state(negative).down {
        axis -= 1.0;
    }
    axis
}

/// Frame RTS camera on the package map center (#1891).
pub(super) fn build_runtime_rts_camera(
    width: u32,
    height: u32,
    map_center: Vec3,
    map_half_extent: f32,
) -> Result<RtsCamera, EngineError> {
    let pad = (map_half_extent + 4.0).max(12.0);
    let bounds = Aabb::from_center_half_extents(map_center, Vec3::new(pad, 40.0, pad))?;
    let mut config = RtsCameraConfig::standard(width.max(1), height.max(1), bounds);
    config.max_distance = (map_half_extent * 4.0).clamp(24.0, 120.0);
    config.min_distance = 6.0;
    config.pan_speed = 28.0;
    config.zoom_speed = 36.0;
    config.rotate_speed_rad = 100.0_f32.to_radians();
    let state = RtsCameraState {
        target: map_center,
        yaw_rad: 45.0_f32.to_radians(),
        pitch_rad: 58.0_f32.to_radians(),
        distance: (map_half_extent * 2.4).clamp(18.0, 80.0),
    };
    RtsCamera::new(config, state)
}

/// Drawable (non-zero) physical size, or `None` when Windows reports a suspended surface.
pub(super) fn drawable_size(size: PhysicalSize<u32>) -> Option<PhysicalSize<u32>> {
    if size.width == 0 || size.height == 0 {
        None
    } else {
        Some(size)
    }
}

pub(super) fn color_array(color: wgpu::Color) -> [f64; 4] {
    [color.r, color.g, color.b, color.a]
}
