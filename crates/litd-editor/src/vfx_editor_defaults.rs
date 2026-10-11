use litd_engine::{
    EngineError,
    math::{Aabb, Vec3},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset,
        VfxEmitter, VfxGradient, VfxGradientStop, VfxInitializeModule, VfxOutputModule, VfxRange,
        VfxShape, VfxSpawnModule, VfxUpdateModule, VfxVelocity,
    },
};

pub(super) fn default_fireball_effect() -> Result<VfxEffectAsset, EngineError> {
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: "editor.vfx.fireball".to_string(),
        display_name: "Fireball Explosion".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "core_flame".to_string(),
            display_name: "Core Flame".to_string(),
            max_particles: 65_536,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(3.0))?,
            spawn: vec![
                VfxSpawnModule::ConstantRate {
                    particles_per_second: 2048.0,
                },
                VfxSpawnModule::Burst {
                    count: 512,
                    time_seconds: 0.0,
                },
            ],
            initialize: vec![
                VfxInitializeModule::Shape(VfxShape::Sphere { radius_meters: 1.8 }),
                VfxInitializeModule::Velocity(VfxVelocity::Radial {
                    speed_meters_per_second: VfxRange::new(8.0, 22.0),
                }),
                VfxInitializeModule::Color {
                    color: VfxColor::rgba(1.0, 0.44, 0.1, 1.0),
                },
                VfxInitializeModule::Size {
                    pixels: VfxRange::new(18.0, 64.0),
                },
                VfxInitializeModule::Lifetime {
                    seconds: VfxRange::new(0.55, 2.4),
                },
            ],
            update: vec![
                VfxUpdateModule::Acceleration {
                    world_acceleration: Vec3::new(0.0, 7.5, 0.0),
                },
                VfxUpdateModule::Drag { coefficient: 0.24 },
                VfxUpdateModule::Noise {
                    amplitude: 0.42,
                    frequency: 3.5,
                },
                VfxUpdateModule::ColorOverLife {
                    gradient: default_fire_gradient(),
                },
                VfxUpdateModule::SizeOverLife {
                    curve: default_size_curve(),
                },
            ],
            output: vec![
                VfxOutputModule::Sprite {
                    material_id: "mat.vfx.fire.additive".to_string(),
                    texture_atlas_slot: "atlas.vfx.fireball.impact_01".to_string(),
                    blend_mode: VfxBlendMode::SoftAdditive,
                },
                VfxOutputModule::Light {
                    color: VfxColor::rgba(1.0, 0.42, 0.12, 1.0),
                    intensity_lumens: 6400.0,
                    radius_meters: 6.0,
                },
            ],
        }],
    })
}

pub(super) fn default_size_curve() -> VfxCurve {
    VfxCurve {
        points: vec![
            VfxCurvePoint { t: 0.0, value: 0.2 },
            VfxCurvePoint {
                t: 0.35,
                value: 1.0,
            },
            VfxCurvePoint { t: 1.0, value: 0.0 },
        ],
    }
}

fn default_fire_gradient() -> VfxGradient {
    VfxGradient {
        stops: vec![
            VfxGradientStop {
                t: 0.0,
                color: VfxColor::rgba(1.0, 0.9, 0.32, 1.0),
            },
            VfxGradientStop {
                t: 0.45,
                color: VfxColor::rgba(1.0, 0.28, 0.08, 0.82),
            },
            VfxGradientStop {
                t: 1.0,
                color: VfxColor::rgba(0.04, 0.02, 0.01, 0.0),
            },
        ],
    }
}
