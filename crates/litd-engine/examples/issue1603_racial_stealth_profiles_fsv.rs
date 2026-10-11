//! FSV: author the 20-race racial stealth profile table (#1603), validate its
//! referential integrity, persist it to a REAL Calyx content store, and read
//! every row back asserting the stored content matches what was authored. One
//! edge case injects a dangling ability id and proves the validator fails loud
//! with the exact diagnostic.

use std::fs;

use litd_engine::{
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        persist_and_lower_rpg_authored_content, racial_stealth_content_records,
        racial_stealth_profile_table, validate_racial_stealth_table,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

const SOURCE_PATHS: [&str; 4] = [
    "crates/litd-engine/src/rpg/racial_stealth.rs",
    "crates/litd-engine/src/rpg/racial_stealth_types.rs",
    "crates/litd-engine/src/rpg/racial_stealth_content.rs",
    "crates/litd-engine/examples/issue1603_racial_stealth_profiles_fsv.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1603_racial_stealth_profiles")?;
    let table = racial_stealth_profile_table();
    let readback = validate_racial_stealth_table(&table)?;
    let records = racial_stealth_content_records(&table)?;

    // Persist to a REAL Calyx content store backed by an on-disk vault.
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    // Independent readback: every authored profile row must round-trip exactly.
    let mut mismatches = Vec::new();
    for profile in &table.profiles {
        let id = format!("{}.{}", table.table_id, profile.race_id);
        let stored = store.get_content_for_editor(EngineContentKind::Trigger, &id)?;
        let authored_sha = sha256_hex(&serde_json::to_vec(profile)?);
        let matches = stored.payload["profile_sha256"] == json!(authored_sha)
            && stored.payload["race_id"] == json!(profile.race_id)
            && stored.payload["movement_noise"] == json!(profile.movement_noise);
        if !matches {
            mismatches.push(profile.race_id.clone());
        }
    }

    let summary = store.get_content_for_editor(
        EngineContentKind::Trigger,
        &format!("{}.summary", table.table_id),
    )?;
    let summary_matches = summary.payload["surface_kind"] == json!("racial_stealth_table")
        && summary.payload["profile_count"] == json!(readback.profile_count);

    let stored_matches =
        mismatches.is_empty() && summary_matches && lowered.row_count == records.len();

    // Edge case: inject a dangling innate ability id and prove it fails loud.
    let mut broken = table.clone();
    let idx = broken
        .profiles
        .iter()
        .position(|p| !p.innate_traits.is_empty())
        .expect("some race has an innate trait");
    broken.profiles[idx].innate_traits[0].ability_id = "ability.does_not_exist".to_string();
    let dangling = validate_racial_stealth_table(&broken).unwrap_err();

    let report = json!({
        "schema": "litd.issue1603.racial_stealth_fsv.v1",
        "issue": 1603,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks()?,
        },
        "expected_actuals": {
            "profile_count": readback.profile_count,
            "content_rows_written": lowered.row_count,
            "content_rows_expected": records.len(),
            "profile_mismatches": mismatches,
            "summary_matches": summary_matches,
            "stored_matches": stored_matches,
            "noise_class_counts": readback.noise_class_counts,
            "trait_role_counts": readback.trait_role_counts,
            "innate_trait_total": readback.innate_trait_total,
            "creep_penalized_races": readback.creep_penalized_races,
            "referenced_ability_ids": readback.referenced_ability_ids,
        },
        "edge_dangling_ability": {
            "error_code": dangling.code(),
            "error": dangling.to_string(),
            "fails_loud": dangling.to_string().contains("ability.does_not_exist"),
        },
    });

    if !stored_matches {
        return Err(format!("stored_matches=false; mismatches={mismatches:?}").into());
    }
    if dangling.code() != "LITD-RPG-STATS-INVALID-CONFIG" {
        return Err(format!("edge did not fail loud: {}", dangling.code()).into());
    }

    let written = run.write_report(&report)?;
    println!(
        "ISSUE1603_RACIAL_STEALTH_FSV report={} sha256={} profiles={} rows_written={} stored_matches={} dangling_code={}",
        written.path.display(),
        written.sha256,
        written.json["expected_actuals"]["profile_count"],
        written.json["expected_actuals"]["content_rows_written"],
        written.json["expected_actuals"]["stored_matches"],
        written.json["edge_dangling_ability"]["error_code"],
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
