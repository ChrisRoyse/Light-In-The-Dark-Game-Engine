//! Baked probe-grid global illumination (DDGI-style irradiance + visibility volumes).
//!
//! This is the offline-baked, leak-resistant diffuse GI answer proven on low-end
//! hardware (Overwatch 2, REAC 2025). A regular 3D grid of probes stores, per probe:
//!   * an octahedral irradiance field (cosine-weighted mean incident radiance), and
//!   * an octahedral visibility field (mean distance + mean squared distance) used for
//!     the Chebyshev / variance-shadow occlusion test that kills light leaks.
//!
//! The grid is baked once per lighting scenario (day/night states, #814/#848) on the
//! authoring box, frozen into a fingerprinted lowered artifact (never authoritative;
//! reproducible from Calyx-authored scene input, doctrine #1348), and sampled at runtime
//! by two consumers:
//!   * the renderer (`probe_gi_sample::sample_irradiance`), and
//!   * the stealth light-level query (`probe_gi_sample::light_level_at`) that feeds
//!     hide/detect legality (#1597/#1601) -- "how lit is this world position" for free.
//!
//! Math follows the DDGI irradiance-field-with-visibility formulation:
//!   * Majercik, Guertin, Nowrouzezahrai, McGuire, "Dynamic Diffuse Global Illumination
//!     with Ray-Traced Irradiance Fields", JCGT 8(2), 2019 (jcgt.org/published/0008/02/01).
//!   * Majercik et al., "Scaling Probe-Based Real-Time Dynamic GI for Production",
//!     JCGT 10(2), 2021 (self-shadow bias, production tuning).
//!   * Pearce, "Evolving Global Illumination in Overwatch 2", REAC 2025 (the BAKED
//!     variant: prebake multiple lighting scenarios, half-res eval, RG11B10 targets).
//!
//! NO runtime ray tracing (#1746 skip list): the bake CPU-ray-traces the analytic scene
//! offline; the runtime is a pure grid lookup.

use serde::{Deserialize, Serialize};

use crate::math::{Vec2, Vec3};

#[path = "probe_gi_bake.rs"]
mod probe_gi_bake;
#[path = "probe_gi_lower.rs"]
mod probe_gi_lower;
#[path = "probe_gi_sample.rs"]
mod probe_gi_sample;
#[path = "probe_gi_scene.rs"]
mod probe_gi_scene;

pub use probe_gi_bake::{BakeConfig, bake_probe_volume};
pub use probe_gi_lower::{
    LOWERED_PROBE_VOLUME_SCHEMA, LoweredProbeVolume, LoweredProbeVolumeReadback,
    lower_probe_volume, verify_lowered_probe_volume,
};
pub use probe_gi_sample::{
    HalfResLightField, LightLevelSample, ProbeSampleConfig, light_level_at, light_level_field,
    sample_irradiance,
};
pub use probe_gi_scene::{BakeScene, EmissivePanel, PanelAxis, SkyLighting};

/// Reference full-sphere irradiance luminance that maps to a stealth light level of 1.0.
/// The stealth query normalizes sampled luminance against this so callers get a stable
/// `[0, 1]` "how lit" scalar independent of the absolute radiometric scale of a scene.
pub const STEALTH_FULL_LIGHT_REFERENCE: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProbeGiError {
    #[error("probe GI config field {field} is invalid: {reason}")]
    InvalidConfig { field: String, reason: String },
    #[error("probe GI scene field {field} is invalid: {reason}")]
    InvalidScene { field: String, reason: String },
    #[error("probe GI lowered artifact fingerprint mismatch: expected {expected}, actual {actual}")]
    FingerprintMismatch { expected: String, actual: String },
    #[error("probe GI lowered artifact field {field} is invalid: {reason}")]
    InvalidLowered { field: String, reason: String },
    #[error("probe GI sample position {position:?} is outside the baked volume bounds")]
    SampleOutOfBounds { position: [f32; 3] },
}

impl ProbeGiError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig { .. } => "LITD-RENDER-PROBE-GI-INVALID-CONFIG",
            Self::InvalidScene { .. } => "LITD-RENDER-PROBE-GI-INVALID-SCENE",
            Self::FingerprintMismatch { .. } => "LITD-RENDER-PROBE-GI-FINGERPRINT-MISMATCH",
            Self::InvalidLowered { .. } => "LITD-RENDER-PROBE-GI-INVALID-LOWERED",
            Self::SampleOutOfBounds { .. } => "LITD-RENDER-PROBE-GI-SAMPLE-OUT-OF-BOUNDS",
        }
    }
}

pub(crate) fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, ProbeGiError> {
    Err(ProbeGiError::InvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

pub(crate) fn invalid_scene<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, ProbeGiError> {
    Err(ProbeGiError::InvalidScene {
        field: field.into(),
        reason: reason.into(),
    })
}

/// Regular 3D probe grid layout. Probes sit on a uniform lattice from `origin`, one probe
/// per lattice node, `counts` nodes along each axis, `spacing` world units apart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProbeGridSpec {
    pub origin: Vec3,
    pub spacing: Vec3,
    pub counts: [u32; 3],
    /// Octahedral side length for the irradiance field (DDGI ships 8; a small value keeps
    /// the baked artifact tiny for the vertical slice while preserving the algorithm).
    pub irradiance_dim: u32,
    /// Octahedral side length for the visibility (distance moment) field.
    pub visibility_dim: u32,
}

impl ProbeGridSpec {
    pub fn validate(&self) -> Result<(), ProbeGiError> {
        if !self.origin.is_finite() || !self.spacing.is_finite() {
            return invalid_config("origin/spacing", "grid origin and spacing must be finite");
        }
        if self.spacing.x <= 0.0 || self.spacing.y <= 0.0 || self.spacing.z <= 0.0 {
            return invalid_config("spacing", "grid spacing must be positive on every axis");
        }
        for (axis, count) in ["x", "y", "z"].iter().zip(self.counts) {
            if count < 2 {
                return invalid_config(
                    format!("counts.{axis}"),
                    "each axis needs at least 2 probes to form an interpolation cage",
                );
            }
        }
        if self.irradiance_dim < 2 || self.visibility_dim < 2 {
            return invalid_config(
                "octahedral_dim",
                "irradiance_dim and visibility_dim must be at least 2",
            );
        }
        self.probe_count()?;
        Ok(())
    }

    pub fn probe_count(&self) -> Result<usize, ProbeGiError> {
        let [x, y, z] = self.counts;
        let total = (x as u64) * (y as u64) * (z as u64);
        usize::try_from(total).map_err(|_| ProbeGiError::InvalidConfig {
            field: "counts".to_string(),
            reason: "probe count does not fit usize".to_string(),
        })
    }

    pub fn irradiance_texels(&self) -> usize {
        (self.irradiance_dim * self.irradiance_dim) as usize
    }

    pub fn visibility_texels(&self) -> usize {
        (self.visibility_dim * self.visibility_dim) as usize
    }

    /// Linear probe index for lattice coordinates, x-fastest then y then z.
    pub fn linear_index(&self, x: u32, y: u32, z: u32) -> Result<usize, ProbeGiError> {
        if x >= self.counts[0] || y >= self.counts[1] || z >= self.counts[2] {
            return invalid_config("lattice_coords", "probe coordinates outside the grid");
        }
        let index = (z * self.counts[1] + y) * self.counts[0] + x;
        Ok(index as usize)
    }

    pub fn probe_position(&self, x: u32, y: u32, z: u32) -> Vec3 {
        self.origin + self.spacing * Vec3::new(x as f32, y as f32, z as f32)
    }

    /// Inclusive world-space min corner of the probe lattice.
    pub fn min_corner(&self) -> Vec3 {
        self.origin
    }

    /// Inclusive world-space max corner of the probe lattice (last probe node).
    pub fn max_corner(&self) -> Vec3 {
        self.origin
            + self.spacing
                * Vec3::new(
                    (self.counts[0] - 1) as f32,
                    (self.counts[1] - 1) as f32,
                    (self.counts[2] - 1) as f32,
                )
    }

    /// Diagonal length of one grid cell; the visibility distance clamp and self-shadow
    /// bias are expressed as fractions of it (DDGI convention).
    pub fn cell_diagonal(&self) -> f32 {
        self.spacing.length()
    }
}

/// One baked probe: octahedral irradiance texels and octahedral visibility moments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Probe {
    /// Cosine-weighted mean incident radiance per octahedral texel (RGB).
    pub irradiance: Vec<Vec3>,
    /// `(mean_distance, mean_squared_distance)` per octahedral texel.
    pub visibility: Vec<[f32; 2]>,
}

/// A fully baked probe volume for a single lighting state (e.g. "day" or "night").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeVolume {
    pub spec: ProbeGridSpec,
    pub state: String,
    pub probes: Vec<Probe>,
}

impl ProbeVolume {
    pub fn probe(&self, x: u32, y: u32, z: u32) -> Result<&Probe, ProbeGiError> {
        let index = self.spec.linear_index(x, y, z)?;
        self.probes
            .get(index)
            .ok_or_else(|| ProbeGiError::InvalidConfig {
                field: "probes".to_string(),
                reason: "probe index out of range for baked volume".to_string(),
            })
    }
}

// ---------------------------------------------------------------------------
// Octahedral direction <-> texel mapping (Cigolle et al. 2014, as used by DDGI).
// ---------------------------------------------------------------------------

/// Map a unit direction to octahedral texture coordinates in `[0, 1]^2`.
pub(crate) fn oct_encode(dir: Vec3) -> Vec2 {
    let n = dir / (dir.x.abs() + dir.y.abs() + dir.z.abs()).max(f32::MIN_POSITIVE);
    let mut xy = Vec2::new(n.x, n.y);
    if n.z < 0.0 {
        let signs = Vec2::new(sign_nonzero(xy.x), sign_nonzero(xy.y));
        xy = (Vec2::ONE - Vec2::new(xy.y.abs(), xy.x.abs())) * signs;
    }
    xy * 0.5 + Vec2::splat(0.5)
}

/// Inverse of [`oct_encode`]: octahedral coords in `[0, 1]^2` to a unit direction.
pub(crate) fn oct_decode(uv01: Vec2) -> Vec3 {
    let f = uv01 * 2.0 - Vec2::splat(1.0);
    let mut n = Vec3::new(f.x, f.y, 1.0 - f.x.abs() - f.y.abs());
    let t = (-n.z).clamp(0.0, 1.0);
    n.x += if n.x >= 0.0 { -t } else { t };
    n.y += if n.y >= 0.0 { -t } else { t };
    n.normalize()
}

/// Direction at the center of octahedral texel `(tx, ty)` for a `dim x dim` grid.
pub(crate) fn oct_texel_center_dir(tx: u32, ty: u32, dim: u32) -> Vec3 {
    let uv = Vec2::new(
        (tx as f32 + 0.5) / dim as f32,
        (ty as f32 + 0.5) / dim as f32,
    );
    oct_decode(uv)
}

fn sign_nonzero(v: f32) -> f32 {
    if v >= 0.0 { 1.0 } else { -1.0 }
}

/// Bilinear octahedral fetch with clamped texel coordinates. `fetch(tx, ty)` returns the
/// stored texel value; the four texels around `dir` are blended by their fractional
/// coverage. Clamp (not toroidal gutter wrapping) is sufficient for the baked CPU slice.
pub(crate) fn oct_bilinear<T, F>(dir: Vec3, dim: u32, mut fetch: F) -> T
where
    T: OctBlend,
    F: FnMut(u32, u32) -> T,
{
    let uv = oct_encode(dir);
    let fx = (uv.x * dim as f32 - 0.5).clamp(0.0, (dim - 1) as f32);
    let fy = (uv.y * dim as f32 - 0.5).clamp(0.0, (dim - 1) as f32);
    let x0 = fx.floor() as u32;
    let y0 = fy.floor() as u32;
    let x1 = (x0 + 1).min(dim - 1);
    let y1 = (y0 + 1).min(dim - 1);
    let ax = fx - x0 as f32;
    let ay = fy - y0 as f32;
    let c00 = fetch(x0, y0);
    let c10 = fetch(x1, y0);
    let c01 = fetch(x0, y1);
    let c11 = fetch(x1, y1);
    let top = c00.blend(&c10, ax);
    let bottom = c01.blend(&c11, ax);
    top.blend(&bottom, ay)
}

/// Linear blend helper so [`oct_bilinear`] works for both RGB irradiance and `[mean, mean^2]`.
pub(crate) trait OctBlend {
    fn blend(&self, other: &Self, t: f32) -> Self;
}

impl OctBlend for Vec3 {
    fn blend(&self, other: &Self, t: f32) -> Self {
        self.lerp(*other, t)
    }
}

impl OctBlend for [f32; 2] {
    fn blend(&self, other: &Self, t: f32) -> Self {
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
        ]
    }
}

/// Relative luminance (Rec. 709) used by the stealth light-level query to collapse RGB
/// irradiance into a single "how lit" scalar.
pub(crate) fn luminance(rgb: Vec3) -> f32 {
    0.2126 * rgb.x + 0.7152 * rgb.y + 0.0722 * rgb.z
}

#[cfg(test)]
#[path = "probe_gi_tests.rs"]
mod probe_gi_tests;
