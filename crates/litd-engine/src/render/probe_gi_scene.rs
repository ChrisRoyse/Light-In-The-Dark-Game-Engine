//! Analytic bake scene for probe-grid GI: axis-aligned emissive panels, box occluders,
//! and a sky/floor dome. The bake CPU-ray-traces this scene per probe (no runtime rays,
//! #1746). Geometry is authored/lowered from Calyx scene data; this module is the frozen
//! deterministic form the bake consumes.

use serde::{Deserialize, Serialize};

use crate::math::{Aabb, Vec3};

use super::{ProbeGiError, invalid_scene};

/// A far sentinel distance for rays that escape all geometry. Recorded distances are
/// clamped to a fraction of the grid spacing by the bake before storing visibility moments.
pub(crate) const ESCAPE_DISTANCE: f32 = 1.0e4;

const HIT_EPSILON: f32 = 1.0e-4;

/// Which world axis an emissive panel faces along (its normal axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelAxis {
    X,
    Y,
    Z,
}

impl PanelAxis {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    /// The two tangent-axis indices (the plane the panel spans).
    fn tangents(self) -> (usize, usize) {
        match self {
            Self::X => (1, 2),
            Self::Y => (0, 2),
            Self::Z => (0, 1),
        }
    }
}

/// An axis-aligned rectangular area emitter (e.g. the red panel). Two-sided: it emits from
/// both faces so probe placement relative to the panel does not need special-casing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EmissivePanel {
    pub axis: PanelAxis,
    /// Coordinate of the panel along its normal axis.
    pub plane_coord: f32,
    /// Inclusive `[min, max]` bounds along the two tangent axes, in tangent order.
    pub tangent_min: [f32; 2],
    pub tangent_max: [f32; 2],
    /// Emitted radiance (RGB), non-negative.
    pub radiance: Vec3,
}

impl EmissivePanel {
    pub fn new(
        axis: PanelAxis,
        plane_coord: f32,
        tangent_min: [f32; 2],
        tangent_max: [f32; 2],
        radiance: Vec3,
    ) -> Self {
        Self {
            axis,
            plane_coord,
            tangent_min,
            tangent_max,
            radiance,
        }
    }

    fn validate(&self, index: usize) -> Result<(), ProbeGiError> {
        if !self.plane_coord.is_finite() || !self.radiance.is_finite() {
            return invalid_scene(
                format!("emitters[{index}]"),
                "plane_coord and radiance must be finite",
            );
        }
        if self.radiance.cmplt(Vec3::ZERO).any() {
            return invalid_scene(
                format!("emitters[{index}].radiance"),
                "radiance must be non-negative",
            );
        }
        for axis in 0..2 {
            let (lo, hi) = (self.tangent_min[axis], self.tangent_max[axis]);
            if !lo.is_finite() || !hi.is_finite() || lo >= hi {
                return invalid_scene(
                    format!("emitters[{index}].tangent"),
                    "tangent_min must be finite and strictly less than tangent_max",
                );
            }
        }
        Ok(())
    }

    /// Ray-panel intersection distance, or `None` if the ray misses the finite rectangle.
    fn intersect(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let a = self.axis.index();
        let d_a = dir[a];
        if d_a.abs() <= HIT_EPSILON {
            return None;
        }
        let t = (self.plane_coord - origin[a]) / d_a;
        if t <= HIT_EPSILON {
            return None;
        }
        let (u, v) = self.axis.tangents();
        let hu = origin[u] + t * dir[u];
        let hv = origin[v] + t * dir[v];
        if hu < self.tangent_min[0]
            || hu > self.tangent_max[0]
            || hv < self.tangent_min[1]
            || hv > self.tangent_max[1]
        {
            return None;
        }
        Some(t)
    }
}

/// Uniform sky/floor dome for rays that escape all geometry. Upward escaping rays sample
/// `sky_radiance`; downward escaping rays sample `floor_radiance` (the lit "white floor"
/// bouncing fill light). Day and night bake states differ mainly in these two values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SkyLighting {
    pub sky_radiance: Vec3,
    pub floor_radiance: Vec3,
}

impl SkyLighting {
    pub fn new(sky_radiance: Vec3, floor_radiance: Vec3) -> Self {
        Self {
            sky_radiance,
            floor_radiance,
        }
    }

    fn validate(&self) -> Result<(), ProbeGiError> {
        if !self.sky_radiance.is_finite() || !self.floor_radiance.is_finite() {
            return invalid_scene("sky", "sky/floor radiance must be finite");
        }
        if self.sky_radiance.cmplt(Vec3::ZERO).any() || self.floor_radiance.cmplt(Vec3::ZERO).any()
        {
            return invalid_scene("sky", "sky/floor radiance must be non-negative");
        }
        Ok(())
    }

    fn escape_radiance(&self, dir: Vec3) -> Vec3 {
        if dir.y >= 0.0 {
            self.sky_radiance
        } else {
            self.floor_radiance
        }
    }
}

/// The full analytic scene the bake integrates against, for one lighting state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BakeScene {
    pub state: String,
    pub emitters: Vec<EmissivePanel>,
    pub occluders: Vec<Aabb>,
    pub sky: SkyLighting,
}

/// Result of tracing one bake ray: the incident radiance along the ray and the distance to
/// the nearest surface (or [`ESCAPE_DISTANCE`] if the ray escapes).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TraceHit {
    pub radiance: Vec3,
    pub distance: f32,
}

impl BakeScene {
    pub fn new(
        state: impl Into<String>,
        emitters: Vec<EmissivePanel>,
        occluders: Vec<Aabb>,
        sky: SkyLighting,
    ) -> Self {
        Self {
            state: state.into(),
            emitters,
            occluders,
            sky,
        }
    }

    pub fn validate(&self) -> Result<(), ProbeGiError> {
        if self.state.trim().is_empty() {
            return invalid_scene("state", "bake scene must carry a non-empty state label");
        }
        if self.emitters.is_empty() {
            return invalid_scene("emitters", "bake scene needs at least one emissive panel");
        }
        for (index, emitter) in self.emitters.iter().enumerate() {
            emitter.validate(index)?;
        }
        for (index, occluder) in self.occluders.iter().enumerate() {
            if !occluder.min.is_finite() || !occluder.max.is_finite() {
                return invalid_scene(
                    format!("occluders[{index}]"),
                    "occluder bounds must be finite",
                );
            }
        }
        self.sky.validate()
    }

    /// Trace one ray, returning the nearest surface's incident radiance and distance.
    /// Occluders are opaque black (single-bounce direct-from-emitter model keeps the bake
    /// deterministic and cheap); emitters win ties only when strictly nearer.
    pub(crate) fn trace(&self, origin: Vec3, dir: Vec3) -> TraceHit {
        let mut best_distance = ESCAPE_DISTANCE;
        let mut best_radiance: Option<Vec3> = None;

        for occluder in &self.occluders {
            if let Some(t) = intersect_aabb(*occluder, origin, dir)
                && t < best_distance
            {
                best_distance = t;
                best_radiance = Some(Vec3::ZERO);
            }
        }
        for emitter in &self.emitters {
            if let Some(t) = emitter.intersect(origin, dir)
                && t < best_distance
            {
                best_distance = t;
                best_radiance = Some(emitter.radiance);
            }
        }

        match best_radiance {
            Some(radiance) => TraceHit {
                radiance,
                distance: best_distance,
            },
            None => TraceHit {
                radiance: self.sky.escape_radiance(dir),
                distance: ESCAPE_DISTANCE,
            },
        }
    }
}

/// Slab ray-AABB intersection returning the nearest positive entry distance.
fn intersect_aabb(aabb: Aabb, origin: Vec3, dir: Vec3) -> Option<f32> {
    let mut t_min = HIT_EPSILON;
    let mut t_max = ESCAPE_DISTANCE;
    for axis in 0..3 {
        let d = dir[axis];
        let o = origin[axis];
        let lo = aabb.min[axis];
        let hi = aabb.max[axis];
        if d.abs() <= HIT_EPSILON {
            if !(lo..=hi).contains(&o) {
                return None;
            }
        } else {
            let inv = 1.0 / d;
            let mut t0 = (lo - o) * inv;
            let mut t1 = (hi - o) * inv;
            if t0 > t1 {
                std::mem::swap(&mut t0, &mut t1);
            }
            t_min = t_min.max(t0);
            t_max = t_max.min(t1);
            if t_min > t_max {
                return None;
            }
        }
    }
    Some(t_min)
}
