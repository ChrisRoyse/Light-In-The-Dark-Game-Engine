use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    math::Vec3,
    render::post::{
        BloomSettings, ColorGrade, HdrFormat, PostGpuInputImage, PostProcessSettings,
        TonemapOperator, build_post_process_plan, render_post_process_gpu_with_context,
    },
};

#[test]
fn issue1334_post_gpu_dispatches_tonemap_bloom_and_grade() {
    pollster::block_on(issue1334_fsv()).unwrap();
}

async fn issue1334_fsv() -> Result<(), EngineError> {
    let run = begin_issue_run("issue1334_post_gpu").unwrap();
    let root = run.run_dir();
    let input_bmp = root.join("post-hdr-input-visualization.bmp");
    let happy_bmp = root.join("post-happy-output.bmp");
    let bloom_off_bmp = root.join("post-bloom-disabled-output.bmp");
    let report_path = run.report_path().to_path_buf();
    let context = GpuContext::new_headless().await?;
    let settings = post_settings(true);
    let input = PostGpuInputImage::synthetic_hdr_scene(320, 180)?;
    input.write_bmp(&input_bmp)?;
    let plan = build_post_process_plan(settings, input.width, input.height)?;
    let before = json!({
        "context": context_state(&context),
        "settings": settings,
        "input": input_state(&input, settings),
        "expected_plan_pass_count": plan.passes.len(),
        "expected_downsample_passes": plan.downsample_pass_count(),
        "expected_upsample_passes": plan.upsample_pass_count(),
    });

    let frame = render_post_process_gpu_with_context(&context, settings, &input).await?;
    assert_eq!(frame.stats.gpu_context_id, context.context_id());
    assert_eq!(frame.stats.source_texture_format, "Rgba16Float");
    assert_eq!(frame.stats.bloom_texture_format, "Rgba16Float");
    assert_eq!(frame.stats.target_texture_format, "Rgba8Unorm");
    assert_eq!(frame.stats.plan_pass_count, plan.passes.len());
    assert_eq!(frame.stats.draw_calls as usize, plan.passes.len());
    assert_eq!(frame.stats.executed_pass_labels.len(), plan.passes.len());
    assert!(
        frame
            .stats
            .executed_pass_labels
            .iter()
            .any(|label| label.contains("downsample-mip-4"))
    );
    assert!(
        frame
            .stats
            .executed_pass_labels
            .iter()
            .any(|label| label.contains("upsample-mip-0"))
    );
    assert_eq!(
        frame.stats.bloom_downsample_pass_count,
        plan.downsample_pass_count()
    );
    assert_eq!(
        frame.stats.bloom_upsample_pass_count,
        plan.upsample_pass_count()
    );
    assert_eq!(frame.stats.bloom_prefilter_pass_count, 1);
    assert_eq!(frame.stats.composite_pass_count, 1);
    assert!(frame.stats.input_max_hdr_value > 3.5);
    assert!(frame.stats.input_bloom_candidate_ratio > 0.01);
    assert!(frame.stats.metrics.average_luma > 0.08);
    assert!(frame.stats.metrics.average_luma < 0.95);
    assert!(frame.stats.metrics.hot_pixel_ratio > 0.005);
    assert!(frame.stats.metrics.blue_pixel_ratio > 0.005);
    assert_eq!(frame.stats.metrics.alpha_opaque_ratio, 1.0);
    frame.image.write_bmp(&happy_bmp)?;
    let happy_state = image_state(&happy_bmp);

    let edges = json!({
        "bloom_disabled": bloom_disabled_edge(&context, &input, &bloom_off_bmp).await?,
        "invalid_lut_size": invalid_lut_size_edge(&context, &input).await,
        "nan_hdr_input": nan_hdr_input_edge(),
        "empty_backends_no_adapter": empty_backends_edge().await,
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1334,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "hdr_input_visualization_bmp": input_bmp.display().to_string(),
            "happy_output_bmp": happy_bmp.display().to_string(),
            "bloom_disabled_output_bmp": bloom_off_bmp.display().to_string(),
            "verified_fields": [
                "plan_pass_count",
                "bloom_downsample_pass_count",
                "bloom_upsample_pass_count",
                "output_bmp_sha256",
                "average_luma",
                "input_max_hdr_value"
            ]
        },
        "happy_path": {
            "before": before,
            "after": {
                "context": context_state(&context),
                "stats": frame.stats,
                "input_bmp_readback": image_state(&input_bmp),
                "output_bmp_readback": happy_state,
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_bytes = report_readback.bytes;
    let report_json = report_readback.json;

    assert_eq!(report_json["issue"], 1334);
    assert_eq!(
        report_json["happy_path"]["after"]["stats"]["plan_pass_count"],
        10
    );
    assert_eq!(
        report_json["edges"]["bloom_disabled"]["after"]["stats"]["plan_pass_count"],
        1
    );
    assert_eq!(
        report_json["edges"]["invalid_lut_size"]["after"]["error_code"],
        "LITD-RENDER-POST-INVALID-CONFIG"
    );
    assert_eq!(
        report_json["edges"]["nan_hdr_input"]["after"]["error_code"],
        "LITD-RENDER-POST-INVALID-CONFIG"
    );
    println!(
        "ISSUE1334_FSV_REPORT_READBACK path={} bytes={} sha256={} passes={} luma={} hot_ratio={} blue_ratio={} output_bmp_sha256={} bloom_off_passes={} invalid_lut_code={}",
        report_path.display(),
        report_bytes.len(),
        report_readback.sha256,
        report_json["happy_path"]["after"]["stats"]["plan_pass_count"],
        report_json["happy_path"]["after"]["stats"]["metrics"]["average_luma"],
        report_json["happy_path"]["after"]["stats"]["metrics"]["hot_pixel_ratio"],
        report_json["happy_path"]["after"]["stats"]["metrics"]["blue_pixel_ratio"],
        report_json["happy_path"]["after"]["output_bmp_readback"]["sha256"],
        report_json["edges"]["bloom_disabled"]["after"]["stats"]["plan_pass_count"],
        report_json["edges"]["invalid_lut_size"]["after"]["error_code"],
    );
    Ok(())
}

async fn bloom_disabled_edge(
    context: &GpuContext,
    input: &PostGpuInputImage,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let settings = post_settings(false);
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "input": input_state(input, settings),
        "expected_plan_pass_count": 1,
        "expected_bloom_downsample_passes": 0,
        "expected_bloom_upsample_passes": 0,
    });
    let frame = render_post_process_gpu_with_context(context, settings, input).await?;
    assert_eq!(frame.stats.plan_pass_count, 1);
    assert_eq!(frame.stats.bloom_downsample_pass_count, 0);
    assert_eq!(frame.stats.bloom_upsample_pass_count, 0);
    assert_eq!(frame.stats.draw_calls, 1);
    assert_eq!(
        frame.stats.executed_pass_labels,
        vec!["litd-post-composite-pass"]
    );
    assert!(!frame.stats.bloom_enabled);
    assert!(frame.stats.metrics.average_luma > 0.05);
    frame.image.write_bmp(bmp_path)?;
    let after = json!({
        "stats": frame.stats,
        "bmp_readback": image_state(bmp_path),
    });
    println!("ISSUE1334_EDGE bloom_disabled before={before} after={after}");
    Ok(json!({ "before": before, "after": after }))
}

async fn invalid_lut_size_edge(context: &GpuContext, input: &PostGpuInputImage) -> Value {
    let mut settings = post_settings(true);
    settings.color_grade.lut_size = 1;
    let before = json!({
        "context_id": context.context_id(),
        "settings": settings,
        "expected_error_code": "LITD-RENDER-POST-INVALID-CONFIG",
    });
    let error = render_post_process_gpu_with_context(context, settings, input)
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1334_EDGE invalid_lut_size before={before} after={after}");
    json!({ "before": before, "after": after })
}

fn nan_hdr_input_edge() -> Value {
    let before = json!({
        "width": 2,
        "height": 1,
        "pixel_values": "first red component is NaN",
        "expected_error_code": "LITD-RENDER-POST-INVALID-CONFIG",
    });
    let error = PostGpuInputImage::from_hdr_pixels(
        2,
        1,
        vec![[f32::NAN, 0.0, 0.0, 1.0], [0.25, 0.5, 0.75, 1.0]],
    )
    .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1334_EDGE nan_hdr_input before={before} after={after}");
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
    println!("ISSUE1334_EDGE empty_backends before={before} after={after}");
    json!({ "before": before, "after": after })
}

fn post_settings(bloom_enabled: bool) -> PostProcessSettings {
    PostProcessSettings {
        hdr_format: HdrFormat::Rgba16Float,
        exposure_ev: 0.25,
        tonemap: TonemapOperator::Aces,
        bloom: BloomSettings {
            enabled: bloom_enabled,
            threshold: 1.0,
            knee: 0.45,
            intensity: 0.30,
            mip_count: 5,
        },
        color_grade: ColorGrade {
            lut_size: 32,
            lut_strength: 0.72,
            saturation: 1.18,
            contrast: 1.10,
            lift: Vec3::new(0.012, 0.008, 0.0),
            gamma: Vec3::new(1.0, 0.98, 1.02),
            gain: Vec3::new(1.05, 1.02, 1.0),
        },
        output_gamma: 2.2,
    }
}

fn input_state(input: &PostGpuInputImage, settings: PostProcessSettings) -> Value {
    let mut max_hdr = 0.0_f32;
    let mut bloom_candidates = 0_u32;
    for pixel in input.pixels() {
        let rgb_max = pixel[0].max(pixel[1]).max(pixel[2]);
        max_hdr = max_hdr.max(rgb_max);
        if rgb_max > settings.bloom.threshold {
            bloom_candidates += 1;
        }
    }
    json!({
        "width": input.width,
        "height": input.height,
        "pixel_count": input.pixels().len(),
        "max_hdr_value": max_hdr,
        "bloom_candidate_ratio": bloom_candidates as f32 / input.pixels().len().max(1) as f32,
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
