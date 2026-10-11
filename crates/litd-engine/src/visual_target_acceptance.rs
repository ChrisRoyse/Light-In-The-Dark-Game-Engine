//! Supporting visual-target acceptance harness for #1147.

use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::{FsvReportReadback, begin_issue_run},
    render::reference::{
        ReferenceSceneComparison, ReferenceSceneImage, ReferenceSceneSettings,
        build_reference_scene_plan, compare_reference_scene_image, render_reference_scene_gpu,
        render_reference_scene_image,
    },
};

pub const ISSUE1147_VISUAL_TARGET_SCHEMA: &str =
    "litd.engine.issue1147.visual_target_acceptance.v1";
pub const ISSUE1147_VISUAL_TARGET_SLUG: &str = "issue1147_visual_target_acceptance";

const CANONICAL_WIDTH: u32 = 960;
const CANONICAL_HEIGHT: u32 = 540;
// Composition/quality references ONLY — never a HUD/behavior spec.
// ED12 is RUNTIME_CLEAN after #1537 (no omniscient navigation).
const README_REFERENCES: [&str; 4] = [
    "docs/readme/ED11-world-view.png",
    "docs/readme/ED12-play-topdown.png",
    "docs/readme/GP03-combat.png",
    "docs/readme/GP09-inventory.png",
];

pub async fn run_issue1147_visual_target_harness() -> Result<FsvReportReadback, EngineError> {
    let run = begin_issue_run(ISSUE1147_VISUAL_TARGET_SLUG)
        .map_err(|error| harness_error("fsv.begin_issue_run", error))?;
    let cpu_bmp = run
        .artifact_path("reference-cpu-960x540.bmp")
        .map_err(|error| harness_error("fsv.cpu_bmp_path", error))?;
    let gpu_bmp = run
        .artifact_path("reference-gpu-960x540.bmp")
        .map_err(|error| harness_error("fsv.gpu_bmp_path", error))?;
    let settings = ReferenceSceneSettings {
        width: CANONICAL_WIDTH,
        height: CANONICAL_HEIGHT,
    };
    let readme = README_REFERENCES
        .iter()
        .map(|path| readme_reference_readback(path))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = build_reference_scene_plan(settings)?;
    let cpu = cpu_visual_probe(&plan, &cpu_bmp)?;
    let gpu = gpu_visual_probe(settings, &gpu_bmp).await?;
    let edges = edge_proofs().await?;
    let report = json!({
        "schema": ISSUE1147_VISUAL_TARGET_SCHEMA,
        "schema_version": 1,
        "issue": 1147,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "cpu_bmp_path": cpu_bmp.display().to_string(),
            "gpu_bmp_path": gpu_bmp.display().to_string(),
            "manual_gate": "supporting automated evidence only; manual Synapse/HWND screenshot and pixel inspection is still required"
        },
        "research_basis": [
            "visual comparison should persist concrete screenshots/artifacts and compare against references",
            "glTF PBR metallic-roughness is the material baseline for real-time stylized PBR",
            "graphics frame tools inspect frame events down to the draw call/pixel"
        ],
        "reference_images_reviewed": README_REFERENCES,
        "readme_reference_readback": readme,
        "happy_path": {
            "settings": settings,
            "plan": {
                "pbr_material_count": plan.pbr_material_count(),
                "light_count": plan.lights.len(),
                "shadow_cascades": plan.shadow_plan.directional_cascade_count(),
                "post_passes": plan.post_plan.passes.len(),
                "screen_space_passes": plan.screen_space_plan.passes.len(),
                "stylized_preset_count": plan.stylized_library.preset_count(),
                "visible_batch_instances": plan.batch_plan.visible_instance_ids.len(),
                "ui_passes": plan.ui_plan.passes.len(),
                "reference_images": plan.reference_image_list(),
            },
            "cpu_reference": cpu,
            "gpu_reference": gpu,
        },
        "edges": edges,
    });
    run.write_report(&report)
        .map_err(|error| harness_error("fsv.write_report", error))
}

fn cpu_visual_probe(
    plan: &crate::render::reference::ReferenceScenePlan,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let image = render_reference_scene_image(plan)?;
    let comparison = compare_reference_scene_image(&image);
    if !comparison.pass {
        return Err(EngineError::ReferenceSceneInvalidConfig {
            field: "visual_target.cpu_bands".to_string(),
            reason: comparison.band_summary(),
        });
    }
    image.write_bmp(bmp_path)?;
    Ok(json!({
        "comparison": comparison_json(&comparison),
        "bmp_readback": file_readback(bmp_path)?,
        "pixel_count": image.pixels().len(),
    }))
}

async fn gpu_visual_probe(
    settings: ReferenceSceneSettings,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let frame = render_reference_scene_gpu(settings).await?;
    if !frame.comparison.pass {
        return Err(EngineError::ReferenceSceneInvalidConfig {
            field: "visual_target.gpu_bands".to_string(),
            reason: frame.comparison.band_summary(),
        });
    }
    frame.image.write_bmp(bmp_path)?;
    Ok(json!({
        "gpu_context_id": frame.gpu_context_id,
        "adapter_name": frame.adapter_name,
        "backend": frame.backend,
        "texture_format": frame.texture_format,
        "mesh_count": frame.mesh_count,
        "instance_count": frame.instance_count,
        "draw_calls": frame.draw_calls,
        "readback_bytes": frame.readback_bytes,
        "row_pitch": frame.row_pitch,
        "comparison": comparison_json(&frame.comparison),
        "bmp_readback": file_readback(bmp_path)?,
    }))
}

async fn edge_proofs() -> Result<Value, EngineError> {
    Ok(json!({
        "invalid_cpu_size": invalid_cpu_size_edge(),
        "invalid_gpu_size": invalid_gpu_size_edge().await,
        "blank_visual_target": blank_visual_target_edge()?,
        "missing_reference_image": missing_reference_image_edge(),
    }))
}

fn invalid_cpu_size_edge() -> Value {
    let error = build_reference_scene_plan(ReferenceSceneSettings {
        width: 0,
        height: CANONICAL_HEIGHT,
    })
    .unwrap_err();
    json!({
        "before": { "width": 0, "height": CANONICAL_HEIGHT },
        "after": engine_error_report(&error)
    })
}

async fn invalid_gpu_size_edge() -> Value {
    let error = render_reference_scene_gpu(ReferenceSceneSettings {
        width: 0,
        height: CANONICAL_HEIGHT,
    })
    .await
    .unwrap_err();
    json!({
        "before": { "width": 0, "height": CANONICAL_HEIGHT },
        "after": engine_error_report(&error)
    })
}

fn blank_visual_target_edge() -> Result<Value, EngineError> {
    let image = ReferenceSceneImage::from_rgba_pixels(
        320,
        180,
        vec![[0, 0, 0, 255]; 320_usize * 180_usize],
    )?;
    let comparison = compare_reference_scene_image(&image);
    Ok(json!({
        "before": { "width": 320, "height": 180, "fill": [0, 0, 0, 255] },
        "after": comparison_json(&comparison),
    }))
}

fn missing_reference_image_edge() -> Value {
    let path = "docs/readme/DOES-NOT-EXIST.png";
    let error = readme_reference_readback(path).unwrap_err();
    json!({
        "before": { "path": path },
        "after": engine_error_report(&error)
    })
}

fn readme_reference_readback(relative_path: &str) -> Result<Value, EngineError> {
    let path = repo_root().join(relative_path);
    let bytes = fs::read(&path).map_err(|error| harness_error("readme_reference.read", error))?;
    let dimensions = png_dimensions(&bytes).ok_or_else(|| EngineError::SerializationFailed {
        format: "image",
        reason: format!("{} is not a readable PNG", path.display()),
    })?;
    Ok(json!({
        "path": path.display().to_string(),
        "relative_path": relative_path,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
        "format": dimensions.0,
        "width": dimensions.1,
        "height": dimensions.2,
    }))
}

fn comparison_json(comparison: &ReferenceSceneComparison) -> Value {
    json!({
        "pass": comparison.pass,
        "checksum": comparison.checksum,
        "metrics": {
            "average_luma": comparison.metrics.average_luma,
            "warm_highlight_ratio": comparison.metrics.warm_highlight_ratio,
            "cool_shadow_ratio": comparison.metrics.cool_shadow_ratio,
            "emissive_ratio": comparison.metrics.emissive_ratio,
            "edge_contrast": comparison.metrics.edge_contrast,
            "hud_gold_ratio": comparison.metrics.hud_gold_ratio,
        },
        "bands": comparison.bands.iter().map(|band| json!({
            "name": band.name,
            "actual": band.actual,
            "min": band.min,
            "max": band.max,
            "pass": band.pass,
        })).collect::<Vec<_>>(),
    })
}

fn file_readback(path: &Path) -> Result<Value, EngineError> {
    let bytes = fs::read(path).map_err(|error| harness_error("file.readback", error))?;
    Ok(json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn png_dimensions(bytes: &[u8]) -> Option<(&'static str, u32, u32)> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[0..8] != PNG_SIGNATURE {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some(("png", width, height))
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine_error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "error_code": report.code,
        "message": report.message,
        "remediation": report.remediation,
        "source_error": report.source_error,
    })
}

fn harness_error(op: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::SerializationFailed {
        format: "issue1147-visual-target",
        reason: format!("{op}: {error}"),
    }
}
