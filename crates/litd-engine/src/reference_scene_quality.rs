//! Resolution-stable reference-scene quality evidence for #1442.

use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::{FsvReportReadback, begin_issue_run},
    render::reference::{
        ReferenceSceneComparison, ReferenceSceneImage, ReferenceSceneSettings,
        render_reference_scene_gpu,
    },
};

pub const ISSUE1442_REFERENCE_QUALITY_SCHEMA: &str =
    "litd.engine.issue1442.reference_scene_quality.v1";
pub const ISSUE1442_REFERENCE_QUALITY_SLUG: &str = "issue1442_reference_scene_quality";

const CANONICAL_WIDTH: u32 = 960;
const CANONICAL_HEIGHT: u32 = 540;
const FULL_HD_WIDTH: u32 = 1920;
const FULL_HD_HEIGHT: u32 = 1080;
const EDGE_STABILITY_LIMIT: f32 = 0.008;

pub async fn run_issue1442_reference_quality_harness() -> Result<FsvReportReadback, EngineError> {
    let run = begin_issue_run(ISSUE1442_REFERENCE_QUALITY_SLUG)
        .map_err(|error| harness_error("fsv.begin_issue_run", error))?;
    let canonical_bmp = run
        .artifact_path("reference-gpu-960x540.bmp")
        .map_err(|error| harness_error("fsv.canonical_bmp_path", error))?;
    let full_hd_bmp = run
        .artifact_path("reference-gpu-1920x1080.bmp")
        .map_err(|error| harness_error("fsv.full_hd_bmp_path", error))?;

    let canonical = gpu_resolution_probe(
        "gpu_960x540",
        CANONICAL_WIDTH,
        CANONICAL_HEIGHT,
        &canonical_bmp,
    )
    .await?;
    let full_hd =
        gpu_resolution_probe("gpu_1920x1080", FULL_HD_WIDTH, FULL_HD_HEIGHT, &full_hd_bmp).await?;
    let stability = edge_stability(&canonical, &full_hd)?;
    let edges = edge_proofs()?;

    let report = json!({
        "schema": ISSUE1442_REFERENCE_QUALITY_SCHEMA,
        "schema_version": 1,
        "issue": 1442,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "canonical_gpu_bmp_path": canonical_bmp.display().to_string(),
            "full_hd_gpu_bmp_path": full_hd_bmp.display().to_string(),
            "manual_gate": "supporting automated evidence only; manual Synapse/HWND screenshot inspection is still required"
        },
        "metric_contract": {
            "edge_contrast": "raw adjacent-pixel luma edge energy normalized to the 540px canonical visual-target height",
            "edge_stability_limit": EDGE_STABILITY_LIMIT,
        },
        "reference_images_reviewed": [
            "docs/readme/S06-infernal-deeps.png",
            "docs/readme/S14-tavern-rp.png",
            "docs/readme/S20-living-world.png"
        ],
        "happy_path": {
            "gpu_960x540": canonical,
            "gpu_1920x1080": full_hd,
            "edge_stability": stability,
        },
        "edges": edges,
    });
    run.write_report(&report)
        .map_err(|error| harness_error("fsv.write_report", error))
}

async fn gpu_resolution_probe(
    label: &'static str,
    width: u32,
    height: u32,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let settings = ReferenceSceneSettings { width, height };
    let frame = render_reference_scene_gpu(settings).await?;
    if !frame.comparison.pass {
        return Err(EngineError::ReferenceSceneInvalidConfig {
            field: format!("reference_scene_quality.{label}.gpu_bands"),
            reason: frame.comparison.band_summary(),
        });
    }
    frame.image.write_bmp(bmp_path)?;
    Ok(json!({
        "before": {
            "width": width,
            "height": height,
            "metric_contract": "same reference scene and visual bands must pass at this output resolution",
        },
        "after": {
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
        }
    }))
}

fn edge_stability(canonical: &Value, full_hd: &Value) -> Result<Value, EngineError> {
    let canonical_edge = edge_metric(canonical)?;
    let full_hd_edge = edge_metric(full_hd)?;
    let delta = (canonical_edge - full_hd_edge).abs();
    let pass = delta <= EDGE_STABILITY_LIMIT;
    if !pass {
        return Err(EngineError::ReferenceSceneInvalidConfig {
            field: "reference_scene_quality.edge_stability".to_string(),
            reason: format!(
                "edge contrast delta {delta:.6} exceeded limit {EDGE_STABILITY_LIMIT:.6}"
            ),
        });
    }
    Ok(json!({
        "canonical_edge_contrast": canonical_edge,
        "full_hd_edge_contrast": full_hd_edge,
        "delta_abs": delta,
        "limit": EDGE_STABILITY_LIMIT,
        "pass": pass,
    }))
}

fn edge_metric(probe: &Value) -> Result<f32, EngineError> {
    probe["after"]["comparison"]["metrics"]["edge_contrast"]
        .as_f64()
        .map(|value| value as f32)
        .ok_or_else(|| EngineError::ReferenceSceneInvalidConfig {
            field: "reference_scene_quality.edge_metric".to_string(),
            reason: "probe JSON did not contain a numeric edge_contrast".to_string(),
        })
}

fn edge_proofs() -> Result<Value, EngineError> {
    Ok(json!({
        "flat_reference_image_fails_edge_band": flat_reference_image_fails_edge_band()?,
    }))
}

fn flat_reference_image_fails_edge_band() -> Result<Value, EngineError> {
    let image = ReferenceSceneImage::from_rgba_pixels(
        CANONICAL_WIDTH,
        CANONICAL_HEIGHT,
        vec![[52, 52, 52, 255]; CANONICAL_WIDTH as usize * CANONICAL_HEIGHT as usize],
    )?;
    let comparison = crate::render::reference::compare_reference_scene_image(&image);
    let edge = comparison
        .bands
        .iter()
        .find(|band| band.name == "edge_contrast")
        .ok_or_else(|| EngineError::ReferenceSceneInvalidConfig {
            field: "reference_scene_quality.edge_band".to_string(),
            reason: "edge_contrast band was missing".to_string(),
        })?;
    Ok(json!({
        "before": {
            "width": CANONICAL_WIDTH,
            "height": CANONICAL_HEIGHT,
            "fill_rgba": [52, 52, 52, 255],
        },
        "after": {
            "comparison": comparison_json(&comparison),
            "edge_band": {
                "actual": edge.actual,
                "min": edge.min,
                "max": edge.max,
                "pass": edge.pass,
            }
        }
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

fn harness_error(op: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::SerializationFailed {
        format: "issue1442-reference-scene-quality",
        reason: format!("{op}: {error}"),
    }
}
