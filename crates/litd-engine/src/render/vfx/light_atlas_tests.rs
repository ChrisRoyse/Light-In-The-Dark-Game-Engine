use super::*;
use crate::{
    EngineError,
    math::{Aabb, Vec3},
    render::lighting::ClusterGrid,
};

#[test]
fn lightning_plan_resolves_atlas_frame_and_clustered_light() {
    let effect = lightning_effect("issue752.lightning", 1).unwrap();
    let atlas = lightning_atlas();
    let terms = presentation_terms(0.375, 1.0);

    let plan = resolve_vfx_output_plan(&effect, &atlas, terms).unwrap();
    let assignment = cluster_vfx_output_lights(&plan, grid()).unwrap();

    assert_eq!(plan.effect_id, "issue752.lightning");
    assert_eq!(plan.sprite_count(), 1);
    assert_eq!(plan.local_light_count(), 1);
    assert_eq!(plan.sprite_samples[0].frame_index, 3);
    assert_eq!(plan.sprite_samples[0].frame_column, 3);
    assert_eq!(plan.sprite_samples[0].frame_row, 0);
    assert_eq!(
        plan.sprite_samples[0].uv_min,
        Vec3::new(0.75, 0.0, 0.0).truncate()
    );
    assert_eq!(
        plan.sprite_samples[0].uv_max,
        Vec3::new(1.0, 0.25, 0.0).truncate()
    );
    assert!(assignment.non_empty_cluster_count() > 0);
    assert!(assignment.total_light_refs() > 0);
    assert!(assignment.max_cluster_occupancy() <= VFX_MAX_LOCAL_LIGHTS);
    println!(
        "issue752 happy sprites={} lights={} frame={} uv_min={:?} uv_max={:?} cluster_refs={}",
        plan.sprite_count(),
        plan.local_light_count(),
        plan.sprite_samples[0].frame_index,
        plan.sprite_samples[0].uv_min,
        plan.sprite_samples[0].uv_max,
        assignment.refs_summary()
    );
}

#[test]
fn vfx_light_cap_rejects_ninth_local_light() {
    let mut effect = lightning_effect("issue752.over_cap", 1).unwrap();
    let base = effect.emitters[0].clone();
    for index in 0..8 {
        let mut emitter = base.clone();
        emitter.emitter_id = format!("emitter.lightning.extra.{index}");
        emitter.output = vec![VfxOutputModule::Light {
            color: VfxColor::rgba(0.55, 0.78, 1.0, 1.0),
            intensity_lumens: 6400.0,
            radius_meters: 6.0,
        }];
        effect.emitters.push(emitter);
    }
    let atlas = lightning_atlas();
    let error = resolve_vfx_output_plan(&effect, &atlas, presentation_terms(0.0, 1.0))
        .expect_err("ninth VFX light must fail loudly");

    assert!(error.to_string().contains("render.vfx.vfxlights.count"));
    println!("issue752 edge=light_cap error={error}");
}

#[test]
fn atlas_slot_out_of_range_is_rejected() {
    let mut atlas = lightning_atlas();
    atlas.slots[0].first_frame = 15;
    atlas.slots[0].frame_count = 2;
    let error = resolve_vfx_output_plan(
        &lightning_effect("issue752.bad_atlas", 1).unwrap(),
        &atlas,
        presentation_terms(0.0, 1.0),
    )
    .expect_err("slot must not extend beyond atlas capacity");

    assert!(
        error
            .to_string()
            .contains("render.vfx.vfxatlas.slots[0].frame_range")
    );
    println!("issue752 edge=atlas_range error={error}");
}

#[test]
fn fog_terms_dim_sprite_alpha_and_light() {
    let effect = lightning_effect("issue752.fog", 1).unwrap();
    let atlas = lightning_atlas();
    let bright = resolve_vfx_output_plan(&effect, &atlas, presentation_terms(0.0, 1.0)).unwrap();
    let dim = resolve_vfx_output_plan(&effect, &atlas, presentation_terms(0.0, 0.25)).unwrap();

    assert_eq!(bright.sprite_samples[0].fog_alpha_multiplier, 1.0);
    assert_eq!(dim.sprite_samples[0].fog_alpha_multiplier, 0.25);
    assert_eq!(
        bright.local_lights[0].intensity * 0.25,
        dim.local_lights[0].intensity
    );
    assert_eq!(
        bright.local_lights[0].color * 0.25,
        dim.local_lights[0].color
    );
    println!(
        "issue752 edge=fog bright_intensity={} dim_intensity={} dim_alpha={}",
        bright.local_lights[0].intensity,
        dim.local_lights[0].intensity,
        dim.sprite_samples[0].fog_alpha_multiplier
    );
}

#[test]
fn invalid_fog_visibility_fails_before_plan_mutation() {
    let effect = lightning_effect("issue752.bad_fog", 1).unwrap();
    let atlas = lightning_atlas();
    let error = resolve_vfx_output_plan(&effect, &atlas, presentation_terms(0.0, 1.25))
        .expect_err("fog must stay normalized");

    assert!(
        error
            .to_string()
            .contains("render.vfx.presentation.fog_visibility")
    );
    println!("issue752 edge=invalid_fog error={error}");
}

fn lightning_effect(effect_id: &str, light_count: usize) -> Result<VfxEffectAsset, EngineError> {
    let mut outputs = vec![VfxOutputModule::Sprite {
        material_id: "mat.vfx.lightning.additive".to_string(),
        texture_atlas_slot: "atlas.vfx.lightning.arc".to_string(),
        blend_mode: VfxBlendMode::Additive,
    }];
    for _ in 0..light_count {
        outputs.push(VfxOutputModule::Light {
            color: VfxColor::rgba(0.55, 0.78, 1.0, 1.0),
            intensity_lumens: 6400.0,
            radius_meters: 6.0,
        });
    }
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "Lightning Flash".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "emitter.lightning.arc".to_string(),
            display_name: "Lightning Arc".to_string(),
            max_particles: 2048,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(4.0))?,
            spawn: vec![VfxSpawnModule::Burst {
                count: 96,
                time_seconds: 0.0,
            }],
            initialize: vec![
                VfxInitializeModule::Shape(VfxShape::Sphere { radius_meters: 0.2 }),
                VfxInitializeModule::Velocity(VfxVelocity::Radial {
                    speed_meters_per_second: VfxRange::new(1.0, 7.0),
                }),
                VfxInitializeModule::Color {
                    color: VfxColor::rgba(0.55, 0.78, 1.0, 1.0),
                },
                VfxInitializeModule::Size {
                    pixels: VfxRange::new(24.0, 72.0),
                },
                VfxInitializeModule::Lifetime {
                    seconds: VfxRange::new(0.12, 0.38),
                },
            ],
            update: vec![
                VfxUpdateModule::Drag { coefficient: 0.1 },
                VfxUpdateModule::ColorOverLife {
                    gradient: lightning_gradient(),
                },
                VfxUpdateModule::SizeOverLife {
                    curve: lightning_curve(),
                },
            ],
            output: outputs,
        }],
    })
}

fn lightning_atlas() -> VfxAtlasCatalog {
    VfxAtlasCatalog {
        atlas_id: "atlas.vfx.shared".to_string(),
        columns: 4,
        rows: 4,
        slots: vec![VfxAtlasSlot {
            slot_id: "atlas.vfx.lightning.arc".to_string(),
            first_frame: 0,
            frame_count: 4,
            frames_per_second: 8.0,
        }],
    }
}

fn presentation_terms(elapsed_seconds: f32, fog_visibility: f32) -> VfxPresentationTerms {
    VfxPresentationTerms::new(
        Vec3::new(1.0, 0.25, 1.5),
        elapsed_seconds,
        VfxColor::rgba(0.8, 0.95, 1.0, 1.0),
        fog_visibility,
    )
}

fn grid() -> ClusterGrid {
    ClusterGrid::new(
        2,
        2,
        2,
        Vec3::new(-2.0, -2.0, -1.0),
        Vec3::new(3.0, 2.0, 4.0),
        VFX_MAX_LOCAL_LIGHTS,
    )
}

fn lightning_gradient() -> VfxGradient {
    VfxGradient {
        stops: vec![
            VfxGradientStop {
                t: 0.0,
                color: VfxColor::rgba(0.7, 0.9, 1.0, 1.0),
            },
            VfxGradientStop {
                t: 0.5,
                color: VfxColor::rgba(0.4, 0.65, 1.0, 0.85),
            },
            VfxGradientStop {
                t: 1.0,
                color: VfxColor::rgba(0.05, 0.1, 0.35, 0.0),
            },
        ],
    }
}

fn lightning_curve() -> VfxCurve {
    VfxCurve {
        points: vec![
            VfxCurvePoint { t: 0.0, value: 1.0 },
            VfxCurvePoint {
                t: 0.35,
                value: 0.65,
            },
            VfxCurvePoint { t: 1.0, value: 0.0 },
        ],
    }
}
