use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    gpu::GpuContext,
    render::shader::{
        BindingDecl, ShaderEntryPoints, ShaderPlan, ShaderSource, ShaderSystemSettings,
        VariantAxis, build_shader_plan,
    },
};

use super::{
    StylizedMaterial, StylizedPresetKind, StylizedShaderLibrary, default_material,
    validate_material,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGallerySelection {
    pub selected_preset: StylizedPresetKind,
    pub materials: Vec<StylizedMaterial>,
    pub custom_wgsl: Option<WgslShaderEscape>,
}

impl StylizedGallerySelection {
    pub fn curated_defaults(library: &StylizedShaderLibrary) -> Result<Self, EngineError> {
        let selected_preset = StylizedPresetKind::Water;
        let materials = library
            .presets
            .iter()
            .map(|plan| default_material(&plan.preset))
            .collect();
        let selection = Self {
            selected_preset,
            materials,
            custom_wgsl: None,
        };
        build_stylized_gallery_readback(ShaderSystemSettings::default(), library, &selection)?;
        Ok(selection)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WgslShaderEscape {
    pub shader_id: String,
    pub source: String,
    pub entry_points: ShaderEntryPoints,
    pub bindings: Vec<BindingDecl>,
    pub variant_axes: Vec<VariantAxis>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGalleryReadback {
    pub selected_preset: String,
    pub library_preset_count: usize,
    pub library_preset_ids: String,
    pub material_count: usize,
    pub material_preset_ids: Vec<String>,
    pub material_parameter_counts: Vec<usize>,
    pub custom_wgsl: Option<StylizedGalleryShaderReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGalleryShaderReadback {
    pub shader_id: String,
    pub source_hash: String,
    pub source_bytes: usize,
    pub binding_count: usize,
    pub variant_count: usize,
    pub entry_points: ShaderEntryPoints,
}

pub fn build_stylized_gallery_readback(
    settings: ShaderSystemSettings,
    library: &StylizedShaderLibrary,
    selection: &StylizedGallerySelection,
) -> Result<StylizedGalleryReadback, EngineError> {
    if library.find(selection.selected_preset).is_none() {
        return invalid_gallery(
            "selected_preset",
            format!(
                "{} is not present in the shader library",
                selection.selected_preset.id()
            ),
        );
    }
    if selection.materials.is_empty() {
        return invalid_gallery("materials", "at least one no-code material is required");
    }
    let mut material_preset_ids = Vec::with_capacity(selection.materials.len());
    let mut material_parameter_counts = Vec::with_capacity(selection.materials.len());
    for material in &selection.materials {
        let plan = library.find(material.preset).ok_or_else(|| {
            gallery_error(
                "materials.preset",
                format!(
                    "{} is not present in the shader library",
                    material.preset.id()
                ),
            )
        })?;
        validate_material(&plan.preset, material)?;
        material_preset_ids.push(material.preset.id().to_string());
        material_parameter_counts.push(material.values.len());
    }
    let custom_wgsl = selection
        .custom_wgsl
        .as_ref()
        .map(|escape| custom_wgsl_readback(settings, escape))
        .transpose()?;
    Ok(StylizedGalleryReadback {
        selected_preset: selection.selected_preset.id().to_string(),
        library_preset_count: library.preset_count(),
        library_preset_ids: library.preset_ids(),
        material_count: selection.materials.len(),
        material_preset_ids,
        material_parameter_counts,
        custom_wgsl,
    })
}

pub async fn validate_wgsl_escape_gpu(
    context: &GpuContext,
    escape: &WgslShaderEscape,
) -> Result<StylizedGalleryShaderReadback, EngineError> {
    let readback = custom_wgsl_readback(ShaderSystemSettings::default(), escape)?;
    let device = context.device();
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-wgsl-escape-validation"),
        source: wgpu::ShaderSource::Wgsl(escape.source.clone().into()),
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| gallery_error("custom_wgsl.wgpu_poll", error.to_string()))?;
    if let Some(error) = device.pop_error_scope().await {
        return invalid_gallery("custom_wgsl.wgpu_validation", error.to_string());
    }
    Ok(readback)
}

fn custom_wgsl_readback(
    settings: ShaderSystemSettings,
    escape: &WgslShaderEscape,
) -> Result<StylizedGalleryShaderReadback, EngineError> {
    reject_glsl_escape(&escape.source)?;
    let source = ShaderSource::new(
        escape.shader_id.clone(),
        escape.source.clone(),
        escape.entry_points.clone(),
    )
    .with_bindings(escape.bindings.clone())
    .with_variant_axes(escape.variant_axes.clone());
    let ShaderPlan {
        id,
        source_hash,
        variants,
        bindings,
        entry_points,
        ..
    } = build_shader_plan(settings, &source, &escape.bindings)?;
    Ok(StylizedGalleryShaderReadback {
        shader_id: id,
        source_hash,
        source_bytes: escape.source.len(),
        binding_count: bindings.len(),
        variant_count: variants.len(),
        entry_points,
    })
}

fn reject_glsl_escape(source: &str) -> Result<(), EngineError> {
    let lowered = source.to_ascii_lowercase();
    let glsl_markers = [
        "#version",
        "gl_fragcolor",
        "gl_position",
        "void main",
        "layout(",
        "precision mediump",
    ];
    if let Some(marker) = glsl_markers
        .iter()
        .find(|marker| lowered.contains(**marker))
    {
        return invalid_gallery(
            "custom_wgsl.source",
            format!("GLSL marker `{marker}` is not accepted; shader escape must be WGSL"),
        );
    }
    Ok(())
}

fn invalid_gallery<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(gallery_error(field, reason))
}

fn gallery_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::StylizedShaderInvalidConfig {
            preset: "shader_gallery".to_string(),
            field: field.into(),
            reason: reason.into(),
        },
    )
}
