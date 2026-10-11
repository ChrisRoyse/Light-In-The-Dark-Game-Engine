//! Deterministic doodad/scatter placement (rocks, foliage, debris).
//!
//! Placement is a pure function of the integer `seed` and the spec: it uses a
//! `splitmix64` bit-mixer (no OS RNG, no wall-clock), so the same seed yields a
//! byte-identical instance set on every machine and every run — safe to run
//! inside the deterministic simulation and safe to content-address. Only the
//! transform stream is generated here; the referenced prototype meshes are
//! produced by the primitive/heightfield generators and lowered separately.

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::mesh::invalid;

pub const SCATTER_SCHEMA: &str = "litd.procgen.scatter.v1";
const MAX_SCATTER_INSTANCES: u32 = 1_000_000;

/// A placeable prototype with a relative selection weight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterPrototype {
    pub prototype_id: String,
    pub weight: f32,
}

/// Input for [`generate_scatter`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterSpec {
    pub scatter_id: String,
    pub seed: u64,
    pub count: u32,
    /// Placement area (width on X, depth on Z), centred on the origin.
    pub area: [f32; 2],
    pub prototypes: Vec<ScatterPrototype>,
    pub min_scale: f32,
    pub max_scale: f32,
    /// When true each instance gets a random Y rotation; otherwise rotation is 0.
    pub random_yaw: bool,
}

/// One placed instance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScatterInstance {
    pub prototype_index: u32,
    pub position: [f32; 3],
    pub rotation_y: f32,
    pub uniform_scale: f32,
}

/// The deterministic placement result, content-addressed by `content_hash`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterProduct {
    pub schema: String,
    pub scatter_id: String,
    pub seed: u64,
    pub instance_count: usize,
    pub prototype_ids: Vec<String>,
    pub instances: Vec<ScatterInstance>,
    pub content_hash: String,
}

pub fn generate_scatter(spec: &ScatterSpec) -> Result<ScatterProduct, EngineError> {
    validate(spec)?;
    let total_weight: f32 = spec.prototypes.iter().map(|proto| proto.weight).sum();
    // Cumulative weight table for O(log n) weighted selection.
    let mut cumulative = Vec::with_capacity(spec.prototypes.len());
    let mut running = 0.0f32;
    for proto in &spec.prototypes {
        running += proto.weight;
        cumulative.push(running);
    }
    let half_x = spec.area[0] * 0.5;
    let half_z = spec.area[1] * 0.5;

    let mut instances = Vec::with_capacity(spec.count as usize);
    for index in 0..spec.count {
        // Independent, order-stable sub-stream per instance.
        let mut state = mix_seed(spec.seed, index as u64);
        let pick = unit_f32(splitmix64(&mut state)) * total_weight;
        let prototype_index = select(&cumulative, pick);
        let x = (unit_f32(splitmix64(&mut state)) - 0.5) * 2.0 * half_x;
        let z = (unit_f32(splitmix64(&mut state)) - 0.5) * 2.0 * half_z;
        let scale =
            spec.min_scale + unit_f32(splitmix64(&mut state)) * (spec.max_scale - spec.min_scale);
        let rotation_y = if spec.random_yaw {
            unit_f32(splitmix64(&mut state)) * std::f32::consts::TAU
        } else {
            0.0
        };
        instances.push(ScatterInstance {
            prototype_index,
            position: [x, 0.0, z],
            rotation_y,
            uniform_scale: scale,
        });
    }

    let content_hash = content_hash(spec, &instances);
    Ok(ScatterProduct {
        schema: SCATTER_SCHEMA.to_string(),
        scatter_id: spec.scatter_id.clone(),
        seed: spec.seed,
        instance_count: instances.len(),
        prototype_ids: spec
            .prototypes
            .iter()
            .map(|proto| proto.prototype_id.clone())
            .collect(),
        instances,
        content_hash,
    })
}

fn validate(spec: &ScatterSpec) -> Result<(), EngineError> {
    if spec.scatter_id.trim().is_empty() {
        return invalid("scatter.scatter_id", "scatter_id must be non-empty");
    }
    if spec.count == 0 {
        return invalid("scatter.count", "count must be at least 1");
    }
    if spec.count > MAX_SCATTER_INSTANCES {
        return invalid(
            "scatter.count",
            format!(
                "count {} exceeds the maximum of {MAX_SCATTER_INSTANCES}",
                spec.count
            ),
        );
    }
    if !spec.area[0].is_finite()
        || !spec.area[1].is_finite()
        || spec.area[0] <= 0.0
        || spec.area[1] <= 0.0
    {
        return invalid("scatter.area", "area extents must be finite and positive");
    }
    if spec.prototypes.is_empty() {
        return invalid("scatter.prototypes", "at least one prototype is required");
    }
    for (index, proto) in spec.prototypes.iter().enumerate() {
        if proto.prototype_id.trim().is_empty() {
            return invalid(
                "scatter.prototype_id",
                format!("prototype {index} has an empty id"),
            );
        }
        if !proto.weight.is_finite() || proto.weight <= 0.0 {
            return invalid(
                "scatter.prototype_weight",
                format!("prototype {index} weight must be finite and positive"),
            );
        }
    }
    if !spec.min_scale.is_finite()
        || !spec.max_scale.is_finite()
        || spec.min_scale <= 0.0
        || spec.max_scale < spec.min_scale
    {
        return invalid(
            "scatter.scale",
            "min_scale must be positive and max_scale must be >= min_scale",
        );
    }
    Ok(())
}

fn select(cumulative: &[f32], pick: f32) -> u32 {
    for (index, threshold) in cumulative.iter().enumerate() {
        if pick < *threshold {
            return index as u32;
        }
    }
    (cumulative.len() - 1) as u32
}

fn content_hash(spec: &ScatterSpec, instances: &[ScatterInstance]) -> String {
    let mut block = Vec::with_capacity(instances.len() * 24 + 64);
    block.extend_from_slice(SCATTER_SCHEMA.as_bytes());
    block.push(0);
    block.extend_from_slice(spec.scatter_id.as_bytes());
    block.push(0);
    block.extend_from_slice(&spec.seed.to_le_bytes());
    for instance in instances {
        block.extend_from_slice(&instance.prototype_index.to_le_bytes());
        for component in instance.position {
            block.extend_from_slice(&component.to_le_bytes());
        }
        block.extend_from_slice(&instance.rotation_y.to_le_bytes());
        block.extend_from_slice(&instance.uniform_scale.to_le_bytes());
    }
    sha256_hex(&block)
}

/// Fold the instance index into the seed so each instance has an independent
/// stream while the whole set stays a pure function of `seed`.
fn mix_seed(seed: u64, index: u64) -> u64 {
    let mut state = seed ^ index.wrapping_mul(0x9E3779B97F4A7C15);
    splitmix64(&mut state);
    state
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Map the top 24 bits of a 64-bit draw into `[0, 1)` as `f32` (24-bit mantissa
/// worth of entropy — exactly representable, no rounding surprises).
fn unit_f32(bits: u64) -> f32 {
    ((bits >> 40) as f32) / (1u64 << 24) as f32
}
