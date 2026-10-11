//! Runtime probe-volume sampling: the leak-resistant DDGI 8-probe interpolation shared by
//! the renderer (`sample_irradiance`) and the stealth light-level query (`light_level_at`).
//!
//! For a query point we find the 8-probe cage that brackets it and weight each probe by:
//!   * trilinear distance weight (position within the cell),
//!   * an optional backface/wrap weight (renderer only, uses the surface normal), and
//!   * a Chebyshev / variance-shadow visibility weight from the probe's stored distance
//!     moments -- this is what rejects light leaks through walls (Majercik et al. 2019 sec 5).
//!
//! The stealth query reuses the exact same cage + visibility weighting but collapses each
//! probe's full-sphere-average irradiance to a Rec.709 luminance, giving a direction-agnostic
//! "how lit is this world position" scalar in `[0, 1]` for hide/detect legality (#1597/#1601).

use serde::{Deserialize, Serialize};

use super::{ProbeGiError, ProbeVolume, STEALTH_FULL_LIGHT_REFERENCE, luminance, oct_bilinear};
use crate::math::Vec3;

/// Tuning for runtime sampling. Biases are expressed as fractions of the cell diagonal so a
/// volume can be rescaled without re-tuning (DDGI self-shadow bias, JCGT 10(2)).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProbeSampleConfig {
    /// Shrinks the query-to-probe distance before the Chebyshev test, pulling the shading
    /// point off shadowed/unshadowed discontinuities. Fraction of the cell diagonal.
    pub self_shadow_bias: f32,
    /// Offsets the shading point along its surface normal before sampling. Fraction of the
    /// cell diagonal. Only used when a normal is supplied.
    pub normal_bias: f32,
    /// Full-sphere irradiance luminance that maps to stealth light level 1.0.
    pub full_light_reference: f32,
}

impl Default for ProbeSampleConfig {
    fn default() -> Self {
        Self {
            self_shadow_bias: 0.2,
            normal_bias: 0.1,
            full_light_reference: STEALTH_FULL_LIGHT_REFERENCE,
        }
    }
}

/// Result of the stealth light-level query at a world position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightLevelSample {
    /// Full-sphere average incident irradiance luminance (raw, before normalization).
    pub luminance: f32,
    /// Normalized "how lit" scalar in `[0, 1]` for stealth AI.
    pub light_level: f32,
}

/// One probe in the interpolation cage with its final blended weight.
struct CageWeight {
    index: usize,
    weight: f32,
}

/// The 8-probe cage bracketing a query position plus each probe's trilinear/visibility weight.
struct Cage {
    probes: [CageWeight; 8],
    weight_sum: f32,
}

/// Renderer entry point: diffuse irradiance-field value reaching `position` for a surface
/// facing `normal`. Returns the cage-interpolated, visibility-weighted incident radiance in
/// the normal direction (the DDGI diffuse GI term; modulate by albedo at the shading site).
pub fn sample_irradiance(
    volume: &ProbeVolume,
    position: Vec3,
    normal: Vec3,
    config: ProbeSampleConfig,
) -> Result<Vec3, ProbeGiError> {
    let normal = if normal.length_squared() > f32::EPSILON {
        normal.normalize()
    } else {
        Vec3::Y
    };
    let cage = build_cage(volume, position, Some(normal), config)?;
    let mut acc = Vec3::ZERO;
    for probe_weight in &cage.probes {
        let probe = &volume.probes[probe_weight.index];
        let irr = oct_bilinear(normal, volume.spec.irradiance_dim, |tx, ty| {
            probe.irradiance[(ty * volume.spec.irradiance_dim + tx) as usize]
        });
        acc += irr * probe_weight.weight;
    }
    Ok(acc / cage.weight_sum)
}

/// Stealth entry point: direction-agnostic light level at `position`, normalized to `[0, 1]`.
pub fn light_level_at(
    volume: &ProbeVolume,
    position: Vec3,
    config: ProbeSampleConfig,
) -> Result<LightLevelSample, ProbeGiError> {
    let cage = build_cage(volume, position, None, config)?;
    let mut acc = 0.0_f32;
    for probe_weight in &cage.probes {
        let probe = &volume.probes[probe_weight.index];
        acc += probe_average_luminance(probe) * probe_weight.weight;
    }
    let lum = acc / cage.weight_sum;
    let reference = config.full_light_reference.max(f32::MIN_POSITIVE);
    Ok(LightLevelSample {
        luminance: lum,
        light_level: (lum / reference).clamp(0.0, 1.0),
    })
}

/// Full-sphere average incident radiance luminance for a probe (mean over its octahedral
/// irradiance texels) -- the omnidirectional "ambient light present here" the stealth query
/// wants, independent of any surface orientation.
fn probe_average_luminance(probe: &super::Probe) -> f32 {
    if probe.irradiance.is_empty() {
        return 0.0;
    }
    let sum: f32 = probe.irradiance.iter().map(|c| luminance(*c)).sum();
    sum / probe.irradiance.len() as f32
}

fn build_cage(
    volume: &ProbeVolume,
    position: Vec3,
    normal: Option<Vec3>,
    config: ProbeSampleConfig,
) -> Result<Cage, ProbeGiError> {
    let spec = &volume.spec;
    let cell = spec.cell_diagonal();
    let biased = match normal {
        Some(n) => position + n * (config.normal_bias * cell),
        None => position,
    };

    let grid = (position - spec.origin) / spec.spacing;
    let counts = spec.counts;
    let tolerance = 0.5_f32;
    for axis in 0..3 {
        if grid[axis] < -tolerance || grid[axis] > (counts[axis] - 1) as f32 + tolerance {
            return Err(ProbeGiError::SampleOutOfBounds {
                position: position.to_array(),
            });
        }
    }

    let base = [
        clamp_base(grid.x, counts[0]),
        clamp_base(grid.y, counts[1]),
        clamp_base(grid.z, counts[2]),
    ];
    let frac = Vec3::new(
        (grid.x - base[0] as f32).clamp(0.0, 1.0),
        (grid.y - base[1] as f32).clamp(0.0, 1.0),
        (grid.z - base[2] as f32).clamp(0.0, 1.0),
    );

    let mut probes: [CageWeight; 8] = std::array::from_fn(|_| CageWeight {
        index: 0,
        weight: 0.0,
    });
    let mut weight_sum = 0.0_f32;
    let self_shadow_bias = config.self_shadow_bias * cell;

    let mut slot = 0;
    for dz in 0..2u32 {
        for dy in 0..2u32 {
            for dx in 0..2u32 {
                let cx = base[0] + dx;
                let cy = base[1] + dy;
                let cz = base[2] + dz;
                let index = spec.linear_index(cx, cy, cz)?;
                let probe_pos = spec.probe_position(cx, cy, cz);

                let trilinear =
                    axis_weight(frac.x, dx) * axis_weight(frac.y, dy) * axis_weight(frac.z, dz);

                let to_probe = probe_pos - biased;
                let dist = to_probe.length();
                let dir = if dist > f32::MIN_POSITIVE {
                    to_probe / dist
                } else {
                    Vec3::Y
                };

                let dir_weight = match normal {
                    // Backface/wrap weight: probes behind the surface tangent plane fade out.
                    Some(n) => {
                        let wrap = (dir.dot(n) + 1.0) * 0.5;
                        wrap * wrap + 0.2
                    }
                    None => 1.0,
                };

                let cheb =
                    chebyshev_visibility(&volume.probes[index], -dir, dist, self_shadow_bias, spec);

                let weight = (trilinear * dir_weight * cheb).max(0.0);
                probes[slot] = CageWeight { index, weight };
                weight_sum += weight;
                slot += 1;
            }
        }
    }

    // If every probe was rejected by visibility (e.g. a fully occluded degenerate cage),
    // fall back to trilinear-only weights so the query returns the (low) interpolated field
    // rather than dividing by zero -- fail soft, never NaN.
    if weight_sum <= f32::MIN_POSITIVE {
        weight_sum = 0.0;
        let mut slot = 0;
        for dz in 0..2u32 {
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let w =
                        axis_weight(frac.x, dx) * axis_weight(frac.y, dy) * axis_weight(frac.z, dz);
                    probes[slot].weight = w;
                    weight_sum += w;
                    slot += 1;
                }
            }
        }
    }

    Ok(Cage { probes, weight_sum })
}

/// Variance-shadow (Chebyshev) visibility weight in `[0, 1]`: the probability the query point
/// is visible to the probe given the probe's stored distance moments in `to_point_dir`.
fn chebyshev_visibility(
    probe: &super::Probe,
    to_point_dir: Vec3,
    distance: f32,
    self_shadow_bias: f32,
    spec: &super::ProbeGridSpec,
) -> f32 {
    let moments = oct_bilinear(to_point_dir, spec.visibility_dim, |tx, ty| {
        probe.visibility[(ty * spec.visibility_dim + tx) as usize]
    });
    let mean = moments[0];
    let mean_sq = moments[1];
    let biased = distance - self_shadow_bias;
    if biased <= mean {
        return 1.0;
    }
    let variance = (mean_sq - mean * mean).max(0.0);
    let delta = biased - mean;
    let cheb = variance / (variance + delta * delta);
    // Sharpen (DDGI cubes the Chebyshev term) so partial occlusion falls off fast.
    (cheb * cheb * cheb).clamp(0.0, 1.0)
}

fn axis_weight(frac: f32, corner: u32) -> f32 {
    if corner == 0 { 1.0 - frac } else { frac }
}

fn clamp_base(coord: f32, count: u32) -> u32 {
    let max_base = count - 2;
    let floored = coord.floor();
    if floored < 0.0 {
        0
    } else if floored as u32 > max_base {
        max_base
    } else {
        floored as u32
    }
}

// ---------------------------------------------------------------------------
// Half-resolution light-level field (the CPU analogue of Overwatch 2's half-res
// GI evaluation + upsample on low tiers, REAC 2025). Evaluate the stealth query on a
// coarse lattice once, then trilinearly upsample -- avoids a per-pixel/per-agent cage
// walk on the min-spec $500 box (#928).
// ---------------------------------------------------------------------------

/// A precomputed light-level lattice over a volume's bounds, sampled at `dims` nodes per axis
/// and trilinearly upsampled on query. Pass halved `dims` for the half-res evaluation tier.
#[derive(Debug, Clone, PartialEq)]
pub struct HalfResLightField {
    origin: Vec3,
    spacing: Vec3,
    dims: [u32; 3],
    values: Vec<f32>,
}

impl HalfResLightField {
    pub fn dims(&self) -> [u32; 3] {
        self.dims
    }

    pub fn node_count(&self) -> usize {
        self.values.len()
    }

    /// Trilinearly interpolate the stored light level at a world position (upsample step).
    pub fn sample(&self, position: Vec3) -> f32 {
        let grid = (position - self.origin) / self.spacing;
        let base = [
            clamp_base(grid.x, self.dims[0]),
            clamp_base(grid.y, self.dims[1]),
            clamp_base(grid.z, self.dims[2]),
        ];
        let frac = Vec3::new(
            (grid.x - base[0] as f32).clamp(0.0, 1.0),
            (grid.y - base[1] as f32).clamp(0.0, 1.0),
            (grid.z - base[2] as f32).clamp(0.0, 1.0),
        );
        let mut acc = 0.0_f32;
        for dz in 0..2u32 {
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let w =
                        axis_weight(frac.x, dx) * axis_weight(frac.y, dy) * axis_weight(frac.z, dz);
                    let idx = self.node_index(base[0] + dx, base[1] + dy, base[2] + dz);
                    acc += self.values[idx] * w;
                }
            }
        }
        acc
    }

    fn node_index(&self, x: u32, y: u32, z: u32) -> usize {
        let x = x.min(self.dims[0] - 1);
        let y = y.min(self.dims[1] - 1);
        let z = z.min(self.dims[2] - 1);
        ((z * self.dims[1] + y) * self.dims[0] + x) as usize
    }
}

/// Build a light-level evaluation field over the volume bounds at `dims` nodes per axis.
pub fn light_level_field(
    volume: &ProbeVolume,
    config: ProbeSampleConfig,
    dims: [u32; 3],
) -> Result<HalfResLightField, ProbeGiError> {
    for (axis, d) in dims.iter().enumerate() {
        if *d < 2 {
            return super::invalid_config(
                format!("dims[{axis}]"),
                "light-level field needs at least 2 nodes per axis",
            );
        }
    }
    let min = volume.spec.min_corner();
    let max = volume.spec.max_corner();
    let extent = max - min;
    let spacing = Vec3::new(
        extent.x / (dims[0] - 1) as f32,
        extent.y / (dims[1] - 1) as f32,
        extent.z / (dims[2] - 1) as f32,
    );
    let mut values = Vec::with_capacity((dims[0] * dims[1] * dims[2]) as usize);
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let pos = min + spacing * Vec3::new(x as f32, y as f32, z as f32);
                values.push(light_level_at(volume, pos, config)?.light_level);
            }
        }
    }
    Ok(HalfResLightField {
        origin: min,
        spacing,
        dims,
        values,
    })
}
