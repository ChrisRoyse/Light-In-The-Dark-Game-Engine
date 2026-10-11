//! GPU morph-target vertex shader contract for blend-shape weights.

use crate::{
    EngineError,
    animation::{
        MORPH_DELTA_BINDING, MORPH_SHADER_ID, MORPH_WEIGHT_BIND_GROUP, MORPH_WEIGHT_BINDING,
    },
    render::shader::{
        BindingDecl, BindingKind, ShaderEntryPoints, ShaderPlan, ShaderSource, ShaderStage,
        ShaderSystemSettings, build_shader_plan,
    },
};

pub const MORPHING_WGSL: &str = include_str!("morphing.wgsl");

pub fn morphing_bindings() -> Vec<BindingDecl> {
    vec![
        BindingDecl::new(
            MORPH_WEIGHT_BIND_GROUP,
            MORPH_WEIGHT_BINDING,
            vec![ShaderStage::Vertex],
            BindingKind::StorageBuffer { read_only: true },
        ),
        BindingDecl::new(
            MORPH_WEIGHT_BIND_GROUP,
            MORPH_DELTA_BINDING,
            vec![ShaderStage::Vertex],
            BindingKind::StorageBuffer { read_only: true },
        ),
    ]
}

pub fn morphing_shader_source() -> ShaderSource {
    ShaderSource::new(
        MORPH_SHADER_ID,
        MORPHING_WGSL,
        ShaderEntryPoints {
            vertex: Some("litd_morphing_vs".to_string()),
            fragment: None,
            compute: None,
        },
    )
    .with_bindings(morphing_bindings())
}

pub fn build_morphing_shader_plan(
    settings: ShaderSystemSettings,
) -> Result<ShaderPlan, EngineError> {
    let source = morphing_shader_source();
    build_shader_plan(settings, &source, &morphing_bindings())
}
