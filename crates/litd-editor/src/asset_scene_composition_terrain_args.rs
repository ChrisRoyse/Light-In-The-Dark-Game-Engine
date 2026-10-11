use super::*;

pub(super) struct AddSurfaceQuadArgs<'a> {
    pub(super) build: &'a mut CompositionBuild,
    pub(super) center: Vec3,
    pub(super) half_length: f32,
    pub(super) half_width: f32,
    pub(super) yaw_degrees: f32,
    pub(super) color: [f32; 4],
    pub(super) pbr: [f32; 4],
    pub(super) y_offset: f32,
}
