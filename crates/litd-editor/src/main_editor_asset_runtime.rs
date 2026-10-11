use super::*;

impl EditorShellRuntime {
    pub(super) fn selected_asset_entry(&self) -> Option<&AssetEntry> {
        if self.asset_entries.is_empty() {
            None
        } else {
            Some(&self.asset_entries[self.selected_asset.min(self.asset_entries.len() - 1)])
        }
    }
}

pub(super) struct AssetRuntimeInit {
    pub(super) selected_asset: usize,
    pub(super) asset_entries: Vec<AssetEntry>,
    pub(super) asset_vault_record_count: usize,
    pub(super) asset_vault_usable_record_count: usize,
    pub(super) asset_vault_sources: String,
    pub(super) asset_vault_record_types: String,
    pub(super) asset_vault_asset_types: String,
    pub(super) asset_vault_licenses: String,
    pub(super) asset_vault_semantic_role_counts: String,
    pub(super) asset_vault_semantic_profile_count: usize,
    pub(super) asset_vault_semantic_classified_profile_count: usize,
    pub(super) asset_vault_semantic_unclassified_profile_count: usize,
    pub(super) asset_vault_semantic_tag_count: usize,
    pub(super) asset_vault_download_bytes_on_disk: u64,
    pub(super) asset_vault_extraction_bytes_on_disk: u64,
    pub(super) asset_vault_issue1242_p0_status: &'static str,
    pub(super) asset_vault_manifests: String,
}

pub(super) fn initialize_asset_runtime(
    plan: &EditorShellPlan,
) -> Result<AssetRuntimeInit, EngineError> {
    if matches!(plan.active_mode, EditorMode::Objects | EditorMode::Triggers) {
        let deferred_reason = match plan.active_mode {
            EditorMode::Objects => "not_loaded_for_objects_mode",
            EditorMode::Triggers => "not_loaded_for_triggers_mode",
            _ => unreachable!(),
        };
        info!(
            mode = ?plan.active_mode,
            "editor shell runtime deferring asset vault catalog for mode"
        );
        return Ok(AssetRuntimeInit {
            selected_asset: 0,
            asset_entries: Vec::new(),
            asset_vault_record_count: 0,
            asset_vault_usable_record_count: 0,
            asset_vault_sources: deferred_reason.to_string(),
            asset_vault_record_types: deferred_reason.to_string(),
            asset_vault_asset_types: deferred_reason.to_string(),
            asset_vault_licenses: deferred_reason.to_string(),
            asset_vault_semantic_role_counts: deferred_reason.to_string(),
            asset_vault_semantic_profile_count: 0,
            asset_vault_semantic_classified_profile_count: 0,
            asset_vault_semantic_unclassified_profile_count: 0,
            asset_vault_semantic_tag_count: 0,
            asset_vault_download_bytes_on_disk: 0,
            asset_vault_extraction_bytes_on_disk: 0,
            asset_vault_issue1242_p0_status: deferred_reason,
            asset_vault_manifests: deferred_reason.to_string(),
        });
    }

    info!("editor shell runtime loading asset vault catalog");
    let asset_vault = AssetVaultCatalog::load_default_cached()?;
    info!("editor shell runtime summarizing asset vault catalog (#1878 progressive boot path)");
    // Progressive rematerialization: residual P0 floors are reported loud; full floors
    // still required for coverage_summary() / full-corpus FSV claims.
    let asset_vault_coverage = asset_vault.coverage_summary_for_editor_boot()?;
    let asset_entries = build_runtime_asset_entries(&asset_vault)?;
    let selected_asset =
        main_editor_asset_entries::default_model_preview_asset_index(&asset_entries)?;
    Ok(AssetRuntimeInit {
        selected_asset,
        asset_entries,
        asset_vault_record_count: asset_vault.record_count(),
        asset_vault_usable_record_count: asset_vault.usable_record_count(),
        asset_vault_sources: asset_vault.download_source_summary_line(),
        asset_vault_record_types: asset_vault_coverage.record_types,
        asset_vault_asset_types: asset_vault_coverage.asset_types,
        asset_vault_licenses: asset_vault_coverage.licenses,
        asset_vault_semantic_role_counts: asset_vault_coverage.semantic_role_counts,
        asset_vault_semantic_profile_count: asset_vault_coverage.semantic_profile_count,
        asset_vault_semantic_classified_profile_count: asset_vault_coverage
            .semantic_classified_profile_count,
        asset_vault_semantic_unclassified_profile_count: asset_vault_coverage
            .semantic_unclassified_profile_count,
        asset_vault_semantic_tag_count: asset_vault_coverage.semantic_tag_count,
        asset_vault_download_bytes_on_disk: asset_vault_coverage.download_bytes_on_disk,
        asset_vault_extraction_bytes_on_disk: asset_vault_coverage.extraction_bytes_on_disk,
        asset_vault_issue1242_p0_status: asset_vault_coverage.issue1242_p0_status,
        asset_vault_manifests: asset_vault.manifest_summary_line(),
    })
}
