use super::*;

pub(super) struct AppendWithNormalsArgs<'a> {
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) normals: Vec<[f32; 3]>,
    pub(super) tangents: Option<Vec<[f32; 4]>>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) indices: Vec<u32>,
    pub(super) transform: Mat4,
    pub(super) color: [f32; 4],
    pub(super) pbr: [f32; 4],
    pub(super) requires_tangent_space: bool,
    pub(super) build: &'a mut SceneBuild,
}

pub(super) struct AppendWithFlatNormalsArgs<'a> {
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) indices: Vec<u32>,
    pub(super) transform: Mat4,
    pub(super) color: [f32; 4],
    pub(super) pbr: [f32; 4],
    pub(super) requires_tangent_space: bool,
    pub(super) build: &'a mut SceneBuild,
}
