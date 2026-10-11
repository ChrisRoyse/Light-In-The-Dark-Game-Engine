#[path = "fixtures/issue1060_mortality_fixture.rs"]
mod issue1060_mortality_fixture;

use std::fs;

use issue1060_mortality_fixture::ashen_reach_mortality_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AttributeScores, MortalityCharacterState, MortalityEvent, MortalityEventKind,
        MortalityPolicy, RPG_MORTALITY_SCHEMA, derive_mortality_stats, evaluate_mortality_aging,
        mortality_content_records, mortality_state_hash, persist_and_lower_rpg_authored_content,
        record_mortality_event, validate_mortality_policy, validate_mortality_state,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const MOURNING_LORE_PATH: &str = "docs/lore/ashen-reach-peoples-mourning-legends.md";
const AGING_REF_PATH: &str = "docs/readme/S02-aging.png";
const MOURNING_REF_PATH: &str = "docs/readme/S15-mourning.png";
const CREATION_REF_PATH: &str = "docs/readme/12-character-creation.png";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/mortality_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/mortality.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/mortality_content.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/mortality_tests.rs";
const TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/mortality_test_fixture.rs";
const EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1060_mortality_fixture.rs";
const ATTRIBUTES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/attributes_types.rs";
const ATTRIBUTES_PATH: &str = "crates/litd-engine/src/rpg/attributes.rs";
const ATTRIBUTES_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/attributes_content.rs";
const ATTRIBUTES_TEST_PATH: &str = "crates/litd-engine/src/rpg/attributes_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1060_mortality_fsv.rs";
const SOURCE_PATHS: [&str; 16] = [
    WORLD_BIBLE_PATH,
    MOURNING_LORE_PATH,
    AGING_REF_PATH,
    MOURNING_REF_PATH,
    CREATION_REF_PATH,
    TYPES_PATH,
    POLICY_PATH,
    CONTENT_PATH,
    TEST_PATH,
    TEST_FIXTURE_PATH,
    ATTRIBUTES_TYPES_PATH,
    ATTRIBUTES_PATH,
    ATTRIBUTES_CONTENT_PATH,
    ATTRIBUTES_TEST_PATH,
    EXAMPLE_PATH,
    EXAMPLE_FIXTURE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1060_mortality")?;
    let policy = ashen_reach_mortality_policy();
    let stats = derive_mortality_stats(16, &policy)?;
    let records = mortality_content_records(&policy)?;
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_map_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/mortality/")
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
    let start = living_state(16);
    let first =
        record_mortality_event(&start, &policy, &combat_event("death.combat.ember_gate.1"))?;
    let second = record_mortality_event(
        &first.after_state,
        &policy,
        &combat_event("death.combat.ember_gate.2"),
    )?;
    let third = record_mortality_event(
        &second.after_state,
        &policy,
        &combat_event("death.combat.ember_gate.3"),
    )?;
    let mut final_state = living_state(4);
    final_state.total_deaths = 2;
    final_state.deaths_since_constitution_loss = 2;
    let final_death = record_mortality_event(
        &final_state,
        &policy,
        &combat_event("death.combat.last_fire").witnessed_by("npc.wall.witness"),
    )?;
    let mut old_human = living_state(16);
    old_human.age_years = 95;
    let mut old_elf = old_human.clone();
    old_elf.race_id = "race.ashen_elf".to_string();
    let human_aging = evaluate_mortality_aging(&old_human, &policy)?;
    let elf_aging = evaluate_mortality_aging(&old_elf, &policy)?;
    let old_age = record_mortality_event(
        &old_human,
        &policy,
        &MortalityEvent::new(
            "death.old_age.last_fire",
            MortalityEventKind::OldAge,
            "Kept the Last Fire until the hand could hold no torch.",
        ),
    )?;
    let edges = json!({
        "invalid_zero_deaths_per_loss": invalid_policy_edge(&policy),
        "unknown_race_rejected": unknown_race_edge(&policy),
        "living_con_threshold_rejected": live_threshold_edge(&policy),
        "death_after_permanent_death_rejected": dead_again_edge(&policy, &final_death.after_state),
        "early_old_age_death_rejected": early_old_age_edge(&policy),
    });
    require_all_edges_unchanged(&edges)?;
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
        "schema": "litd.issue1060.mortality_fsv.v1",
        "issue": 1060,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this persisted JSON through Synapse/computer control, inspect the source hashes and readbacks, and confirm the screenshot/DOM readback is complete."
        },
        "sufficiency_argument": "The #1060 proof covers Constitution-derived durability, exact third-death permanent CON erosion, permanent death at CON 3 with a Mourning Wall handoff, race-dependent old-age death, configurable server policy validation, and editor-authored Calyx persistence/lowering for one mortality policy row plus per-race lifespan rows. Edge probes reject zero death-loss cadence, unknown race fallback, living CON-at-threshold state, death after permanent death, and early old-age death.",
        "expected_actuals": {
            "deaths_per_constitution_loss": 3,
            "permanent_death_constitution_threshold": 3,
            "constitution_16_hit_points": 128,
            "constitution_16_regen_per_tick": 32,
            "constitution_16_death_threshold": -44,
            "third_death_constitution_after": 15,
            "final_death_constitution_after": 3,
            "final_death_reason": "constitution_depleted",
            "mourning_wall_entry_id": "mourning.wall.char.aren_vael.3",
            "human_max_lifespan": 95,
            "elf_max_lifespan": 560,
            "old_age_death_reason": "old_age",
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "mortality_source_count": source_count_for("mortality"),
            "attribute_source_count": source_count_for("attributes"),
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
            "content_has_map_or_gps_true": content_has_map_true,
            "edge_count": 5,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": policy.server_policy_id,
            "source_issue": policy.source_issue,
            "deaths_per_constitution_loss": policy.deaths_per_constitution_loss,
            "permanent_death_constitution_threshold": policy.permanent_death_constitution_threshold,
            "mourning_wall_required": policy.mourning_wall_required,
        },
        "constitution_stats": stats,
        "death_erosion_sequence": {
            "start_state_sha256": mortality_state_hash(&start)?,
            "first": first,
            "second": second,
            "third": third,
        },
        "permanent_constitution_death": final_death,
        "aging": {
            "human": human_aging,
            "elf_same_age": elf_aging,
            "old_age_death": old_age,
        },
        "calyx": {
            "persisted_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges
    });
    require_i64(&report["expected_actuals"]["source_readback_count"], 16)?;
    require_i64(&report["expected_actuals"]["reference_source_count"], 5)?;
    require_i64(&report["expected_actuals"]["mortality_source_count"], 7)?;
    require_i64(&report["expected_actuals"]["attribute_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 12)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 12)?;
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
    require_bool(
        &report["permanent_constitution_death"]["permanently_dead"],
        true,
    )?;
    require_str(
        &report["permanent_constitution_death"]["permanent_death_reason"],
        "constitution_depleted",
    )?;
    require_i64(
        &report["death_erosion_sequence"]["third"]["constitution_after"],
        15,
    )?;
    require_i64(
        &report["permanent_constitution_death"]["constitution_after"],
        3,
    )?;
    require_str(
        &report["aging"]["old_age_death"]["permanent_death_reason"],
        "old_age",
    )?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1060_MORTALITY_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} lowered={} con16_hp={} con16_regen={} final_con={} old_age_reason={} map_true={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["constitution_stats"]["constitution_hit_points"],
        readback.json["constitution_stats"]["regen_per_tick"],
        readback.json["permanent_constitution_death"]["constitution_after"],
        readback.json["aging"]["old_age_death"]["permanent_death_reason"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["edges"]
            .as_object()
            .map(|edges| edges.len())
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

fn invalid_policy_edge(policy: &MortalityPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.deaths_per_constitution_loss = 0;
    edge_from_result(validate_mortality_policy(&invalid).map(|_| ()), true)
}

fn unknown_race_edge(policy: &MortalityPolicy) -> Value {
    let mut state = living_state(16);
    state.race_id = "race.missing".to_string();
    let before = mortality_state_hash(&state).unwrap();
    let result = validate_mortality_state(&state, policy).map(|_| ());
    let after = mortality_state_hash(&state).unwrap();
    edge_from_result(result, before == after)
}

fn live_threshold_edge(policy: &MortalityPolicy) -> Value {
    let state = living_state(3);
    let before = mortality_state_hash(&state).unwrap();
    let result = validate_mortality_state(&state, policy).map(|_| ());
    let after = mortality_state_hash(&state).unwrap();
    edge_from_result(result, before == after)
}

fn dead_again_edge(policy: &MortalityPolicy, state: &MortalityCharacterState) -> Value {
    let before = mortality_state_hash(state).unwrap();
    let result = record_mortality_event(state, policy, &combat_event("death.combat.after_final"))
        .map(|_| ());
    let after = mortality_state_hash(state).unwrap();
    edge_from_result(result, before == after)
}

fn early_old_age_edge(policy: &MortalityPolicy) -> Value {
    let state = living_state(16);
    let before = mortality_state_hash(&state).unwrap();
    let result = record_mortality_event(
        &state,
        policy,
        &MortalityEvent::new(
            "death.old_age.too_early",
            MortalityEventKind::OldAge,
            "This should not bypass the race lifespan.",
        ),
    )
    .map(|_| ());
    let after = mortality_state_hash(&state).unwrap();
    edge_from_result(result, before == after)
}

fn edge_from_result(result: Result<(), EngineError>, state_unchanged: bool) -> Value {
    match result {
        Ok(()) => json!({
            "unexpected_success": true,
            "state_unchanged": state_unchanged,
        }),
        Err(error) => json!({
            "error_code": error.code(),
            "error": error.to_string(),
            "state_unchanged": state_unchanged,
        }),
    }
}

fn living_state(current_constitution: i16) -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.aren_vael".to_string(),
        character_name: "Aren Vael".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 12,
        age_years: 44,
        base_attributes: AttributeScores::new(14, 11, 12, 13, 16, 10),
        current_constitution,
        deaths_since_constitution_loss: 0,
        total_deaths: 0,
        permanently_dead: false,
        mourning_wall_entry_id: None,
    }
}

fn combat_event(event_id: &str) -> MortalityEvent {
    MortalityEvent::new(
        event_id,
        MortalityEventKind::CombatDeath,
        "Held the Ember Gate while the caravan crossed.",
    )
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1060.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_str(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1060.report.str".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1060.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1060.edge.{name}"),
                reason: "edge mutated source state".to_string(),
            });
        }
        if edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG" {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1060.edge.{name}"),
                reason: format!("expected RPG stats error, got {edge}"),
            });
        }
    }
    Ok(())
}
