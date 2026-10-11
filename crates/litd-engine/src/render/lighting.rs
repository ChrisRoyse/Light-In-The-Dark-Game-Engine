//! Forward+/clustered lighting assignment and shader contract helpers.

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Aabb, Vec3},
};

pub const FORWARD_PLUS_WGSL: &str = include_str!("lighting_forward_plus.wgsl");

#[path = "lighting_ambient.rs"]
mod lighting_ambient;
#[path = "lighting_daynight.rs"]
mod lighting_daynight;
#[path = "lighting_gpu.rs"]
mod lighting_gpu;

pub use lighting_ambient::{AmbientLight, LightingRig};
pub use lighting_daynight::{
    DEFAULT_TIME_OF_DAY_HOURS, DayNightLightingInput, DayNightLightingPlan, TimeOfDayPhase,
    WeatherLightingState, build_day_night_lighting_plan,
};
pub use lighting_gpu::{
    ForwardPlusGpuFrame, ForwardPlusGpuImage, ForwardPlusGpuMetrics, ForwardPlusGpuSettings,
    ForwardPlusGpuStats, render_forward_plus_lighting_gpu,
    render_forward_plus_lighting_gpu_with_context, render_forward_plus_lighting_rig_gpu,
    render_forward_plus_lighting_rig_gpu_with_context,
};

pub const LIGHT_KIND_DIRECTIONAL: u32 = 0;
pub const LIGHT_KIND_POINT: u32 = 1;
pub const LIGHT_KIND_SPOT: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClusterGrid {
    pub x_slices: u32,
    pub y_slices: u32,
    pub z_slices: u32,
    pub view_min: Vec3,
    pub view_max: Vec3,
    pub max_lights_per_cluster: usize,
}

impl ClusterGrid {
    pub fn new(
        x_slices: u32,
        y_slices: u32,
        z_slices: u32,
        view_min: Vec3,
        view_max: Vec3,
        max_lights_per_cluster: usize,
    ) -> Self {
        Self {
            x_slices,
            y_slices,
            z_slices,
            view_min,
            view_max,
            max_lights_per_cluster,
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.x_slices == 0 || self.y_slices == 0 || self.z_slices == 0 {
            return invalid_config("slices", "all cluster slice counts must be non-zero");
        }
        if self.max_lights_per_cluster == 0 {
            return invalid_config("max_lights_per_cluster", "must be greater than zero");
        }
        if !self.view_min.is_finite() || !self.view_max.is_finite() {
            return invalid_config("view_bounds", "view-space bounds must be finite");
        }
        if self.view_min.x >= self.view_max.x
            || self.view_min.y >= self.view_max.y
            || self.view_min.z >= self.view_max.z
        {
            return invalid_config("view_bounds", "view_min must be less than view_max");
        }
        self.cluster_count()?;
        Ok(())
    }

    pub fn cluster_count(&self) -> Result<usize, EngineError> {
        let xy = self
            .x_slices
            .checked_mul(self.y_slices)
            .ok_or_else(|| invalid_config_error("slices", "x*y slice count overflowed"))?;
        let xyz = xy
            .checked_mul(self.z_slices)
            .ok_or_else(|| invalid_config_error("slices", "x*y*z slice count overflowed"))?;
        usize::try_from(xyz)
            .map_err(|_| invalid_config_error("slices", "cluster count does not fit usize"))
    }

    pub fn cluster_index(&self, x: u32, y: u32, z: u32) -> Result<usize, EngineError> {
        if x >= self.x_slices || y >= self.y_slices || z >= self.z_slices {
            return invalid_config("cluster_index", "cluster coordinates are outside the grid");
        }
        let xy = self
            .x_slices
            .checked_mul(self.y_slices)
            .ok_or_else(|| invalid_config_error("slices", "x*y slice count overflowed"))?;
        let z_offset = z
            .checked_mul(xy)
            .ok_or_else(|| invalid_config_error("cluster_index", "z offset overflowed"))?;
        let y_offset = y
            .checked_mul(self.x_slices)
            .ok_or_else(|| invalid_config_error("cluster_index", "y offset overflowed"))?;
        let index = z_offset
            .checked_add(y_offset)
            .and_then(|value| value.checked_add(x))
            .ok_or_else(|| invalid_config_error("cluster_index", "linear index overflowed"))?;
        usize::try_from(index)
            .map_err(|_| invalid_config_error("cluster_index", "index does not fit usize"))
    }

    pub fn cluster_coords(&self, index: usize) -> Result<(u32, u32, u32), EngineError> {
        if index >= self.cluster_count()? {
            return invalid_config("cluster_index", "cluster index is outside the grid");
        }
        let index = index as u32;
        let xy = self.x_slices * self.y_slices;
        let z = index / xy;
        let rem = index % xy;
        let y = rem / self.x_slices;
        let x = rem % self.x_slices;
        Ok((x, y, z))
    }

    pub fn cluster_bounds(&self, index: usize) -> Result<Aabb, EngineError> {
        self.validate()?;
        let (x, y, z) = self.cluster_coords(index)?;
        let slices = Vec3::new(
            self.x_slices as f32,
            self.y_slices as f32,
            self.z_slices as f32,
        );
        let step = (self.view_max - self.view_min) / slices;
        let min = self.view_min + step * Vec3::new(x as f32, y as f32, z as f32);
        let max = min + step;
        Aabb::new(min, max)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderLight {
    pub name: String,
    pub color: Vec3,
    pub intensity: f32,
    pub kind: LightKind,
}

impl RenderLight {
    pub fn directional(
        name: impl Into<String>,
        direction_to_light: Vec3,
        color: Vec3,
        intensity: f32,
    ) -> Self {
        Self {
            name: name.into(),
            color,
            intensity,
            kind: LightKind::Directional { direction_to_light },
        }
    }

    pub fn point(
        name: impl Into<String>,
        position: Vec3,
        radius: f32,
        color: Vec3,
        intensity: f32,
    ) -> Self {
        Self {
            name: name.into(),
            color,
            intensity,
            kind: LightKind::Point { position, radius },
        }
    }

    pub fn spot(
        name: impl Into<String>,
        position: Vec3,
        direction: Vec3,
        radius: f32,
        cone: SpotCone,
        color: Vec3,
        intensity: f32,
    ) -> Self {
        Self {
            name: name.into(),
            color,
            intensity,
            kind: LightKind::Spot {
                position,
                direction,
                radius,
                cone,
            },
        }
    }

    pub fn validate(&self, index: usize) -> Result<(), EngineError> {
        if !self.color.is_finite() || self.color.cmplt(Vec3::ZERO).any() {
            return invalid_light(index, "color", "RGB values must be finite and non-negative");
        }
        if !self.intensity.is_finite() || self.intensity < 0.0 {
            return invalid_light(index, "intensity", "must be finite and non-negative");
        }
        match self.kind {
            LightKind::Directional { direction_to_light } => {
                validate_direction(index, "direction_to_light", direction_to_light)
            }
            LightKind::Point { position, radius } => {
                validate_position(index, position)?;
                validate_radius(index, radius)
            }
            LightKind::Spot {
                position,
                direction,
                radius,
                cone,
            } => {
                validate_position(index, position)?;
                validate_direction(index, "direction", direction)?;
                validate_radius(index, radius)?;
                if !cone.inner_angle_rad.is_finite()
                    || !cone.outer_angle_rad.is_finite()
                    || cone.inner_angle_rad < 0.0
                    || cone.outer_angle_rad <= cone.inner_angle_rad
                    || cone.outer_angle_rad > std::f32::consts::PI
                {
                    return invalid_light(
                        index,
                        "spot_angles",
                        "require 0 <= inner < outer <= PI radians",
                    );
                }
                Ok(())
            }
        }
    }

    pub fn kind_id(&self) -> u32 {
        match self.kind {
            LightKind::Directional { .. } => LIGHT_KIND_DIRECTIONAL,
            LightKind::Point { .. } => LIGHT_KIND_POINT,
            LightKind::Spot { .. } => LIGHT_KIND_SPOT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LightKind {
    Directional {
        direction_to_light: Vec3,
    },
    Point {
        position: Vec3,
        radius: f32,
    },
    Spot {
        position: Vec3,
        direction: Vec3,
        radius: f32,
        cone: SpotCone,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpotCone {
    pub inner_angle_rad: f32,
    pub outer_angle_rad: f32,
}

impl SpotCone {
    pub fn new(inner_angle_rad: f32, outer_angle_rad: f32) -> Self {
        Self {
            inner_angle_rad,
            outer_angle_rad,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusteredLightAssignment {
    pub grid: ClusterGrid,
    pub directional_light_indices: Vec<usize>,
    pub cluster_light_indices: Vec<Vec<usize>>,
}

impl ClusteredLightAssignment {
    pub fn cluster_count(&self) -> usize {
        self.cluster_light_indices.len()
    }

    pub fn directional_count(&self) -> usize {
        self.directional_light_indices.len()
    }

    pub fn total_light_refs(&self) -> usize {
        self.cluster_light_indices.iter().map(Vec::len).sum()
    }

    pub fn non_empty_cluster_count(&self) -> usize {
        self.cluster_light_indices
            .iter()
            .filter(|indices| !indices.is_empty())
            .count()
    }

    pub fn max_cluster_occupancy(&self) -> usize {
        self.cluster_light_indices
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(0)
    }

    pub fn refs_summary(&self) -> String {
        self.cluster_light_indices
            .iter()
            .enumerate()
            .map(|(index, lights)| {
                let refs = lights
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{index}:[{refs}]")
            })
            .collect::<Vec<_>>()
            .join("|")
    }
}

pub fn cluster_lights(
    grid: ClusterGrid,
    lights: &[RenderLight],
) -> Result<ClusteredLightAssignment, EngineError> {
    grid.validate()?;
    for (index, light) in lights.iter().enumerate() {
        light.validate(index)?;
    }

    let mut directional_light_indices = Vec::new();
    let mut cluster_light_indices = vec![Vec::new(); grid.cluster_count()?];
    for (light_index, light) in lights.iter().enumerate() {
        match light.kind {
            LightKind::Directional { .. } => directional_light_indices.push(light_index),
            LightKind::Point { position, radius }
            | LightKind::Spot {
                position, radius, ..
            } => {
                for (cluster_index, refs) in cluster_light_indices.iter_mut().enumerate() {
                    let bounds = grid.cluster_bounds(cluster_index)?;
                    if sphere_intersects_aabb(position, radius, bounds) {
                        if refs.len() >= grid.max_lights_per_cluster {
                            return Err(EngineError::Rendering(litd_engine::error::RenderingError::ClusteredLightingClusterOverflow {
                                cluster_index,
                                max_lights: grid.max_lights_per_cluster,
                            }));
                        }
                        refs.push(light_index);
                    }
                }
            }
        }
    }

    Ok(ClusteredLightAssignment {
        grid,
        directional_light_indices,
        cluster_light_indices,
    })
}

fn sphere_intersects_aabb(center: Vec3, radius: f32, bounds: Aabb) -> bool {
    let closest = center.clamp(bounds.min, bounds.max);
    center.distance_squared(closest) <= radius * radius
}

fn validate_position(index: usize, position: Vec3) -> Result<(), EngineError> {
    if !position.is_finite() {
        return invalid_light(index, "position", "must be finite");
    }
    Ok(())
}

fn validate_radius(index: usize, radius: f32) -> Result<(), EngineError> {
    if !radius.is_finite() || radius <= 0.0 {
        return invalid_light(index, "radius", "must be finite and greater than zero");
    }
    Ok(())
}

fn validate_direction(
    index: usize,
    field: &'static str,
    direction: Vec3,
) -> Result<(), EngineError> {
    if !direction.is_finite() || direction.length_squared() <= f32::EPSILON {
        return invalid_light(index, field, "must be finite and non-zero");
    }
    Ok(())
}

fn invalid_light(
    index: usize,
    field: &'static str,
    reason: &'static str,
) -> Result<(), EngineError> {
    invalid_config(format!("lights[{index}].{field}"), reason)
}

fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_config_error(field, reason))
}

fn invalid_config_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::ClusteredLightingInvalidConfig {
            field: field.into(),
            reason: reason.into(),
        },
    )
}

#[cfg(test)]
#[path = "lighting_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lighting_gpu_tests.rs"]
mod lighting_gpu_tests;
