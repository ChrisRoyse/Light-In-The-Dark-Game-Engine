use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    math::Vec3,
    render::lighting::{
        ClusterGrid, ForwardPlusGpuSettings, RenderLight, SpotCone, cluster_lights,
        render_forward_plus_lighting_gpu_with_context,
    },
};

#[test]
fn issue1332_forward_plus_gpu_dispatches_compute_and_shades_framebuffer() {
    pollster::block_on(issue1332_fsv()).unwrap();
}

async fn issue1332_fsv() -> Result<(), EngineError> {
    let run = begin_issue_run("issue1332_forward_plus_gpu").unwrap();
    let root = run.run_dir();
    let happy_bmp = root.join("forward-plus-happy-path.bmp");
    let empty_bmp = root.join("forward-plus-empty-lights.bmp");
    let report_path = run.report_path().to_path_buf();
    let settings = ForwardPlusGpuSettings {
        width: 640,
        height: 360,
        shading_z_slice: 0,
    };
    let grid = known_grid(4);
    let lights = known_lights();
    let context = GpuContext::new_headless().await?;
    let cpu_assignment = cluster_lights(grid, &lights)?;
    let before = json!({
        "context": context_state(&context),
        "settings": settings,
        "grid": grid,
        "light_names": lights.iter().map(|light| light.name.clone()).collect::<Vec<_>>(),
        "cpu_directional_indices": cpu_assignment.directional_light_indices,
        "cpu_refs_summary": cpu_assignment.refs_summary(),
    });

    let frame =
        render_forward_plus_lighting_gpu_with_context(&context, settings, grid, &lights).await?;
    assert_eq!(frame.stats.gpu_context_id, context.context_id());
    assert_eq!(frame.stats.cpu_refs_summary, "0:[1]|1:[2]");
    assert_eq!(frame.stats.gpu_refs_summary, "0:[1]|1:[2]");
    assert_eq!(frame.stats.gpu_cluster_counts, vec![1, 1]);
    assert!(frame.stats.metrics.lit_pixel_ratio > 0.95);
    assert!(frame.stats.metrics.hot_pixel_ratio > 0.40);
    assert!(frame.stats.metrics.blue_pixel_ratio > 0.40);
    frame.image.write_bmp(&happy_bmp)?;
    let happy_state = image_state(&happy_bmp);

    let edges = json!({
        "empty_lights": empty_lights_edge(&context, &empty_bmp).await?,
        "invalid_target_size": invalid_target_size_edge(&context).await,
        "cluster_overflow": cluster_overflow_edge(&context).await,
        "empty_backends_no_adapter": empty_backends_edge().await,
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1332,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "happy_path_bmp": happy_bmp.display().to_string(),
            "empty_lights_bmp": empty_bmp.display().to_string(),
            "cluster_assignment_fields": ["cpu_refs_summary", "gpu_refs_summary", "gpu_cluster_counts"],
            "framebuffer_fields": ["bmp_signature", "bmp_width", "bmp_height", "sha256"],
        },
        "happy_path": {
            "before": before,
            "after": {
                "context": context_state(&context),
                "stats": frame.stats,
                "assignment": frame.assignment,
                "bmp_readback": happy_state,
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_bytes = report_readback.bytes;
    let report_json = report_readback.json;

    assert_eq!(report_json["issue"], 1332);
    assert_eq!(
        report_json["happy_path"]["after"]["stats"]["gpu_refs_summary"],
        "0:[1]|1:[2]"
    );
    assert_eq!(
        report_json["edges"]["empty_lights"]["after"]["stats"]["gpu_total_light_refs"],
        0
    );
    assert_eq!(
        report_json["edges"]["invalid_target_size"]["after"]["error_code"],
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG"
    );
    println!(
        "ISSUE1332_FSV_REPORT_READBACK path={} bytes={} sha256={} refs={} counts={} lit_ratio={} bmp_sha256={}",
        report_path.display(),
        report_bytes.len(),
        report_readback.sha256,
        report_json["happy_path"]["after"]["stats"]["gpu_refs_summary"],
        report_json["happy_path"]["after"]["stats"]["gpu_cluster_counts"],
        report_json["happy_path"]["after"]["stats"]["metrics"]["lit_pixel_ratio"],
        report_json["happy_path"]["after"]["bmp_readback"]["sha256"],
    );
    Ok(())
}

async fn empty_lights_edge(context: &GpuContext, bmp_path: &Path) -> Result<Value, EngineError> {
    let settings = ForwardPlusGpuSettings {
        width: 320,
        height: 180,
        shading_z_slice: 0,
    };
    let grid = known_grid(4);
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "grid": grid,
        "lights": [],
        "expected_gpu_refs_summary": "0:[]|1:[]",
    });
    let frame = render_forward_plus_lighting_gpu_with_context(context, settings, grid, &[]).await?;
    assert_eq!(frame.stats.gpu_refs_summary, "0:[]|1:[]");
    assert_eq!(frame.stats.gpu_total_light_refs, 0);
    frame.image.write_bmp(bmp_path)?;
    let after = json!({
        "stats": frame.stats,
        "assignment": frame.assignment,
        "bmp_readback": image_state(bmp_path),
    });
    println!("ISSUE1332_EDGE empty_lights before={before} after={after}");
    Ok(json!({ "before": before, "after": after }))
}

async fn invalid_target_size_edge(context: &GpuContext) -> Value {
    let settings = ForwardPlusGpuSettings {
        width: 0,
        height: 180,
        shading_z_slice: 0,
    };
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "expected_error_code": "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    });
    let error = render_forward_plus_lighting_gpu_with_context(
        context,
        settings,
        known_grid(4),
        &known_lights(),
    )
    .await
    .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1332_EDGE invalid_target_size before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn cluster_overflow_edge(context: &GpuContext) -> Value {
    let settings = ForwardPlusGpuSettings::default();
    let grid = ClusterGrid::new(1, 1, 1, Vec3::ZERO, Vec3::splat(2.0), 1);
    let lights = vec![
        RenderLight::point("overlap_a", Vec3::ONE, 2.0, Vec3::ONE, 1.0),
        RenderLight::point("overlap_b", Vec3::ONE, 2.0, Vec3::ONE, 1.0),
    ];
    let before = json!({
        "context_id": context.context_id(),
        "grid": grid,
        "light_count": lights.len(),
        "expected_error_code": "LITD-RENDER-CLUSTERED-LIGHTING-OVERFLOW",
    });
    let error = render_forward_plus_lighting_gpu_with_context(context, settings, grid, &lights)
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1332_EDGE cluster_overflow before={before} after={after}");
    json!({ "before": before, "after": after })
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
    println!("ISSUE1332_EDGE empty_backends before={before} after={after}");
    json!({ "before": before, "after": after })
}

fn known_grid(max_lights_per_cluster: usize) -> ClusterGrid {
    ClusterGrid::new(
        2,
        1,
        1,
        Vec3::new(-2.0, -1.0, 0.0),
        Vec3::new(2.0, 1.0, 2.0),
        max_lights_per_cluster,
    )
}

fn known_lights() -> Vec<RenderLight> {
    vec![
        RenderLight::directional("sun", Vec3::Y, Vec3::new(1.0, 0.82, 0.62), 2.0),
        RenderLight::point(
            "left_ember",
            Vec3::new(-1.0, 0.0, 1.0),
            0.25,
            Vec3::new(1.0, 0.25, 0.05),
            6.0,
        ),
        RenderLight::spot(
            "right_arcane",
            Vec3::new(1.0, 0.0, 1.0),
            -Vec3::Z,
            0.25,
            SpotCone::new(0.2, 0.5),
            Vec3::new(0.25, 0.75, 1.0),
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
