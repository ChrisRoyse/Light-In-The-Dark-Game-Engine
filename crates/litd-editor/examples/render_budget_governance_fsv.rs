use std::{fs, path::PathBuf};

use litd_editor::{
    asset_scene::DEFAULT_EDITOR_WORLD_GLTF,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    render_budget_governance::{
        RenderBudgetGovernanceReport, RenderBudgetGovernanceSettings, RenderBudgetViolation,
        RenderBudgetViolationField, build_render_budget_governance_report,
    },
};
use serde_json::json;

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const LEELA_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Leela.gltf";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_RENDER_BUDGET_GOVERNANCE_FSV",
        "issue1234_render_budget_governance",
        "fsv.json",
    )?;
    let before = path_state(&path);

    let happy_records =
        real_records(&[STAN_GLTF, LEELA_GLTF, GATE_GLB, DEFAULT_EDITOR_WORLD_GLTF])?;
    let within_records = real_records(&[STAN_GLTF, LEELA_GLTF, GATE_GLB])?;
    let singleton_records = real_records(&[GATE_GLB])?;
    let boundary_records = real_records(&[STAN_GLTF])?;

    let happy_settings = RenderBudgetGovernanceSettings {
        max_triangles_per_asset: 100_000,
        max_draw_calls_per_asset: 32,
        max_texture_bytes_per_asset: 64 * 1024 * 1024,
        max_atlas_group_texture_bytes: 128 * 1024 * 1024,
        min_cluster_size: 2,
    };
    let happy = build_render_budget_governance_report(happy_settings, &happy_records)?;
    let all_within = build_render_budget_governance_report(
        RenderBudgetGovernanceSettings::default(),
        &within_records,
    )?;
    let singleton = build_render_budget_governance_report(
        RenderBudgetGovernanceSettings::default(),
        &singleton_records,
    )?;
    let boundary_at_limit = build_render_budget_governance_report(
        RenderBudgetGovernanceSettings {
            max_triangles_per_asset: 5_972,
            max_draw_calls_per_asset: 6,
            max_texture_bytes_per_asset: 12,
            max_atlas_group_texture_bytes: 128,
            min_cluster_size: 2,
        },
        &boundary_records,
    )?;
    let boundary_over_by_one = build_render_budget_governance_report(
        RenderBudgetGovernanceSettings {
            max_triangles_per_asset: 5_971,
            max_draw_calls_per_asset: 6,
            max_texture_bytes_per_asset: 12,
            max_atlas_group_texture_bytes: 128,
            min_cluster_size: 2,
        },
        &boundary_records,
    )?;

    let document = json!({
        "schema": "litd.issue1234.render_budget_governance_fsv.v1",
        "source_of_truth": path,
        "before": before,
        "happy_path": {
            "before": {
                "records": [STAN_GLTF, LEELA_GLTF, GATE_GLB, DEFAULT_EDITOR_WORLD_GLTF],
                "max_triangles_per_asset": happy_settings.max_triangles_per_asset,
                "max_draw_calls_per_asset": happy_settings.max_draw_calls_per_asset,
                "max_texture_bytes_per_asset": happy_settings.max_texture_bytes_per_asset
            },
            "after": summarize_report(&happy)
        },
        "edges": [
            {
                "edge": "all_within_budget_reports_none",
                "before": {"records": [STAN_GLTF, LEELA_GLTF, GATE_GLB]},
                "after": summarize_report(&all_within)
            },
            {
                "edge": "singleton_cluster_has_no_fake_group",
                "before": {"records": [GATE_GLB]},
                "after": summarize_report(&singleton)
            },
            {
                "edge": "budget_threshold_boundary",
                "before": {"record": STAN_GLTF, "triangle_count": 5972, "at_limit": 5972, "over_by_one_limit": 5971},
                "after": {
                    "at_limit_violation_count": boundary_at_limit.violation_count,
                    "over_by_one_violation_count": boundary_over_by_one.violation_count,
                    "over_by_one_first_field": boundary_over_by_one.violations.first().map(|violation| format!("{:?}", violation.field))
                }
            }
        ],
    });

    write_document_with_final_state(&path, document)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote render budget governance FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    Ok(())
}

fn summarize_report(report: &RenderBudgetGovernanceReport) -> serde_json::Value {
    let over_budget = violation_for(
        report,
        DEFAULT_EDITOR_WORLD_GLTF,
        RenderBudgetViolationField::TriangleCount,
    );
    let mech_group = report.atlas_groups.iter().find(|group| {
        group
            .member_source_paths
            .iter()
            .any(|path| path == STAN_GLTF)
            && group
                .member_source_paths
                .iter()
                .any(|path| path == LEELA_GLTF)
    });
    json!({
        "asset_count": report.asset_count,
        "violation_count": report.violation_count,
        "atlas_group_count": report.atlas_group_count,
        "singleton_cluster_count": report.singleton_cluster_count,
        "over_budget_source": over_budget.map(|violation| violation.source_relative_path.clone()),
        "over_budget_field": over_budget.map(|violation| format!("{:?}", violation.field)),
        "over_budget_observed": over_budget.map(|violation| violation.observed),
        "over_budget_limit": over_budget.map(|violation| violation.limit),
        "mech_group_members": mech_group.map(|group| group.member_source_paths.clone()),
        "mech_group_current_draw_calls": mech_group.map(|group| group.estimated_current_draw_calls),
        "mech_group_grouped_draw_calls": mech_group.map(|group| group.estimated_grouped_draw_calls),
        "assets": report.assets.iter().map(|asset| json!({
            "source_relative_path": asset.source_relative_path,
            "triangles": asset.metrics.triangle_count,
            "estimated_draw_calls": asset.metrics.estimated_draw_calls,
            "texture_bytes": asset.metrics.texture_bytes,
            "cluster_key": asset.cluster_key
        })).collect::<Vec<_>>(),
        "violations": report.violations,
        "atlas_groups": report.atlas_groups
    })
}

fn violation_for<'a>(
    report: &'a RenderBudgetGovernanceReport,
    source_path: &str,
    field: RenderBudgetViolationField,
) -> Option<&'a RenderBudgetViolation> {
    report
        .violations
        .iter()
        .find(|violation| violation.source_relative_path == source_path && violation.field == field)
}

fn real_records(paths: &[&str]) -> Result<Vec<AssetVaultRecord>, Box<dyn std::error::Error>> {
    let catalog = AssetVaultCatalog::load_default()?;
    paths
        .iter()
        .map(|path| {
            catalog
                .records()
                .iter()
                .find(|record| record.relative_path == *path)
                .cloned()
                .ok_or_else(|| format!("missing real vault record {path}").into())
        })
        .collect()
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            if disk_len != expected_len {
                return Err(format!(
                    "FSV artifact write verification failed for {}: embedded after.bytes={}, disk bytes={}",
                    path.display(),
                    expected_len,
                    disk_len
                )
                .into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }

    Err(format!(
        "FSV artifact byte-count stabilization failed for {} after 16 attempts",
        path.display()
    )
    .into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}
