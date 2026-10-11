//! Supporting #1154 harness for the 2D sprite/tilemap/lighting pipeline.

use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::{FsvReportReadback, begin_issue_run},
    render::{
        reference::{ReferenceSceneComparison, compare_reference_scene_image},
        two_d::{
            Render2dSettings, build_issue1154_2d_pipeline_plan, issue1154_settings,
            render_2d_pipeline_reference,
        },
    },
};

pub const ISSUE1154_2D_PIPELINE_SCHEMA: &str = "litd.engine.issue1154.two_d_pipeline.v1";
pub const ISSUE1154_2D_PIPELINE_SLUG: &str = "issue1154_2d_pipeline";

pub fn run_issue1154_2d_pipeline_harness() -> Result<FsvReportReadback, EngineError> {
    let run = begin_issue_run(ISSUE1154_2D_PIPELINE_SLUG)
        .map_err(|error| harness_error("fsv.begin_issue_run", error))?;
    let bmp_path = run
        .artifact_path("reference-2d-960x540.bmp")
        .map_err(|error| harness_error("fsv.bmp_path", error))?;
    let plan = build_issue1154_2d_pipeline_plan(issue1154_settings())?;
    let image = render_2d_pipeline_reference(&plan)?;
    let comparison = compare_reference_scene_image(&image);
    if !comparison.pass {
        return Err(EngineError::ReferenceSceneInvalidConfig {
            field: "render.two_d.visual_floor".to_string(),
            reason: comparison.band_summary(),
        });
    }
    image.write_bmp(&bmp_path)?;
    let sprite_uvs = plan.sprite_frame_uvs()?;
    let report = json!({
        "schema": ISSUE1154_2D_PIPELINE_SCHEMA,
        "schema_version": 1,
        "issue": 1154,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "render_bmp_path": bmp_path.display().to_string(),
            "manual_gate": "supporting automated evidence only; manual Synapse/player-runtime FSV is still required"
        },
        "reference_images_reviewed": plan.reference_images,
        "happy_path": {
            "plan": {
                "settings": plan.settings,
                "sprite_sheet": plan.sprite_sheet,
                "sprite_count": plan.sprites.len(),
                "sprite_frame_uvs": sprite_uvs,
                "tile_columns": plan.tilemap.columns,
                "tile_rows": plan.tilemap.rows,
                "tile_count": plan.tilemap.tiles.len(),
                "normal_mapped_tile_count": plan.normal_mapped_tile_count(),
                "light_count": plan.lights.len(),
                "draw_calls": plan.draw_calls,
                "draw_call_budget": plan.settings.max_draw_calls,
                "normal_map_contract": plan.normal_map_contract,
            },
            "visual_floor": {
                "comparison": comparison_json(&comparison),
                "bmp_readback": file_readback(&bmp_path)?,
            }
        },
        "edges": edge_proofs(),
    });
    run.write_report(&report)
        .map_err(|error| harness_error("fsv.write_report", error))
}

fn edge_proofs() -> Value {
    json!({
        "invalid_output_size": invalid_output_size_edge(),
        "invalid_sprite_frame": invalid_sprite_frame_edge(),
        "tile_count_mismatch": tile_count_mismatch_edge(),
        "missing_light": missing_light_edge(),
        "over_draw_budget": over_draw_budget_edge(),
    })
}

fn invalid_output_size_edge() -> Value {
    let error = build_issue1154_2d_pipeline_plan(Render2dSettings {
        output_width: 319,
        output_height: 180,
        ..issue1154_settings()
    })
    .unwrap_err();
    edge(error)
}

fn invalid_sprite_frame_edge() -> Value {
    let mut plan = build_issue1154_2d_pipeline_plan(issue1154_settings()).unwrap();
    plan.sprites[0].frame_index = plan.sprite_sheet.frame_count();
    edge(plan.validate().unwrap_err())
}

fn tile_count_mismatch_edge() -> Value {
    let mut plan = build_issue1154_2d_pipeline_plan(issue1154_settings()).unwrap();
    plan.tilemap.tiles.pop();
    edge(plan.validate().unwrap_err())
}

fn missing_light_edge() -> Value {
    let mut plan = build_issue1154_2d_pipeline_plan(issue1154_settings()).unwrap();
    plan.lights.clear();
    edge(plan.validate().unwrap_err())
}

fn over_draw_budget_edge() -> Value {
    let mut plan = build_issue1154_2d_pipeline_plan(issue1154_settings()).unwrap();
    plan.settings.max_draw_calls = plan.draw_calls - 1;
    edge(plan.validate().unwrap_err())
}

fn edge(error: EngineError) -> Value {
    json!({ "after": engine_error_report(&error) })
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
        format: "issue1154-2d-pipeline",
        reason: format!("{op}: {error}"),
    }
}
