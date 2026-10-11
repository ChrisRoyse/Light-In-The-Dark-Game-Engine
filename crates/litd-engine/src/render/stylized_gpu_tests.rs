use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    render::{
        shader::ShaderSystemSettings,
        stylized::{
            StylizedGpuScene, StylizedPresetKind, StylizedValue, build_stylized_shader_library,
            default_material, render_stylized_materials_gpu_with_context,
        },
    },
};

#[test]
fn issue1336_stylized_gpu_dispatches_material_passes() {
    pollster::block_on(issue1336_fsv()).unwrap();
}

async fn issue1336_fsv() -> Result<(), EngineError> {
    let run = begin_issue_run("issue1336_stylized_gpu").unwrap();
    let root = run.run_dir();
    let output_bmp = root.join("stylized-effects-output.bmp");
    let max_wave_bmp = root.join("stylized-water-max-wave.bmp");
    let report_path = run.report_path().to_path_buf();

    let context = GpuContext::new_headless().await?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let scene = StylizedGpuScene::curated_defaults(&library, 384, 192)?;
    let before = json!({
        "context": context_state(&context),
        "library_preset_count": library.preset_count(),
        "total_parameter_count": library.total_parameter_count(),
        "preset_ids": library.preset_ids(),
        "expected_material_count": scene.materials.len(),
        "expected_fragment_entries": scene.materials.iter()
            .map(|material| material.preset.fragment_entry())
            .collect::<Vec<_>>(),
    });

    let frame = render_stylized_materials_gpu_with_context(&context, &library, &scene).await?;
    assert_eq!(frame.stats.gpu_context_id, context.context_id());
    assert_eq!(frame.stats.material_count, 6);
    assert_eq!(frame.stats.draw_calls, 6);
    assert_eq!(frame.stats.library_preset_count, 6);
    assert_eq!(frame.stats.total_parameter_count, 21);
    assert_eq!(frame.stats.preset_readbacks.len(), 6);
    assert!(frame.stats.output_metrics.nonblack_pixel_ratio > 0.96);
    assert!(frame.stats.output_metrics.hot_pixel_ratio > 0.12);
    assert!(frame.stats.output_metrics.cyan_pixel_ratio > 0.10);
    assert!(frame.stats.output_metrics.edge_contrast > 0.0005);
    frame.image.write_bmp(&output_bmp)?;

    let edges = json!({
        "invalid_wave_strength": invalid_wave_strength_edge(&context, &library).await,
        "empty_materials": empty_materials_edge(&context, &library).await,
        "max_wave_strength": max_wave_strength_edge(&context, &library, &max_wave_bmp).await?,
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1336,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "output_bmp": output_bmp.display().to_string(),
            "max_wave_bmp": max_wave_bmp.display().to_string(),
            "reference_images_reviewed": [
                "docs/readme/ED08-vfx-editor.png",
                "docs/readme/GP04-mage-spell.png",
                "docs/readme/GP07-boss.png"
            ],
            "verified_fields": [
                "executed_fragment_entries",
                "per_preset_parameter_counts",
                "output_bmp_sha256",
                "hot_pixel_ratio",
                "cyan_pixel_ratio",
                "invalid_wave_strength_error"
            ]
        },
        "happy_path": {
            "before": before,
            "after": {
                "context": context_state(&context),
                "stats": frame.stats,
                "output_bmp_readback": image_state(&output_bmp),
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    let report_bytes = report_readback.bytes;
    let report_json = report_readback.json;
    assert_eq!(report_json["issue"], 1336);
    assert_eq!(report_json["happy_path"]["after"]["stats"]["draw_calls"], 6);
    assert_eq!(
        report_json["edges"]["invalid_wave_strength"]["after"]["error_code"],
        "LITD-RENDER-STYLIZED-SHADER-INVALID-CONFIG"
    );
    assert_eq!(
        report_json["edges"]["empty_materials"]["after"]["error_code"],
        "LITD-RENDER-STYLIZED-SHADER-INVALID-CONFIG"
    );
    assert_eq!(
        report_json["edges"]["max_wave_strength"]["after"]["stats"]["draw_calls"],
        1
    );
    println!(
        "ISSUE1336_FSV_REPORT_READBACK path={} bytes={} sha256={} draws={} presets={} params={} hot_ratio={} cyan_ratio={} output_bmp_sha256={} invalid_wave_code={} empty_code={} max_wave_bmp_sha256={}",
        report_path.display(),
        report_bytes.len(),
        report_readback.sha256,
        report_json["happy_path"]["after"]["stats"]["draw_calls"],
        report_json["happy_path"]["after"]["stats"]["library_preset_count"],
        report_json["happy_path"]["after"]["stats"]["total_parameter_count"],
        report_json["happy_path"]["after"]["stats"]["output_metrics"]["hot_pixel_ratio"],
        report_json["happy_path"]["after"]["stats"]["output_metrics"]["cyan_pixel_ratio"],
        report_json["happy_path"]["after"]["output_bmp_readback"]["sha256"],
        report_json["edges"]["invalid_wave_strength"]["after"]["error_code"],
        report_json["edges"]["empty_materials"]["after"]["error_code"],
        report_json["edges"]["max_wave_strength"]["after"]["bmp_readback"]["sha256"],
    );
    Ok(())
}

async fn invalid_wave_strength_edge(
    context: &GpuContext,
    library: &super::StylizedShaderLibrary,
) -> Value {
    let water = &library.find(StylizedPresetKind::Water).unwrap().preset;
    let mut material = default_material(water);
    material
        .values
        .insert("wave_strength".to_string(), StylizedValue::Scalar(9.0));
    let scene = StylizedGpuScene::from_materials(384, 192, vec![material]);
    let before = json!({
        "context_id": context.context_id(),
        "preset": "water",
        "wave_strength": 9.0,
        "expected_error_code": "LITD-RENDER-STYLIZED-SHADER-INVALID-CONFIG",
    });
    let error = render_stylized_materials_gpu_with_context(context, library, &scene)
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1336_EDGE invalid_wave_strength before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn empty_materials_edge(
    context: &GpuContext,
    library: &super::StylizedShaderLibrary,
) -> Value {
    let scene = StylizedGpuScene::from_materials(384, 192, Vec::new());
    let before = json!({
        "context_id": context.context_id(),
        "material_count": 0,
        "expected_error_code": "LITD-RENDER-STYLIZED-SHADER-INVALID-CONFIG",
    });
    let error = render_stylized_materials_gpu_with_context(context, library, &scene)
        .await
        .unwrap_err();
    let after = error_report(&error);
    println!("ISSUE1336_EDGE empty_materials before={before} after={after}");
    json!({ "before": before, "after": after })
}

async fn max_wave_strength_edge(
    context: &GpuContext,
    library: &super::StylizedShaderLibrary,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let water = &library.find(StylizedPresetKind::Water).unwrap().preset;
    let mut material = default_material(water);
    material
        .values
        .insert("wave_strength".to_string(), StylizedValue::Scalar(2.0));
    let scene = StylizedGpuScene::from_materials(192, 128, vec![material]);
    let before = json!({
        "context_id": context.context_id(),
        "preset": "water",
        "wave_strength": 2.0,
        "expected_draw_calls": 1,
    });
    let frame = render_stylized_materials_gpu_with_context(context, library, &scene).await?;
    assert_eq!(frame.stats.draw_calls, 1);
    assert_eq!(frame.stats.preset_readbacks[0].parameter_count, 4);
    assert!(frame.stats.output_metrics.cyan_pixel_ratio > 0.10);
    frame.image.write_bmp(bmp_path)?;
    let after = json!({
        "stats": frame.stats,
        "bmp_readback": image_state(bmp_path),
    });
    println!("ISSUE1336_EDGE max_wave_strength before={before} after={after}");
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
