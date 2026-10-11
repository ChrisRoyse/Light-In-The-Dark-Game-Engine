#[path = "fixtures/issue1060_mortality_fixture.rs"]
mod issue1060_mortality_fixture;
#[path = "fixtures/issue1061_undeath_fixture.rs"]
mod issue1061_undeath_fixture;
#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

use std::fs;

use issue1060_mortality_fixture::ashen_reach_mortality_policy;
use issue1061_undeath_fixture::ashen_reach_undeath_policy;
use issue1067_infernal_zones_fixture::ashen_reach_infernal_zone_policy;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AttributeScores, InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalZonePolicy,
        MortalityCharacterState, RPG_MORTALITY_SCHEMA, UndeadForm, UndeathPolicy,
        WitheringRaidPathPolicy, apply_withering, derive_withering_raid_path_readback,
        persist_and_lower_rpg_authored_content, validate_withering_raid_path_policy,
        withering_raid_content_records, withering_raid_path_policy_hash,
        withering_request_from_raid_path,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const FACTIONS_PATH: &str = "docs/lore/ashen-reach-pantheon-factions.md";
const MOURNING_PATH: &str = "docs/lore/ashen-reach-peoples-mourning-legends.md";
const INFERNAL_REF_PATH: &str = "docs/readme/S06-infernal-deeps.png";
const TOMBS_REF_PATH: &str = "docs/readme/H06-bone-choir.png";
const HOLLOW_REF_PATH: &str = "docs/readme/L17-faction-hollow.png";
const RAID_REF_PATH: &str = "docs/readme/L18-raid-warfare.png";
const FACTION_WAR_REF_PATH: &str = "docs/readme/GP18-faction-war.png";
const BOSS_REF_PATH: &str = "docs/readme/GP07-boss.png";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path_content.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path_tests.rs";
const COMPOSED_POLICY_SOURCE_PATHS: [&str; 28] = [
    "crates/litd-engine/src/rpg/mortality_types.rs",
    "crates/litd-engine/src/rpg/mortality.rs",
    "crates/litd-engine/src/rpg/mortality_content.rs",
    "crates/litd-engine/src/rpg/mortality_tests.rs",
    "crates/litd-engine/src/rpg/mortality_test_fixture.rs",
    "crates/litd-engine/examples/fixtures/issue1060_mortality_fixture.rs",
    "crates/litd-engine/src/rpg/undeath_types.rs",
    "crates/litd-engine/src/rpg/undeath.rs",
    "crates/litd-engine/src/rpg/undeath_content.rs",
    "crates/litd-engine/src/rpg/undeath_test_fixture.rs",
    "crates/litd-engine/examples/fixtures/issue1061_undeath_fixture.rs",
    "crates/litd-engine/src/rpg/undeath_tests.rs",
    "crates/litd-engine/src/rpg/infernal_attrition_types.rs",
    "crates/litd-engine/src/rpg/infernal_attrition.rs",
    "crates/litd-engine/src/rpg/infernal_attrition_content.rs",
    "crates/litd-engine/src/rpg/infernal_attrition_tests.rs",
    "crates/litd-engine/src/rpg/infernal_attrition_test_fixture.rs",
    "crates/litd-engine/examples/fixtures/issue1070_infernal_attrition_fixture.rs",
    "crates/litd-engine/src/rpg/infernal_zones_types.rs",
    "crates/litd-engine/src/rpg/infernal_zones.rs",
    "crates/litd-engine/src/rpg/infernal_zones_content.rs",
    "crates/litd-engine/src/rpg/infernal_zones_tests.rs",
    "crates/litd-engine/src/rpg/infernal_zones_test_fixture.rs",
    "crates/litd-engine/examples/fixtures/issue1067_infernal_zones_fixture.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents_types.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents_content.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents_tests.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1064_withering_raid_path")?;
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let path = WitheringRaidPathPolicy::ashen_reach_defaults();
    validate_withering_raid_path_policy(&path, &undeath, &zones, &bosses, &attrition)?;
    let readback =
        derive_withering_raid_path_readback(&path, &undeath, &zones, &bosses, &attrition)?;
    let request = withering_request_from_raid_path(&path, &undeath, UndeadForm::Lich)?;
    let withering = apply_withering(&mortal_state(), &mortality, &undeath, &request)?;
    let records = withering_raid_content_records(&path, &undeath, &zones, &bosses, &attrition)?;

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
            .starts_with("editor://rpg/ashen_reach/withering_raid_path/")
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

    let edges = json!({
        "built_in_map_rejected": built_in_map_edge(&path, &undeath, &zones, &bosses, &attrition),
        "default_ui_reveal_rejected": ui_reveal_edge(&path, &undeath, &zones, &bosses, &attrition),
        "quest_autocomplete_rejected": autocomplete_edge(&path, &undeath, &zones, &bosses, &attrition),
        "solo_raid_rejected": solo_edge(&path, &undeath, &zones, &bosses, &attrition),
        "low_pressure_rejected": pressure_edge(&path, &undeath, &zones, &bosses, &attrition),
        "missing_phase_rejected": missing_phase_edge(&path, &undeath, &zones, &bosses, &attrition),
        "wrong_zone_rejected": wrong_zone_edge(&path, &undeath, &zones, &bosses, &attrition),
        "wrong_guard_rejected": wrong_guard_edge(&path, &undeath, &zones, &bosses, &attrition),
        "missing_discovery_rejected": missing_discovery_edge(&path, &undeath, &zones, &bosses, &attrition),
        "missing_component_rejected": missing_component_edge(&path, &undeath, &zones, &bosses, &attrition),
        "phase_gps_rejected": phase_gps_edge(&path, &undeath, &zones, &bosses, &attrition),
        "phase_bypass_rejected": phase_bypass_edge(&path, &undeath, &zones, &bosses, &attrition),
    });
    require_all_edges_unchanged(&edges)?;
    let source_readbacks = [
        WORLD_BIBLE_PATH,
        COSMOLOGY_PATH,
        FACTIONS_PATH,
        MOURNING_PATH,
        INFERNAL_REF_PATH,
        TOMBS_REF_PATH,
        HOLLOW_REF_PATH,
        RAID_REF_PATH,
        FACTION_WAR_REF_PATH,
        BOSS_REF_PATH,
        TYPES_PATH,
        POLICY_PATH,
        CONTENT_PATH,
        TEST_PATH,
    ]
    .into_iter()
    .chain(COMPOSED_POLICY_SOURCE_PATHS)
    .map(source_readback)
    .collect::<Result<Vec<_>, _>>()?;

    let report = json!({
        "schema": "litd.issue1064.withering_raid_path_fsv.v1",
        "issue": 1064,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON and the Calyx WAL. Compare expected_actuals to persisted content rows and Withering readback."
        },
        "sufficiency_argument": "The #1064 proof uses one complete raid path because the issue names a single hardest faction raid and a single secret Withering acquisition route. Six phases cover cipher discovery, outer guardian, inner guardian, item-of-power seizure, Wailing Tombs reagent harvest, and the final rite. One Lich request proves the path feeds the existing #1061 Withering runtime; the policy separately requires all three forms/foci. Nine real Calyx content rows prove the raid, questline, item, and phases persist. Twelve edge probes cover map/UI shortcuts, weak raid gates, phase drift, wrong zone/guard, missing secrets/components, GPS, and Withering bypass.",
        "expected_actuals": {
            "source_issue": 1064,
            "phase_count": 6,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "minimum_party_size": 8,
            "minimum_character_level": 47,
            "pressure_tier_milli": 1000,
            "built_in_map_required": false,
            "content_has_map_or_gps_true": content_has_map_true,
            "source_zone_has_reagents": true,
            "final_guard_agent_id": "agent.infernal.wailing_tombs.choirmaster",
            "target_item_of_power_id": "item.hollow_pact.rift_shard",
            "withering_constitution_after": 6,
            "withering_discovery_ref_count": 3,
            "edge_count": 12,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": path.server_policy_id,
            "source_issue": path.source_issue,
            "built_in_map_required": path.built_in_map_required,
            "policy_sha256": withering_raid_path_policy_hash(&path)?,
            "raid": path.raid,
            "questline": path.questline,
        },
        "readback": readback,
        "withering_request": request,
        "withering": withering,
        "calyx": {
            "persisted_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_eq(&report["readback"]["phase_count"], json!(6))?;
    require_eq(
        &report["expected_actuals"]["content_rows_read_back"],
        json!(9),
    )?;
    require_eq(&report["expected_actuals"]["source_anchor_count"], json!(0))?;
    require_eq(
        &report["expected_actuals"]["lowered_artifact_count"],
        json!(9),
    )?;
    require_eq(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        json!(true),
    )?;
    require_eq(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        json!(false),
    )?;
    require_eq(&report["readback"]["built_in_map_required"], json!(false))?;
    require_eq(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        json!(false),
    )?;
    require_eq(&report["withering"]["constitution_after"], json!(6))?;
    require_eq(
        &report["readback"]["final_guard_agent_id"],
        json!("agent.infernal.wailing_tombs.choirmaster"),
    )?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1064_WITHERING_RAID_PATH_FSV report={} bytes={} sha256={} phases={} rows={} anchors={} editor_sources={} con_after={} map_true={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["readback"]["phase_count"],
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["withering"]["constitution_after"],
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
    Ok(json!({"path": path, "bytes": bytes.len(), "sha256": sha256_hex(&bytes)}))
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

fn built_in_map_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.built_in_map_required = true;
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn ui_reveal_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.questline.revealed_by_default_ui = true;
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn autocomplete_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.questline.quest_log_autocomplete = true;
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn solo_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.raid.minimum_party_size = 1;
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn pressure_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.raid.pressure_tier_milli = 990;
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn missing_phase_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.raid.phases.pop();
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn wrong_zone_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.raid.source_zone_id = "zone.infernal.threshold.ashgate".to_string();
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn wrong_guard_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.raid.final_guard_agent_id = "agent.infernal.fake".to_string();
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn missing_discovery_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.questline.required_discovery_refs.clear();
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn missing_component_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.questline.required_components.pop();
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn phase_gps_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.raid.phases[0].grants_gps_markers = true;
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}
fn phase_bypass_edge(
    p: &WitheringRaidPathPolicy,
    u: &UndeathPolicy,
    z: &InfernalZonePolicy,
    b: &InfernalBossAgentPolicy,
    a: &InfernalAttritionPolicy,
) -> Value {
    let mut invalid = p.clone();
    invalid.raid.phases[5].bypasses_withering_requirements = true;
    edge_from_result(
        validate_withering_raid_path_policy(&invalid, u, z, b, a),
        true,
    )
}

fn edge_from_result<T>(result: Result<T, EngineError>, state_unchanged: bool) -> Value {
    match result {
        Ok(_) => json!({"unexpected_success": true, "state_unchanged": state_unchanged}),
        Err(error) => {
            json!({"error_code": error.code(), "error": error.to_string(), "state_unchanged": state_unchanged})
        }
    }
}

fn mortal_state() -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.varn_black".to_string(),
        character_name: "Varn Black".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 47,
        age_years: 64,
        base_attributes: AttributeScores::new(9, 18, 15, 10, 16, 8),
        current_constitution: 16,
        deaths_since_constitution_loss: 0,
        total_deaths: 0,
        permanently_dead: false,
        mourning_wall_entry_id: None,
    }
}

fn require_eq(value: &Value, expected: Value) -> Result<(), EngineError> {
    if value != &expected {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1064.report.value".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

include!("issue1064_withering_raid_path_fsv/assertions.rs");
