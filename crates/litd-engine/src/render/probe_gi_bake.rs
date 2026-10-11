//! Offline probe bake: CPU ray-trace the analytic [`BakeScene`] per probe and store an
//! octahedral irradiance field + visibility (distance moment) field per probe.
//!
//! The estimator follows the DDGI irradiance-field formulation (Majercik et al. 2019,
//! "Irradiance Integral" in RTXGI-DDGI Math.md): each octahedral irradiance texel holds the
//! cosine-weighted mean incident radiance around that texel direction; the visibility texel
//! holds the cosine-weighted mean and mean-squared distance to the nearest surface, the two
//! moments a Chebyshev / variance-shadow test needs to reject light leaks at sample time.
//!
//! The bake is deterministic: a fixed Fibonacci-sphere ray set, sequential f32 accumulation,
//! no threading-induced reduction reordering. Baking the same spec + scene + config twice
//! yields bit-identical probes (proven via the lowered-artifact fingerprint).

use serde::{Deserialize, Serialize};

use super::{
    Probe, ProbeGiError, ProbeGridSpec, ProbeVolume, invalid_config, oct_texel_center_dir,
    probe_gi_scene::BakeScene,
};
use crate::math::Vec3;

const MIN_SAMPLE_COUNT: u32 = 32;

/// Tuning for the offline bake.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BakeConfig {
    /// Rays cast per probe over the sphere (Fibonacci-distributed). DDGI uses 64-256+.
    pub sample_count: u32,
    /// Recorded visibility distances are clamped to `distance_scale * cell_diagonal`, so a
    /// probe's stored occlusion reflects geometry within its interpolation cage (DDGI clamps
    /// distance to a small multiple of probe spacing). Must be positive.
    pub distance_scale: f32,
}

impl Default for BakeConfig {
    fn default() -> Self {
        Self {
            sample_count: 256,
            distance_scale: 1.5,
        }
    }
}

impl BakeConfig {
    pub fn validate(&self) -> Result<(), ProbeGiError> {
        if self.sample_count < MIN_SAMPLE_COUNT {
            return invalid_config(
                "sample_count",
                format!("bake needs at least {MIN_SAMPLE_COUNT} rays per probe"),
            );
        }
        if !self.distance_scale.is_finite() || self.distance_scale <= 0.0 {
            return invalid_config("distance_scale", "must be finite and positive");
        }
        Ok(())
    }
}

/// Bake a probe volume for one lighting state by ray-tracing `scene` at every probe.
pub fn bake_probe_volume(
    spec: ProbeGridSpec,
    scene: &BakeScene,
    config: BakeConfig,
) -> Result<ProbeVolume, ProbeGiError> {
    spec.validate()?;
    scene.validate()?;
    config.validate()?;

    let directions = fibonacci_sphere(config.sample_count);
    let irr_dirs = octahedral_texel_dirs(spec.irradiance_dim);
    let vis_dirs = octahedral_texel_dirs(spec.visibility_dim);
    let distance_clamp = spec.cell_diagonal() * config.distance_scale;

    let mut probes = Vec::with_capacity(spec.probe_count()?);
    for z in 0..spec.counts[2] {
        for y in 0..spec.counts[1] {
            for x in 0..spec.counts[0] {
                let position = spec.probe_position(x, y, z);
                probes.push(bake_probe(
                    scene,
                    position,
                    &directions,
                    &irr_dirs,
                    &vis_dirs,
                    distance_clamp,
                ));
            }
        }
    }

    Ok(ProbeVolume {
        spec,
        state: scene.state.clone(),
        probes,
    })
}

fn bake_probe(
    scene: &BakeScene,
    position: Vec3,
    directions: &[Vec3],
    irr_dirs: &[Vec3],
    vis_dirs: &[Vec3],
    distance_clamp: f32,
) -> Probe {
    // Trace every ray once; reuse the (radiance, distance) samples for both fields.
    let mut radiances = Vec::with_capacity(directions.len());
    let mut distances = Vec::with_capacity(directions.len());
    for &dir in directions {
        let hit = scene.trace(position, dir);
        radiances.push(hit.radiance);
        distances.push(hit.distance.min(distance_clamp));
    }

    let mut irradiance = Vec::with_capacity(irr_dirs.len());
    for &texel_dir in irr_dirs {
        let mut num = Vec3::ZERO;
        let mut den = 0.0_f32;
        for (sample_dir, radiance) in directions.iter().zip(&radiances) {
            let weight = sample_dir.dot(texel_dir).max(0.0);
            num += *radiance * weight;
            den += weight;
        }
        irradiance.push(num / den.max(f32::MIN_POSITIVE));
    }

    let mut visibility = Vec::with_capacity(vis_dirs.len());
    for &texel_dir in vis_dirs {
        let mut mean = 0.0_f32;
        let mut mean_sq = 0.0_f32;
        let mut den = 0.0_f32;
        for (sample_dir, distance) in directions.iter().zip(&distances) {
            let weight = sample_dir.dot(texel_dir).max(0.0);
            mean += *distance * weight;
            mean_sq += *distance * *distance * weight;
            den += weight;
        }
        let den = den.max(f32::MIN_POSITIVE);
        visibility.push([mean / den, mean_sq / den]);
    }

    Probe {
        irradiance,
        visibility,
    }
}

/// Precompute the direction at the center of every octahedral texel, row-major `(ty, tx)`
/// to match the sample-time linear index `ty * dim + tx`.
pub(crate) fn octahedral_texel_dirs(dim: u32) -> Vec<Vec3> {
    let mut dirs = Vec::with_capacity((dim * dim) as usize);
    for ty in 0..dim {
        for tx in 0..dim {
            dirs.push(oct_texel_center_dir(tx, ty, dim));
        }
    }
    dirs
}

/// Deterministic near-uniform sphere sampling via the Fibonacci lattice (golden angle).
fn fibonacci_sphere(count: u32) -> Vec<Vec3> {
    let n = count as f32;
    let golden = std::f32::consts::PI * (1.0 + 5.0_f32.sqrt());
    let mut dirs = Vec::with_capacity(count as usize);
    for i in 0..count {
        let k = i as f32 + 0.5;
        let y = 1.0 - 2.0 * k / n;
        let radius = (1.0 - y * y).max(0.0).sqrt();
        let theta = golden * k;
        dirs.push(Vec3::new(theta.cos() * radius, y, theta.sin() * radius).normalize());
    }
    dirs
}
