//! GPU vertex-skinning shader contract for skeletal joint palettes.

use bytemuck::{Pod, Zeroable};

use crate::{
    EngineError,
    animation::{SKINNING_PALETTE_BIND_GROUP, SKINNING_PALETTE_BINDING, SKINNING_SHADER_ID},
    render::shader::{
        BindingDecl, BindingKind, ShaderEntryPoints, ShaderPlan, ShaderSource, ShaderStage,
        ShaderSystemSettings, build_shader_plan,
    },
};

pub const SKINNING_WGSL: &str = include_str!("skinning.wgsl");

pub const SKINNING_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 10,
        format: wgpu::VertexFormat::Uint32x4,
    },
    wgpu::VertexAttribute {
        offset: 16,
        shader_location: 11,
        format: wgpu::VertexFormat::Float32x4,
    },
];

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct SkinningVertexBuffer {
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

pub fn skinning_palette_bindings() -> Vec<BindingDecl> {
    vec![BindingDecl::new(
        SKINNING_PALETTE_BIND_GROUP,
        SKINNING_PALETTE_BINDING,
        vec![ShaderStage::Vertex],
        BindingKind::StorageBuffer { read_only: true },
    )]
}

pub fn skinning_shader_source() -> ShaderSource {
    ShaderSource::new(
        SKINNING_SHADER_ID,
        SKINNING_WGSL,
        ShaderEntryPoints {
            vertex: Some("litd_skinning_vs".to_string()),
            fragment: None,
            compute: None,
        },
    )
    .with_bindings(skinning_palette_bindings())
}

pub fn build_skinning_shader_plan(
    settings: ShaderSystemSettings,
) -> Result<ShaderPlan, EngineError> {
    let source = skinning_shader_source();
    build_shader_plan(settings, &source, &skinning_palette_bindings())
}
