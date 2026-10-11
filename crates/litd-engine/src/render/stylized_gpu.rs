use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::{EngineError, gpu::GpuContext, render::shader::ShaderSystemSettings};

use super::{
    STYLIZED_EFFECTS_WGSL, StylizedGpuImage, StylizedGpuMetrics, StylizedMaterial,
    StylizedPresetKind, StylizedShaderLibrary, StylizedValue, build_stylized_shader_library,
    default_material, stylized_gpu_image, validate_material,
};

#[path = "stylized_gpu_wgpu.rs"]
mod stylized_gpu_wgpu;

use stylized_gpu_wgpu::*;

const MAX_TARGET_PIXELS: u64 = 4096 * 4096;
const MAX_MATERIAL_PASSES: usize = 64;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct StylizedCameraGpu {
    view_proj: [f32; 16],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct StylizedParamsGpu {
    primary_color: [f32; 4],
    secondary_color: [f32; 4],
    params0: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGpuScene {
    pub width: u32,
    pub height: u32,
    pub materials: Vec<StylizedMaterial>,
}

impl StylizedGpuScene {
    pub fn from_materials(width: u32, height: u32, materials: Vec<StylizedMaterial>) -> Self {
        Self {
            width,
            height,
            materials,
        }
    }

    pub fn curated_defaults(
        library: &StylizedShaderLibrary,
        width: u32,
        height: u32,
    ) -> Result<Self, EngineError> {
        let materials = library
            .presets
            .iter()
            .map(|plan| default_material(&plan.preset))
            .collect();
        let scene = Self::from_materials(width, height, materials);
        validate_scene(library, &scene)?;
        Ok(scene)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGpuFrame {
    pub image: StylizedGpuImage,
    pub stats: StylizedGpuStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub shader_bytes: usize,
    pub target_texture_format: String,
    pub width: u32,
    pub height: u32,
    pub library_preset_count: usize,
    pub total_parameter_count: usize,
    pub material_count: usize,
    pub draw_calls: u32,
    pub executed_fragment_entries: Vec<String>,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub output_checksum: String,
    pub output_metrics: StylizedGpuMetrics,
    pub preset_readbacks: Vec<StylizedGpuMaterialReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGpuMaterialReadback {
    pub preset_id: String,
    pub fragment_entry: String,
    pub parameter_count: usize,
    pub region: [u32; 4],
    pub metrics: StylizedGpuMetrics,
}

#[derive(Debug, Clone)]
pub(super) struct StylizedGpuMaterialPass {
    pub preset_id: String,
    pub fragment_entry: &'static str,
    pub parameter_count: usize,
    pub region: [u32; 4],
    pub params: StylizedParamsGpu,
}

pub async fn render_stylized_materials_gpu(
    scene: &StylizedGpuScene,
) -> Result<StylizedGpuFrame, EngineError> {
    let context = GpuContext::new_headless().await?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    render_stylized_materials_gpu_with_context(&context, &library, scene).await
}

pub async fn render_stylized_materials_gpu_with_context(
    context: &GpuContext,
    library: &StylizedShaderLibrary,
    scene: &StylizedGpuScene,
) -> Result<StylizedGpuFrame, EngineError> {
    validate_scene(library, scene)?;
    let passes = material_passes(library, scene)?;
    let readback = render_material_passes(context, scene, &passes)?;
    let image = StylizedGpuImage::from_pixels(scene.width, scene.height, readback.pixels);
    let preset_readbacks = passes
        .iter()
        .map(|pass| StylizedGpuMaterialReadback {
            preset_id: pass.preset_id.clone(),
            fragment_entry: pass.fragment_entry.to_string(),
            parameter_count: pass.parameter_count,
            region: pass.region,
            metrics: stylized_gpu_image::measure_region(
                &image,
                pass.region[0],
                pass.region[1],
                pass.region[2],
                pass.region[3],
            ),
        })
        .collect();
    let output_metrics = stylized_gpu_image::measure(&image);
    let stats = StylizedGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        shader_bytes: STYLIZED_EFFECTS_WGSL.len(),
        target_texture_format: format!("{:?}", wgpu::TextureFormat::Rgba8Unorm),
        width: scene.width,
        height: scene.height,
        library_preset_count: library.preset_count(),
        total_parameter_count: library.total_parameter_count(),
        material_count: scene.materials.len(),
        draw_calls: readback.executed_fragment_entries.len() as u32,
        executed_fragment_entries: readback.executed_fragment_entries,
        readback_bytes: readback.readback_bytes,
        row_pitch: readback.row_pitch,
        output_checksum: image.checksum(),
        output_metrics,
        preset_readbacks,
    };
    Ok(StylizedGpuFrame { image, stats })
}

fn validate_scene(
    library: &StylizedShaderLibrary,
    scene: &StylizedGpuScene,
) -> Result<(), EngineError> {
    if scene.width == 0 || scene.height == 0 {
        return invalid_stylized_gpu(
            "scene",
            "stylized_gpu.viewport",
            "width and height must be non-zero",
        );
    }
    if scene.width as u64 * scene.height as u64 > MAX_TARGET_PIXELS {
        return invalid_stylized_gpu(
            "scene",
            "stylized_gpu.viewport",
            "width * height must be <= 4096 * 4096",
        );
    }
    if scene.materials.is_empty() {
        return invalid_stylized_gpu(
            "scene",
            "stylized_gpu.materials",
            "at least one material pass is required",
        );
    }
    if scene.materials.len() > MAX_MATERIAL_PASSES || scene.width < scene.materials.len() as u32 {
        return invalid_stylized_gpu(
            "scene",
            "stylized_gpu.materials",
            "material count must fit inside the target width and stay <= 64",
        );
    }
    for material in &scene.materials {
        let plan = library.find(material.preset).ok_or_else(|| {
            stylized_gpu_error(
                material.preset.id(),
                "material.preset",
                "preset is not present in the stylized library",
            )
        })?;
        validate_material(&plan.preset, material)?;
    }
    Ok(())
}

fn material_passes(
    library: &StylizedShaderLibrary,
    scene: &StylizedGpuScene,
) -> Result<Vec<StylizedGpuMaterialPass>, EngineError> {
    scene
        .materials
        .iter()
        .enumerate()
        .map(|(index, material)| {
            let plan = library.find(material.preset).ok_or_else(|| {
                stylized_gpu_error(
                    material.preset.id(),
                    "material.preset",
                    "preset is not present in the stylized library",
                )
            })?;
            Ok(StylizedGpuMaterialPass {
                preset_id: plan.preset.id.clone(),
                fragment_entry: material.preset.fragment_entry(),
                parameter_count: plan.preset.parameters.len(),
                region: stripe_region(index, scene.materials.len(), scene.width, scene.height),
                params: params_for_material(&plan.preset.id, material)?,
            })
        })
        .collect()
}

fn stripe_region(index: usize, count: usize, width: u32, height: u32) -> [u32; 4] {
    let x0 = (index as u32 * width) / count as u32;
    let x1 = ((index as u32 + 1) * width) / count as u32;
    [x0, 0, x1 - x0, height]
}

fn params_for_material(
    preset: &str,
    material: &StylizedMaterial,
) -> Result<StylizedParamsGpu, EngineError> {
    let params = match material.preset {
        StylizedPresetKind::Water => StylizedParamsGpu {
            primary_color: color_value(preset, material, "water_color")?,
            secondary_color: color_value(preset, material, "foam_color")?,
            params0: [
                scalar_value(preset, material, "wave_strength")?,
                scalar_value(preset, material, "flow_speed")?,
                0.0,
                0.0,
            ],
        },
        StylizedPresetKind::Fire => StylizedParamsGpu {
            primary_color: color_value(preset, material, "core_color")?,
            secondary_color: color_value(preset, material, "edge_color")?,
            params0: [
                scalar_value(preset, material, "intensity")?,
                scalar_value(preset, material, "noise_speed")?,
                0.0,
                0.0,
            ],
        },
        StylizedPresetKind::Glow => StylizedParamsGpu {
            primary_color: color_value(preset, material, "emissive_color")?,
            secondary_color: [0.0, 0.0, 0.0, 1.0],
            params0: [
                scalar_value(preset, material, "intensity")?,
                0.0,
                scalar_value(preset, material, "pulse_speed")?,
                0.0,
            ],
        },
        StylizedPresetKind::Toon => StylizedParamsGpu {
            primary_color: color_value(preset, material, "base_color")?,
            secondary_color: color_value(preset, material, "shadow_color")?,
            params0: [
                scalar_value(preset, material, "bands")?,
                toggle_value(preset, material, "rim_light")?,
                0.0,
                0.0,
            ],
        },
        StylizedPresetKind::Dissolve => StylizedParamsGpu {
            primary_color: color_value(preset, material, "edge_color")?,
            secondary_color: [0.02, 0.02, 0.04, 1.0],
            params0: [
                scalar_value(preset, material, "threshold")?,
                scalar_value(preset, material, "edge_width")?,
                0.0,
                0.0,
            ],
        },
        StylizedPresetKind::ForceField => StylizedParamsGpu {
            primary_color: color_value(preset, material, "field_color")?,
            secondary_color: [0.0, 0.0, 0.0, 1.0],
            params0: [
                scalar_value(preset, material, "opacity")?,
                scalar_value(preset, material, "pulse_rate")?,
                0.0,
                0.0,
            ],
        },
    };
    Ok(params)
}

fn color_value(
    preset: &str,
    material: &StylizedMaterial,
    name: &str,
) -> Result<[f32; 4], EngineError> {
    match material.values.get(name) {
        Some(StylizedValue::Color(value)) => Ok(*value),
        _ => Err(stylized_gpu_error(preset, name, "expected color parameter")),
    }
}

fn scalar_value(preset: &str, material: &StylizedMaterial, name: &str) -> Result<f32, EngineError> {
    match material.values.get(name) {
        Some(StylizedValue::Scalar(value)) => Ok(*value),
        _ => Err(stylized_gpu_error(
            preset,
            name,
            "expected scalar parameter",
        )),
    }
}

fn toggle_value(preset: &str, material: &StylizedMaterial, name: &str) -> Result<f32, EngineError> {
    match material.values.get(name) {
        Some(StylizedValue::Toggle(true)) => Ok(1.0),
        Some(StylizedValue::Toggle(false)) => Ok(0.0),
        _ => Err(stylized_gpu_error(
            preset,
            name,
            "expected toggle parameter",
        )),
    }
}

pub(super) fn identity_camera() -> StylizedCameraGpu {
    StylizedCameraGpu {
        view_proj: [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
    }
}

fn invalid_stylized_gpu<T>(
    preset: impl Into<String>,
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(stylized_gpu_error(preset, field, reason))
}

pub(super) fn stylized_gpu_error(
    preset: impl Into<String>,
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::StylizedShaderInvalidConfig {
            preset: preset.into(),
            field: field.into(),
            reason: reason.into(),
        },
    )
}
