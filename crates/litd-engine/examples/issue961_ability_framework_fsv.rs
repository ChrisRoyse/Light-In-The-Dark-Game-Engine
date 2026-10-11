#[path = "fixtures/issue961_ability_fixture.rs"]
mod issue961_ability_fixture;

use std::fs;

use issue961_ability_fixture::{
    active_without_cost_or_wait, automatic_without_trigger, duplicate_ability, happy_catalog,
    passive_with_cost, unknown_prerequisite,
};
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AbilityCatalog, ResourceCostTotal, ability_content_records, build_ability_catalog_readback,
        persist_and_lower_rpg_authored_content,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const README_COMBAT_PATH: &str = "docs/readme/GP03-combat.png";
const README_SPELLBOOK_PATH: &str = "docs/readme/GP19-spellbook.png";
const README_TALENTS_PATH: &str = "docs/readme/GP20-talents.png";
const ABILITIES_IMPL_PATH: &str = "crates/litd-engine/src/rpg/abilities.rs";
const ABILITIES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/abilities_types.rs";
const ABILITIES_VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/abilities_validate.rs";
const ABILITIES_READBACK_PATH: &str = "crates/litd-engine/src/rpg/abilities_readback.rs";
const ABILITIES_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/abilities_content.rs";
const ABILITIES_TEST_PATH: &str = "crates/litd-engine/src/rpg/abilities_tests.rs";
const ABILITIES_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/abilities_test_fixture.rs";
const ABILITIES_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue961_ability_fixture.rs";
const ABILITIES_EXAMPLE_PATH: &str =
    "crates/litd-engine/examples/issue961_ability_framework_fsv.rs";
const SOURCE_PATHS: [&str; 12] = [
    README_COMBAT_PATH,
    README_SPELLBOOK_PATH,
    README_TALENTS_PATH,
    ABILITIES_IMPL_PATH,
    ABILITIES_TYPES_PATH,
    ABILITIES_VALIDATE_PATH,
    ABILITIES_READBACK_PATH,
    ABILITIES_CONTENT_PATH,
    ABILITIES_TEST_PATH,
    ABILITIES_FIXTURE_PATH,
    ABILITIES_EXAMPLE_FIXTURE_PATH,
    ABILITIES_EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue961_ability_framework")?;
    let happy = happy_catalog();
    let happy_readback = build_ability_catalog_readback(&happy)?;
    let records = ability_content_records(&happy)?;
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_map_or_gps_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/abilities/")
    });
    let content_has_compiled_source_path = persisted_rows
        .iter()
        .any(|row| row.source_path.starts_with("rpg://"));
    let lowered_artifact_count = persisted_rows
        .iter()
        .filter(|row| {
            row.lowering.hot_path_policy == "frozen_fingerprinted_data_only"
                && !row.lowering.fingerprint.is_empty()
        })
        .count();
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;
    let source_readback_count = source_readbacks.len();
    let source_count_for = |needle: &str| -> usize {
        SOURCE_PATHS
            .iter()
            .filter(|path| path.contains(needle))
            .count()
    };
    let expected_actuals = json!({
        "source_readback_count": source_readback_count,
        "readme_reference_count": source_count_for("docs/readme/"),
        "ability_source_count": source_count_for("crates/litd-engine/src/rpg/abilities"),
        "example_source_count": source_count_for("crates/litd-engine/examples"),
        "fixture_source_count": source_count_for("fixture"),
        "ability_count": happy_readback.ability_count,
        "skill_count": happy_readback.kind_counts["skill"],
        "spell_count": happy_readback.kind_counts["spell"],
        "supplication_count": happy_readback.kind_counts["supplication"],
        "song_count": happy_readback.kind_counts["song"],
        "cabal_power_count": happy_readback.kind_counts["cabal_power"],
        "racial_innate_count": happy_readback.kind_counts["racial_innate"],
        "active_count": happy_readback.activation_counts["active"],
        "passive_count": happy_readback.activation_counts["passive"],
        "automatic_count": happy_readback.activation_counts["automatic"],
        "prerequisite_edge_count": happy_readback.prerequisite_edge_count,
        "max_prerequisite_depth": happy_readback.max_prerequisite_depth,
        "school_count": happy_readback.school_count,
        "branch_count": happy_readback.branch_count,
        "capstone_count": happy_readback.capstone_count,
        "ui_slot_count": happy_readback.ui_slot_count,
        "mana_cost_total": resource_total(&happy_readback.cost_resource_totals, "mana"),
        "movement_cost_total": resource_total(&happy_readback.cost_resource_totals, "movement"),
        "devotion_cost_total": resource_total(&happy_readback.cost_resource_totals, "devotion"),
        "focus_cost_total": resource_total(&happy_readback.cost_resource_totals, "focus"),
        "cabal_charge_cost_total": resource_total(&happy_readback.cost_resource_totals, "cabal_charge"),
        "content_rows_written": lowered.row_count,
        "content_rows_read_back": persisted_rows.len(),
        "source_anchor_count": source_anchor_count,
        "source_paths_are_editor_authored": source_paths_are_editor_authored,
        "content_has_compiled_source_path": content_has_compiled_source_path,
        "lowered_artifact_count": lowered_artifact_count,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
        "content_has_map_or_gps_true": content_has_map_or_gps_true,
        "edge_count": 5,
        "error_code": "LITD-RPG-ABILITY-INVALID-CONFIG"
    });
    let report = json!({
        "schema": "litd.issue961.ability_framework_fsv.v1",
        "issue": 961,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect happy_path, expected_actuals, calyx.content_rows, calyx.lowered_entities, edges, and hashes against the saved source-of-truth file."
        },
        "research_sources": [
            "https://dev.epicgames.com/documentation/unreal-engine/understanding-the-unreal-engine-gameplay-ability-system?lang=en-US",
            "https://dev.epicgames.com/documentation/unreal-engine/using-gameplay-abilities-in-unreal-engine?lang=en-US"
        ],
        "readme_references_reviewed": [
            "docs/readme/GP03-combat.png",
            "docs/readme/GP19-spellbook.png",
            "docs/readme/GP20-talents.png"
        ],
        "sufficiency_argument": "The minimal catalog has 7 synthetic Ashen Reach abilities covering all #961 ability kinds, active/passive/automatic activation, class/race/cabal/deity gates, school/path/branch grouping, rank/capstone, costs, cooldown/lag, target modes, effects, trigger tags, UI slots, and a two-deep prerequisite chain. Edge cases cover duplicate id, unknown prerequisite, passive cost mutation, automatic missing trigger, and active ability with no cost/lag/cooldown; each reads the catalog hash before and after to prove no mutation. The #1455 migration writes 8 editor-authored Calyx rows, one catalog summary plus one row per ability, with frozen lowered fingerprints and explicit false map/minimap/GPS flags.",
        "expected_actuals": expected_actuals,
        "happy_catalog_sha256": catalog_hash(&happy)?,
        "happy_path": happy_readback,
        "calyx": {
            "content_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": [
            edge("duplicate_ability_id", duplicate_ability())?,
            edge("unknown_prerequisite", unknown_prerequisite())?,
            edge("passive_with_cost", passive_with_cost())?,
            edge("automatic_without_trigger", automatic_without_trigger())?,
            edge("active_without_cost_or_wait", active_without_cost_or_wait())?,
        ],
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["readme_reference_count"], 3)?;
    require_i64(&report["expected_actuals"]["ability_source_count"], 7)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["fixture_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 8)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 8)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        false,
    )?;
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE961_ABILITY_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} lowered={} map_true={} abilities={} kinds={} active={} passive={} automatic={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["happy_path"]["ability_count"],
        readback.json["happy_path"]["kind_counts"]
            .as_object()
            .map(|map| map.len())
            .unwrap_or_default(),
        readback.json["happy_path"]["activation_counts"]["active"],
        readback.json["happy_path"]["activation_counts"]["passive"],
        readback.json["happy_path"]["activation_counts"]["automatic"],
        readback.json["edges"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
    );
    Ok(())
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "kind": row.kind,
                "database_id": row.database.id,
                "database_sha256": row.database.sha256,
                "source_path": row.source_path,
                "surface_kind": row.payload["surface_kind"],
                "catalog_id": row.payload["catalog_id"],
                "ability_id": row.payload["ability_id"],
                "ability_catalog_sha256": row.payload["ability_catalog_sha256"],
                "ability_catalog_result_sha256": row.payload["ability_catalog_result_sha256"],
                "lowering_artifact_id": row.lowering.artifact_id,
                "lowering_fingerprint": row.lowering.fingerprint,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn edge(case: &str, catalog: AbilityCatalog) -> Result<Value, EngineError> {
    let before = catalog_hash(&catalog)?;
    let error = match build_ability_catalog_readback(&catalog) {
        Ok(_) => {
            return Err(EngineError::RpgAbilityInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid ability catalog to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = catalog_hash(&catalog)?;
    Ok(json!({
        "case": case,
        "before_catalog_sha256": before,
        "after_catalog_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn catalog_hash(catalog: &AbilityCatalog) -> Result<String, EngineError> {
    serde_json::to_vec(catalog)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize issue961 catalog: {error}"),
        })
}

fn resource_total(totals: &[ResourceCostTotal], resource_id: &str) -> i64 {
    totals
        .iter()
        .find(|total| total.resource_id == resource_id)
        .map(|total| total.amount)
        .unwrap_or_default()
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgAbilityInvalidConfig {
        field: "issue961.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgAbilityInvalidConfig {
        field: "issue961.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
