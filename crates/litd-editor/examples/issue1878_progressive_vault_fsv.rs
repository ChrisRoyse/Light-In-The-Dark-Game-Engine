//! #1878 FSV: progressive vault residual floors + editor-boot coverage path.
//!
//! Source of truth: real `local-asset-vault` intake/extraction manifests on disk.
//! Proves: (a) residual floor report is read independently from catalog counts;
//! (b) editor-boot coverage either reports progressive residual status or full P0;
//! (c) full coverage_summary fails loud while residual floors remain;
//! (d) three edges: empty residual when full floors (if present), progressive min,
//!     and full-summary fail-closed.

use litd_editor::asset_vault::{
    AssetVaultCatalog, ISSUE1242_PROGRESSIVE_STATUS, ISSUE1242_SEMANTIC_STATUS,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1878_progressive_vault")?;

    let catalog = AssetVaultCatalog::load_default_cached()?;
    let residual = catalog.issue1242_p0_residual_report();
    let records = catalog.record_count();
    let usable = catalog.usable_record_count();
    let models = catalog.asset_type_count("model");
    let surfaces = catalog.asset_type_count("surface");
    let materials = catalog.asset_type_count("material");
    let terrain = catalog.asset_type_count("terrain");
    let downloads = catalog.record_type_count("download");
    let extractions = catalog.record_type_count("extraction");
    let sources = catalog.download_source_summary_line();

    // Boot path: progressive residual OR full P0 — never silent.
    let boot = catalog.coverage_summary_for_editor_boot()?;
    let progressive = boot.issue1242_p0_status == ISSUE1242_PROGRESSIVE_STATUS;
    let full_p0 = boot.issue1242_p0_status == ISSUE1242_SEMANTIC_STATUS;
    let boot_ok = progressive || full_p0;

    // Full coverage_summary must fail loud while residual floors remain.
    let full_summary = catalog.coverage_summary();
    let full_fails_when_residual = if residual.is_empty() {
        full_summary.is_ok()
    } else {
        match full_summary {
            Ok(_) => false,
            Err(error) => {
                let code = error.code().to_string();
                let msg = error.to_string();
                code.contains("INVALID") || msg.contains("issue1242_p0_baseline")
            }
        }
    };

    // Edge 1: residual report agrees with independent floor arithmetic for records.
    let residual_has_records = residual.iter().any(|r| r.starts_with("records:"));
    let records_edge = if records >= 24_000 {
        !residual_has_records
    } else {
        residual_has_records
    };

    // Edge 2: progressive path requires usable models when residual non-empty.
    let progressive_edge = if residual.is_empty() {
        full_p0
    } else {
        progressive && usable >= 50 && models >= 1
    };

    // Edge 3: extraction residual present when extraction manifest absent.
    let extraction_edge = if extractions >= 8_000 {
        !residual.iter().any(|r| r.starts_with("extractions:"))
    } else {
        residual.iter().any(|r| r.starts_with("extractions:"))
    };

    let verdict =
        boot_ok && full_fails_when_residual && records_edge && progressive_edge && extraction_edge;

    let report = json!({
        "schema": "litd.editor.issue1878.progressive_vault_fsv.v1",
        "issue": 1878,
        "parents": [1242, 1784, 1569],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "Real local-asset-vault catalog counts are the SoT. Residual floors are computed independently and must match shortfalls; editor-boot coverage is progressive (loud residual log + progressive status) or full P0; full coverage_summary fails closed while residual remains. Three edges: records residual consistency, progressive min usable/models, extraction residual when extract phase incomplete.",
        "catalog": {
            "records": records,
            "usable": usable,
            "downloads": downloads,
            "extractions": extractions,
            "models": models,
            "surfaces": surfaces,
            "materials": materials,
            "terrain": terrain,
            "sources": sources,
        },
        "residual": residual,
        "residual_count": residual.len(),
        "boot_status": boot.issue1242_p0_status,
        "progressive": progressive,
        "full_p0": full_p0,
        "full_fails_when_residual": full_fails_when_residual,
        "edges": {
            "records_edge": records_edge,
            "progressive_edge": progressive_edge,
            "extraction_edge": extraction_edge,
        },
    });

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1878_PROGRESSIVE_VAULT_FSV verdict={} report={} report_sha256={} records={} usable={} residual_count={} boot_status={} progressive={} full_p0={} full_fails_when_residual={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        records,
        usable,
        residual.len(),
        boot.issue1242_p0_status,
        progressive,
        full_p0,
        full_fails_when_residual,
    );
    for line in &residual {
        println!("  residual {line}");
    }
    if !verdict {
        return Err("ISSUE1878 progressive vault FSV RED".into());
    }
    Ok(())
}
