use super::*;
use crate::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, HzbOccluder, ScreenRect},
        vfx::{
            VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset, VfxEmitter,
            VfxGradient, VfxGradientStop, VfxInitializeModule, VfxOutputModule, VfxRange, VfxShape,
            VfxSpawnModule, VfxUpdateModule, VfxVelocity,
        },
    },
};

#[test]
fn dense_vfx_workload_batches_hundreds_of_effects() {
    let effects = dense_scene(400, Vec3::new(0.0, 0.0, 4.0)).unwrap();
    let stats = build_vfx_render_workload(settings(), &frustum().unwrap(), &[], &effects).unwrap();

    assert_eq!(stats.input_effect_instances, 400);
    assert_eq!(stats.visible_effect_instances, 400);
    assert_eq!(stats.visible_render_instances, 400);
    assert_eq!(stats.draw_batches, 1);
    assert_eq!(stats.indirect_commands, 1);
    assert_eq!(stats.simulated_particles, 400_000);
    assert_eq!(stats.batch_plan.indirect_commands[0].instance_count, 400);
    println!(
        "issue746 happy effects={} visible={} particles={} batches={} indirect_instances={}",
        stats.input_effect_instances,
        stats.visible_effect_instances,
        stats.simulated_particles,
        stats.draw_batches,
        stats.batch_plan.indirect_commands[0].instance_count
    );
}

#[test]
fn offscreen_and_far_lod_reduce_workload() {
    let scene = vec![
        scene_effect(
            1,
            Vec3::new(0.0, 0.0, 4.0),
            25.0,
            rect(0.10, 0.10, 0.20, 0.20).unwrap(),
        )
        .unwrap(),
        scene_effect(
            2,
            Vec3::new(12.0, 0.0, 4.0),
            25.0,
            rect(0.30, 0.10, 0.40, 0.20).unwrap(),
        )
        .unwrap(),
        scene_effect(
            3,
            Vec3::new(1.0, 0.0, 4.0),
            120.0,
            rect(0.50, 0.10, 0.60, 0.20).unwrap(),
        )
        .unwrap(),
        scene_effect(
            4,
            Vec3::new(0.0, 0.0, 4.0),
            25.0,
            rect(0.46, 0.46, 0.54, 0.54).unwrap(),
        )
        .unwrap(),
    ];
    let occluders = vec![HzbOccluder::new(rect(0.45, 0.45, 0.55, 0.55).unwrap(), 0.2)];
    let stats =
        build_vfx_render_workload(settings(), &frustum().unwrap(), &occluders, &scene).unwrap();

    assert_eq!(stats.frustum_culled_effect_ids, vec![2]);
    assert_eq!(stats.occlusion_culled_effect_ids, vec![4]);
    assert_eq!(stats.far_lod_effect_ids, vec![3]);
    assert_eq!(stats.visible_effect_instances, 2);
    assert_eq!(stats.simulated_particles, 1_250);
    assert_eq!(stats.lod_saved_particles, 750);
    println!(
        "issue746 edges cull={:?} occluded={:?} far={:?} particles={} saved={}",
        stats.frustum_culled_effect_ids,
        stats.occlusion_culled_effect_ids,
        stats.far_lod_effect_ids,
        stats.simulated_particles,
        stats.lod_saved_particles
    );
}

#[test]
fn particle_budget_fails_loud() {
    let mut limited = settings();
    limited.max_simulated_particles = 999;
    let scene = vec![
        scene_effect(
            1,
            Vec3::new(0.0, 0.0, 4.0),
            25.0,
            rect(0.10, 0.10, 0.20, 0.20).unwrap(),
        )
        .unwrap(),
    ];
    let error = build_vfx_render_workload(limited, &frustum().unwrap(), &[], &scene)
        .expect_err("over-budget particle workload must fail");

    assert!(
        error
            .to_string()
            .contains("render.vfx.render_workload.simulated_particles")
    );
    println!("issue746 edge=particle_budget error={error}");
}

#[test]
fn invalid_scene_instance_fails_before_batching() {
    let scene = vec![
        scene_effect(
            0,
            Vec3::new(0.0, 0.0, 4.0),
            25.0,
            rect(0.10, 0.10, 0.20, 0.20).unwrap(),
        )
        .unwrap(),
    ];
    let error = build_vfx_render_workload(settings(), &frustum().unwrap(), &[], &scene)
        .expect_err("zero instance id must fail");

    assert!(
        error
            .to_string()
            .contains("render.vfx.render_workload.instances[0].id")
    );
    println!("issue746 edge=invalid_instance error={error}");
}

fn settings() -> VfxRenderWorkloadSettings {
    VfxRenderWorkloadSettings {
        max_simulated_particles: 1_000_000,
        batching: BatchingSettings {
            max_draw_batches: 64,
            ..BatchingSettings::default()
        },
        ..VfxRenderWorkloadSettings::default()
    }
}

fn dense_scene(count: u64, center: Vec3) -> Result<Vec<VfxSceneEffectInstance>, EngineError> {
    (1..=count)
        .map(|id| {
            scene_effect(
                id,
                center + Vec3::new((id % 20) as f32 * 0.05, 0.0, 0.0),
                25.0,
                rect(0.10, 0.10, 0.90, 0.90)?,
            )
        })
        .collect()
}

fn scene_effect(
    id: u64,
    center: Vec3,
    distance_meters: f32,
    screen_rect: ScreenRect,
) -> Result<VfxSceneEffectInstance, EngineError> {
    Ok(VfxSceneEffectInstance {
        instance_id: id,
        effect: spark_effect(&format!("issue746.spark.{id}"))?,
        world_bounds: Aabb::from_center_half_extents(center, Vec3::splat(0.5))?,
        screen_rect,
        nearest_depth: if id == 4 { 0.8 } else { 0.1 },
        distance_meters,
    })
}

fn spark_effect(effect_id: &str) -> Result<VfxEffectAsset, EngineError> {
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "Dense Spark".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "emitter.spark".to_string(),
            display_name: "Spark".to_string(),
            max_particles: 1000,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(1.0))?,
            spawn: vec![VfxSpawnModule::ConstantRate {
                particles_per_second: 1000.0,
            }],
            initialize: vec![
                VfxInitializeModule::Shape(VfxShape::Sphere { radius_meters: 0.1 }),
                VfxInitializeModule::Velocity(VfxVelocity::Radial {
                    speed_meters_per_second: VfxRange::new(1.0, 3.0),
                }),
                VfxInitializeModule::Color {
                    color: VfxColor::rgba(1.0, 0.6, 0.1, 1.0),
                },
                VfxInitializeModule::Size {
                    pixels: VfxRange::new(4.0, 12.0),
                },
                VfxInitializeModule::Lifetime {
                    seconds: VfxRange::new(0.2, 0.8),
                },
            ],
            update: vec![
                VfxUpdateModule::Drag { coefficient: 0.2 },
                VfxUpdateModule::ColorOverLife {
                    gradient: spark_gradient(),
                },
                VfxUpdateModule::SizeOverLife {
                    curve: spark_curve(),
                },
            ],
            output: vec![VfxOutputModule::Sprite {
                material_id: "mat.vfx.spark.additive".to_string(),
                texture_atlas_slot: "atlas.vfx.spark".to_string(),
                blend_mode: VfxBlendMode::Additive,
            }],
        }],
    })
}

fn spark_gradient() -> VfxGradient {
    VfxGradient {
        stops: vec![
            VfxGradientStop {
                t: 0.0,
                color: VfxColor::rgba(1.0, 0.8, 0.2, 1.0),
            },
            VfxGradientStop {
                t: 1.0,
                color: VfxColor::rgba(0.2, 0.02, 0.0, 0.0),
            },
        ],
    }
}

fn spark_curve() -> VfxCurve {
    VfxCurve {
        points: vec![
            VfxCurvePoint { t: 0.0, value: 1.0 },
            VfxCurvePoint { t: 1.0, value: 0.0 },
        ],
    }
}

fn frustum() -> Result<Frustum, EngineError> {
    Frustum::orthographic(Aabb::new(
        Vec3::new(-5.0, -5.0, 0.0),
        Vec3::new(5.0, 5.0, 10.0),
    )?)
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}
