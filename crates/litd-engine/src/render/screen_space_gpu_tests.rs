use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    render::screen_space::{
        AntiAliasingSettings, ScreenSpaceGpuInputImage, ScreenSpaceSettings, SsrSettings,
        build_screen_space_plan, render_screen_space_gpu_with_context,
    },
};

#[test]
fn issue1335_screen_space_gpu_dispatches_planned_passes() {
    pollster::block_on(issue1335_fsv()).unwrap();
}

async fn issue1335_fsv() -> Result<(), EngineError> {
    let run = begin_issue_run("issue1335_screen_space_gpu").unwrap();
    let root = run.run_dir();
    let input_bmp = root.join("screen-space-input.bmp");
    let ssao_bmp = root.join("screen-space-ssao.bmp");
    let taa_bmp = root.join("screen-space-taa.bmp");
    let output_bmp = root.join("screen-space-output.bmp");
    let ssr_off_bmp = root.join("screen-space-ssr-off.bmp");
    let taa_off_bmp = root.join("screen-space-taa-off.bmp");
    let report_path = run.report_path().to_path_buf();

    let context = GpuContext::new_headless().await?;
    let settings = screen_settings(true, true);
    let input = ScreenSpaceGpuInputImage::synthetic_scene(256, 144)?;
    input.write_color_bmp(&input_bmp)?;
    let plan = build_screen_space_plan(settings, input.width, input.height)?;
    let before = json!({
        "context": context_state(&context),
        "settings": settings,
        "input": input_state(&input),
        "expected_plan_pass_count": plan.passes.len(),
        "expected_msaa_samples": settings.anti_aliasing.msaa_samples,
    });

    let frame = render_screen_space_gpu_with_context(&context, settings, &input, 0).await?;
    assert_eq!(frame.stats.gpu_context_id, context.context_id());
    assert_eq!(frame.stats.plan_pass_count, plan.passes.len());
    assert_eq!(frame.stats.draw_calls as usize, plan.passes.len());
    assert_eq!(frame.stats.msaa_samples, 4);
    assert_eq!(frame.stats.jitter_sequence_len, 8);
    assert_eq!(frame.stats.jitter_sample_index, 1);
    assert_eq!(frame.stats.msaa_resolve_pass_count, 1);
    assert_eq!(frame.stats.ssao_gather_pass_count, 1);
    assert_eq!(frame.stats.ssao_blur_pass_count, 1);
    assert_eq!(frame.stats.taa_resolve_pass_count, 1);
    assert_eq!(frame.stats.fxaa_pass_count, 1);
    assert_eq!(frame.stats.stylized_ssr_pass_count, 1);
    assert!(frame.ssao_image.is_some());
    assert!(frame.taa_image.is_some());
    assert!(frame.stats.output_metrics.average_luma > 0.08);
    assert!(frame.stats.output_metrics.edge_contrast > 0.001);
    assert!(frame.stats.output_metrics.edge_contrast < 0.22);
    assert_eq!(frame.stats.output_metrics.alpha_opaque_ratio, 1.0);
    frame.ssao_image.as_ref().unwrap().write_bmp(&ssao_bmp)?;
    frame.taa_image.as_ref().unwrap().write_bmp(&taa_bmp)?;
    frame.output_image.write_bmp(&output_bmp)?;

    let edges = json!({
        "invalid_msaa": invalid_msaa_edge(&context, &input).await,
        "ssr_off": ssr_off_edge(&context, &input, &ssr_off_bmp).await?,
        "taa_off": taa_off_edge(&context, &input, &taa_off_bmp).await?,
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1335,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "input_bmp": input_bmp.display().to_string(),
            "ssao_bmp": ssao_bmp.display().to_string(),
            "taa_bmp": taa_bmp.display().to_string(),
            "output_bmp": output_bmp.display().to_string(),
            "verified_fields": [
                "plan_pass_count",
                "msaa_samples",
                "jitter_sample_index",
                "ssao_bmp_sha256",
                "taa_bmp_sha256",
                "output_edge_contrast"
            ]
        },
        "happy_path": {
            "before": before,
            "after": {
                "context": context_state(&context),
                "stats": frame.stats,
                "input_bmp_readback": image_state(&input_bmp),
                "ssao_bmp_readback": image_state(&ssao_bmp),
                "taa_bmp_readback": image_state(&taa_bmp),
                "output_bmp_readback": image_state(&output_bmp),
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_bytes = report_readback.bytes;
    let report_json = report_readback.json;
    assert_eq!(report_json["issue"], 1335);
    assert_eq!(
        report_json["happy_path"]["after"]["stats"]["plan_pass_count"],
        7
    );
    assert_eq!(
        report_json["happy_path"]["after"]["stats"]["msaa_samples"],
        4
    );
    assert_eq!(
        report_json["edges"]["invalid_msaa"]["after"]["error_code"],
        "LITD-RENDER-SCREEN-SPACE-INVALID-CONFIG"
    );
    assert_eq!(
        report_json["edges"]["ssr_off"]["after"]["stats"]["stylized_ssr_pass_count"],
        0
    );
    assert_eq!(
        report_json["edges"]["taa_off"]["after"]["stats"]["taa_resolve_pass_count"],
        0
    );
    println!(
        "ISSUE1335_FSV_REPORT_READBACK path={} bytes={} sha256={} passes={} msaa={} jitter={} edge_contrast={} ssao_bmp_sha256={} taa_bmp_sha256={} output_bmp_sha256={} invalid_msaa_code={} ssr_off_passes={} taa_off_passes={}",
        report_path.display(),
        report_bytes.len(),
        report_readback.sha256,
        report_json["happy_path"]["after"]["stats"]["plan_pass_count"],
        report_json["happy_path"]["after"]["stats"]["msaa_samples"],
        report_json["happy_path"]["after"]["stats"]["jitter_sample_index"],
        report_json["happy_path"]["after"]["stats"]["output_metrics"]["edge_contrast"],
        report_json["happy_path"]["after"]["ssao_bmp_readback"]["sha256"],
        report_json["happy_path"]["after"]["taa_bmp_readback"]["sha256"],
        report_json["happy_path"]["after"]["output_bmp_readback"]["sha256"],
        report_json["edges"]["invalid_msaa"]["after"]["error_code"],
        report_json["edges"]["ssr_off"]["after"]["stats"]["stylized_ssr_pass_count"],
        report_json["edges"]["taa_off"]["after"]["stats"]["taa_resolve_pass_count"],
    );
    Ok(())
}

async fn invalid_msaa_edge(context: &GpuContext, input: &ScreenSpaceGpuInputImage) -> Value {
    let mut settings = screen_settings(true, true);
    settings.anti_aliasing.msaa_samples = 3;
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "expected_error_code": "LITD-RENDER-SCREEN-SPACE-INVALID-CONFIG",
    });
    let error = render_screen_space_gpu_with_context(context, settings, input, 0)
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1335_EDGE invalid_msaa before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn ssr_off_edge(
    context: &GpuContext,
    input: &ScreenSpaceGpuInputImage,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let settings = screen_settings(false, true);
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "expected_stylized_ssr_pass_count": 0,
    });
    let frame = render_screen_space_gpu_with_context(context, settings, input, 0).await?;
    assert_eq!(frame.stats.stylized_ssr_pass_count, 0);
    assert_eq!(frame.stats.taa_resolve_pass_count, 1);
    assert!(frame.stats.output_metrics.edge_contrast > 0.001);
    frame.output_image.write_bmp(bmp_path)?;
    let after = json!({
        "stats": frame.stats,
        "bmp_readback": image_state(bmp_path),
    });
    println!("ISSUE1335_EDGE ssr_off before={before} after={after}");
    Ok(json!({ "before": before, "after": after }))
}

async fn taa_off_edge(
    context: &GpuContext,
    input: &ScreenSpaceGpuInputImage,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let settings = screen_settings(true, false);
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "expected_taa_resolve_pass_count": 0,
    });
    let frame = render_screen_space_gpu_with_context(context, settings, input, 0).await?;
    assert_eq!(frame.stats.taa_resolve_pass_count, 0);
    assert!(frame.taa_image.is_none());
    assert_eq!(frame.stats.motion_vectors_pass_count, 0);
    assert!(frame.stats.output_metrics.edge_contrast > 0.001);
    frame.output_image.write_bmp(bmp_path)?;
    let after = json!({
        "stats": frame.stats,
        "taa_image_present": frame.taa_image.is_some(),
        "bmp_readback": image_state(bmp_path),
    });
    println!("ISSUE1335_EDGE taa_off before={before} after={after}");
    Ok(json!({ "before": before, "after": after }))
}

fn screen_settings(ssr_enabled: bool, taa_enabled: bool) -> ScreenSpaceSettings {
    ScreenSpaceSettings {
        anti_aliasing: AntiAliasingSettings {
            msaa_samples: 4,
            fxaa_enabled: true,
            taa_enabled,
            jitter_sequence_len: 8,
            taa_feedback: 0.82,
            motion_rejection: 0.70,
        },
        ssr: SsrSettings {
            enabled: ssr_enabled,
            max_steps: 48,
            thickness: 0.12,
            intensity: 0.38,
        },
        ..ScreenSpaceSettings::default()
    }
}

fn input_state(input: &ScreenSpaceGpuInputImage) -> Value {
    json!({
        "width": input.width,
        "height": input.height,
        "color_pixels": input.color_pixels().len(),
        "history_pixels": input.history_pixels().len(),
        "depth_pixels": input.depth_pixels().len(),
        "motion_pixels": input.motion_pixels().len(),
    })
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
