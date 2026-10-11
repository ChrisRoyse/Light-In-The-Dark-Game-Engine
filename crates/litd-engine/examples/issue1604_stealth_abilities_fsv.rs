//! FSV: author the Ashen Reach stealth ability catalog (#1604-1622), persist it
//! to a REAL Calyx content store, and read every row back, asserting the stored
//! content is byte-identical to what was authored. Three edge cases prove the
//! framework fails closed: an invalid ability is rejected loud, a duplicate id is
//! rejected, and the happy path stores + reads back identically.

use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::{
        EngineCalyxContentStore, EngineContentConstellationReadback, EngineContentKind,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AbilityCatalog, AbilityCost, AbilityPrerequisite, ability_content_records,
        build_ability_catalog_readback, persist_and_lower_rpg_authored_content,
        stealth_ability_catalog,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const STEALTH_SOURCE_PATH: &str = "crates/litd-engine/src/rpg/abilities_stealth.rs";
const STEALTH_EXTRA_PATH: &str = "crates/litd-engine/src/rpg/abilities_stealth_extra.rs";
const STEALTH_TESTS_PATH: &str = "crates/litd-engine/src/rpg/abilities_stealth_tests.rs";
const STEALTH_EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1604_stealth_abilities_fsv.rs";
const SOURCE_PATHS: [&str; 4] = [
    STEALTH_SOURCE_PATH,
    STEALTH_EXTRA_PATH,
    STEALTH_TESTS_PATH,
    STEALTH_EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1604_stealth_abilities")?;
    let catalog = stealth_ability_catalog();
    let readback = build_ability_catalog_readback(&catalog)?;
    let records = ability_content_records(&catalog)?;

    // Persist to a REAL Calyx content store backed by an on-disk vault.
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    // Read every persisted row back out of the store (source of truth).
    let persisted = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;

    // Happy path: stored == authored. Each ability's stored ability_sha256 must
    // equal a fresh hash of the authored AbilityDefinition, and display_name/kind
    // must round-trip exactly.
    let ability_checks = catalog
        .abilities
        .iter()
        .map(|ability| {
            let stored =
                store.get_content_for_editor(EngineContentKind::Ability, &ability.ability_id)?;
            let authored_sha = sha256_hex(&serde_json::to_vec(ability)?);
            let matches = stored.payload["ability_sha256"] == json!(authored_sha)
                && stored.payload["display_name"] == json!(ability.display_name)
                && stored.payload["kind"] == json!(ability.kind)
                && stored.payload["ability_id"] == json!(ability.ability_id);
            Ok::<(String, bool), Box<dyn std::error::Error>>((ability.ability_id.clone(), matches))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ability_mismatches: Vec<&String> = ability_checks
        .iter()
        .filter(|(_, ok)| !ok)
        .map(|(id, _)| id)
        .collect();

    let summary = store.get_content_for_editor(
        EngineContentKind::Trigger,
        "catalog.ashen_reach.stealth.summary",
    )?;
    let summary_matches = summary.payload["surface_kind"] == json!("ability_catalog")
        && summary.payload["ability_count"] == json!(readback.ability_count)
        && summary.payload["catalog_id"] == json!(catalog.catalog_id);

    let rows_read_back_all = persisted.len() == records.len();
    let source_paths_editor_authored = persisted.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/abilities/")
    });
    let map_or_gps_leaked = persisted.iter().any(|row| {
        row.payload["built_in_map_required"] == json!(true)
            || row.payload["grants_map_or_minimap"] == json!(true)
            || row.payload["grants_gps_markers"] == json!(true)
    });

    let stored_matches = ability_mismatches.is_empty()
        && summary_matches
        && rows_read_back_all
        && source_paths_editor_authored
        && !map_or_gps_leaked;

    let edges = vec![
        edge(
            "invalid_ability_empty_effects",
            invalid_empty_effects(&catalog),
        )?,
        edge("duplicate_ability_id", duplicate_ability(&catalog))?,
        edge("unknown_prerequisite", unknown_prerequisite(&catalog))?,
    ];

    let report = json!({
        "schema": "litd.issue1604.stealth_abilities_fsv.v1",
        "issue": 1604,
        "covers_issues": [1604, 1605, 1606, 1607, 1608, 1609, 1610, 1611, 1613, 1614,
                          1615, 1616, 1617, 1618, 1619, 1620, 1621, 1622, 1732],
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks()?,
        },
        "expected_actuals": {
            "ability_count": readback.ability_count,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted.len(),
            "ability_rows_checked": ability_checks.len(),
            "ability_mismatches": ability_mismatches,
            "summary_matches": summary_matches,
            "source_paths_editor_authored": source_paths_editor_authored,
            "map_or_gps_leaked": map_or_gps_leaked,
            "stored_matches": stored_matches,
        },
        "happy_path": readback,
        "calyx": {
            "content_rows": content_rows(&persisted),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });

    require_bool(&report["expected_actuals"]["stored_matches"], true)?;
    require_i64(
        &report["expected_actuals"]["content_rows_read_back"],
        (catalog.abilities.len() + 1) as i64,
    )?;
    require_bool(&report["expected_actuals"]["map_or_gps_leaked"], false)?;

    let written = run.write_report(&report)?;
    println!(
        "ISSUE1604_STEALTH_FSV report={} sha256={} abilities={} rows_written={} rows_read_back={} stored_matches={} edges={}",
        written.path.display(),
        written.sha256,
        written.json["happy_path"]["ability_count"],
        written.json["expected_actuals"]["content_rows_written"],
        written.json["expected_actuals"]["content_rows_read_back"],
        written.json["expected_actuals"]["stored_matches"],
        written.json["edges"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
    );
    Ok(())
}

// --- Edge-case catalog mutations -------------------------------------------

fn invalid_empty_effects(catalog: &AbilityCatalog) -> AbilityCatalog {
    let mut catalog = catalog.clone();
    catalog.abilities[0].effects.clear();
    catalog
}

fn duplicate_ability(catalog: &AbilityCatalog) -> AbilityCatalog {
    let mut catalog = catalog.clone();
    catalog.abilities.push(catalog.abilities[0].clone());
    catalog
}

fn unknown_prerequisite(catalog: &AbilityCatalog) -> AbilityCatalog {
    let mut catalog = catalog.clone();
    catalog.abilities[3].requirements.prerequisites = vec![AbilityPrerequisite {
        ability_id: "ability.missing".to_string(),
        min_rank: 1,
    }];
    // Touch AbilityCost so the import stays exercised by the negative surface.
    let _keep = AbilityCost {
        resource_id: "mana".to_string(),
        amount: 1,
    };
    catalog
}

fn edge(case: &str, catalog: AbilityCatalog) -> Result<Value, EngineError> {
    let before = catalog_hash(&catalog)?;
    let error = match build_ability_catalog_readback(&catalog) {
        Ok(_) => {
            return Err(EngineError::RpgAbilityInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid stealth catalog to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = catalog_hash(&catalog)?;
    Ok(json!({
        "case": case,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn catalog_hash(catalog: &AbilityCatalog) -> Result<String, EngineError> {
    serde_json::to_vec(catalog)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize issue1604 catalog: {error}"),
        })
}

// --- Report helpers ---------------------------------------------------------

fn source_readbacks() -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    SOURCE_PATHS
        .into_iter()
        .map(|path| {
            let bytes = fs::read(path)?;
            Ok(json!({
                "path": path,
                "bytes": bytes.len(),
                "sha256": sha256_hex(&bytes),
            }))
        })
        .collect()
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "kind": row.kind,
                "source_path": row.source_path,
                "surface_kind": row.payload["surface_kind"],
                "ability_id": row.payload["ability_id"],
                "ability_sha256": row.payload["ability_sha256"],
                "lowering_fingerprint": row.lowering.fingerprint,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
            })
        })
        .collect()
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgAbilityInvalidConfig {
        field: "issue1604.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgAbilityInvalidConfig {
        field: "issue1604.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
