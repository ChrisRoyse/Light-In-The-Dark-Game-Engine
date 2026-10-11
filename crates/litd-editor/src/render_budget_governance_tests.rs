use super::*;

use crate::{
    asset_scene::DEFAULT_EDITOR_WORLD_GLTF,
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const LEELA_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Leela.gltf";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const REAL_AUDIO: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

#[test]
fn flags_real_over_budget_asset_and_proposes_shared_mech_group() {
    let records = real_records(&[STAN_GLTF, LEELA_GLTF, GATE_GLB, DEFAULT_EDITOR_WORLD_GLTF]);
    let report = build_render_budget_governance_report(
        RenderBudgetGovernanceSettings {
            max_triangles_per_asset: 100_000,
            max_draw_calls_per_asset: 32,
            max_texture_bytes_per_asset: 64 * 1024 * 1024,
            max_atlas_group_texture_bytes: 128 * 1024 * 1024,
            min_cluster_size: 2,
        },
        &records,
    )
    .unwrap();

    assert_eq!(report.asset_count, 4);
    assert_eq!(report.violation_count, 1);
    let violation = report.violations.first().unwrap();
    assert_eq!(violation.source_relative_path, DEFAULT_EDITOR_WORLD_GLTF);
    assert_eq!(violation.field, RenderBudgetViolationField::TriangleCount);
    assert_eq!(violation.observed, 153_472);
    assert_eq!(violation.limit, 100_000);
    let group = group_containing(&report, STAN_GLTF, LEELA_GLTF);
    assert_eq!(
        group.proposal_kind,
        RenderBudgetProposalKind::SharedAtlasAndBatchCandidate
    );
    assert_eq!(group.member_source_paths.len(), 2);
    assert_eq!(group.estimated_grouped_draw_calls, 2);
    assert!(group.estimated_current_draw_calls >= group.estimated_grouped_draw_calls);
    println!(
        "issue1234 happy before_assets=4 budget_triangles=100000 after_violations={} over_budget={} observed={} group_members={} current_draws={} grouped_draws={}",
        report.violation_count,
        violation.source_relative_path,
        violation.observed,
        group.member_source_paths.join("|"),
        group.estimated_current_draw_calls,
        group.estimated_grouped_draw_calls
    );
}

#[test]
fn all_within_budget_reports_no_violations_without_hiding_groups() {
    let records = real_records(&[STAN_GLTF, LEELA_GLTF, GATE_GLB]);
    let report =
        build_render_budget_governance_report(RenderBudgetGovernanceSettings::default(), &records)
            .unwrap();

    assert_eq!(report.asset_count, 3);
    assert_eq!(report.violation_count, 0);
    assert!(
        group_containing(&report, STAN_GLTF, LEELA_GLTF)
            .member_source_paths
            .len()
            == 2
    );
    println!(
        "issue1234 edge=all_within before_assets=3 after_violations={} groups={}",
        report.violation_count, report.atlas_group_count
    );
}

#[test]
fn singleton_cluster_is_reported_without_fake_group() {
    let records = real_records(&[GATE_GLB]);
    let report =
        build_render_budget_governance_report(RenderBudgetGovernanceSettings::default(), &records)
            .unwrap();

    assert_eq!(report.asset_count, 1);
    assert_eq!(report.atlas_group_count, 0);
    assert_eq!(report.singleton_cluster_count, 1);
    assert_eq!(report.violation_count, 0);
    println!(
        "issue1234 edge=singleton before_assets=1 after_groups={} singleton_clusters={}",
        report.atlas_group_count, report.singleton_cluster_count
    );
}

#[test]
fn budget_threshold_boundary_is_inclusive() {
    let records = real_records(&[STAN_GLTF]);
    let at_limit = build_render_budget_governance_report(
        RenderBudgetGovernanceSettings {
            max_triangles_per_asset: 5_972,
            max_draw_calls_per_asset: 6,
            max_texture_bytes_per_asset: 12,
            max_atlas_group_texture_bytes: 128,
            min_cluster_size: 2,
        },
        &records,
    )
    .unwrap();
    assert_eq!(at_limit.violation_count, 0);

    let over_by_one = build_render_budget_governance_report(
        RenderBudgetGovernanceSettings {
            max_triangles_per_asset: 5_971,
            max_draw_calls_per_asset: 6,
            max_texture_bytes_per_asset: 12,
            max_atlas_group_texture_bytes: 128,
            min_cluster_size: 2,
        },
        &records,
    )
    .unwrap();
    assert_eq!(over_by_one.violation_count, 1);
    assert_eq!(
        over_by_one.violations[0].field,
        RenderBudgetViolationField::TriangleCount
    );
    println!(
        "issue1234 edge=threshold before_triangles=5972 limit=5972 after_violations={} over_by_one_violations={}",
        at_limit.violation_count, over_by_one.violation_count
    );
}

#[test]
fn invalid_inputs_fail_loud_without_skipping_records() {
    let empty_error =
        build_render_budget_governance_report(RenderBudgetGovernanceSettings::default(), &[])
            .unwrap_err();
    assert!(empty_error.to_string().contains("records"));

    let audio = real_records(&[REAL_AUDIO]);
    let audio_error =
        build_render_budget_governance_report(RenderBudgetGovernanceSettings::default(), &audio)
            .unwrap_err();
    assert!(audio_error.to_string().contains("records.product_kind"));
    println!(
        "issue1234 edge=invalid empty_error={} audio_error={}",
        empty_error, audio_error
    );
}

fn group_containing<'a>(
    report: &'a RenderBudgetGovernanceReport,
    left_path: &str,
    right_path: &str,
) -> &'a RenderBudgetAtlasGroup {
    report
        .atlas_groups
        .iter()
        .find(|group| {
            group
                .member_source_paths
                .iter()
                .any(|path| path == left_path)
                && group
                    .member_source_paths
                    .iter()
                    .any(|path| path == right_path)
        })
        .unwrap_or_else(|| panic!("missing atlas group for {left_path} and {right_path}"))
}

fn real_records(paths: &[&str]) -> Vec<AssetVaultRecord> {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    paths
        .iter()
        .map(|path| {
            catalog
                .records()
                .iter()
                .find(|record| record.relative_path == *path)
                .cloned()
                .unwrap_or_else(|| panic!("{}", crate::asset_vault_fixture::missing_fixture(path)))
        })
        .collect()
}
