use serde::{Deserialize, Serialize};

use crate::math::{Aabb, Vec3};

pub const VFX_EFFECT_SCHEMA_VERSION: u32 = 1;
pub const VFX_MAX_EMITTERS: usize = 16;
pub const VFX_MAX_PARTICLES_PER_EMITTER: u32 = 65_536;
pub const VFX_MAX_PARTICLES_PER_EFFECT: u32 = 100_000;
pub const VFX_MAX_OUTPUTS_PER_EMITTER: usize = 8;
pub const VFX_MAX_GRADIENT_STOPS: usize = 16;
pub const VFX_MAX_CURVE_POINTS: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxEffectAsset {
    pub schema_version: u32,
    pub effect_id: String,
    pub display_name: String,
    pub presentation_only: bool,
    pub emitters: Vec<VfxEmitter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxEmitter {
    pub emitter_id: String,
    pub display_name: String,
    pub max_particles: u32,
    pub local_bounds: Aabb,
    pub spawn: Vec<VfxSpawnModule>,
    pub initialize: Vec<VfxInitializeModule>,
    pub update: Vec<VfxUpdateModule>,
    pub output: Vec<VfxOutputModule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VfxSpawnModule {
    ConstantRate { particles_per_second: f32 },
    Burst { count: u32, time_seconds: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VfxInitializeModule {
    Shape(VfxShape),
    Velocity(VfxVelocity),
    Color { color: VfxColor },
    Size { pixels: VfxRange },
    Lifetime { seconds: VfxRange },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VfxUpdateModule {
    Acceleration { world_acceleration: Vec3 },
    Drag { coefficient: f32 },
    Noise { amplitude: f32, frequency: f32 },
    ColorOverLife { gradient: VfxGradient },
    SizeOverLife { curve: VfxCurve },
    Collision { restitution: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VfxOutputModule {
    Sprite {
        material_id: String,
        texture_atlas_slot: String,
        blend_mode: VfxBlendMode,
    },
    Mesh {
        mesh_id: String,
        material_id: String,
        blend_mode: VfxBlendMode,
    },
    Trail {
        material_id: String,
        width_pixels: f32,
        blend_mode: VfxBlendMode,
    },
    Light {
        color: VfxColor,
        intensity_lumens: f32,
        radius_meters: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VfxBlendMode {
    Alpha,
    Additive,
    SoftAdditive,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VfxShape {
    Point,
    Sphere {
        radius_meters: f32,
    },
    Cone {
        radius_meters: f32,
        angle_degrees: f32,
    },
    Box {
        half_extents_meters: Vec3,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VfxVelocity {
    Radial {
        speed_meters_per_second: VfxRange,
    },
    Directional {
        direction: Vec3,
        speed_meters_per_second: VfxRange,
    },
    Random {
        speed_meters_per_second: VfxRange,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VfxRange {
    pub min: f32,
    pub max: f32,
}

impl VfxRange {
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VfxColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl VfxColor {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxGradient {
    pub stops: Vec<VfxGradientStop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VfxGradientStop {
    pub t: f32,
    pub color: VfxColor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxCurve {
    pub points: Vec<VfxCurvePoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VfxCurvePoint {
    pub t: f32,
    pub value: f32,
}
