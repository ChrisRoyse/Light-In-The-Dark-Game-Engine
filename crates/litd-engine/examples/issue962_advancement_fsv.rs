#![recursion_limit = "256"]

#[path = "fixtures/issue962_advancement_fixture.rs"]
mod issue962_advancement_fixture;

#[path = "../src/rpg/class_specs_fixture_thief.rs"]
mod issue1750_thief_class_spec;

#[path = "../src/rpg/class_specs_fixture_raider.rs"]
mod issue1753_raider_class_spec;

use std::fs;

use issue962_advancement_fixture::{
    duplicate_learning_state, happy_input, happy_rules, insufficient_practices,
    study_insufficient_points, train_attribute_cap_exceeded, unknown_ability_use,
};
use litd_engine::{
    EngineError,
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AdvancementInput, AdvancementRules, advancement_content_records, apply_advancement_plan,
        apply_class_spec_advancement_plan, persist_advancement_readback,
        persist_and_lower_rpg_authored_content, read_persisted_advancement_readback,
        validate_class_race_access, validate_rpg_authored_content_records,
    },
};
use serde_json::{Value, json};

const ADVANCEMENT_PATH: &str = "crates/litd-engine/src/rpg/advancement.rs";
const APPLY_PATH: &str = "crates/litd-engine/src/rpg/advancement_apply.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/advancement_content.rs";
const READBACK_PATH: &str = "crates/litd-engine/src/rpg/advancement_readback.rs";
const STATE_STORE_PATH: &str = "crates/litd-engine/src/rpg/advancement_state_store.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/advancement_types.rs";
const VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/advancement_validate.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/advancement_tests.rs";
const TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/advancement_test_fixture.rs";
const EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue962_advancement_fixture.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue962_advancement_fsv.rs";
const SOURCE_PATHS: [&str; 11] = [
    ADVANCEMENT_PATH,
    APPLY_PATH,
    CONTENT_PATH,
    READBACK_PATH,
    STATE_STORE_PATH,
    TYPES_PATH,
    VALIDATE_PATH,
    TEST_PATH,
    TEST_FIXTURE_PATH,
    EXAMPLE_FIXTURE_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue962_ability_advancement")?;
    let input = happy_input();
    let rules = happy_rules();
    let happy = apply_advancement_plan(&input, &rules)?;
    let class_spec = issue1750_thief_class_spec::thief_class_spec();
    validate_class_race_access(&class_spec, &issue1750_thief_class_spec::known_races())?;
    let mut class_input = input.clone();
    class_input.class_id = class_spec.class_id.clone();
    let class_boundary = apply_class_spec_advancement_plan(&class_input, &rules, &class_spec)
        .expect_err("a cross-class ability must fail at the ClassSpec boundary");
    let raider_spec = issue1753_raider_class_spec::raider_class_spec();
    validate_class_race_access(&raider_spec, &issue1753_raider_class_spec::known_races())?;
    let mut exact_cap_input = input.clone();
    exact_cap_input.class_id = raider_spec.class_id.clone();
    exact_cap_input.from_level = 39;
    exact_cap_input.to_level = 40;
    exact_cap_input.starting.level = 39;
    exact_cap_input.actions.clear();
    let exact_cap = apply_class_spec_advancement_plan(&exact_cap_input, &rules, &raider_spec)?;
    let mut beyond_cap_input = exact_cap_input;
    beyond_cap_input.from_level = 40;
    beyond_cap_input.to_level = 41;
    beyond_cap_input.starting.level = 40;
    let beyond_cap_error =
        apply_class_spec_advancement_plan(&beyond_cap_input, &rules, &raider_spec)
            .expect_err("Raider advancement beyond level 40 must fail closed");
    let records = advancement_content_records(&rules, &input.ability_catalog)?;
    let validation = validate_rpg_authored_content_records(&records)?;
    let vault_dir = run.artifact_dir("calyx-vault-advancement")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let persisted_state = persist_advancement_readback(&vault_dir, &happy)?;
    let reread_state = read_persisted_advancement_readback(&vault_dir, &happy.character_id)?;
    if persisted_state.readback != happy || reread_state.readback != happy {
        return Err(
            "persisted advancement state did not independently read back identically".into(),
        );
    }
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
        "schema": "litd.issue962.ability_advancement_fsv.v2",
        "issue": 962,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect happy_path, authored_validation, lowered_advancement, expected_actuals, edges, and hashes against the saved source-of-truth file."
        },
        "research_sources": [
            "https://wiki.carrionfields.net/game/newbie-in-depth",
            "https://carrionfields.net/helpsearch.php?id=2535",
            "https://github.com/Yuffster/CircleMUD/blob/master/lib/text/help/commands.hlp"
        ],
        "readme_references_reviewed": [
            "docs/readme/GP03-combat.png",
            "docs/readme/GP19-spellbook.png",
            "docs/readme/GP20-talents.png"
        ],
        "sufficiency_argument": "The fixture covers the smallest #962 advancement loop that exercises every requested mechanism: WIS-based practice awards over two level gains, train-session awards, thief-style study point awards, study unlocking, practice gain from INT, improve-through-use, focused skill-learn bonus, level proficiency cap, stat training, vital training, and >=3 fail-loud edge classes. It excludes large class tables because #961 already proves catalog shape; this run proves the advancement state transitions.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "advancement_source_count": source_count_for("crates/litd-engine/src/rpg/advancement"),
            "example_source_count": source_count_for("crates/litd-engine/examples"),
            "fixture_source_count": source_count_for("fixture"),
            "levels_gained": 2,
            "practice_per_level": 4,
            "practice_awarded": 8,
            "trains_awarded": 2,
            "study_points_awarded": 4,
            "level_proficiency_cap": 66,
            "final_practices": 7,
            "final_trains": 0,
            "final_study_points": 1,
            "final_dexterity": 18,
            "final_mana": 112,
            "final_cinder_bash_proficiency": 66,
            "focused_skill_learn_bonus": 2,
            "action_count": 5,
            "edge_count": 5,
            "authored_record_count": validation.record_count,
            "lowered_row_count": lowered.row_count,
            "acquired_map_record_count": validation.acquired_map_record_count,
            "all_editor_authored_sources": validation.all_editor_authored_sources,
            "no_builtin_map_runtime": validation.no_builtin_map_runtime,
            "calyx_live_query_allowed_in_hot_path": lowered.calyx_live_query_allowed_in_hot_path,
            "character_state_bytes": reread_state.bytes,
            "character_state_sha256": reread_state.sha256,
            "character_state_readback_identical": reread_state.readback == happy,
            "class_boundary_error_code": class_boundary.code(),
            "class_boundary_rejected_cross_class_ability": class_boundary
                .to_string()
                .contains("not authored for class"),
            "raider_max_level": raider_spec.max_level,
            "raider_exact_cap_level": exact_cap.final_state.level,
            "raider_beyond_cap_error_code": beyond_cap_error.code(),
            "raider_beyond_cap_rejected": beyond_cap_error
                .to_string()
                .contains("stops at level 40"),
            "error_code": "LITD-RPG-ADVANCEMENT-INVALID-CONFIG"
        },
        "happy_input_sha256": hash_json(&input)?,
        "happy_rules_sha256": hash_json(&rules)?,
        "happy_path": happy,
        "authored_validation": validation,
        "lowered_advancement": {
            "package_fingerprint": lowered.package_fingerprint,
            "entities": lowered.entities,
        },
        "edges": [
            edge("duplicate_learning_state", duplicate_learning_state())?,
            edge("insufficient_practices", insufficient_practices())?,
            edge("train_attribute_cap_exceeded", train_attribute_cap_exceeded())?,
            edge("unknown_ability_use", unknown_ability_use())?,
            edge("study_insufficient_points", study_insufficient_points())?,
        ],
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["advancement_source_count"], 9)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["fixture_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["authored_record_count"], 8)?;
    require_i64(&report["expected_actuals"]["lowered_row_count"], 8)?;
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
    require_bool(
        &report["expected_actuals"]["character_state_readback_identical"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["class_boundary_rejected_cross_class_ability"],
        true,
    )?;
    require_i64(&report["expected_actuals"]["raider_max_level"], 40)?;
    require_i64(&report["expected_actuals"]["raider_exact_cap_level"], 40)?;
    require_bool(
        &report["expected_actuals"]["raider_beyond_cap_rejected"],
        true,
    )?;
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE962_ADVANCEMENT_FSV report={} bytes={} sha256={} practices={} trains={} study={} proficiency={} rows={} state_bytes={} state_sha256={} no_builtin_map={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["final_state"]["pools"]["practices"],
        readback.json["happy_path"]["final_state"]["pools"]["trains"],
        readback.json["happy_path"]["final_state"]["pools"]["study_points"],
        readback.json["expected_actuals"]["final_cinder_bash_proficiency"],
        readback.json["expected_actuals"]["lowered_row_count"],
        readback.json["expected_actuals"]["character_state_bytes"],
        readback.json["expected_actuals"]["character_state_sha256"],
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

fn edge(
    case: &str,
    (input, rules): (AdvancementInput, AdvancementRules),
) -> Result<Value, EngineError> {
    let before_input = hash_json(&input)?;
    let before_rules = hash_json(&rules)?;
    let error = match apply_advancement_plan(&input, &rules) {
        Ok(_) => {
            return Err(EngineError::RpgAdvancementInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid advancement input to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    Ok(json!({
        "case": case,
        "before_input_sha256": before_input,
        "after_input_sha256": hash_json(&input)?,
        "before_rules_sha256": before_rules,
        "after_rules_sha256": hash_json(&rules)?,
        "state_unchanged": before_input == hash_json(&input)? && before_rules == hash_json(&rules)?,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn hash_json<T: serde::Serialize>(value: &T) -> Result<String, EngineError> {
    serde_json::to_vec(value)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize issue962 advancement FSV value: {error}"),
        })
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
