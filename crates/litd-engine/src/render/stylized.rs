//! No-code stylized shader preset library built on the WGSL shader system.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    render::shader::{
        BindingDecl, BindingKind, ShaderEntryPoints, ShaderPlan, ShaderSource, ShaderStage,
        ShaderSystemSettings, VariantAxis, build_shader_plan,
    },
};

pub const STYLIZED_EFFECTS_WGSL: &str = include_str!("stylized_effects.wgsl");

#[path = "stylized_gallery.rs"]
mod stylized_gallery;
#[path = "stylized_gpu.rs"]
mod stylized_gpu;
#[path = "stylized_gpu_image.rs"]
mod stylized_gpu_image;

pub use stylized_gallery::{
    StylizedGalleryReadback, StylizedGallerySelection, StylizedGalleryShaderReadback,
    WgslShaderEscape, build_stylized_gallery_readback, validate_wgsl_escape_gpu,
};
pub use stylized_gpu::{
    StylizedGpuFrame, StylizedGpuMaterialReadback, StylizedGpuScene, StylizedGpuStats,
    render_stylized_materials_gpu, render_stylized_materials_gpu_with_context,
};
pub use stylized_gpu_image::{StylizedGpuImage, StylizedGpuMetrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StylizedPresetKind {
    Water,
    Fire,
    Glow,
    Toon,
    Dissolve,
    ForceField,
}

impl StylizedPresetKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Water => "water",
            Self::Fire => "fire",
            Self::Glow => "glow",
            Self::Toon => "toon",
            Self::Dissolve => "dissolve",
            Self::ForceField => "force_field",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Water => "Water",
            Self::Fire => "Fire",
            Self::Glow => "Glow / Emissive",
            Self::Toon => "Toon",
            Self::Dissolve => "Dissolve",
            Self::ForceField => "Force Field",
        }
    }

    pub fn fragment_entry(self) -> &'static str {
        match self {
            Self::Water => "litd_stylized_water_fs",
            Self::Fire => "litd_stylized_fire_fs",
            Self::Glow => "litd_stylized_glow_fs",
            Self::Toon => "litd_stylized_toon_fs",
            Self::Dissolve => "litd_stylized_dissolve_fs",
            Self::ForceField => "litd_stylized_force_field_fs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StylizedParameterKind {
    Scalar,
    Color,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StylizedValue {
    Scalar(f32),
    Color([f32; 4]),
    Toggle(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedParameter {
    pub name: String,
    pub kind: StylizedParameterKind,
    pub default: StylizedValue,
    pub range: Option<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedShaderPreset {
    pub kind: StylizedPresetKind,
    pub id: String,
    pub display_name: String,
    pub parameters: Vec<StylizedParameter>,
    pub variant_axes: Vec<VariantAxis>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedShaderPlan {
    pub preset: StylizedShaderPreset,
    pub shader_plan: ShaderPlan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedShaderLibrary {
    pub presets: Vec<StylizedShaderPlan>,
}

impl StylizedShaderLibrary {
    pub fn preset_count(&self) -> usize {
        self.presets.len()
    }

    pub fn total_parameter_count(&self) -> usize {
        self.presets
            .iter()
            .map(|plan| plan.preset.parameters.len())
            .sum()
    }

    pub fn total_variant_count(&self) -> usize {
        self.presets
            .iter()
            .map(|plan| plan.shader_plan.variants.len())
            .sum()
    }

    pub fn preset_ids(&self) -> String {
        self.presets
            .iter()
            .map(|plan| plan.preset.id.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn find(&self, kind: StylizedPresetKind) -> Option<&StylizedShaderPlan> {
        self.presets.iter().find(|plan| plan.preset.kind == kind)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedMaterial {
    pub preset: StylizedPresetKind,
    pub values: BTreeMap<String, StylizedValue>,
}

pub fn build_stylized_shader_library(
    settings: ShaderSystemSettings,
) -> Result<StylizedShaderLibrary, EngineError> {
    let mut presets = Vec::new();
    let mut seen = BTreeSet::new();
    for preset in preset_templates() {
        if !seen.insert(preset.kind) {
            return invalid_stylized(preset.id, "preset", "duplicate preset kind");
        }
        validate_preset(&preset)?;
        let source = shader_source_for(&preset);
        let shader_plan = build_shader_plan(settings, &source, &stylized_bindings())?;
        presets.push(StylizedShaderPlan {
            preset,
            shader_plan,
        });
    }
    Ok(StylizedShaderLibrary { presets })
}

pub fn default_material(preset: &StylizedShaderPreset) -> StylizedMaterial {
    StylizedMaterial {
        preset: preset.kind,
        values: preset
            .parameters
            .iter()
            .map(|param| (param.name.clone(), param.default.clone()))
            .collect(),
    }
}

pub fn validate_material(
    preset: &StylizedShaderPreset,
    material: &StylizedMaterial,
) -> Result<(), EngineError> {
    if material.preset != preset.kind {
        return invalid_stylized(
            preset.id.clone(),
            "material.preset",
            "material targets a different preset",
        );
    }
    for key in material.values.keys() {
        if !preset.parameters.iter().any(|param| &param.name == key) {
            return invalid_stylized(preset.id.clone(), key.clone(), "unknown tweak parameter");
        }
    }
    for param in &preset.parameters {
        let Some(value) = material.values.get(&param.name) else {
            return invalid_stylized(preset.id.clone(), param.name.clone(), "missing parameter");
        };
        validate_value(&preset.id, param, value)?;
    }
    Ok(())
}

pub fn stylized_bindings() -> Vec<BindingDecl> {
    vec![
        BindingDecl::new(
            0,
            0,
            vec![ShaderStage::Vertex, ShaderStage::Fragment],
            BindingKind::UniformBuffer,
        ),
        BindingDecl::new(1, 0, vec![ShaderStage::Fragment], BindingKind::Texture),
        BindingDecl::new(1, 1, vec![ShaderStage::Fragment], BindingKind::Sampler),
        BindingDecl::new(
            2,
            0,
            vec![ShaderStage::Fragment],
            BindingKind::UniformBuffer,
        ),
    ]
}

fn preset_templates() -> Vec<StylizedShaderPreset> {
    vec![
        preset(
            StylizedPresetKind::Water,
            vec![
                color("water_color", [0.05, 0.35, 0.85, 1.0]),
                color("foam_color", [0.7, 0.95, 1.0, 1.0]),
                scalar("wave_strength", 0.55, 0.0, 2.0),
                scalar("flow_speed", 1.25, 0.0, 10.0),
            ],
            vec![
                VariantAxis::new("FLOW", vec!["LOW", "HIGH"]),
                VariantAxis::new("FOAM", vec!["OFF", "ON"]),
            ],
        ),
        preset(
            StylizedPresetKind::Fire,
            vec![
                color("core_color", [1.0, 0.82, 0.25, 1.0]),
                color("edge_color", [1.0, 0.18, 0.02, 1.0]),
                scalar("intensity", 4.0, 0.0, 12.0),
                scalar("noise_speed", 2.0, 0.0, 12.0),
            ],
            vec![
                VariantAxis::new("QUALITY", vec!["LOW", "HIGH"]),
                VariantAxis::new("EMBERS", vec!["OFF", "ON"]),
            ],
        ),
        preset(
            StylizedPresetKind::Glow,
            vec![
                color("emissive_color", [0.35, 0.75, 1.0, 1.0]),
                scalar("intensity", 3.0, 0.0, 20.0),
                scalar("pulse_speed", 0.5, 0.0, 10.0),
            ],
            vec![VariantAxis::new("BLOOM", vec!["SOFT", "HOT"])],
        ),
        preset(
            StylizedPresetKind::Toon,
            vec![
                color("base_color", [0.75, 0.62, 0.42, 1.0]),
                color("shadow_color", [0.22, 0.20, 0.28, 1.0]),
                scalar("bands", 4.0, 2.0, 6.0),
                toggle("rim_light", true),
            ],
            vec![
                VariantAxis::new("BANDS", vec!["THREE", "FIVE"]),
                VariantAxis::new("RIM", vec!["OFF", "ON"]),
            ],
        ),
        preset(
            StylizedPresetKind::Dissolve,
            vec![
                color("edge_color", [0.2, 0.85, 1.0, 1.0]),
                scalar("threshold", 0.45, 0.0, 1.0),
                scalar("edge_width", 0.05, 0.0, 0.25),
            ],
            vec![
                VariantAxis::new("EDGE", vec!["SOFT", "HARD"]),
                VariantAxis::new("NOISE", vec!["LOW", "HIGH"]),
            ],
        ),
        preset(
            StylizedPresetKind::ForceField,
            vec![
                color("field_color", [0.25, 0.85, 1.0, 1.0]),
                scalar("opacity", 0.55, 0.0, 1.0),
                scalar("pulse_rate", 1.5, 0.0, 12.0),
            ],
            vec![
                VariantAxis::new("PULSE", vec!["OFF", "ON"]),
                VariantAxis::new("HEX", vec!["OFF", "ON"]),
            ],
        ),
    ]
}

fn preset(
    kind: StylizedPresetKind,
    parameters: Vec<StylizedParameter>,
    variant_axes: Vec<VariantAxis>,
) -> StylizedShaderPreset {
    StylizedShaderPreset {
        kind,
        id: kind.id().to_string(),
        display_name: kind.display_name().to_string(),
        parameters,
        variant_axes,
    }
}

fn shader_source_for(preset: &StylizedShaderPreset) -> ShaderSource {
    ShaderSource::new(
        format!("litd.stylized.{}", preset.id),
        STYLIZED_EFFECTS_WGSL,
        ShaderEntryPoints::graphics("litd_stylized_vs", preset.kind.fragment_entry()),
    )
    .with_bindings(stylized_bindings())
    .with_variant_axes(preset.variant_axes.clone())
}

fn validate_preset(preset: &StylizedShaderPreset) -> Result<(), EngineError> {
    if preset.parameters.is_empty() {
        return invalid_stylized(preset.id.clone(), "parameters", "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for param in &preset.parameters {
        validate_parameter_name(&preset.id, &param.name)?;
        if !seen.insert(&param.name) {
            return invalid_stylized(preset.id.clone(), param.name.clone(), "duplicate parameter");
        }
        validate_value(&preset.id, param, &param.default)?;
    }
    Ok(())
}

fn validate_parameter_name(preset: &str, name: &str) -> Result<(), EngineError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return invalid_stylized(preset, name, "parameter names must be lower_snake_case");
    }
    Ok(())
}

fn validate_value(
    preset: &str,
    param: &StylizedParameter,
    value: &StylizedValue,
) -> Result<(), EngineError> {
    match (param.kind, value) {
        (StylizedParameterKind::Scalar, StylizedValue::Scalar(value)) => {
            let [min, max] = param.range.unwrap_or([f32::NEG_INFINITY, f32::INFINITY]);
            if !value.is_finite() || *value < min || *value > max {
                return invalid_stylized(preset, param.name.clone(), "scalar outside range");
            }
        }
        (StylizedParameterKind::Color, StylizedValue::Color(color)) => {
            if color
                .iter()
                .any(|channel| !channel.is_finite() || *channel < 0.0 || *channel > 1.0)
            {
                return invalid_stylized(
                    preset,
                    param.name.clone(),
                    "color channels must be 0..=1",
                );
            }
        }
        (StylizedParameterKind::Toggle, StylizedValue::Toggle(_)) => {}
        _ => return invalid_stylized(preset, param.name.clone(), "value type mismatch"),
    }
    Ok(())
}

fn scalar(name: &str, default: f32, min: f32, max: f32) -> StylizedParameter {
    StylizedParameter {
        name: name.to_string(),
        kind: StylizedParameterKind::Scalar,
        default: StylizedValue::Scalar(default),
        range: Some([min, max]),
    }
}

fn color(name: &str, default: [f32; 4]) -> StylizedParameter {
    StylizedParameter {
        name: name.to_string(),
        kind: StylizedParameterKind::Color,
        default: StylizedValue::Color(default),
        range: None,
    }
}

fn toggle(name: &str, default: bool) -> StylizedParameter {
    StylizedParameter {
        name: name.to_string(),
        kind: StylizedParameterKind::Toggle,
        default: StylizedValue::Toggle(default),
        range: None,
    }
}

fn invalid_stylized<T>(
    preset: impl Into<String>,
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::Rendering(
        litd_engine::error::RenderingError::StylizedShaderInvalidConfig {
            preset: preset.into(),
            field: field.into(),
            reason: reason.into(),
        },
    ))
}

#[cfg(test)]
#[path = "stylized_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stylized_gpu_tests.rs"]
mod stylized_gpu_tests;
