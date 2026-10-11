use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, render::shader::shader_fingerprint};
use serde::{Deserialize, Serialize};

use crate::{
    asset_products::{AssetProductKind, product_from_vault_record},
    asset_scene::{EditorAssetSceneReport, load_editor_world_asset_scene},
    asset_vault::AssetVaultRecord,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderBudgetGovernanceSettings {
    pub max_triangles_per_asset: usize,
    pub max_draw_calls_per_asset: u32,
    pub max_texture_bytes_per_asset: u64,
    pub max_atlas_group_texture_bytes: u64,
    pub min_cluster_size: usize,
}

impl Default for RenderBudgetGovernanceSettings {
    fn default() -> Self {
        Self {
            max_triangles_per_asset: 100_000,
            max_draw_calls_per_asset: 8,
            max_texture_bytes_per_asset: 64 * 1024 * 1024,
            max_atlas_group_texture_bytes: 128 * 1024 * 1024,
            min_cluster_size: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderBudgetGovernanceReport {
    pub schema: String,
    pub source_of_truth: String,
    pub settings: RenderBudgetGovernanceSettings,
    pub asset_count: usize,
    pub violation_count: usize,
    pub atlas_group_count: usize,
    pub singleton_cluster_count: usize,
    pub assets: Vec<RenderBudgetAssetConstellation>,
    pub violations: Vec<RenderBudgetViolation>,
    pub atlas_groups: Vec<RenderBudgetAtlasGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderBudgetAssetConstellation {
    pub record_id: String,
    pub product_id: String,
    pub title: String,
    pub source: String,
    pub asset_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub constellation_id: String,
    pub cluster_key: String,
    pub batch_key: String,
    pub visual_signature: String,
    pub structural_signature: String,
    pub metrics: RenderBudgetMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderBudgetMetrics {
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub image_count: usize,
    pub mesh_primitives: u32,
    pub estimated_draw_calls: u32,
    pub texture_bytes: u64,
    pub max_texture_width: u32,
    pub max_texture_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderBudgetViolation {
    pub record_id: String,
    pub source_relative_path: String,
    pub field: RenderBudgetViolationField,
    pub observed: u64,
    pub limit: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderBudgetViolationField {
    TriangleCount,
    EstimatedDrawCalls,
    TextureBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderBudgetAtlasGroup {
    pub group_id: String,
    pub cluster_key: String,
    pub proposal_kind: RenderBudgetProposalKind,
    pub member_record_ids: Vec<String>,
    pub member_source_paths: Vec<String>,
    pub total_texture_bytes: u64,
    pub max_triangle_count: usize,
    pub estimated_current_draw_calls: u32,
    pub estimated_grouped_draw_calls: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderBudgetProposalKind {
    SharedAtlasAndBatchCandidate,
    SharedBatchCandidate,
}

pub fn build_render_budget_governance_report(
    settings: RenderBudgetGovernanceSettings,
    records: &[AssetVaultRecord],
) -> Result<RenderBudgetGovernanceReport, EngineError> {
    validate_settings(settings)?;
    if records.is_empty() {
        return invalid_governance("records", "must contain at least one real asset record");
    }

    let mut seen = BTreeSet::new();
    let mut assets = Vec::with_capacity(records.len());
    for record in records {
        let record_key = format!("{}|{}", record.relative_path, record.sha256);
        if !seen.insert(record_key) {
            return invalid_governance("records.duplicate", "duplicate asset record input");
        }
        assets.push(measure_record(record)?);
    }
    assets.sort_by(|left, right| left.record_id.cmp(&right.record_id));

    let violations = collect_violations(settings, &assets);
    let (atlas_groups, singleton_cluster_count) = build_atlas_groups(settings, &assets)?;
    Ok(RenderBudgetGovernanceReport {
        schema: "litd.render_budget_governance.v1".to_string(),
        source_of_truth: "real AssetVaultRecord files loaded through EditorWorldSceneBuffers"
            .to_string(),
        settings,
        asset_count: assets.len(),
        violation_count: violations.len(),
        atlas_group_count: atlas_groups.len(),
        singleton_cluster_count,
        assets,
        violations,
        atlas_groups,
    })
}

fn measure_record(
    record: &AssetVaultRecord,
) -> Result<RenderBudgetAssetConstellation, EngineError> {
    let product = product_from_vault_record(record)?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_governance(
            "records.product_kind",
            format!("{} is not a mesh product", record.relative_path),
        );
    }
    let extension = std::path::Path::new(&record.relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("gltf" | "glb")) {
        return invalid_governance(
            "records.source.extension",
            format!(
                "{} is not measurable on the current glTF/GLB render path",
                record.relative_path
            ),
        );
    }

    let loaded = load_editor_world_asset_scene(&product)?;
    let metrics = metrics_from_report(&loaded.report)?;
    let visual_signature = visual_signature(&loaded.report);
    let structural_signature = structural_signature(&metrics);
    let batch_key = format!(
        "material={};textures={};visual={}",
        loaded.report.material_count, loaded.report.texture_count, visual_signature
    );
    let cluster_key = format!(
        "source={};asset={};batch={}",
        record.source, record.asset_id, batch_key
    );
    let record_id = stable_id(&format!("{}|{}", record.relative_path, record.sha256));
    let constellation_id = stable_id(&format!(
        "{}|{}|{}|{}",
        record_id, cluster_key, structural_signature, visual_signature
    ));

    Ok(RenderBudgetAssetConstellation {
        record_id,
        product_id: product.product_id,
        title: product.title,
        source: record.source.clone(),
        asset_id: record.asset_id.clone(),
        source_relative_path: record.relative_path.clone(),
        source_sha256: record.sha256.clone(),
        constellation_id,
        cluster_key,
        batch_key,
        visual_signature,
        structural_signature,
        metrics,
    })
}

fn metrics_from_report(
    report: &EditorAssetSceneReport,
) -> Result<RenderBudgetMetrics, EngineError> {
    let texture_bytes = checked_sum([
        report.base_color_texture_bytes as u64,
        report.normal_texture_bytes as u64,
        report.metallic_roughness_texture_bytes as u64,
    ])?;
    let estimated_draw_calls = report
        .mesh_primitives
        .max(report.material_count as u32)
        .max(1);
    Ok(RenderBudgetMetrics {
        vertex_count: report.vertex_count,
        index_count: report.index_count,
        triangle_count: report.triangle_count,
        material_count: report.material_count,
        texture_count: report.texture_count,
        image_count: report.image_count,
        mesh_primitives: report.mesh_primitives,
        estimated_draw_calls,
        texture_bytes,
        max_texture_width: report
            .base_color_texture_width
            .max(report.normal_texture_width)
            .max(report.metallic_roughness_texture_width),
        max_texture_height: report
            .base_color_texture_height
            .max(report.normal_texture_height)
            .max(report.metallic_roughness_texture_height),
    })
}

fn collect_violations(
    settings: RenderBudgetGovernanceSettings,
    assets: &[RenderBudgetAssetConstellation],
) -> Vec<RenderBudgetViolation> {
    let mut violations = Vec::new();
    for asset in assets {
        push_violation(
            &mut violations,
            asset,
            RenderBudgetViolationField::TriangleCount,
            asset.metrics.triangle_count as u64,
            settings.max_triangles_per_asset as u64,
        );
        push_violation(
            &mut violations,
            asset,
            RenderBudgetViolationField::EstimatedDrawCalls,
            asset.metrics.estimated_draw_calls as u64,
            settings.max_draw_calls_per_asset as u64,
        );
        push_violation(
            &mut violations,
            asset,
            RenderBudgetViolationField::TextureBytes,
            asset.metrics.texture_bytes,
            settings.max_texture_bytes_per_asset,
        );
    }
    violations
}

fn push_violation(
    violations: &mut Vec<RenderBudgetViolation>,
    asset: &RenderBudgetAssetConstellation,
    field: RenderBudgetViolationField,
    observed: u64,
    limit: u64,
) {
    if observed <= limit {
        return;
    }
    violations.push(RenderBudgetViolation {
        record_id: asset.record_id.clone(),
        source_relative_path: asset.source_relative_path.clone(),
        field,
        observed,
        limit,
        reason: format!(
            "{} measured {} but the configured render budget allows {}",
            asset.source_relative_path, observed, limit
        ),
    });
}

fn build_atlas_groups(
    settings: RenderBudgetGovernanceSettings,
    assets: &[RenderBudgetAssetConstellation],
) -> Result<(Vec<RenderBudgetAtlasGroup>, usize), EngineError> {
    let mut by_cluster = BTreeMap::<String, Vec<&RenderBudgetAssetConstellation>>::new();
    for asset in assets {
        by_cluster
            .entry(asset.cluster_key.clone())
            .or_default()
            .push(asset);
    }

    let mut groups = Vec::new();
    let mut singleton_count = 0;
    for (cluster_key, members) in by_cluster {
        if members.len() < settings.min_cluster_size {
            singleton_count += 1;
            continue;
        }
        let total_texture_bytes =
            checked_sum(members.iter().map(|asset| asset.metrics.texture_bytes))?;
        let current_draw_calls = checked_draw_sum(
            members
                .iter()
                .map(|asset| asset.metrics.estimated_draw_calls),
        )?;
        let grouped_draw_calls = u32::try_from(members.len()).map_err(|_| {
            invalid_governance_error("atlas_group.members", "member count does not fit u32")
        })?;
        let proposal_kind = if total_texture_bytes <= settings.max_atlas_group_texture_bytes {
            RenderBudgetProposalKind::SharedAtlasAndBatchCandidate
        } else {
            RenderBudgetProposalKind::SharedBatchCandidate
        };
        groups.push(RenderBudgetAtlasGroup {
            group_id: stable_id(&format!("atlas:{cluster_key}")),
            cluster_key,
            proposal_kind,
            member_record_ids: members
                .iter()
                .map(|asset| asset.record_id.clone())
                .collect(),
            member_source_paths: members
                .iter()
                .map(|asset| asset.source_relative_path.clone())
                .collect(),
            total_texture_bytes,
            max_triangle_count: members
                .iter()
                .map(|asset| asset.metrics.triangle_count)
                .max()
                .unwrap_or(0),
            estimated_current_draw_calls: current_draw_calls,
            estimated_grouped_draw_calls: grouped_draw_calls,
            reason: "same source asset family plus compatible material/texture signature"
                .to_string(),
        });
    }
    Ok((groups, singleton_count))
}

fn visual_signature(report: &EditorAssetSceneReport) -> String {
    format!(
        "base={}@{}x{};normal={}@{}x{};mr={}@{}x{}",
        report.base_color_texture_source_path,
        report.base_color_texture_width,
        report.base_color_texture_height,
        report.normal_texture_source_path,
        report.normal_texture_width,
        report.normal_texture_height,
        report.metallic_roughness_texture_source_path,
        report.metallic_roughness_texture_width,
        report.metallic_roughness_texture_height
    )
}

fn structural_signature(metrics: &RenderBudgetMetrics) -> String {
    format!(
        "tri_bucket={};draws={};materials={};textures={};max_tex={}x{}",
        bucket(metrics.triangle_count),
        metrics.estimated_draw_calls,
        metrics.material_count,
        metrics.texture_count,
        metrics.max_texture_width,
        metrics.max_texture_height
    )
}

fn bucket(value: usize) -> &'static str {
    match value {
        0..=999 => "tiny",
        1_000..=9_999 => "small",
        10_000..=99_999 => "medium",
        _ => "large",
    }
}

fn checked_sum(values: impl IntoIterator<Item = u64>) -> Result<u64, EngineError> {
    let mut total = 0_u64;
    for value in values {
        total = total
            .checked_add(value)
            .ok_or_else(|| invalid_governance_error("metrics.bytes", "byte count overflowed"))?;
    }
    Ok(total)
}

fn checked_draw_sum(values: impl IntoIterator<Item = u32>) -> Result<u32, EngineError> {
    let mut total = 0_u32;
    for value in values {
        total = total.checked_add(value).ok_or_else(|| {
            invalid_governance_error("metrics.draw_calls", "draw count overflowed")
        })?;
    }
    Ok(total)
}

fn validate_settings(settings: RenderBudgetGovernanceSettings) -> Result<(), EngineError> {
    if settings.max_triangles_per_asset == 0 {
        return invalid_governance(
            "settings.max_triangles_per_asset",
            "must be greater than zero",
        );
    }
    if settings.max_draw_calls_per_asset == 0 {
        return invalid_governance(
            "settings.max_draw_calls_per_asset",
            "must be greater than zero",
        );
    }
    if settings.max_texture_bytes_per_asset == 0 || settings.max_atlas_group_texture_bytes == 0 {
        return invalid_governance(
            "settings.texture_bytes",
            "byte budgets must be greater than zero",
        );
    }
    if settings.min_cluster_size < 2 {
        return invalid_governance("settings.min_cluster_size", "must be at least 2");
    }
    Ok(())
}

fn stable_id(value: &str) -> String {
    shader_fingerprint(value)
}

fn invalid_governance<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_governance_error(field, reason))
}

fn invalid_governance_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.render_budget_governance.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "render_budget_governance_tests.rs"]
mod tests;
