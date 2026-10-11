//! Lowering valve for a baked probe volume: freeze the f32 bake into a fingerprinted,
//! integer-quantized artifact the runtime consumes without touching Calyx or floats in the
//! hot path (doctrine #1348, determinism valve #725).
//!
//! A lowered artifact is a permitted non-Calyx byte form: derived-from-Calyx (the scene is
//! authored/lowered from the Aster vault), reproducible, fingerprinted, and NEVER
//! authoritative. Quantization mirrors the GPU storage intent of the technique (DDGI/
//! Overwatch 2 store irradiance and distance moments in low-precision RG11B10/RG16F targets):
//! irradiance and distance moments are quantized to u16 with per-volume scales recorded in the
//! header, and the sha256 fingerprint is taken over the integer canonical form only, so it is
//! stable across platforms (no raw float bytes, matching the Calyx lowering valve's
//! integers-only rule).

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{Probe, ProbeGiError, ProbeGridSpec, ProbeVolume};
use crate::math::Vec3;

pub const LOWERED_PROBE_VOLUME_SCHEMA: &str = "litd.render.probe_gi.lowered_volume.v1";

const QUANT_MAX: f32 = 65535.0;
const MILLI: f32 = 1000.0;
const MICRO: f32 = 1_000_000.0;

/// Frozen, fingerprinted, integer-quantized probe volume for one lighting state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredProbeVolume {
    pub schema: String,
    pub state: String,
    pub counts: [u32; 3],
    pub irradiance_dim: u32,
    pub visibility_dim: u32,
    pub origin_milli: [i32; 3],
    pub spacing_milli: [i32; 3],
    /// Irradiance quantization scale (micro-units): decoded = q / 65535 * (scale_micro / 1e6).
    pub irradiance_scale_micro: u32,
    /// Distance quantization max (micro-units): decoded mean = q / 65535 * (dist_max_micro / 1e6).
    pub distance_max_micro: u32,
    /// Per probe, row-major octahedral texels, RGB triples.
    pub irradiance_q: Vec<u16>,
    /// Per probe, row-major octahedral texels, `(mean, mean_squared)` pairs.
    pub visibility_q: Vec<u16>,
    /// sha256 over the integer canonical form of every field above.
    pub fingerprint: String,
}

/// Independent verification summary produced by re-reading a lowered artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProbeVolumeReadback {
    pub schema: String,
    pub state: String,
    pub probe_count: usize,
    pub irradiance_word_count: usize,
    pub visibility_word_count: usize,
    pub fingerprint: String,
}

/// Lower a baked f32 probe volume into its frozen fingerprinted integer form.
pub fn lower_probe_volume(volume: &ProbeVolume) -> Result<LoweredProbeVolume, ProbeGiError> {
    volume.spec.validate()?;
    let expected_probes = volume.spec.probe_count()?;
    if volume.probes.len() != expected_probes {
        return Err(ProbeGiError::InvalidLowered {
            field: "probes".to_string(),
            reason: format!(
                "volume has {} probes, spec expects {expected_probes}",
                volume.probes.len()
            ),
        });
    }

    let irr_texels = volume.spec.irradiance_texels();
    let vis_texels = volume.spec.visibility_texels();
    let (irradiance_scale, distance_max) = compute_scales(volume);

    let mut irradiance_q = Vec::with_capacity(expected_probes * irr_texels * 3);
    let mut visibility_q = Vec::with_capacity(expected_probes * vis_texels * 2);
    for probe in &volume.probes {
        validate_probe(probe, irr_texels, vis_texels)?;
        for texel in &probe.irradiance {
            irradiance_q.push(quantize(texel.x, irradiance_scale));
            irradiance_q.push(quantize(texel.y, irradiance_scale));
            irradiance_q.push(quantize(texel.z, irradiance_scale));
        }
        for moment in &probe.visibility {
            visibility_q.push(quantize(moment[0], distance_max));
            visibility_q.push(quantize(moment[1], distance_max * distance_max));
        }
    }

    let mut lowered = LoweredProbeVolume {
        schema: LOWERED_PROBE_VOLUME_SCHEMA.to_string(),
        state: volume.state.clone(),
        counts: volume.spec.counts,
        irradiance_dim: volume.spec.irradiance_dim,
        visibility_dim: volume.spec.visibility_dim,
        origin_milli: milli_vec(volume.spec.origin),
        spacing_milli: milli_vec(volume.spec.spacing),
        irradiance_scale_micro: micro(irradiance_scale),
        distance_max_micro: micro(distance_max),
        irradiance_q,
        visibility_q,
        fingerprint: String::new(),
    };
    lowered.fingerprint = fingerprint(&lowered);
    Ok(lowered)
}

/// Recompute the fingerprint over the stored integers and confirm it matches -- an
/// independent readback (not a recompute of the same in-memory float buffer).
pub fn verify_lowered_probe_volume(
    lowered: &LoweredProbeVolume,
) -> Result<LoweredProbeVolumeReadback, ProbeGiError> {
    if lowered.schema != LOWERED_PROBE_VOLUME_SCHEMA {
        return Err(ProbeGiError::InvalidLowered {
            field: "schema".to_string(),
            reason: format!("expected {LOWERED_PROBE_VOLUME_SCHEMA}"),
        });
    }
    let probe_count = (lowered.counts[0] * lowered.counts[1] * lowered.counts[2]) as usize;
    let irr_texels = (lowered.irradiance_dim * lowered.irradiance_dim) as usize;
    let vis_texels = (lowered.visibility_dim * lowered.visibility_dim) as usize;
    if lowered.irradiance_q.len() != probe_count * irr_texels * 3 {
        return Err(ProbeGiError::InvalidLowered {
            field: "irradiance_q".to_string(),
            reason: "quantized irradiance length does not match grid dimensions".to_string(),
        });
    }
    if lowered.visibility_q.len() != probe_count * vis_texels * 2 {
        return Err(ProbeGiError::InvalidLowered {
            field: "visibility_q".to_string(),
            reason: "quantized visibility length does not match grid dimensions".to_string(),
        });
    }
    let expected = fingerprint(lowered);
    if expected != lowered.fingerprint {
        return Err(ProbeGiError::FingerprintMismatch {
            expected,
            actual: lowered.fingerprint.clone(),
        });
    }
    Ok(LoweredProbeVolumeReadback {
        schema: lowered.schema.clone(),
        state: lowered.state.clone(),
        probe_count,
        irradiance_word_count: lowered.irradiance_q.len(),
        visibility_word_count: lowered.visibility_q.len(),
        fingerprint: lowered.fingerprint.clone(),
    })
}

impl LoweredProbeVolume {
    /// Reconstruct a sampleable f32 [`ProbeVolume`] from the frozen integers -- the runtime
    /// consumption path. Values differ from the pre-lowering bake only by quantization.
    pub fn decode(&self) -> Result<ProbeVolume, ProbeGiError> {
        verify_lowered_probe_volume(self)?;
        let spec = ProbeGridSpec {
            origin: from_milli(self.origin_milli),
            spacing: from_milli(self.spacing_milli),
            counts: self.counts,
            irradiance_dim: self.irradiance_dim,
            visibility_dim: self.visibility_dim,
        };
        spec.validate()?;
        let irr_texels = spec.irradiance_texels();
        let vis_texels = spec.visibility_texels();
        let irr_scale = self.irradiance_scale_micro as f32 / MICRO;
        let dist_max = self.distance_max_micro as f32 / MICRO;
        let probe_count = spec.probe_count()?;

        let mut probes = Vec::with_capacity(probe_count);
        for p in 0..probe_count {
            let mut irradiance = Vec::with_capacity(irr_texels);
            let base = p * irr_texels * 3;
            for t in 0..irr_texels {
                let i = base + t * 3;
                irradiance.push(Vec3::new(
                    dequantize(self.irradiance_q[i], irr_scale),
                    dequantize(self.irradiance_q[i + 1], irr_scale),
                    dequantize(self.irradiance_q[i + 2], irr_scale),
                ));
            }
            let mut visibility = Vec::with_capacity(vis_texels);
            let vbase = p * vis_texels * 2;
            for t in 0..vis_texels {
                let i = vbase + t * 2;
                visibility.push([
                    dequantize(self.visibility_q[i], dist_max),
                    dequantize(self.visibility_q[i + 1], dist_max * dist_max),
                ]);
            }
            probes.push(Probe {
                irradiance,
                visibility,
            });
        }
        Ok(ProbeVolume {
            spec,
            state: self.state.clone(),
            probes,
        })
    }
}

fn validate_probe(probe: &Probe, irr_texels: usize, vis_texels: usize) -> Result<(), ProbeGiError> {
    if probe.irradiance.len() != irr_texels {
        return Err(ProbeGiError::InvalidLowered {
            field: "probe.irradiance".to_string(),
            reason: format!(
                "probe has {} irradiance texels, expected {irr_texels}",
                probe.irradiance.len()
            ),
        });
    }
    if probe.visibility.len() != vis_texels {
        return Err(ProbeGiError::InvalidLowered {
            field: "probe.visibility".to_string(),
            reason: format!(
                "probe has {} visibility texels, expected {vis_texels}",
                probe.visibility.len()
            ),
        });
    }
    Ok(())
}

fn compute_scales(volume: &ProbeVolume) -> (f32, f32) {
    let mut max_irr = 0.0_f32;
    let mut max_dist = 0.0_f32;
    for probe in &volume.probes {
        for texel in &probe.irradiance {
            max_irr = max_irr.max(texel.x).max(texel.y).max(texel.z);
        }
        for moment in &probe.visibility {
            max_dist = max_dist.max(moment[0]);
        }
    }
    (max_irr.max(1.0e-4), max_dist.max(1.0e-4))
}

fn quantize(value: f32, scale: f32) -> u16 {
    let normalized = (value / scale).clamp(0.0, 1.0);
    (normalized * QUANT_MAX).round() as u16
}

fn dequantize(q: u16, scale: f32) -> f32 {
    q as f32 / QUANT_MAX * scale
}

fn milli_vec(v: Vec3) -> [i32; 3] {
    [
        (v.x * MILLI).round() as i32,
        (v.y * MILLI).round() as i32,
        (v.z * MILLI).round() as i32,
    ]
}

fn from_milli(v: [i32; 3]) -> Vec3 {
    Vec3::new(
        v[0] as f32 / MILLI,
        v[1] as f32 / MILLI,
        v[2] as f32 / MILLI,
    )
}

fn micro(value: f32) -> u32 {
    (value * MICRO).round().clamp(0.0, u32::MAX as f32) as u32
}

fn fingerprint(lowered: &LoweredProbeVolume) -> String {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(lowered.schema.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(lowered.state.as_bytes());
    buf.push(b'\n');
    for v in lowered.counts {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf.extend_from_slice(&lowered.irradiance_dim.to_le_bytes());
    buf.extend_from_slice(&lowered.visibility_dim.to_le_bytes());
    for v in lowered.origin_milli {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    for v in lowered.spacing_milli {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf.extend_from_slice(&lowered.irradiance_scale_micro.to_le_bytes());
    buf.extend_from_slice(&lowered.distance_max_micro.to_le_bytes());
    for q in &lowered.irradiance_q {
        buf.extend_from_slice(&q.to_le_bytes());
    }
    for q in &lowered.visibility_q {
        buf.extend_from_slice(&q.to_le_bytes());
    }
    sha256_hex(&buf)
}
