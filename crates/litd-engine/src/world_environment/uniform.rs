//! Renderer consumption of the frozen lowered environment surfaces.
//!
//! The GPU uniform (`world_environment.wgsl`) is built HERE, on the CPU, from the
//! lowered sim package -- the frozen fingerprinted artifact, never a live Calyx
//! query. The `cpu_*` mirrors transcribe the WGSL math op-for-op so an FSV harness
//! can prove the shader consumes the lowered data correctly without a GPU.

use crate::{
    calyx_lowering::EngineLoweredSimPackage,
    world_environment::{MILLI, WorldEnvironmentError, record::lowered_u64},
};

/// The WGSL renderer that consumes the lowered environment uniform.
pub const WORLD_ENVIRONMENT_WGSL: &str = include_str!("../render/world_environment.wgsl");

/// GPU-side environment uniform (matches `EnvUniform` in the WGSL, std140 vec4s).
/// All colours are 0..1; all distances/depths are metres; density is a coefficient.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct EnvironmentUniform {
    pub fog_color: [f32; 4],
    pub fog_range: [f32; 4],
    pub water_shallow: [f32; 4],
    pub water_deep: [f32; 4],
    pub water_depths: [f32; 4],
    pub ambient: [f32; 4],
}

impl Default for EnvironmentUniform {
    fn default() -> Self {
        Self {
            fog_color: [0.0, 0.0, 0.0, 0.0],
            fog_range: [0.0, 1.0, 0.0, 0.0],
            water_shallow: [0.0, 0.0, 0.0, 0.0],
            water_deep: [0.0, 0.0, 0.0, 1.0],
            water_depths: [0.0, 1.0, 0.0, 0.0],
            ambient: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

fn channel(value: u64) -> f32 {
    (value as f32) / 255.0
}

fn milli_to_unit(value: u64) -> f32 {
    (value as f32) / (MILLI as f32)
}

/// Build the fog portion of the uniform from a lowered #815 weather package.
pub fn apply_weather_fog(
    uniform: &mut EnvironmentUniform,
    package: &EngineLoweredSimPackage,
) -> Result<(), WorldEnvironmentError> {
    let color = [
        channel(lowered_u64(package, "/fog_color/0")?),
        channel(lowered_u64(package, "/fog_color/1")?),
        channel(lowered_u64(package, "/fog_color/2")?),
    ];
    let density = milli_to_unit(lowered_u64(package, "/fog_density_milli")?);
    let start = milli_to_unit(lowered_u64(package, "/fog_start_distance_mm")?);
    let visibility = milli_to_unit(lowered_u64(package, "/fog_visibility_distance_mm")?);
    uniform.fog_color = [color[0], color[1], color[2], density];
    uniform.fog_range = [start, visibility, 0.0, 0.0];
    Ok(())
}

/// Build the water portion of the uniform from a lowered #808 water package.
pub fn apply_water(
    uniform: &mut EnvironmentUniform,
    package: &EngineLoweredSimPackage,
) -> Result<(), WorldEnvironmentError> {
    uniform.water_shallow = [
        channel(lowered_u64(package, "/water_shallow_color/0")?),
        channel(lowered_u64(package, "/water_shallow_color/1")?),
        channel(lowered_u64(package, "/water_shallow_color/2")?),
        channel(lowered_u64(package, "/water_shallow_color/3")?),
    ];
    uniform.water_deep = [
        channel(lowered_u64(package, "/water_deep_color/0")?),
        channel(lowered_u64(package, "/water_deep_color/1")?),
        channel(lowered_u64(package, "/water_deep_color/2")?),
        channel(lowered_u64(package, "/water_deep_color/3")?),
    ];
    let shallow = milli_to_unit(lowered_u64(package, "/water_shallow_depth_mm")?);
    let deep = milli_to_unit(lowered_u64(package, "/water_deep_depth_mm")?);
    let shore = milli_to_unit(lowered_u64(package, "/water_shore_blend_mm")?);
    uniform.water_depths = [shallow, deep, shore, 0.0];
    Ok(())
}

/// Convenience: build a uniform from a lowered package by surface kind.
pub fn environment_uniform_from_package(
    package: &EngineLoweredSimPackage,
) -> Result<EnvironmentUniform, WorldEnvironmentError> {
    let mut uniform = EnvironmentUniform::default();
    // Detect the surface by a signature field rather than guessing.
    if package
        .fields
        .iter()
        .any(|f| f.path == "/fog_visibility_distance_mm")
    {
        apply_weather_fog(&mut uniform, package)?;
    }
    if package
        .fields
        .iter()
        .any(|f| f.path == "/water_deep_depth_mm")
    {
        apply_water(&mut uniform, package)?;
    }
    Ok(uniform)
}

/// CPU mirror of `env_fog_factor` in the WGSL (op-for-op). Returns 0 (clear) .. 1.
pub fn cpu_fog_factor(uniform: &EnvironmentUniform, dist: f32) -> f32 {
    let d = (dist - uniform.fog_range[0]).max(0.0);
    let f = 1.0 - (-uniform.fog_color[3] * d).exp();
    let capped = if dist >= uniform.fog_range[1] { 1.0 } else { f };
    capped.clamp(0.0, 1.0)
}

/// CPU mirror of `env_water_tint` in the WGSL (op-for-op). Returns rgba (0..1).
pub fn cpu_water_tint(uniform: &EnvironmentUniform, depth: f32) -> [f32; 4] {
    let span = (uniform.water_depths[1] - uniform.water_depths[0]).max(1.0e-3);
    let t = ((depth - uniform.water_depths[0]) / span).clamp(0.0, 1.0);
    std::array::from_fn(|i| uniform.water_shallow[i] * (1.0 - t) + uniform.water_deep[i] * t)
}
