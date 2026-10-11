//! Shadow-map planning for directional CSM, point cube shadows, and spot shadows.

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Mat4, Vec3},
    render::lighting::{LightKind, RenderLight},
};

#[path = "shadow_cube.rs"]
mod cube;
#[path = "shadow_gpu.rs"]
mod shadow_gpu;

pub use cube::CubeFace;
pub use shadow_gpu::{
    ShadowGpuFrame, ShadowGpuImage, ShadowGpuStats, ShadowTileReadback, render_shadow_maps_gpu,
    render_shadow_maps_gpu_with_context,
};

pub const SHADOW_WGSL: &str = include_str!("shadow.wgsl");

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShadowSettings {
    pub atlas_size: u32,
    pub tile_size: u32,
    pub cascade_count: u32,
    pub cascade_lambda: f32,
    pub max_shadowed_point_lights: u32,
    pub max_shadowed_spot_lights: u32,
    pub pcf_radius: u32,
    pub depth_bias: f32,
    pub normal_bias: f32,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            atlas_size: 4096,
            tile_size: 1024,
            cascade_count: 4,
            cascade_lambda: 0.65,
            max_shadowed_point_lights: 4,
            max_shadowed_spot_lights: 8,
            pcf_radius: 2,
            depth_bias: 0.0015,
            normal_bias: 0.04,
        }
    }
}

impl ShadowSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.atlas_size == 0 || self.tile_size == 0 || self.tile_size > self.atlas_size {
            return invalid_shadow("atlas", "atlas and tile sizes must be positive and ordered");
        }
        if !self.atlas_size.is_multiple_of(self.tile_size) {
            return invalid_shadow(
                "atlas",
                "atlas_size must be an integer multiple of tile_size",
            );
        }
        if self.cascade_count == 0 || self.cascade_count > 8 {
            return invalid_shadow("cascade_count", "must be in the range 1..=8");
        }
        if !self.cascade_lambda.is_finite() || !(0.0..=1.0).contains(&self.cascade_lambda) {
            return invalid_shadow("cascade_lambda", "must be finite and inside 0..=1");
        }
        if self.pcf_radius > 4 {
            return invalid_shadow("pcf_radius", "must be <= 4 for the fixed WGSL PCF kernel");
        }
        if !self.depth_bias.is_finite()
            || !self.normal_bias.is_finite()
            || self.depth_bias < 0.0
            || self.normal_bias < 0.0
        {
            return invalid_shadow(
                "bias",
                "depth and normal bias must be finite and non-negative",
            );
        }
        self.tile_capacity()?;
        Ok(())
    }

    pub fn tile_capacity(&self) -> Result<usize, EngineError> {
        let per_row = self.atlas_size / self.tile_size;
        let total = per_row
            .checked_mul(per_row)
            .ok_or_else(|| invalid_shadow_error("atlas", "tile capacity overflowed"))?;
        usize::try_from(total)
            .map_err(|_| invalid_shadow_error("atlas", "tile capacity does not fit usize"))
    }

    fn tile_for(&self, index: usize) -> Result<AtlasTile, EngineError> {
        let capacity = self.tile_capacity()?;
        if index >= capacity {
            return Err(EngineError::ShadowAtlasOverflow {
                required_tiles: index + 1,
                available_tiles: capacity,
            });
        }
        let per_row = self.atlas_size / self.tile_size;
        let x = (index as u32 % per_row) * self.tile_size;
        let y = (index as u32 / per_row) * self.tile_size;
        Ok(AtlasTile {
            x,
            y,
            size: self.tile_size,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShadowCamera {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub fov_y_rad: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl ShadowCamera {
    pub fn validate(&self) -> Result<(), EngineError> {
        normalize_field("camera.forward", self.forward)?;
        normalize_field("camera.up", self.up)?;
        if self.forward.cross(self.up).length_squared() <= f32::EPSILON {
            return invalid_shadow(
                "camera.basis",
                "forward and up vectors must not be parallel",
            );
        }
        if !self.position.is_finite() {
            return invalid_shadow("camera.position", "must be finite");
        }
        if !self.fov_y_rad.is_finite()
            || self.fov_y_rad <= 0.0
            || self.fov_y_rad >= std::f32::consts::PI
        {
            return invalid_shadow("camera.fov_y_rad", "must be finite and inside 0..PI");
        }
        if !self.aspect.is_finite() || self.aspect <= 0.0 {
            return invalid_shadow("camera.aspect", "must be finite and greater than zero");
        }
        if !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.far <= self.near
        {
            return invalid_shadow("camera.depth_range", "require 0 < near < far");
        }
        Ok(())
    }

    fn basis(&self) -> Result<(Vec3, Vec3, Vec3), EngineError> {
        let forward = normalize_field("camera.forward", self.forward)?;
        let up_input = normalize_field("camera.up", self.up)?;
        let right = forward.cross(up_input).normalize();
        let up = right.cross(forward).normalize();
        Ok((forward, right, up))
    }

    fn frustum_corners(&self, near: f32, far: f32) -> Result<[Vec3; 8], EngineError> {
        let (forward, right, up) = self.basis()?;
        let tan_half = (self.fov_y_rad * 0.5).tan();
        let mut corners = [Vec3::ZERO; 8];
        let mut cursor = 0;
        for distance in [near, far] {
            let center = self.position + forward * distance;
            let half_y = tan_half * distance;
            let half_x = half_y * self.aspect;
            for y_sign in [-1.0, 1.0] {
                for x_sign in [-1.0, 1.0] {
                    corners[cursor] = center + right * half_x * x_sign + up * half_y * y_sign;
                    cursor += 1;
                }
            }
        }
        Ok(corners)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowPlan {
    pub settings: ShadowSettings,
    pub entries: Vec<ShadowMapEntry>,
}

impl ShadowPlan {
    pub fn directional_cascade_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry.kind, ShadowMapKind::DirectionalCascade { .. }))
            .count()
    }

    pub fn point_face_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry.kind, ShadowMapKind::PointFace { .. }))
            .count()
    }

    pub fn spot_map_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry.kind, ShadowMapKind::Spot))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowMapEntry {
    pub light_index: usize,
    pub light_name: String,
    pub kind: ShadowMapKind,
    pub tile: AtlasTile,
    pub view_projection: Mat4,
    pub depth_bias: f32,
    pub normal_bias: f32,
    pub pcf_radius: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AtlasTile {
    pub x: u32,
    pub y: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadowMapKind {
    DirectionalCascade {
        cascade_index: u32,
        split_near: f32,
        split_far: f32,
    },
    PointFace {
        face: CubeFace,
    },
    Spot,
}

pub fn build_shadow_plan(
    settings: ShadowSettings,
    camera: ShadowCamera,
    lights: &[RenderLight],
) -> Result<ShadowPlan, EngineError> {
    settings.validate()?;
    camera.validate()?;
    for (index, light) in lights.iter().enumerate() {
        light.validate(index)?;
    }
    validate_light_limits(settings, lights)?;
    let required_tiles = required_tile_count(settings, lights);
    let capacity = settings.tile_capacity()?;
    if required_tiles > capacity {
        return Err(EngineError::ShadowAtlasOverflow {
            required_tiles,
            available_tiles: capacity,
        });
    }

    let cascades = cascade_ranges(settings, camera)?;
    let mut entries = Vec::with_capacity(required_tiles);
    for (light_index, light) in lights.iter().enumerate() {
        match light.kind {
            LightKind::Directional { direction_to_light } => {
                for (cascade_index, (split_near, split_far)) in cascades.iter().copied().enumerate()
                {
                    entries.push(entry(
                        settings,
                        entries.len(),
                        light_index,
                        light,
                        ShadowMapKind::DirectionalCascade {
                            cascade_index: cascade_index as u32,
                            split_near,
                            split_far,
                        },
                        directional_matrix(&camera, direction_to_light, split_near, split_far)?,
                    )?);
                }
            }
            LightKind::Point { position, radius } => {
                for face in CubeFace::ALL {
                    entries.push(entry(
                        settings,
                        entries.len(),
                        light_index,
                        light,
                        ShadowMapKind::PointFace { face },
                        point_face_matrix(position, radius, face),
                    )?);
                }
            }
            LightKind::Spot {
                position,
                direction,
                radius,
                cone,
            } => entries.push(entry(
                settings,
                entries.len(),
                light_index,
                light,
                ShadowMapKind::Spot,
                spot_matrix(position, direction, radius, cone.outer_angle_rad)?,
            )?),
        }
    }

    Ok(ShadowPlan { settings, entries })
}

fn entry(
    settings: ShadowSettings,
    tile_index: usize,
    light_index: usize,
    light: &RenderLight,
    kind: ShadowMapKind,
    view_projection: Mat4,
) -> Result<ShadowMapEntry, EngineError> {
    let tile = settings.tile_for(tile_index)?;
    Ok(ShadowMapEntry {
        light_index,
        light_name: light.name.clone(),
        kind,
        tile,
        view_projection,
        depth_bias: settings.depth_bias,
        normal_bias: settings.normal_bias,
        pcf_radius: settings.pcf_radius,
    })
}

fn required_tile_count(settings: ShadowSettings, lights: &[RenderLight]) -> usize {
    lights
        .iter()
        .map(|light| match light.kind {
            LightKind::Directional { .. } => settings.cascade_count as usize,
            LightKind::Point { .. } => 6,
            LightKind::Spot { .. } => 1,
        })
        .sum()
}

fn validate_light_limits(
    settings: ShadowSettings,
    lights: &[RenderLight],
) -> Result<(), EngineError> {
    let point_count = lights
        .iter()
        .filter(|light| matches!(light.kind, LightKind::Point { .. }))
        .count();
    let spot_count = lights
        .iter()
        .filter(|light| matches!(light.kind, LightKind::Spot { .. }))
        .count();
    if point_count > settings.max_shadowed_point_lights as usize {
        return invalid_shadow(
            "max_shadowed_point_lights",
            "point light shadow budget exceeded",
        );
    }
    if spot_count > settings.max_shadowed_spot_lights as usize {
        return invalid_shadow(
            "max_shadowed_spot_lights",
            "spot light shadow budget exceeded",
        );
    }
    Ok(())
}

fn cascade_ranges(
    settings: ShadowSettings,
    camera: ShadowCamera,
) -> Result<Vec<(f32, f32)>, EngineError> {
    let count = settings.cascade_count as usize;
    let mut ranges = Vec::with_capacity(count);
    for index in 0..count {
        ranges.push((
            cascade_split(camera, settings.cascade_lambda, index as f32 / count as f32),
            cascade_split(
                camera,
                settings.cascade_lambda,
                (index + 1) as f32 / count as f32,
            ),
        ));
    }
    Ok(ranges)
}

fn cascade_split(camera: ShadowCamera, lambda: f32, t: f32) -> f32 {
    let uniform = camera.near + (camera.far - camera.near) * t;
    let logarithmic = camera.near * (camera.far / camera.near).powf(t);
    logarithmic * lambda + uniform * (1.0 - lambda)
}

fn directional_matrix(
    camera: &ShadowCamera,
    direction_to_light: Vec3,
    split_near: f32,
    split_far: f32,
) -> Result<Mat4, EngineError> {
    let corners = camera.frustum_corners(split_near, split_far)?;
    let center = corners.iter().copied().sum::<Vec3>() / corners.len() as f32;
    let light_forward = -normalize_field("directional.direction_to_light", direction_to_light)?;
    let radius = corners
        .iter()
        .map(|corner| corner.distance(center))
        .fold(0.0_f32, f32::max)
        .max(1.0);
    let view = Mat4::look_at_rh(
        center - light_forward * radius * 2.0,
        center,
        stable_up(light_forward),
    );
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for corner in corners {
        let point = view.transform_point3(corner);
        min = min.min(point);
        max = max.max(point);
    }
    let margin = radius * 0.05;
    let near = (-max.z - margin).max(0.01);
    let far = (-min.z + margin).max(near + 0.01);
    let projection = Mat4::orthographic_rh(
        min.x - margin,
        max.x + margin,
        min.y - margin,
        max.y + margin,
        near,
        far,
    );
    Ok(projection * view)
}

fn point_face_matrix(position: Vec3, radius: f32, face: CubeFace) -> Mat4 {
    let (direction, up) = face.direction_up();
    Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, radius)
        * Mat4::look_at_rh(position, position + direction, up)
}

fn spot_matrix(
    position: Vec3,
    direction: Vec3,
    radius: f32,
    outer_angle_rad: f32,
) -> Result<Mat4, EngineError> {
    let forward = normalize_field("spot.direction", direction)?;
    let fov = (outer_angle_rad * 2.0).min(std::f32::consts::PI - 0.01);
    Ok(Mat4::perspective_rh(fov, 1.0, 0.1, radius)
        * Mat4::look_at_rh(position, position + forward, stable_up(forward)))
}

fn stable_up(forward: Vec3) -> Vec3 {
    if forward.dot(Vec3::Y).abs() > 0.92 {
        Vec3::Z
    } else {
        Vec3::Y
    }
}

fn normalize_field(field: impl Into<String>, vector: Vec3) -> Result<Vec3, EngineError> {
    if !vector.is_finite() || vector.length_squared() <= f32::EPSILON {
        return invalid_shadow(field, "vector must be finite and non-zero");
    }
    Ok(vector.normalize())
}

fn invalid_shadow<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_shadow_error(field, reason))
}

fn invalid_shadow_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ShadowInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "shadow_tests.rs"]
mod shadow_tests;

#[cfg(test)]
#[path = "shadow_gpu_tests.rs"]
mod shadow_gpu_tests;
