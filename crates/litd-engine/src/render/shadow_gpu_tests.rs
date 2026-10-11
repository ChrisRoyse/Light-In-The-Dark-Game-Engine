use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    math::Vec3,
    render::{
        lighting::{RenderLight, SpotCone},
        shadow::{ShadowCamera, ShadowSettings, render_shadow_maps_gpu_with_context},
    },
};

#[test]
fn issue1333_shadow_gpu_dispatches_depth_passes_and_reads_atlas() {
    pollster::block_on(issue1333_fsv()).unwrap();
}

async fn issue1333_fsv() -> Result<(), EngineError> {
    let run = begin_issue_run("issue1333_shadow_gpu").unwrap();
    let root = run.run_dir();
    let happy_bmp = root.join("shadow-happy-atlas.bmp");
    let empty_bmp = root.join("shadow-empty-lights-atlas.bmp");
    let spot_bmp = root.join("shadow-spot-cone-atlas.bmp");
    let report_path = run.report_path().to_path_buf();
    let context = GpuContext::new_headless().await?;
    let settings = shadow_settings();
    let camera = shadow_camera();
    let lights = shadow_lights();
    let before = json!({
        "context": context_state(&context),
        "settings": settings,
        "camera": camera,
        "light_names": lights.iter().map(|light| light.name.clone()).collect::<Vec<_>>(),
        "expected_entries": 9,
        "expected_cascades": 2,
        "expected_point_faces": 6,
        "expected_spot_maps": 1,
    });

    let frame = render_shadow_maps_gpu_with_context(&context, settings, camera, &lights).await?;
    assert_eq!(frame.stats.gpu_context_id, context.context_id());
    assert_eq!(frame.stats.entry_count, 9);
    assert_eq!(frame.stats.directional_cascade_count, 2);
    assert_eq!(frame.stats.point_face_count, 6);
    assert_eq!(frame.stats.spot_map_count, 1);
    assert_eq!(frame.stats.cascade_splits.len(), 2);
    assert!((frame.stats.cascade_splits[0].split_near - camera.near).abs() < 0.0001);
    assert!((frame.stats.cascade_splits[1].split_far - camera.far).abs() < 0.0001);
    assert!(frame.stats.shadow_pixel_ratio > 0.1);
    assert!(
        frame
            .stats
            .tile_depths
            .iter()
            .all(|tile| tile.shadow_pixel_count > 0)
    );
    frame.image.write_bmp(&happy_bmp)?;
    let happy_state = image_state(&happy_bmp);

    let edges = json!({
        "empty_lights": empty_lights_edge(&context, &empty_bmp).await?,
        "atlas_overflow": atlas_overflow_edge(&context).await,
        "spot_cone": spot_cone_edge(&context, &spot_bmp).await?,
        "empty_backends_no_adapter": empty_backends_edge().await,
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1333,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "happy_bmp": happy_bmp.display().to_string(),
            "empty_lights_bmp": empty_bmp.display().to_string(),
            "spot_cone_bmp": spot_bmp.display().to_string(),
            "depth_fields": ["tile_depths", "cascade_splits", "shadow_pixel_ratio"],
        },
        "happy_path": {
            "before": before,
            "after": {
                "context": context_state(&context),
                "stats": frame.stats,
                "bmp_readback": happy_state,
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_bytes = report_readback.bytes;
    let report_json = report_readback.json;

    assert_eq!(report_json["issue"], 1333);
    assert_eq!(
        report_json["happy_path"]["after"]["stats"]["entry_count"],
        9
    );
    assert_eq!(
        report_json["edges"]["empty_lights"]["after"]["stats"]["entry_count"],
        0
    );
    assert_eq!(
        report_json["edges"]["atlas_overflow"]["after"]["error_code"],
        "LITD-RENDER-SHADOW-ATLAS-OVERFLOW"
    );
    println!(
        "ISSUE1333_FSV_REPORT_READBACK path={} bytes={} sha256={} entries={} shadow_ratio={} happy_bmp_sha256={} spot_pixels={}",
        report_path.display(),
        report_bytes.len(),
        report_readback.sha256,
        report_json["happy_path"]["after"]["stats"]["entry_count"],
        report_json["happy_path"]["after"]["stats"]["shadow_pixel_ratio"],
        report_json["happy_path"]["after"]["bmp_readback"]["sha256"],
        report_json["edges"]["spot_cone"]["after"]["stats"]["shadow_pixel_count"],
    );
    Ok(())
}

async fn empty_lights_edge(context: &GpuContext, bmp_path: &Path) -> Result<Value, EngineError> {
    let before = json!({
        "context_id": context.context_id(),
        "settings": shadow_settings(),
        "lights": [],
        "expected_entries": 0,
    });
    let frame =
        render_shadow_maps_gpu_with_context(context, shadow_settings(), shadow_camera(), &[])
            .await?;
    assert_eq!(frame.stats.entry_count, 0);
    assert_eq!(frame.stats.shadow_pixel_count, 0);
    frame.image.write_bmp(bmp_path)?;
    let after = json!({
        "stats": frame.stats,
        "bmp_readback": image_state(bmp_path),
    });
    println!("ISSUE1333_EDGE empty_lights before={before} after={after}");
    Ok(json!({ "before": before, "after": after }))
}

async fn atlas_overflow_edge(context: &GpuContext) -> Value {
    let settings = ShadowSettings {
        atlas_size: 128,
        tile_size: 128,
        cascade_count: 2,
        max_shadowed_point_lights: 1,
        max_shadowed_spot_lights: 1,
        ..ShadowSettings::default()
    };
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "light_count": shadow_lights().len(),
        "expected_error_code": "LITD-RENDER-SHADOW-ATLAS-OVERFLOW",
    });
    let error =
        render_shadow_maps_gpu_with_context(context, settings, shadow_camera(), &shadow_lights())
            .await
            .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1333_EDGE atlas_overflow before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn spot_cone_edge(context: &GpuContext, bmp_path: &Path) -> Result<Value, EngineError> {
    let settings = ShadowSettings {
        atlas_size: 256,
        tile_size: 128,
        cascade_count: 1,
        max_shadowed_point_lights: 0,
        max_shadowed_spot_lights: 1,
        ..ShadowSettings::default()
    };
    let lights = vec![RenderLight::spot(
        "gate_spot_only",
        Vec3::new(-3.0, 4.0, 1.0),
        Vec3::new(0.3, -1.0, 0.6),
        18.0,
        SpotCone::new(0.35, 0.8),
        Vec3::ONE,
        5.0,
    )];
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "spot_outer_angle": 0.8,
        "expected_spot_maps": 1,
    });
    let frame =
        render_shadow_maps_gpu_with_context(context, settings, shadow_camera(), &lights).await?;
    assert_eq!(frame.stats.spot_map_count, 1);
    assert!(frame.stats.shadow_pixel_count > 0);
    frame.image.write_bmp(bmp_path)?;
    let after = json!({
        "stats": frame.stats,
        "bmp_readback": image_state(bmp_path),
    });
    println!("ISSUE1333_EDGE spot_cone before={before} after={after}");
    Ok(json!({ "before": before, "after": after }))
}

async fn empty_backends_edge() -> Value {
    let before = json!({
        "backends": "empty",
        "expected_error_code": "LITD-GPU-NO-ADAPTER",
    });
    let error = GpuContext::new_headless_on(wgpu::Backends::empty())
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1333_EDGE empty_backends before={before} after={after}");
    json!({ "before": before, "after": after })
}

fn shadow_settings() -> ShadowSettings {
    ShadowSettings {
        atlas_size: 512,
        tile_size: 128,
        cascade_count: 2,
        max_shadowed_point_lights: 1,
        max_shadowed_spot_lights: 1,
        pcf_radius: 2,
        depth_bias: 0.0015,
        normal_bias: 0.04,
        ..ShadowSettings::default()
    }
}

fn shadow_camera() -> ShadowCamera {
    ShadowCamera {
        position: Vec3::new(0.0, 8.0, -12.0),
        forward: Vec3::new(0.0, -0.35, 1.0),
        up: Vec3::Y,
        fov_y_rad: 60.0_f32.to_radians(),
        aspect: 16.0 / 9.0,
        near: 0.5,
        far: 80.0,
    }
}

fn shadow_lights() -> Vec<RenderLight> {
    vec![
        RenderLight::directional("sun", Vec3::new(0.2, 0.6, 1.0), Vec3::ONE, 3.0),
        RenderLight::point("torch", Vec3::new(2.0, 2.0, 3.0), 10.0, Vec3::ONE, 6.0),
        RenderLight::spot(
            "gate",
            Vec3::new(-3.0, 4.0, 1.0),
            Vec3::new(0.3, -1.0, 0.6),
            18.0,
            SpotCone::new(0.35, 0.8),
            Vec3::ONE,
            5.0,
        ),
    ]
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
