//! FSV: author the stealth acquisition matrix (#1623), run its referential-
//! integrity validation (every ability id resolves in the #1604 catalog, every
//! grantor id resolves in the authored class/race rosters), persist it to a REAL
//! Calyx content store, and read every row back. Two edge cases prove the
//! validator fails loud: a dangling ability id and a dangling grantor id.

use std::fs;

use litd_engine::{
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        build_catalog_matrix_consistency_readback, persist_and_lower_rpg_authored_content,
        stealth_ability_catalog, stealth_acquisition_content_records, stealth_acquisition_matrix,
        validate_catalog_matrix_level_consistency, validate_stealth_acquisition_matrix,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

const SOURCE_PATHS: [&str; 6] = [
    "crates/litd-engine/src/rpg/stealth_acquisition.rs",
    "crates/litd-engine/src/rpg/stealth_acquisition_types.rs",
    "crates/litd-engine/src/rpg/stealth_acquisition_content.rs",
    "crates/litd-engine/src/rpg/stealth_acquisition_validate.rs",
    "crates/litd-engine/src/rpg/abilities_stealth_extra.rs",
    "crates/litd-engine/examples/issue1623_stealth_acquisition_matrix_fsv.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1623_stealth_acquisition_matrix")?;
    let matrix = stealth_acquisition_matrix();
    let readback = validate_stealth_acquisition_matrix(&matrix)?;
    let records = stealth_acquisition_content_records(&matrix)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    // Independent readback: every authored cell must round-trip exactly.
    let mut mismatches = Vec::new();
    for entry in &matrix.entries {
        let id = format!(
            "{}.{}.{}",
            matrix.table_id, entry.ability_id, entry.grantor_id
        );
        let stored = store.get_content_for_editor(EngineContentKind::Trigger, &id)?;
        let authored_sha = sha256_hex(&serde_json::to_vec(entry)?);
        let ok = stored.payload["entry_sha256"] == json!(authored_sha)
            && stored.payload["level"] == json!(entry.level)
            && stored.payload["ability_id"] == json!(entry.ability_id)
            && stored.payload["grantor_id"] == json!(entry.grantor_id);
        if !ok {
            mismatches.push(id);
        }
    }

    let summary = store.get_content_for_editor(
        EngineContentKind::Trigger,
        &format!("{}.summary", matrix.table_id),
    )?;
    let summary_matches = summary.payload["surface_kind"] == json!("stealth_acquisition_table")
        && summary.payload["entry_count"] == json!(readback.entry_count);

    let stored_matches =
        mismatches.is_empty() && summary_matches && lowered.row_count == records.len();

    // Edge 1: dangling ability id.
    let mut broken_ability = matrix.clone();
    broken_ability.entries[0].ability_id = "ability.phantom".to_string();
    let dangling_ability = validate_stealth_acquisition_matrix(&broken_ability).unwrap_err();

    // Edge 2: dangling grantor id.
    let mut broken_grantor = matrix.clone();
    broken_grantor.entries[0].grantor_id = "class.ghost".to_string();
    let dangling_grantor = validate_stealth_acquisition_matrix(&broken_grantor).unwrap_err();

    // #1732 catalog<->matrix level consistency (happy path).
    let consistency = build_catalog_matrix_consistency_readback()?;

    // Edge 3: inject a level drift (catalog hide L4 vs matrix L1) and prove the
    // #1732 guard fails loud naming the ability, class, and both levels.
    let mut drifted_catalog = stealth_ability_catalog();
    if let Some(hide) = drifted_catalog
        .abilities
        .iter_mut()
        .find(|a| a.ability_id == "ability.hide")
    {
        hide.requirements.min_level = 4;
    }
    let level_drift = validate_catalog_matrix_level_consistency(&drifted_catalog, &matrix)
        .expect_err("injected hide L4 drift must fail loud");

    let report = json!({
        "schema": "litd.issue1623.stealth_acquisition_fsv.v1",
        "issue": 1623,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks()?,
        },
        "expected_actuals": {
            "entry_count": readback.entry_count,
            "class_entry_count": readback.class_entry_count,
            "race_entry_count": readback.race_entry_count,
            "abilities_validated": readback.abilities_validated,
            "grantors_validated": readback.grantors_validated,
            "provisional_entry_count": readback.provisional_entry_count,
            "provisional_grantors": readback.provisional_grantors,
            "acquisition_counts": readback.acquisition_counts,
            "owners_per_ability": readback.owners_per_ability,
            "content_rows_written": lowered.row_count,
            "content_rows_expected": records.len(),
            "cell_mismatches": mismatches,
            "summary_matches": summary_matches,
            "stored_matches": stored_matches,
        },
        "edge_dangling_ability": {
            "error_code": dangling_ability.code(),
            "error": dangling_ability.to_string(),
            "fails_loud": dangling_ability.to_string().contains("ability.phantom"),
        },
        "edge_dangling_grantor": {
            "error_code": dangling_grantor.code(),
            "error": dangling_grantor.to_string(),
            "fails_loud": dangling_grantor.to_string().contains("class.ghost"),
        },
        "catalog_matrix_consistency": {
            "catalog_ability_count": consistency.catalog_ability_count,
            "matrix_entry_count": consistency.matrix_entry_count,
            "cross_checked_count": consistency.cross_checked_count,
            "consistency_sha256": consistency.consistency_sha256,
        },
        "edge_level_drift": {
            "error_code": level_drift.code(),
            "error": level_drift.to_string(),
            "fails_loud": level_drift.to_string().contains("ability.hide")
                && level_drift.to_string().contains("class.galdmere_shade")
                && level_drift.to_string().contains("min_level=4")
                && level_drift.to_string().contains("level=1"),
        },
    });

    if !stored_matches {
        return Err(format!("stored_matches=false; mismatches={mismatches:?}").into());
    }
    for err in [&dangling_ability, &dangling_grantor, &level_drift] {
        if err.code() != "LITD-RPG-ABILITY-INVALID-CONFIG" {
            return Err(format!("edge did not fail loud: {}", err.code()).into());
        }
    }
    if !report["edge_level_drift"]["fails_loud"]
        .as_bool()
        .unwrap_or(false)
    {
        return Err("level-drift edge did not name ability+class+both levels".into());
    }

    let written = run.write_report(&report)?;
    println!(
        "ISSUE1623_STEALTH_ACQUISITION_FSV report={} sha256={} entries={} rows_written={} stored_matches={} dangling_ability={} dangling_grantor={} cross_checked={} level_drift_fails_loud={}",
        written.path.display(),
        written.sha256,
        written.json["expected_actuals"]["entry_count"],
        written.json["expected_actuals"]["content_rows_written"],
        written.json["expected_actuals"]["stored_matches"],
        written.json["edge_dangling_ability"]["error_code"],
        written.json["edge_dangling_grantor"]["error_code"],
        written.json["catalog_matrix_consistency"]["cross_checked_count"],
        written.json["edge_level_drift"]["fails_loud"],
    );
    Ok(())
}

fn source_readbacks() -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    SOURCE_PATHS
        .into_iter()
        .map(|path| {
            let bytes = fs::read(path)?;
            Ok(json!({ "path": path, "bytes": bytes.len(), "sha256": sha256_hex(&bytes) }))
        })
        .collect()
}
