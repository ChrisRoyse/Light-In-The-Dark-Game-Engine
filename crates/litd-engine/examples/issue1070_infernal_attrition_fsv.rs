#[path = "fixtures/issue1060_mortality_fixture.rs"]
mod issue1060_mortality_fixture;
#[path = "fixtures/issue1070_infernal_attrition_edges.rs"]
mod issue1070_infernal_attrition_edges;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;
#[path = "fixtures/workspace_source_readback.rs"]
mod workspace_source_readback;

use issue1060_mortality_fixture::ashen_reach_mortality_policy;
use issue1070_infernal_attrition_edges::attrition_edges;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AttributeScores, InfernalAttritionState, InfernalAttritionStep, InfernalCircleId,
        MortalityCharacterState, RPG_INFERNAL_ATTRITION_SCHEMA, RPG_MORTALITY_SCHEMA,
        apply_infernal_attrition, infernal_attrition_content_records,
        persist_and_lower_rpg_authored_content, validate_infernal_attrition_policy,
    },
};
use serde_json::{Value, json};
use workspace_source_readback::source_readback;

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const GAZETTEER_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const INFERNAL_REF_PATH: &str = "docs/readme/S06-infernal-deeps.png";
const GAMEPLAY_REF_PATH: &str = "docs/readme/GP14-infernal.png";
const EMBER_REF_PATH: &str = "docs/readme/H03-ember-wastes.png";
const DARK_REF_PATH: &str = "docs/readme/H05-hungering-dark.png";
const TOMBS_REF_PATH: &str = "docs/readme/H06-bone-choir.png";
const RELIC_REF_PATH: &str = "docs/readme/H08-relic-vault.png";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition_content.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition_tests.rs";
const TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition_test_fixture.rs";
const EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1070_infernal_attrition_fixture.rs";
const MORTALITY_TYPES_PATH: &str = "crates/litd-engine/src/rpg/mortality_types.rs";
const MORTALITY_POLICY_PATH: &str = "crates/litd-engine/src/rpg/mortality.rs";
const MORTALITY_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/mortality_content.rs";
const MORTALITY_TEST_PATH: &str = "crates/litd-engine/src/rpg/mortality_tests.rs";
const MORTALITY_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/mortality_test_fixture.rs";
const MORTALITY_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1060_mortality_fixture.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1070_infernal_attrition_fsv.rs";
const EDGE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1070_infernal_attrition_edges.rs";
const SOURCE_PATHS: [&str; 23] = [
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    GAZETTEER_PATH,
    INFERNAL_REF_PATH,
    GAMEPLAY_REF_PATH,
    EMBER_REF_PATH,
    DARK_REF_PATH,
    TOMBS_REF_PATH,
    RELIC_REF_PATH,
    TYPES_PATH,
    POLICY_PATH,
    CONTENT_PATH,
    TEST_PATH,
    TEST_FIXTURE_PATH,
    EXAMPLE_FIXTURE_PATH,
    MORTALITY_TYPES_PATH,
    MORTALITY_POLICY_PATH,
    MORTALITY_CONTENT_PATH,
    MORTALITY_TEST_PATH,
    MORTALITY_TEST_FIXTURE_PATH,
    MORTALITY_EXAMPLE_FIXTURE_PATH,
    EXAMPLE_PATH,
    EDGE_FIXTURE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1070_infernal_attrition")?;
    let mortality = ashen_reach_mortality_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    validate_infernal_attrition_policy(&attrition)?;
    let records = infernal_attrition_content_records(&attrition)?;
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let content_readbacks = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&content_readbacks)?;
    let content_has_map_or_gps_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = content_readbacks.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_attrition/")
    });
    let content_has_compiled_source_path = content_readbacks
        .iter()
        .any(|row| row.source_path.starts_with("rpg://"));
    let lowered_artifact_count = content_readbacks
        .iter()
        .filter(|row| {
            row.lowering.hot_path_policy == "frozen_fingerprinted_data_only"
                && !row.lowering.fingerprint.is_empty()
        })
        .count();

    let ember = apply_infernal_attrition(
        &state(InfernalCircleId::EmberWastes, 220, 16, 0),
        &attrition,
        &mortality,
        &step("ember.heat.5", InfernalCircleId::EmberWastes, 5, 0),
    )?;
    let dark = apply_infernal_attrition(
        &state(InfernalCircleId::HungeringDark, 100, 16, 0),
        &attrition,
        &mortality,
        &step("dark.pressure.10", InfernalCircleId::HungeringDark, 10, 50),
    )?;
    let soul = apply_infernal_attrition(
        &state(InfernalCircleId::WailingTombs, 120, 16, 90),
        &attrition,
        &mortality,
        &step("tombs.soul.1", InfernalCircleId::WailingTombs, 1, 0),
    )?;
    let hp_threshold = apply_infernal_attrition(
        &state(InfernalCircleId::EmberWastes, 0, 16, 0),
        &attrition,
        &mortality,
        &step("ember.hp.threshold", InfernalCircleId::EmberWastes, 5, 0),
    )?;
    let direct_con_death = apply_infernal_attrition(
        &state(InfernalCircleId::WailingTombs, 120, 4, 90),
        &attrition,
        &mortality,
        &step("tombs.soul.final", InfernalCircleId::WailingTombs, 1, 0),
    )?;
    let mortality_handoff = apply_infernal_attrition(
        &state_with_deaths(InfernalCircleId::EmberWastes, 0, 4, 0, 2, 8),
        &attrition,
        &mortality,
        &step("ember.hp.third_death", InfernalCircleId::EmberWastes, 5, 0),
    )?;

    let edges = attrition_edges(&attrition, &mortality);
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
        "schema": "litd.issue1070.infernal_attrition_fsv.v1",
        "issue": 1070,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this persisted JSON through Synapse/computer control, inspect the source hashes and expected actuals, and separately inspect a saved bitmap from the same browser target."
        },
        "sufficiency_argument": "The #1070 proof fixes a seven-circle Infernal Deeps attrition policy and exercises the required environmental lanes: Ember Wastes heat drains health and Constitution strain over time, Hungering Dark pressure honors bounded mitigation without a fallback, Wailing Tombs soul-drain converts accumulated strain into direct Constitution loss, health-threshold collapse records a HollowAttrition mortality event, direct Constitution drain at CON 3 creates a #1070 Mourning Wall handoff, and a same-system HP death can forward mortality's own permanent-death handoff. Seven edge probes prove invalid policy, step, state, and circle-mismatch inputs fail closed without mutating source state.",
        "expected_actuals": {
            "circle_count": 7,
            "source_issue": 1070,
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "attrition_source_count": source_count_for("infernal_attrition"),
            "mortality_source_count": source_count_for("mortality"),
            "calyx_content_rows": lowered.row_count,
            "calyx_content_rows_read_back": content_readbacks.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "constitution_strain_per_loss": 100,
            "max_step_ticks": 240,
            "max_mitigation_percent": 80,
            "ember_heat_health_after": 175,
            "ember_heat_strain_after": 60,
            "dark_pressure_health_after": 65,
            "dark_pressure_strain_gained": 75,
            "soul_drain_constitution_lost": 1,
            "soul_drain_constitution_after": 15,
            "hp_threshold_event_kind": "hollow_attrition",
            "hp_threshold_death_reason": "infernal_death_threshold",
            "direct_con_death_reason": "infernal_constitution_drained",
            "direct_con_death_source_issue": 1070,
            "mortality_handoff_reason": "constitution_depleted",
            "mortality_handoff_source_issue": 1060,
            "edge_count": 7,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": attrition.server_policy_id,
            "source_issue": attrition.source_issue,
            "constitution_strain_per_loss": attrition.constitution_strain_per_loss,
            "max_step_ticks": attrition.max_step_ticks,
            "max_mitigation_percent": attrition.max_mitigation_percent,
            "circles": attrition.circles,
        },
        "calyx": {
            "content_rows": content_rows(&content_readbacks),
            "lowered_entities": &lowered.entities,
        },
        "happy_path": {
            "ember_heat": ember,
            "hungering_dark_pressure": dark,
            "wailing_tombs_soul_drain": soul,
            "hp_threshold_mortality": hp_threshold,
            "direct_constitution_death": direct_con_death,
            "mortality_handoff": mortality_handoff,
        },
        "edges": edges
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["reference_source_count"], 9)?;
    require_i64(&report["expected_actuals"]["attrition_source_count"], 8)?;
    require_i64(&report["expected_actuals"]["mortality_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["calyx_content_rows"], 8)?;
    require_i64(
        &report["expected_actuals"]["calyx_content_rows_read_back"],
        8,
    )?;
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
    require_i64(&report["happy_path"]["ember_heat"]["health_after"], 175)?;
    require_i64(
        &report["happy_path"]["hungering_dark_pressure"]["strain_gained"],
        75,
    )?;
    require_i64(
        &report["happy_path"]["wailing_tombs_soul_drain"]["constitution_lost_to_strain"],
        1,
    )?;
    require_str(
        &report["happy_path"]["hp_threshold_mortality"]["death"]["reason"],
        "infernal_death_threshold",
    )?;
    require_str(
        &report["happy_path"]["direct_constitution_death"]["death"]["reason"],
        "infernal_constitution_drained",
    )?;
    require_i64(
        &report["happy_path"]["direct_constitution_death"]["death"]["mourning_wall_entry"]["source_issue"],
        1070,
    )?;
    require_str(
        &report["happy_path"]["mortality_handoff"]["death"]["reason"],
        "constitution_depleted",
    )?;
    require_i64(
        &report["happy_path"]["mortality_handoff"]["death"]["mourning_wall_entry"]["source_issue"],
        1060,
    )?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1070_INFERNAL_ATTRITION_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} lowered={} map_true={} ember_hp={} dark_strain={} soul_con={} direct_death_reason={} handoff_reason={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["calyx_content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["happy_path"]["ember_heat"]["health_after"],
        readback.json["happy_path"]["hungering_dark_pressure"]["strain_gained"],
        readback.json["happy_path"]["wailing_tombs_soul_drain"]["constitution_after"],
        readback.json["happy_path"]["direct_constitution_death"]["death"]["reason"],
        readback.json["happy_path"]["mortality_handoff"]["death"]["reason"],
        readback.json["edges"]
            .as_object()
            .map(|edges| edges.len())
            .unwrap_or_default(),
    );
    Ok(())
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "display_name": row.display_name,
                "kind": row.kind,
                "cx_id": row.cx_id,
                "database_id": row.database.id,
                "database_sha256": row.database.sha256,
                "source_path": row.source_path,
                "input_sha256": row.input_sha256,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
                "surface_kind": row.payload["surface_kind"],
                "circle_id": row.payload["circle_id"],
                "hazard": row.payload["hazard"],
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn state(
    circle_id: InfernalCircleId,
    hit_points: i32,
    current_constitution: i16,
    constitution_strain: i32,
) -> InfernalAttritionState {
    state_with_deaths(
        circle_id,
        hit_points,
        current_constitution,
        constitution_strain,
        0,
        0,
    )
}

fn state_with_deaths(
    circle_id: InfernalCircleId,
    hit_points: i32,
    current_constitution: i16,
    constitution_strain: i32,
    deaths_since_constitution_loss: u16,
    total_deaths: u32,
) -> InfernalAttritionState {
    InfernalAttritionState {
        schema: RPG_INFERNAL_ATTRITION_SCHEMA.to_string(),
        run_id: "run.infernal.deeps.1070".to_string(),
        party_member_id: "party.vanguard".to_string(),
        circle_id,
        hit_points,
        max_hit_points: 220,
        constitution_strain,
        total_exposure_ticks: 0,
        mortality_state: mortality_state(
            current_constitution,
            deaths_since_constitution_loss,
            total_deaths,
        ),
    }
}

fn mortality_state(
    current_constitution: i16,
    deaths_since_constitution_loss: u16,
    total_deaths: u32,
) -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.hellrunner".to_string(),
        character_name: "Ira Cindermark".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 51,
        age_years: 39,
        base_attributes: AttributeScores::new(16, 13, 12, 14, 16, 11),
        current_constitution,
        deaths_since_constitution_loss,
        total_deaths,
        permanently_dead: false,
        mourning_wall_entry_id: None,
    }
}

fn step(
    step_id: &str,
    circle_id: InfernalCircleId,
    tick_count: u16,
    mitigation_percent: u8,
) -> InfernalAttritionStep {
    InfernalAttritionStep {
        step_id: step_id.to_string(),
        circle_id,
        tick_count,
        mitigation_percent,
    }
}

fn require_str(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1070.report.str".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1070.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1070.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1070.edge.{name}"),
                reason: "edge mutated source state".to_string(),
            });
        }
        if edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG" {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1070.edge.{name}"),
                reason: format!("expected RPG stats error, got {edge}"),
            });
        }
    }
    Ok(())
}
