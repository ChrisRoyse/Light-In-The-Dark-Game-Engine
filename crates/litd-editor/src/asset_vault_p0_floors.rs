//! #1242 / #1878 P0 asset-foundation floors and progressive residual reporting.
//!
//! Full floors remain fail-loud for production claims. Progressive rematerialization
//! (#1878) reports every residual shortfall loudly and only allows editor boot when
//! a documented progressive minimum is present — never mock data, never silent skip.

use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    AssetVaultCatalog, AssetVaultCoverageSummary, ISSUE1242_SEMANTIC_STATUS, asset_vault_error,
};

const ISSUE1242_MIN_RECORDS: usize = 24_000;
const ISSUE1242_MIN_USABLE_RECORDS: usize = 16_000;
const ISSUE1242_MIN_DOWNLOAD_RECORDS: usize = 15_000;
const ISSUE1242_MIN_EXTRACTION_RECORDS: usize = 8_000;
const ISSUE1242_MIN_SOURCES: usize = 5;
const ISSUE1242_MIN_DOWNLOAD_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const ISSUE1242_MIN_MODELS: usize = 200;
const ISSUE1242_MIN_SURFACES: usize = 450;
const ISSUE1242_MIN_MATERIALS: usize = 30;
const ISSUE1242_MIN_TERRAIN: usize = 30;

/// #1878 progressive rematerialization status (never the full-verified semantic token).
pub const ISSUE1242_PROGRESSIVE_STATUS: &str =
    "issue1242_p0_progressive_rematerialization_residual";
const ISSUE1878_PROGRESSIVE_MIN_USABLE: usize = 50;
const ISSUE1878_PROGRESSIVE_MIN_MODELS: usize = 1;

impl AssetVaultCatalog {
    /// Full #1242 P0 foundation summary. Fails loud when any floor is unmet.
    pub fn coverage_summary(&self) -> Result<AssetVaultCoverageSummary, EngineError> {
        self.validate_issue1242_p0_foundation()?;
        self.coverage_summary_with_status(ISSUE1242_SEMANTIC_STATUS)
    }

    /// #1878 editor-boot summary: full floors → same as [`Self::coverage_summary`];
    /// progressive residual → greppable residual log + progressive status; below
    /// progressive min → fail loud.
    pub fn coverage_summary_for_editor_boot(
        &self,
    ) -> Result<AssetVaultCoverageSummary, EngineError> {
        let residual = self.issue1242_p0_residual_report();
        if residual.is_empty() {
            return self.coverage_summary_with_status(ISSUE1242_SEMANTIC_STATUS);
        }
        let usable = self.usable_record_count();
        let models = self.asset_type_count("model");
        if usable < ISSUE1878_PROGRESSIVE_MIN_USABLE || models < ISSUE1878_PROGRESSIVE_MIN_MODELS {
            return asset_vault_error(
                "issue1242_p0_progressive_floor",
                format!(
                    "progressive vault below editor-boot floor: usable={usable} \
                     (need >={ISSUE1878_PROGRESSIVE_MIN_USABLE}) models={models} \
                     (need >={ISSUE1878_PROGRESSIVE_MIN_MODELS}); residual={}",
                    residual.join(";")
                ),
            );
        }
        eprintln!(
            "ASSET_VAULT_P0_RESIDUAL status={} residual_count={} residual={} records={} usable={} \
             sources={} asset_types={} -- #1878 progressive rematerialization; full P0 not met",
            ISSUE1242_PROGRESSIVE_STATUS,
            residual.len(),
            residual.join(";"),
            self.record_count(),
            usable,
            self.download_source_summary_line(),
            self.asset_type_summary_line()
        );
        self.coverage_summary_with_status(ISSUE1242_PROGRESSIVE_STATUS)
    }

    /// Independent residual list for every unmet #1242 floor (empty when fully met).
    pub fn issue1242_p0_residual_report(&self) -> Vec<String> {
        let download_records = self.record_type_count("download");
        let extraction_records = self.record_type_count("extraction");
        let usable_records = self.usable_record_count();
        let download_bytes = self.bytes_by_record_type("download");
        let source_count = self
            .records()
            .iter()
            .map(|record| record.source.as_str())
            .collect::<BTreeSet<_>>()
            .len();
        let licenses = self.license_summary_line();
        let mut residual = Vec::new();
        push_floor(
            &mut residual,
            "records",
            self.record_count(),
            ISSUE1242_MIN_RECORDS,
        );
        push_floor(
            &mut residual,
            "usable",
            usable_records,
            ISSUE1242_MIN_USABLE_RECORDS,
        );
        push_floor(
            &mut residual,
            "downloads",
            download_records,
            ISSUE1242_MIN_DOWNLOAD_RECORDS,
        );
        push_floor(
            &mut residual,
            "extractions",
            extraction_records,
            ISSUE1242_MIN_EXTRACTION_RECORDS,
        );
        push_floor(
            &mut residual,
            "sources",
            source_count,
            ISSUE1242_MIN_SOURCES,
        );
        if download_bytes < ISSUE1242_MIN_DOWNLOAD_BYTES {
            residual.push(format!(
                "download_bytes:{download_bytes}<{ISSUE1242_MIN_DOWNLOAD_BYTES}"
            ));
        }
        push_floor(
            &mut residual,
            "model",
            self.asset_type_count("model"),
            ISSUE1242_MIN_MODELS,
        );
        push_floor(
            &mut residual,
            "surface",
            self.asset_type_count("surface"),
            ISSUE1242_MIN_SURFACES,
        );
        push_floor(
            &mut residual,
            "material",
            self.asset_type_count("material"),
            ISSUE1242_MIN_MATERIALS,
        );
        push_floor(
            &mut residual,
            "terrain",
            self.asset_type_count("terrain"),
            ISSUE1242_MIN_TERRAIN,
        );
        let expected_license = format!("CC0-1.0={}", self.record_count());
        if licenses != expected_license {
            residual.push(format!("licenses:{licenses}!={expected_license}"));
        }
        // Semantic role floors are part of full P0 — keep them in residual so
        // progressive editor-boot does not claim full P0 (and then hard-fail
        // semantic coverage) while count floors alone are met (#1878).
        if let Ok(index) = self.semantic_index_cached() {
            for (role, min) in super::semantics::required_role_floors() {
                let have = index.role_count(role);
                if have < *min {
                    residual.push(format!("semantic_{role}:{have}<{min}"));
                }
            }
        }
        residual
    }

    pub(super) fn validate_issue1242_p0_foundation(&self) -> Result<(), EngineError> {
        let residual = self.issue1242_p0_residual_report();
        if residual.is_empty() {
            return Ok(());
        }
        asset_vault_error(
            "issue1242_p0_baseline",
            format!(
                "P0 asset foundation below baseline: residual={} records={} usable={} \
                 downloads={} extractions={} sources={} download_bytes={} asset_types={} licenses={}",
                residual.join(";"),
                self.record_count(),
                self.usable_record_count(),
                self.record_type_count("download"),
                self.record_type_count("extraction"),
                self.records()
                    .iter()
                    .map(|r| r.source.as_str())
                    .collect::<BTreeSet<_>>()
                    .len(),
                self.bytes_by_record_type("download"),
                self.asset_type_summary_line(),
                self.license_summary_line()
            ),
        )
    }

    fn coverage_summary_with_status(
        &self,
        issue1242_p0_status: &'static str,
    ) -> Result<AssetVaultCoverageSummary, EngineError> {
        // Full P0 status enforces semantic role floors; progressive boot uses a snapshot
        // that still reports real role counts without claiming full semantic coverage.
        let semantic_coverage = if issue1242_p0_status == ISSUE1242_SEMANTIC_STATUS {
            self.semantic_coverage_summary()?
        } else {
            self.semantic_index_cached()?.coverage_snapshot()
        };
        Ok(AssetVaultCoverageSummary {
            record_types: self.record_type_summary_line(),
            asset_types: self.asset_type_summary_line(),
            licenses: self.license_summary_line(),
            semantic_role_counts: semantic_coverage.role_counts,
            semantic_profile_count: semantic_coverage.profile_count,
            semantic_classified_profile_count: semantic_coverage.classified_profile_count,
            semantic_unclassified_profile_count: semantic_coverage.unclassified_profile_count,
            semantic_tag_count: semantic_coverage.tag_count,
            download_bytes_on_disk: self.bytes_by_record_type("download"),
            extraction_bytes_on_disk: self.bytes_by_record_type("extraction"),
            issue1242_p0_status,
        })
    }
}

fn push_floor(residual: &mut Vec<String>, name: &str, actual: usize, min: usize) {
    if actual < min {
        residual.push(format!("{name}:{actual}<{min}"));
    }
}
