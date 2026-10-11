//! Metallic-roughness PBR materials and Cook-Torrance/GGX shading helpers.

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Vec3, Vec4},
};

pub const PBR_WGSL: &str = include_str!("material_pbr.wgsl");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureSlot {
    BaseColor,
    MetallicRoughness,
    Normal,
    AmbientOcclusion,
    Emissive,
    Irradiance,
    PrefilteredEnvironment,
    BrdfLut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureBinding {
    pub slot: TextureSlot,
    pub asset_id: String,
}

impl TextureBinding {
    pub fn new(slot: TextureSlot, asset_id: impl Into<String>) -> Self {
        Self {
            slot,
            asset_id: asset_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PbrMaterial {
    pub name: String,
    pub base_color_factor: Vec4,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub normal_scale: f32,
    pub occlusion_strength: f32,
    pub emissive_factor: Vec3,
    pub textures: Vec<TextureBinding>,
}

impl PbrMaterial {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_color_factor: Vec4::ONE,
            metallic_factor: 0.0,
            roughness_factor: 1.0,
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            emissive_factor: Vec3::ZERO,
            textures: Vec::new(),
        }
    }

    pub fn with_texture(mut self, binding: TextureBinding) -> Self {
        self.textures.push(binding);
        self
    }

    pub fn has_texture(&self, slot: TextureSlot) -> bool {
        self.textures.iter().any(|binding| binding.slot == slot)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        validate_range("metallic_factor", self.metallic_factor, 0.0, 1.0)?;
        validate_range("roughness_factor", self.roughness_factor, 0.04, 1.0)?;
        validate_range("normal_scale", self.normal_scale, 0.0, 8.0)?;
        validate_range("occlusion_strength", self.occlusion_strength, 0.0, 1.0)?;
        for (channel, value) in [
            ("base_color_factor.r", self.base_color_factor.x),
            ("base_color_factor.g", self.base_color_factor.y),
            ("base_color_factor.b", self.base_color_factor.z),
            ("base_color_factor.a", self.base_color_factor.w),
        ] {
            validate_range(channel, value, 0.0, 1.0)?;
        }
        Ok(())
    }

    pub fn texture_mask(&self) -> u32 {
        self.textures.iter().fold(0, |mask, binding| {
            mask | (1 << texture_slot_index(binding.slot))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PbrLight {
    pub direction_to_light: Vec3,
    pub radiance: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IblEnvironment {
    pub diffuse_irradiance: Vec3,
    pub specular_prefilter: Vec3,
    pub brdf_scale: f32,
    pub brdf_bias: f32,
}

impl Default for IblEnvironment {
    fn default() -> Self {
        Self {
            diffuse_irradiance: Vec3::splat(0.03),
            specular_prefilter: Vec3::splat(0.02),
            brdf_scale: 0.8,
            brdf_bias: 0.02,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PbrSample {
    pub normal: Vec3,
    pub view: Vec3,
    pub light: PbrLight,
    pub environment: IblEnvironment,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PbrShadeResult {
    pub direct: Vec3,
    pub ibl: Vec3,
    pub emissive: Vec3,
    pub color: Vec3,
}

pub fn shade(material: &PbrMaterial, sample: PbrSample) -> Result<PbrShadeResult, EngineError> {
    material.validate()?;
    let normal = normalize_checked("normal", sample.normal)?;
    let view = normalize_checked("view", sample.view)?;
    let light_dir = normalize_checked("direction_to_light", sample.light.direction_to_light)?;
    let half_vec = normalize_checked("half_vector", view + light_dir)?;
    let n_dot_l = normal.dot(light_dir).max(0.0);
    let n_dot_v = normal.dot(view).max(0.0);
    let n_dot_h = normal.dot(half_vec).max(0.0);
    let v_dot_h = view.dot(half_vec).max(0.0);

    let base_color = material.base_color_factor.truncate();
    let f0 = Vec3::splat(0.04).lerp(base_color, material.metallic_factor);
    let fresnel = fresnel_schlick(v_dot_h, f0);
    let distribution = distribution_ggx(n_dot_h, material.roughness_factor);
    let geometry = geometry_smith(n_dot_v, n_dot_l, material.roughness_factor);
    let denominator = (4.0 * n_dot_v * n_dot_l).max(0.001);
    let specular = fresnel * (distribution * geometry / denominator);
    let diffuse = (Vec3::ONE - fresnel) * (1.0 - material.metallic_factor) * base_color
        / std::f32::consts::PI;
    let direct = (diffuse + specular) * sample.light.radiance * n_dot_l;

    let ibl_diffuse = sample.environment.diffuse_irradiance
        * base_color
        * (1.0 - material.metallic_factor)
        * material.occlusion_strength;
    let ibl_specular = sample.environment.specular_prefilter
        * (fresnel * sample.environment.brdf_scale + Vec3::splat(sample.environment.brdf_bias));
    let ibl = ibl_diffuse + ibl_specular;
    let emissive = material.emissive_factor;
    Ok(PbrShadeResult {
        direct,
        ibl,
        emissive,
        color: direct + ibl + emissive,
    })
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    alpha2 / (std::f32::consts::PI * denom * denom).max(0.001)
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let k = ((roughness + 1.0) * (roughness + 1.0)) / 8.0;
    geometry_schlick_ggx(n_dot_v, k) * geometry_schlick_ggx(n_dot_l, k)
}

fn geometry_schlick_ggx(n_dot: f32, k: f32) -> f32 {
    n_dot / (n_dot * (1.0 - k) + k).max(0.001)
}

fn fresnel_schlick(cos_theta: f32, f0: Vec3) -> Vec3 {
    f0 + (Vec3::ONE - f0) * (1.0 - cos_theta).clamp(0.0, 1.0).powf(5.0)
}

fn validate_range(field: &'static str, value: f32, min: f32, max: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value < min || value > max {
        return Err(EngineError::Rendering(
            litd_engine::error::RenderingError::PbrMaterialInvalidFactor {
                field,
                value,
                min,
                max,
            },
        ));
    }
    Ok(())
}

fn normalize_checked(field: &'static str, vector: Vec3) -> Result<Vec3, EngineError> {
    if !vector.is_finite() || vector.length_squared() <= f32::EPSILON {
        return Err(EngineError::Rendering(
            litd_engine::error::RenderingError::PbrInvalidVector {
                field,
                value: vector.to_array(),
            },
        ));
    }
    Ok(vector.normalize())
}

fn texture_slot_index(slot: TextureSlot) -> u32 {
    match slot {
        TextureSlot::BaseColor => 0,
        TextureSlot::MetallicRoughness => 1,
        TextureSlot::Normal => 2,
        TextureSlot::AmbientOcclusion => 3,
        TextureSlot::Emissive => 4,
        TextureSlot::Irradiance => 5,
        TextureSlot::PrefilteredEnvironment => 6,
        TextureSlot::BrdfLut => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_validation_rejects_invalid_roughness() {
        let mut material = PbrMaterial::new("bad");
        material.roughness_factor = 0.0;
        match material.validate() {
            Err(EngineError::Rendering(
                litd_engine::error::RenderingError::PbrMaterialInvalidFactor { field, .. },
            )) => {
                assert_eq!(field, "roughness_factor");
            }
            other => panic!("expected PbrMaterialInvalidFactor, got {other:?}"),
        }
    }

    #[test]
    fn texture_mask_tracks_required_pbr_slots() {
        let material = PbrMaterial::new("mask")
            .with_texture(TextureBinding::new(TextureSlot::BaseColor, "base"))
            .with_texture(TextureBinding::new(TextureSlot::Normal, "normal"))
            .with_texture(TextureBinding::new(TextureSlot::BrdfLut, "brdf"));
        assert_eq!(material.texture_mask(), 0b1000_0101);
    }

    #[test]
    fn ggx_shading_is_deterministic() {
        let mut material = PbrMaterial::new("ash iron");
        material.base_color_factor = Vec4::new(0.8, 0.2, 0.1, 1.0);
        material.metallic_factor = 0.5;
        material.roughness_factor = 0.35;
        let result = shade(
            &material,
            PbrSample {
                normal: Vec3::Z,
                view: Vec3::Z,
                light: PbrLight {
                    direction_to_light: Vec3::new(0.0, 0.25, 1.0),
                    radiance: Vec3::splat(3.0),
                },
                environment: IblEnvironment::default(),
            },
        )
        .unwrap();

        assert!(result.color.x > result.color.y);
        assert!(result.direct.length() > result.ibl.length());
    }
}
