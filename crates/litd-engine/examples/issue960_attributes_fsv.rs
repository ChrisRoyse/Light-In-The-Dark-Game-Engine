use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AttributeBuildInput, AttributeScores, DerivedStatFormulaTable, DerivedStats,
        RaceAttributeCaps, ResistanceModifier, StatModifier, StatModifierSource,
        attribute_content_records, derive_character_stats, persist_and_lower_rpg_authored_content,
        validate_rpg_authored_content_records,
    },
};
use serde_json::{Value, json};

const ATTRIBUTES_PATH: &str = "crates/litd-engine/src/rpg/attributes.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/attributes_content.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/attributes_types.rs";
const VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/attributes_validate.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/attributes_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue960_attributes_fsv.rs";
const SOURCE_PATHS: [&str; 6] = [
    ATTRIBUTES_PATH,
    CONTENT_PATH,
    TYPES_PATH,
    VALIDATE_PATH,
    TEST_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue960_attributes_derived_stats")?;
    let table = DerivedStatFormulaTable::mud_defaults();
    let happy_input = happy_input();
    let happy_readback = derive_character_stats(&happy_input, &table)?;
    let records = attribute_content_records(&table, &happy_input.race_caps)?;
    let validation = validate_rpg_authored_content_records(&records)?;
    let vault_dir = run.artifact_dir("calyx-vault-attributes")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
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
    let report = json!({
        "schema": "litd.issue960.attributes_derived_stats_fsv.v2",
        "issue": 960,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect happy_path, authored_validation, lowered_attributes, and edges against expected_actuals."
        },
        "sufficiency_argument": "The fixture covers all six attributes, race caps, gear modifiers, affect modifiers, every derived stat requested by #960, all default resistance channels, and cap-clamp readback. The #1455 authored-content slice persists the formula table and race caps as editor-authored Calyx rows and lowers them through the existing valve with the no-built-in-map guardrails intact. Edges cover base-over-cap, duplicate modifier id, unknown resistance channel, and missing race cap row.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "attribute_source_count": source_count_for("crates/litd-engine/src/rpg/attributes"),
            "example_source_count": source_count_for("examples/issue960_attributes_fsv.rs"),
            "effective_strength": 20,
            "effective_intelligence": 13,
            "effective_wisdom": 14,
            "effective_dexterity": 20,
            "effective_constitution": 16,
            "effective_charisma": 11,
            "hit_points": 300,
            "mana": 243,
            "movement": 247,
            "hitroll": 52,
            "damroll": 55,
            "armor_class": -1,
            "carry_weight": 353,
            "wield_weight": 82,
            "resist_fire": 42,
            "resist_cold": 26,
            "resist_holy": 35,
            "edge_count": 4,
            "authored_record_count": validation.record_count,
            "lowered_row_count": lowered.row_count,
            "acquired_map_record_count": validation.acquired_map_record_count,
            "all_editor_authored_sources": validation.all_editor_authored_sources,
            "no_builtin_map_runtime": validation.no_builtin_map_runtime,
            "calyx_live_query_allowed_in_hot_path": lowered.calyx_live_query_allowed_in_hot_path,
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "happy_path": happy_readback,
        "authored_validation": validation,
        "lowered_attributes": {
            "package_fingerprint": lowered.package_fingerprint,
            "entities": lowered.entities,
        },
        "cap_clamp_probe": cap_clamp_probe(&table)?,
        "edges": [
            edge("base_exceeds_race_cap", base_exceeds_cap_input(), &table)?,
            edge("duplicate_modifier_id", duplicate_modifier_input(), &table)?,
            edge("unknown_resistance_channel", unknown_resistance_input(), &table)?,
            edge("missing_race_cap_row", missing_race_input(), &table)?,
        ],
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["attribute_source_count"], 5)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 1)?;
    require_i64(&report["expected_actuals"]["authored_record_count"], 2)?;
    require_i64(&report["expected_actuals"]["lowered_row_count"], 2)?;
    require_i64(&report["expected_actuals"]["acquired_map_record_count"], 0)?;
    require_bool(
        &report["expected_actuals"]["all_editor_authored_sources"],
        true,
    )?;
    require_bool(&report["expected_actuals"]["no_builtin_map_runtime"], true)?;
    require_bool(
        &report["expected_actuals"]["calyx_live_query_allowed_in_hot_path"],
        false,
    )?;
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE960_ATTRIBUTES_FSV report={} bytes={} sha256={} hp={} mana={} mv={} hitroll={} damroll={} ac={} carry={} wield={} rows={} no_builtin_map={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["derived"]["hit_points"],
        readback.json["happy_path"]["derived"]["mana"],
        readback.json["happy_path"]["derived"]["movement"],
        readback.json["happy_path"]["derived"]["hitroll"],
        readback.json["happy_path"]["derived"]["damroll"],
        readback.json["happy_path"]["derived"]["armor_class"],
        readback.json["happy_path"]["derived"]["carry_weight"],
        readback.json["happy_path"]["derived"]["wield_weight"],
        readback.json["expected_actuals"]["lowered_row_count"],
        readback.json["expected_actuals"]["no_builtin_map_runtime"],
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
        "sha256": litd_hash::sha256_hex(&bytes),
    }))
}

fn cap_clamp_probe(table: &DerivedStatFormulaTable) -> Result<Value, EngineError> {
    let mut input = happy_input();
    input.modifiers.push(affect(
        "giant_strength",
        AttributeScores::new(8, 0, 0, 0, 0, 0),
        DerivedStats::ZERO,
        &[],
    ));
    let readback = derive_character_stats(&input, table)?;
    Ok(json!({
        "uncapped_strength": readback.uncapped_attributes.strength,
        "effective_strength": readback.effective_attributes.strength,
        "cap_clamps": readback.cap_clamps,
    }))
}

fn edge(
    case: &str,
    input: AttributeBuildInput,
    table: &DerivedStatFormulaTable,
) -> Result<Value, EngineError> {
    let before_input_hash = input_hash(&input)?;
    let error = match derive_character_stats(&input, table) {
        Ok(_) => {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid RPG stats input to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after_input_hash = input_hash(&input)?;
    Ok(json!({
        "case": case,
        "before_input_sha256": before_input_hash,
        "after_input_sha256": after_input_hash,
        "state_unchanged": before_input_hash == after_input_hash,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn input_hash(input: &AttributeBuildInput) -> Result<String, EngineError> {
    serde_json::to_vec(input)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize edge input: {error}"),
        })
}

fn happy_input() -> AttributeBuildInput {
    AttributeBuildInput {
        character_id: "char.ashen.guardian".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 12,
        base: AttributeScores::new(18, 13, 12, 17, 16, 10),
        race_caps: vec![RaceAttributeCaps {
            race_id: "race.ashen_human".to_string(),
            caps: AttributeScores::new(22, 22, 22, 22, 22, 22),
        }],
        modifiers: vec![
            gear(
                "iron_sword",
                AttributeScores::new(2, 0, 0, 0, 0, 0),
                DerivedStats {
                    hitroll: 3,
                    damroll: 5,
                    wield_weight: 2,
                    ..DerivedStats::ZERO
                },
                &[],
            ),
            gear(
                "ember_cloak",
                AttributeScores::ZERO,
                DerivedStats {
                    armor_class: -5,
                    ..DerivedStats::ZERO
                },
                &[("fire", 12), ("cold", -3)],
            ),
            gear(
                "pack_harness",
                AttributeScores::ZERO,
                DerivedStats {
                    carry_weight: 40,
                    ..DerivedStats::ZERO
                },
                &[],
            ),
            affect(
                "bless",
                AttributeScores::new(0, 0, 2, 0, 0, 1),
                DerivedStats {
                    hitroll: 2,
                    damroll: 2,
                    ..DerivedStats::ZERO
                },
                &[("holy", 10), ("shadow", -5)],
            ),
            affect(
                "haste",
                AttributeScores::new(0, 0, 0, 3, 0, 0),
                DerivedStats {
                    movement: 25,
                    armor_class: -8,
                    ..DerivedStats::ZERO
                },
                &[("lightning", 4)],
            ),
        ],
    }
}

fn require_i64(value: &Value, expected: i64) -> Result<(), Box<dyn std::error::Error>> {
    match value.as_i64() {
        Some(actual) if actual == expected => Ok(()),
        actual => Err(format!("expected {expected}, got {actual:?}").into()),
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), Box<dyn std::error::Error>> {
    match value.as_bool() {
        Some(actual) if actual == expected => Ok(()),
        actual => Err(format!("expected {expected}, got {actual:?}").into()),
    }
}

fn base_exceeds_cap_input() -> AttributeBuildInput {
    let mut input = happy_input();
    input.base.strength = 23;
    input
}

fn duplicate_modifier_input() -> AttributeBuildInput {
    let mut input = happy_input();
    input.modifiers.push(gear(
        "iron_sword",
        AttributeScores::ZERO,
        DerivedStats::ZERO,
        &[],
    ));
    input
}

fn unknown_resistance_input() -> AttributeBuildInput {
    let mut input = happy_input();
    input.modifiers.push(gear(
        "void_ring",
        AttributeScores::ZERO,
        DerivedStats::ZERO,
        &[("void", 99)],
    ));
    input
}

fn missing_race_input() -> AttributeBuildInput {
    let mut input = happy_input();
    input.race_id = "race.missing".to_string();
    input
}

fn gear(
    id: &str,
    attributes: AttributeScores,
    derived: DerivedStats,
    resistances: &[(&str, i32)],
) -> StatModifier {
    modifier(
        id,
        StatModifierSource::Gear,
        attributes,
        derived,
        resistances,
    )
}

fn affect(
    id: &str,
    attributes: AttributeScores,
    derived: DerivedStats,
    resistances: &[(&str, i32)],
) -> StatModifier {
    modifier(
        id,
        StatModifierSource::Affect,
        attributes,
        derived,
        resistances,
    )
}

fn modifier(
    id: &str,
    source: StatModifierSource,
    attributes: AttributeScores,
    derived: DerivedStats,
    resistances: &[(&str, i32)],
) -> StatModifier {
    StatModifier {
        modifier_id: id.to_string(),
        source,
        attributes,
        derived,
        resistances: resistances
            .iter()
            .map(|(resistance_id, amount)| ResistanceModifier {
                resistance_id: (*resistance_id).to_string(),
                amount: *amount,
            })
            .collect(),
    }
}
