use std::{fs, path::Path, time::Duration};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::{GpuContext, GpuContextMode},
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, ScreenRect},
        reference::{ReferenceSceneSettings, render_reference_scene_gpu_with_context},
        vfx::{
            VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint,
            VfxEffectAsset, VfxEmitter, VfxGpuParticleSettings, VfxGradient, VfxGradientStop,
            VfxInitializeModule, VfxOutputModule, VfxRange, VfxRenderWorkloadSettings,
            VfxSceneEffectInstance, VfxShape, VfxSpawnModule, VfxUpdateModule, VfxVelocity,
            build_vfx_render_workload, render_vfx_particles_gpu_with_context,
        },
    },
};

#[test]
fn issue1337_shared_gpu_context_renders_reference_and_vfx() {
    pollster::block_on(issue1337_fsv()).unwrap();
}

async fn issue1337_fsv() -> Result<(), EngineError> {
    let run = begin_issue_run("issue1337_shared_gpu_context").unwrap();
    let root = run.run_dir();
    let reference_path = root.join("reference-scene-shared.bmp");
    let vfx_path = root.join("vfx-particles-shared.bmp");
    let report_path = run.report_path().to_path_buf();

    let context = GpuContext::new_headless().await?;
    assert_eq!(context.mode(), GpuContextMode::Headless);
    let reference_settings = ReferenceSceneSettings {
        width: 960,
        height: 540,
    };
    let vfx_settings = VfxGpuParticleSettings {
        width: 640,
        height: 360,
        max_particles: 16_384,
    };
    let workload = build_vfx_render_workload(
        workload_settings(),
        &frustum()?,
        &[],
        &dense_scene(8, Vec3::new(0.0, 0.0, 4.0))?,
    )?;
    let before = json!({
        "context": context_state(&context),
        "reference_settings": reference_settings,
        "vfx_settings": vfx_settings,
        "workload_particles": workload.simulated_particles,
        "workload_draw_batches": workload.draw_batches,
    });

    let reference = render_reference_scene_gpu_with_context(&context, reference_settings).await?;
    assert_eq!(reference.gpu_context_id, context.context_id());
    assert!(
        reference.comparison.pass,
        "{}",
        reference.comparison.band_summary()
    );
    reference.image.write_bmp(&reference_path)?;
    let reference_state = image_state(&reference_path);

    let vfx = render_vfx_particles_gpu_with_context(&context, vfx_settings, &workload).await?;
    assert_eq!(vfx.stats.gpu_context_id, context.context_id());
    assert!(vfx.stats.metrics.lit_pixel_ratio > 0.001);
    vfx.image.write_bmp(&vfx_path)?;
    let vfx_state = image_state(&vfx_path);

    let edges = json!({
        "empty_backends_no_adapter": empty_backends_edge().await,
        "invalid_reference_size": invalid_reference_size_edge(&context).await,
        "over_max_vfx_particles": over_max_vfx_particles_edge(&context, vfx_settings, &workload).await,
        "device_destroy_loss_callback": device_destroy_loss_edge().await?,
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1337,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "reference_bmp": reference_path.display().to_string(),
            "vfx_bmp": vfx_path.display().to_string(),
            "shared_context_field": "gpu_context_id",
        },
        "happy_path": {
            "before": before,
            "after": {
                "context": context_state(&context),
                "reference": {
                    "gpu_context_id": reference.gpu_context_id,
                    "adapter_name": reference.adapter_name,
                    "backend": reference.backend,
                    "texture_format": reference.texture_format,
                    "comparison_pass": reference.comparison.pass,
                    "comparison_checksum": reference.comparison.checksum,
                    "draw_calls": reference.draw_calls,
                    "readback_bytes": reference.readback_bytes,
                    "bmp_readback": reference_state,
                },
                "vfx": {
                    "gpu_context_id": vfx.stats.gpu_context_id,
                    "adapter_name": vfx.stats.adapter_name,
                    "backend": vfx.stats.backend,
                    "drawn_particles": vfx.stats.drawn_particles,
                    "draw_calls": vfx.stats.draw_calls,
                    "readback_bytes": vfx.stats.readback_bytes,
                    "image_checksum": vfx.stats.image_checksum,
                    "metrics": vfx.stats.metrics,
                    "bmp_readback": vfx_state,
                }
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_bytes = report_readback.bytes;
    let report_json = report_readback.json;

    assert_eq!(report_json["issue"], 1337);
    assert_eq!(
        report_json["happy_path"]["after"]["reference"]["gpu_context_id"],
        report_json["happy_path"]["after"]["vfx"]["gpu_context_id"]
    );
    assert_eq!(
        report_json["edges"]["empty_backends_no_adapter"]["after"]["error_code"],
        "LITD-GPU-NO-ADAPTER"
    );
    assert_eq!(
        report_json["edges"]["device_destroy_loss_callback"]["after"]["lost_reason"],
        "Destroyed"
    );
    println!(
        "ISSUE1337_FSV_REPORT_READBACK path={} bytes={} sha256={} context_id={} reference_bmp_sha256={} vfx_bmp_sha256={} lost_reason={}",
        report_path.display(),
        report_bytes.len(),
        report_readback.sha256,
        report_json["happy_path"]["after"]["context"]["context_id"],
        report_json["happy_path"]["after"]["reference"]["bmp_readback"]["sha256"],
        report_json["happy_path"]["after"]["vfx"]["bmp_readback"]["sha256"],
        report_json["edges"]["device_destroy_loss_callback"]["after"]["lost_reason"],
    );
    Ok(())
}

async fn empty_backends_edge() -> Value {
    let before = json!({ "backends": "empty", "expected": "NoAdapter" });
    let error = GpuContext::new_headless_on(wgpu::Backends::empty())
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1337_EDGE empty_backends before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn invalid_reference_size_edge(context: &GpuContext) -> Value {
    let before = json!({
        "context_id": context.context_id(),
        "width": 0,
        "height": 360,
    });
    let error = render_reference_scene_gpu_with_context(
        context,
        ReferenceSceneSettings {
            width: 0,
            height: 360,
        },
    )
    .await
    .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1337_EDGE invalid_reference_size before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn over_max_vfx_particles_edge(
    context: &GpuContext,
    mut settings: VfxGpuParticleSettings,
    workload: &crate::render::vfx::VfxRenderWorkloadStats,
) -> Value {
    settings.max_particles = 1;
    let before = json!({
        "context_id": context.context_id(),
        "max_particles": settings.max_particles,
        "workload_particles": workload.simulated_particles,
    });
    let error = render_vfx_particles_gpu_with_context(context, settings, workload)
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1337_EDGE over_max_vfx_particles before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn device_destroy_loss_edge() -> Result<Value, EngineError> {
    let context = GpuContext::new_headless().await?;
    let before = json!({
        "context": context_state(&context),
        "device_lost_report": context.device_lost_report(),
    });
    context.device().destroy();
    for _ in 0..8 {
        context.instance().poll_all(true);
        if context.device_lost_report().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let lost = context
        .device_lost_report()
        .expect("destroyed test device must report device loss");
    let after = json!({
        "context_id": lost.context_id,
        "lost_reason": lost.reason,
        "lost_message": lost.message,
    });
    println!("ISSUE1337_EDGE device_destroy_loss before={before} after={after}");
    Ok(json!({ "before": before, "after": after }))
}

fn context_state(context: &GpuContext) -> Value {
    let report = context.report();
    json!({
        "context_id": report.context_id,
        "mode": format!("{:?}", report.mode),
        "adapter_name": report.adapter.name,
        "backend": report.adapter.backend,
        "device_type": report.adapter.device_type,
        "driver": report.adapter.driver,
        "driver_info": report.adapter.driver_info,
        "device_lost_report": context.device_lost_report(),
    })
}

fn workload_settings() -> VfxRenderWorkloadSettings {
    VfxRenderWorkloadSettings {
        target_width: 640,
        target_height: 360,
        max_simulated_particles: 16_384,
        max_particle_buffer_bytes: 2 * 1024 * 1024,
        batching: BatchingSettings {
            max_draw_batches: 8,
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
                center + Vec3::new((id % 8) as f32 * 0.03, 0.0, 0.0),
                rect(0.1, 0.1, 0.9, 0.9)?,
            )
        })
        .collect()
}

fn scene_effect(
    id: u64,
    center: Vec3,
    screen_rect: ScreenRect,
) -> Result<VfxSceneEffectInstance, EngineError> {
    Ok(VfxSceneEffectInstance {
        instance_id: id,
        effect: spark_effect(&format!("issue1337.spark.{id}"))?,
        world_bounds: Aabb::from_center_half_extents(center, Vec3::splat(0.5))?,
        screen_rect,
        nearest_depth: 0.1,
        distance_meters: 24.0,
    })
}

fn spark_effect(effect_id: &str) -> Result<VfxEffectAsset, EngineError> {
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "Shared GPU Spark".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "emitter.shared.spark".to_string(),
            display_name: "Shared GPU Spark".to_string(),
            max_particles: 256,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(1.0))?,
            spawn: vec![VfxSpawnModule::ConstantRate {
                particles_per_second: 256.0,
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
                    gradient: VfxGradient {
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
                    },
                },
                VfxUpdateModule::SizeOverLife {
                    curve: VfxCurve {
                        points: vec![
                            VfxCurvePoint { t: 0.0, value: 1.0 },
                            VfxCurvePoint { t: 1.0, value: 0.0 },
                        ],
                    },
                },
            ],
            output: vec![VfxOutputModule::Sprite {
                material_id: "mat.vfx.shared.spark".to_string(),
                texture_atlas_slot: "atlas.vfx.shared.spark".to_string(),
                blend_mode: VfxBlendMode::Additive,
            }],
        }],
    })
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

fn image_state(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap();
    json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
        "bmp_signature": String::from_utf8_lossy(&bytes[0..2]).to_string(),
        "bmp_width": u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
        "bmp_height": u32::from_le_bytes(bytes[22..26].try_into().unwrap()),
    })
}

fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "error_code": report.code,
        "error_message": report.message,
        "remediation": report.remediation,
        "source_error": report.source_error,
    })
}
