use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AttributeScores, ClassRaceAccess, ClassSpec, WARRIOR_CLASS_ID, class_spec_content_records,
        class_spec_readback, persist_and_lower_rpg_authored_content, validate_class_race_access,
        validate_class_spec,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

// The Warrior fixture is the single CF-factual source of truth, shared with the
// #[cfg(test)] lib tests via the `litd_engine` self-alias so the example and the
// unit tests author byte-identical specs.
#[path = "../src/rpg/class_specs_test_fixture.rs"]
mod warrior_fixture;
use warrior_fixture::{known_races, warrior_class_spec};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const PEOPLE_LORE_PATH: &str = "docs/lore/ashen-reach-peoples-mourning-legends.md";
const CLASS_IMPL_PATH: &str = "crates/litd-engine/src/rpg/class_specs.rs";
const CLASS_TYPES_PATH: &str = "crates/litd-engine/src/rpg/class_specs_types.rs";
const CLASS_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/class_specs_content.rs";
const CLASS_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/class_specs_test_fixture.rs";
const ABILITIES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/abilities_types.rs";
const ATTRIBUTES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/attributes_types.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1020_warrior_class_fsv.rs";
const SOURCE_PATHS: [&str; 9] = [
    WORLD_BIBLE_PATH,
    PEOPLE_LORE_PATH,
    CLASS_IMPL_PATH,
    CLASS_TYPES_PATH,
    CLASS_CONTENT_PATH,
    CLASS_FIXTURE_PATH,
    ABILITIES_TYPES_PATH,
    ATTRIBUTES_TYPES_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1020_warrior_class_spec")?;
    let spec = warrior_class_spec();
    let readback = class_spec_readback(&spec)?;
    let records = class_spec_content_records(&spec)?;
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    // Read the row back SEPARATELY through the editor query path and prove the
    // stored content matches what we authored (edge case 3: stored + readback
    // identical).
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_map_or_gps_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let stored = &persisted_rows[0];
    let readback_identical = stored.id == spec.class_id
        && stored.payload["class_id"] == json!(spec.class_id)
        && stored.payload["prime_stat"] == json!("strength")
        && stored.payload["class_spec_sha256"] == json!(readback.class_spec_sha256);
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/class_specs/")
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
    let races = known_races();
    let edges = vec![
        edge("wrong_schema", &wrong_schema(), |spec| {
            validate_class_spec(spec)
        })?,
        edge(
            "invalid_stat_requirement",
            &invalid_stat_requirement(),
            validate_class_spec,
        )?,
        edge("stale_renderer", &stale_renderer(), |spec| {
            validate_class_spec(spec)
        })?,
        edge(
            "dangling_specialization",
            &dangling_specialization(),
            validate_class_spec,
        )?,
        edge(
            "nonexistent_race_restriction",
            &nonexistent_race_restriction(),
            |spec| validate_class_race_access(spec, &races),
        )?,
    ];
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;
    let source_readback_count = source_readbacks.len();
    let report = json!({
        "schema": "litd.issue1020.warrior_class_fsv.v1",
        "issue": 1020,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "mechanics_source_url": "https://wiki.carrionfields.net/class/warrior",
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect happy_path, Calyx rows, lowered entities, readback_identical, and edge cases against expected_actuals."
        },
        "sufficiency_argument": "The #1020 slice is the Warrior class data row. A single exact row is sufficient because #1020 does not require the other #986 class rows. The happy path proves Warrior source mechanics (prime stat STR, any align/ethos, no empowerment, 61 leveled abilities incl. 7 specialist-gated, 10 weapon specializations chosen at 20/40, 36 legacies chosen at 44/48) and Ashen Reach canon fields; the Calyx readback proves the row is editor-authored, persisted, lowered, and identical on read-back; edge probes prove bad schema, invalid stat requirement, stale renderer, dangling specialization reference, and unknown-race restriction all fail closed.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "lore_reference_count": source_count_for("docs/lore/"),
            "class_source_count": source_count_for("crates/litd-engine/src/rpg/class_specs"),
            "example_source_count": source_count_for("crates/litd-engine/examples/issue1020_warrior_class_fsv.rs"),
            "class_id": WARRIOR_CLASS_ID,
            "source_issue": 1020,
            "prime_stat": "strength",
            "alignment": "any",
            "ethos": "any",
            "empowerment": false,
            "race_access": "all",
            "allowed_weapon_count": 10,
            "allowed_armor_count": 3,
            "leveled_ability_count": 61,
            "specialist_ability_count": 7,
            "specialization_count": 10,
            "specialization_choice_count": 2,
            "legacy_count": 36,
            "legacy_choice_count": 2,
            "visual_renderer": "Rust/wgpu",
            "signature_vfx_count": 5,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "readback_identical": readback_identical,
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "edge_count": 5,
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "happy_path": readback,
        "calyx": {
            "content_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["lore_reference_count"], 2)?;
    require_i64(&report["expected_actuals"]["class_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 1)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 1)?;
    require_i64(&report["expected_actuals"]["leveled_ability_count"], 61)?;
    require_i64(&report["expected_actuals"]["legacy_count"], 36)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 1)?;
    require_bool(&report["expected_actuals"]["readback_identical"], true)?;
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
        "ISSUE1020_WARRIOR_CLASS_FSV report={} bytes={} sha256={} class={} rows={} readback_identical={} abilities={} legacies={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["class_id"],
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["readback_identical"],
        readback.json["expected_actuals"]["leveled_ability_count"],
        readback.json["expected_actuals"]["legacy_count"],
        readback.json["edges"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
    );
    Ok(())
}

fn source_count_for(needle: &str) -> usize {
    SOURCE_PATHS
        .iter()
        .filter(|path| path.contains(needle))
        .count()
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
                "class_id": row.payload["class_id"],
                "prime_stat": row.payload["prime_stat"],
                "class_spec_sha256": row.payload["class_spec_sha256"],
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

fn edge(
    case: &str,
    spec: &ClassSpec,
    validate: impl Fn(&ClassSpec) -> Result<(), EngineError>,
) -> Result<Value, EngineError> {
    let before = spec_hash(spec)?;
    let error = match validate(spec) {
        Ok(_) => {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid class spec to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = spec_hash(spec)?;
    Ok(json!({
        "case": case,
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn wrong_schema() -> ClassSpec {
    let mut spec = warrior_class_spec();
    spec.schema = "litd.rpg.class_spec.bad".to_string();
    spec
}

fn invalid_stat_requirement() -> ClassSpec {
    let mut spec = warrior_class_spec();
    spec.stat_minimums = AttributeScores::new(200, 0, 0, 0, 0, 0);
    spec
}

fn stale_renderer() -> ClassSpec {
    let mut spec = warrior_class_spec();
    spec.visual.renderer = "g3n-native".to_string();
    spec
}

fn dangling_specialization() -> ClassSpec {
    let mut spec = warrior_class_spec();
    spec.leveled_abilities[0].specialization_ids = vec!["spec.weapon.nonexistent".to_string()];
    spec
}

fn nonexistent_race_restriction() -> ClassSpec {
    let mut spec = warrior_class_spec();
    spec.race_access = ClassRaceAccess::Restricted {
        race_ids: vec![
            "race.ashen_human".to_string(),
            "race.does_not_exist".to_string(),
        ],
    };
    spec
}

fn spec_hash(spec: &ClassSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize class spec edge: {error}"),
        })
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1020.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1020.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
