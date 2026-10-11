#[path = "fixtures/issue963_combat_fixture.rs"]
mod issue963_combat_fixture;

use std::fs;

use issue963_combat_fixture::{
    command_out_of_range, duplicate_defender_skill, duplicate_defense_order, exhausted_roll_stream,
    happy_input, happy_rules, invalid_weapon_damage,
};
use litd_engine::{
    EngineError,
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        CombatInput, CombatReadback, CombatRules, combat_content_records,
        persist_and_lower_rpg_authored_content, resolve_combat_plan,
        validate_rpg_authored_content_records,
    },
};
use serde_json::{Value, json};

const COMBAT_PATH: &str = "crates/litd-engine/src/rpg/combat.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/combat_content.rs";
const READBACK_PATH: &str = "crates/litd-engine/src/rpg/combat_readback.rs";
const RESOLVE_PATH: &str = "crates/litd-engine/src/rpg/combat_resolve.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/combat_types.rs";
const VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/combat_validate.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/combat_tests.rs";
const TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/combat_test_fixture.rs";
const EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue963_combat_fixture.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue963_combat_fsv.rs";
const SOURCE_PATHS: [&str; 10] = [
    COMBAT_PATH,
    CONTENT_PATH,
    READBACK_PATH,
    RESOLVE_PATH,
    TYPES_PATH,
    VALIDATE_PATH,
    TEST_PATH,
    TEST_FIXTURE_PATH,
    EXAMPLE_FIXTURE_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue963_realtime_combat")?;
    let input = happy_input();
    let rules = happy_rules();
    let happy = resolve_combat_plan(&input, &rules)?;
    let records = combat_content_records(&rules)?;
    let validation = validate_rpg_authored_content_records(&records)?;
    let vault_dir = run.artifact_dir("calyx-vault-combat")?;
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
        "schema": "litd.issue963.realtime_combat_fsv.v2",
        "issue": 963,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect happy_path, authored_validation, lowered_combat, expected_actuals, edges, and hashes against the saved source-of-truth file."
        },
        "research_sources": [
            "https://carrionfields.net/helpsearch.php?id=509",
            "https://wiki.carrionfields.net/class/warrior",
            "https://www.circlemud.org/cdp/wtfaq/level.html"
        ],
        "readme_references_reviewed": [
            "docs/readme/GP03-combat.png",
            "docs/readme/GP19-spellbook.png",
            "docs/readme/GP20-talents.png"
        ],
        "sufficiency_argument": "The fixture is the smallest known-roll combat loop that exercises every #963 mechanism: real-time 500ms ticks into 1500ms rounds, second/third/fourth multi-attack checks, hit vs AC with hitroll, damroll damage, ordered distance->spin->evade->parry->shield_block->dodge defense attempts, weapon matchup and weapon specialization modifiers, heavy-armor mitigation, a bash command with lag/wait-state, and >=3 fail-loud invalid classes. Larger class/weapon tables are deliberately excluded because this run proves the universal transition contract.",
        "expected_actuals": expected_actuals(
            &happy,
            &validation,
            &lowered,
            source_readback_count,
            source_count_for("crates/litd-engine/src/rpg/combat"),
            source_count_for("crates/litd-engine/examples"),
            source_count_for("fixture"),
        ),
        "happy_input_sha256": hash_json(&input)?,
        "happy_rules_sha256": hash_json(&rules)?,
        "happy_path": happy,
        "authored_validation": validation,
        "lowered_combat": {
            "package_fingerprint": lowered.package_fingerprint,
            "entities": lowered.entities,
        },
        "edges": [
            edge("duplicate_defender_skill", duplicate_defender_skill())?,
            edge("exhausted_roll_stream", exhausted_roll_stream())?,
            edge("duplicate_defense_order", duplicate_defense_order())?,
            edge("invalid_weapon_damage", invalid_weapon_damage())?,
            edge("command_out_of_range", command_out_of_range())?,
        ],
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["combat_source_count"], 8)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["fixture_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["authored_record_count"], 26)?;
    require_i64(&report["expected_actuals"]["lowered_row_count"], 26)?;
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
        "ISSUE963_COMBAT_FSV report={} bytes={} sha256={} damage={} defender_hp={} attacks={} rows={} no_builtin_map={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["total_damage"],
        readback.json["expected_actuals"]["final_defender_hp"],
        readback.json["expected_actuals"]["attack_count"],
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

fn expected_actuals(
    readback: &CombatReadback,
    validation: &litd_engine::rpg::RpgAuthoredContentValidationReadback,
    lowered: &litd_engine::rpg::RpgAuthoredContentLoweringReadback,
    source_readback_count: usize,
    combat_source_count: usize,
    example_source_count: usize,
    fixture_source_count: usize,
) -> Value {
    let attack_count: usize = readback
        .rounds
        .iter()
        .map(|round| round.attacks.len())
        .sum();
    let damaged_hit_count = readback
        .rounds
        .iter()
        .flat_map(|round| round.attacks.iter())
        .filter(|attack| attack.mitigated_damage > 0)
        .count();
    let defense_success_count = readback
        .rounds
        .iter()
        .flat_map(|round| round.attacks.iter())
        .filter(|attack| attack.defended_by.is_some())
        .count();
    let wait_skip_count = readback
        .rounds
        .iter()
        .filter(|round| round.skipped_by_wait_state)
        .count();
    let multi_granted_count = readback
        .multi_attack_checks
        .iter()
        .filter(|check| check.granted)
        .count();
    json!({
        "source_readback_count": source_readback_count,
        "combat_source_count": combat_source_count,
        "example_source_count": example_source_count,
        "fixture_source_count": fixture_source_count,
        "round_millis": 1500,
        "requested_rounds": 3,
        "resolved_rounds": 3,
        "attack_count": attack_count,
        "multi_attack_check_count": readback.multi_attack_checks.len(),
        "multi_granted_count": multi_granted_count,
        "defense_success_count": defense_success_count,
        "damaged_hit_count": damaged_hit_count,
        "wait_skip_count": wait_skip_count,
        "total_damage": readback.total_damage,
        "final_defender_hp": readback.final_defender.hit_points,
        "rolls_consumed": readback.rolls_consumed,
        "edge_count": 5,
        "authored_record_count": validation.record_count,
        "lowered_row_count": lowered.row_count,
        "acquired_map_record_count": validation.acquired_map_record_count,
        "all_editor_authored_sources": validation.all_editor_authored_sources,
        "no_builtin_map_runtime": validation.no_builtin_map_runtime,
        "calyx_live_query_allowed_in_hot_path": lowered.calyx_live_query_allowed_in_hot_path,
        "error_code": "LITD-RPG-COMBAT-INVALID-CONFIG"
    })
}

fn edge(case: &str, (input, rules): (CombatInput, CombatRules)) -> Result<Value, EngineError> {
    let before_input = hash_json(&input)?;
    let before_rules = hash_json(&rules)?;
    let error = match resolve_combat_plan(&input, &rules) {
        Ok(_) => {
            return Err(EngineError::RpgCombatInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid combat input to fail".to_string(),
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
            reason: format!("serialize issue963 combat FSV value: {error}"),
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
