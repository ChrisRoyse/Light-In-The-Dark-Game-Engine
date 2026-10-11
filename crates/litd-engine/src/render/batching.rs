//! Instancing, visibility culling, and deterministic draw/material batching.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::occlusion_grid::OcclusionGrid,
};

pub const BATCHING_CULLING_WGSL: &str = include_str!("batching_culling.wgsl");

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BatchingSettings {
    pub hzb_enabled: bool,
    pub occlusion_bias: f32,
    pub max_draw_batches: usize,
}

impl Default for BatchingSettings {
    fn default() -> Self {
        Self {
            hzb_enabled: true,
            occlusion_bias: 0.001,
            max_draw_batches: 512,
        }
    }
}

impl BatchingSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.occlusion_bias.is_finite() || self.occlusion_bias < 0.0 {
            return invalid_batching("settings.occlusion_bias", "must be finite and non-negative");
        }
        if self.max_draw_batches == 0 {
            return invalid_batching("settings.max_draw_batches", "must be greater than zero");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderInstance {
    pub id: u64,
    pub mesh_id: String,
    pub material_id: String,
    pub world_bounds: Aabb,
    pub screen_rect: ScreenRect,
    pub nearest_depth: f32,
    pub index_count: u32,
}

impl RenderInstance {
    pub fn validate(&self, index: usize) -> Result<(), EngineError> {
        if self.id == 0 {
            return invalid_batching(format!("instances[{index}].id"), "must be non-zero");
        }
        if self.mesh_id.trim().is_empty() {
            return invalid_batching(format!("instances[{index}].mesh_id"), "must be non-empty");
        }
        if self.material_id.trim().is_empty() {
            return invalid_batching(
                format!("instances[{index}].material_id"),
                "must be non-empty",
            );
        }
        if !self.world_bounds.min.is_finite()
            || !self.world_bounds.max.is_finite()
            || self.world_bounds.min.x > self.world_bounds.max.x
            || self.world_bounds.min.y > self.world_bounds.max.y
            || self.world_bounds.min.z > self.world_bounds.max.z
        {
            return invalid_batching(
                format!("instances[{index}].world_bounds"),
                "must be finite with min <= max",
            );
        }
        self.screen_rect
            .validate(|| format!("instances[{index}].screen_rect"))?;
        if !self.nearest_depth.is_finite() || !(0.0..=1.0).contains(&self.nearest_depth) {
            return invalid_batching(
                format!("instances[{index}].nearest_depth"),
                "must be finite and normalized inside 0..=1",
            );
        }
        if self.index_count == 0 {
            return invalid_batching(
                format!("instances[{index}].index_count"),
                "must be non-zero",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenRect {
    pub min: Vec2,
    pub max: Vec2,
}

impl ScreenRect {
    pub fn new(min: Vec2, max: Vec2) -> Result<Self, EngineError> {
        let rect = Self { min, max };
        rect.validate(|| "screen_rect".to_string())?;
        Ok(rect)
    }

    pub fn contains(&self, other: ScreenRect) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && self.max.x >= other.max.x
            && self.max.y >= other.max.y
    }

    /// Validates the rect. `field` is a lazy label builder invoked **only** on the
    /// error path, so the per-instance `format!` allocation never runs when
    /// validation passes (issue #1524 item 2).
    fn validate(&self, field: impl FnOnce() -> String) -> Result<(), EngineError> {
        if !self.min.is_finite()
            || !self.max.is_finite()
            || self.min.x < 0.0
            || self.min.y < 0.0
            || self.max.x > 1.0
            || self.max.y > 1.0
            || self.min.x > self.max.x
            || self.min.y > self.max.y
        {
            return invalid_batching(
                field(),
                "must be finite, normalized, and ordered inside 0..=1",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrustumPlane {
    pub normal: Vec3,
    pub distance: f32,
}

impl FrustumPlane {
    pub fn new(normal: Vec3, distance: f32) -> Self {
        Self { normal, distance }
    }

    fn validate(&self, index: usize) -> Result<(), EngineError> {
        if !self.normal.is_finite()
            || self.normal.length_squared() <= f32::EPSILON
            || !self.distance.is_finite()
        {
            return invalid_batching(
                format!("frustum.planes[{index}]"),
                "plane normal must be finite and non-zero, and distance must be finite",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frustum {
    pub planes: Vec<FrustumPlane>,
}

impl Frustum {
    pub fn new(planes: Vec<FrustumPlane>) -> Self {
        Self { planes }
    }

    pub fn orthographic(bounds: Aabb) -> Result<Self, EngineError> {
        if !bounds.min.is_finite()
            || !bounds.max.is_finite()
            || bounds.min.x >= bounds.max.x
            || bounds.min.y >= bounds.max.y
            || bounds.min.z >= bounds.max.z
        {
            return invalid_batching("frustum.bounds", "must be finite with min < max");
        }
        Ok(Self {
            planes: vec![
                FrustumPlane::new(Vec3::X, -bounds.min.x),
                FrustumPlane::new(-Vec3::X, bounds.max.x),
                FrustumPlane::new(Vec3::Y, -bounds.min.y),
                FrustumPlane::new(-Vec3::Y, bounds.max.y),
                FrustumPlane::new(Vec3::Z, -bounds.min.z),
                FrustumPlane::new(-Vec3::Z, bounds.max.z),
            ],
        })
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.planes.len() < 4 {
            return invalid_batching("frustum.planes", "must contain at least four planes");
        }
        for (index, plane) in self.planes.iter().enumerate() {
            plane.validate(index)?;
        }
        Ok(())
    }

    pub fn contains_aabb(&self, bounds: Aabb) -> bool {
        self.planes.iter().all(|plane| {
            let positive = Vec3::new(
                if plane.normal.x >= 0.0 {
                    bounds.max.x
                } else {
                    bounds.min.x
                },
                if plane.normal.y >= 0.0 {
                    bounds.max.y
                } else {
                    bounds.min.y
                },
                if plane.normal.z >= 0.0 {
                    bounds.max.z
                } else {
                    bounds.min.z
                },
            );
            plane.normal.dot(positive) + plane.distance >= 0.0
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HzbOccluder {
    pub screen_rect: ScreenRect,
    pub depth: f32,
}

impl HzbOccluder {
    pub fn new(screen_rect: ScreenRect, depth: f32) -> Self {
        Self { screen_rect, depth }
    }

    fn validate(&self, index: usize) -> Result<(), EngineError> {
        self.screen_rect
            .validate(|| format!("occluders[{index}].screen_rect"))?;
        if !self.depth.is_finite() || !(0.0..=1.0).contains(&self.depth) {
            return invalid_batching(
                format!("occluders[{index}].depth"),
                "must be finite and normalized inside 0..=1",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BatchKey {
    pub mesh_id: String,
    pub material_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawBatch {
    pub key: BatchKey,
    pub instance_ids: Vec<u64>,
    pub index_count: u32,
    pub first_instance: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectDrawCommand {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchPlan {
    pub visible_instance_ids: Vec<u64>,
    pub frustum_culled_ids: Vec<u64>,
    pub occlusion_culled_ids: Vec<u64>,
    pub draw_batches: Vec<DrawBatch>,
    pub indirect_commands: Vec<IndirectDrawCommand>,
}

pub fn build_batch_plan(
    settings: BatchingSettings,
    frustum: &Frustum,
    occluders: &[HzbOccluder],
    instances: &[RenderInstance],
) -> Result<BatchPlan, EngineError> {
    settings.validate()?;
    frustum.validate()?;
    for (index, occluder) in occluders.iter().enumerate() {
        occluder.validate(index)?;
    }
    for (index, instance) in instances.iter().enumerate() {
        instance.validate(index)?;
    }

    let mut visible_instance_ids = Vec::new();
    let mut frustum_culled_ids = Vec::new();
    let mut occlusion_culled_ids = Vec::new();
    // Key by borrowed `(mesh_id, material_id)` string slices so grouping does not
    // clone mesh/material Strings per instance; the owned `BatchKey` is built once
    // per emitted batch below. `(&str, &str)` sorts identically to `BatchKey`'s
    // derived `Ord` (mesh_id then material_id), preserving deterministic order
    // (issue #1524 item 2).
    let mut batches = BTreeMap::<(&str, &str), BatchAccumulator>::new();

    // Build the broad-phase occlusion grid once per frame instead of scanning all
    // occluders per instance (issue #1524 item 2). `None` means occlusion is disabled
    // or there are no occluders, so nothing is culled.
    let occlusion = OcclusionGrid::build(settings, occluders);

    for instance in instances {
        if !frustum.contains_aabb(instance.world_bounds) {
            frustum_culled_ids.push(instance.id);
            continue;
        }
        if occlusion
            .as_ref()
            .is_some_and(|grid| grid.is_occluded(occluders, instance))
        {
            occlusion_culled_ids.push(instance.id);
            continue;
        }

        visible_instance_ids.push(instance.id);
        let key = (instance.mesh_id.as_str(), instance.material_id.as_str());
        let batch = batches.entry(key).or_insert_with(|| BatchAccumulator {
            instance_ids: Vec::new(),
            index_count: instance.index_count,
        });
        if batch.index_count != instance.index_count {
            return invalid_batching(
                format!("instances[{}].index_count", instance.id),
                "instances sharing a mesh/material batch must share index_count",
            );
        }
        batch.instance_ids.push(instance.id);
    }

    if batches.len() > settings.max_draw_batches {
        return Err(EngineError::BatchingDrawLimitExceeded {
            draw_count: batches.len(),
            max_draws: settings.max_draw_batches,
        });
    }

    let mut draw_batches = Vec::with_capacity(batches.len());
    let mut indirect_commands = Vec::with_capacity(batches.len());
    let mut first_instance = 0_u32;
    for ((mesh_id, material_id), batch) in batches {
        let instance_count = u32::try_from(batch.instance_ids.len())
            .map_err(|_| invalid_batching_error("batch.instance_count", "does not fit in u32"))?;
        draw_batches.push(DrawBatch {
            key: BatchKey {
                mesh_id: mesh_id.to_string(),
                material_id: material_id.to_string(),
            },
            instance_ids: batch.instance_ids,
            index_count: batch.index_count,
            first_instance,
        });
        indirect_commands.push(IndirectDrawCommand {
            index_count: batch.index_count,
            instance_count,
            first_index: 0,
            base_vertex: 0,
            first_instance,
        });
        first_instance = first_instance.checked_add(instance_count).ok_or_else(|| {
            invalid_batching_error("batch.first_instance", "instance offset overflowed")
        })?;
    }

    Ok(BatchPlan {
        visible_instance_ids,
        frustum_culled_ids,
        occlusion_culled_ids,
        draw_batches,
        indirect_commands,
    })
}

#[derive(Debug, Clone)]
struct BatchAccumulator {
    instance_ids: Vec<u64>,
    index_count: u32,
}

fn invalid_batching<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_batching_error(field, reason))
}

fn invalid_batching_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::BatchingInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "batching_tests.rs"]
mod tests;
