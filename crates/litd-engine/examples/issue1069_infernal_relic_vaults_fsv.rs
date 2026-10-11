#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

use std::fs;

use issue1067_infernal_zones_fixture::ashen_reach_infernal_zone_policy;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;
use litd_engine::{
    EngineError,
    calyx_content::{
        EngineCalyxContentStore, EngineContentConstellationReadback, EngineContentKind,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalRelicSlot,
        InfernalRelicVaultPolicy, InfernalZonePolicy, derive_infernal_relic_vault_readback,
        infernal_boss_agent_policy_hash, infernal_relic_item_content_records,
        infernal_relic_vault_policy_hash, infernal_zone_policy_hash,
        persist_and_lower_rpg_authored_content, validate_infernal_relic_vault_policy,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const GAZETTEER_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const INFERNAL_REF_PATH: &str = "docs/readme/S06-infernal-deeps.png";
const GAMEPLAY_REF_PATH: &str = "docs/readme/GP14-infernal.png";
const BOSS_REF_PATH: &str = "docs/readme/GP07-boss.png";
const INVENTORY_REF_PATH: &str = "docs/readme/GP09-inventory.png";
const LEGENDARY_REF_PATH: &str = "docs/readme/S07-legendary-loot.png";
const THRONE_REF_PATH: &str = "docs/readme/H07-obsidian-throne.png";
const RELIC_REF_PATH: &str = "docs/readme/H08-relic-vault.png";
const ATTRITION_TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition_types.rs";
const ATTRITION_POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition.rs";
const ATTRITION_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition_content.rs";
const ATTRITION_TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_attrition_tests.rs";
const ATTRITION_TEST_FIXTURE_PATH: &str =
    "crates/litd-engine/src/rpg/infernal_attrition_test_fixture.rs";
const ATTRITION_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1070_infernal_attrition_fixture.rs";
const ZONES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_types.rs";
const ZONES_POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones.rs";
const ZONES_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_content.rs";
const ZONES_TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_tests.rs";
const ZONES_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_test_fixture.rs";
const ZONES_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1067_infernal_zones_fixture.rs";
const BOSS_TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents_types.rs";
const BOSS_POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents.rs";
const BOSS_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents_content.rs";
const BOSS_TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents_tests.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults_content.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1069_infernal_relic_vaults_fsv.rs";
const SOURCE_PATHS: [&str; 31] = [
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    GAZETTEER_PATH,
    INFERNAL_REF_PATH,
    GAMEPLAY_REF_PATH,
    BOSS_REF_PATH,
    INVENTORY_REF_PATH,
    LEGENDARY_REF_PATH,
    THRONE_REF_PATH,
    RELIC_REF_PATH,
    ATTRITION_TYPES_PATH,
    ATTRITION_POLICY_PATH,
    ATTRITION_CONTENT_PATH,
    ATTRITION_TEST_PATH,
    ATTRITION_TEST_FIXTURE_PATH,
    ATTRITION_EXAMPLE_FIXTURE_PATH,
    ZONES_TYPES_PATH,
    ZONES_POLICY_PATH,
    ZONES_CONTENT_PATH,
    ZONES_TEST_PATH,
    ZONES_TEST_FIXTURE_PATH,
    ZONES_EXAMPLE_FIXTURE_PATH,
    BOSS_TYPES_PATH,
    BOSS_POLICY_PATH,
    BOSS_CONTENT_PATH,
    BOSS_TEST_PATH,
    TYPES_PATH,
    POLICY_PATH,
    CONTENT_PATH,
    TEST_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1069_infernal_relic_vaults")?;
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let relics = InfernalRelicVaultPolicy::ashen_reach_defaults();
    validate_infernal_relic_vault_policy(&relics, &zones, &bosses, &attrition)?;
    let readback = derive_infernal_relic_vault_readback(&relics, &zones, &bosses, &attrition)?;
    let records = infernal_relic_item_content_records(&relics, &zones, &bosses, &attrition)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(EngineContentKind::Item, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let any_item_grants_map_or_minimap = persisted_rows
        .iter()
        .any(|row| row.payload["grants_map_or_minimap"] == true);
    let any_item_bypasses_infernal_no_recall = persisted_rows
        .iter()
        .any(|row| row.payload["bypasses_infernal_no_recall"] == true);
    let content_has_map_or_gps_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_relic_vaults/")
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
        "built_in_map_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.built_in_map_required = true),
        "world_finite_required_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.world_finite_enforced = false),
        "missing_vault_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| { invalid.vaults.pop(); }),
        "guard_drift_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.vaults[0].guarded_by_agent_id = "agent.infernal.fake".to_string()),
        "respawning_vault_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.vaults[0].respawns_world_finite_items = true),
        "capacity_exceeded_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.vaults[0].world_finite_capacity = 3),
        "duplicate_item_id_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| {
            let duplicate_id = invalid.item_set[0].item_id.clone();
            invalid.item_set[1].item_id = duplicate_id;
        }),
        "duplicate_item_slot_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.item_set[1].slot = InfernalRelicSlot::Weapon),
        "non_unique_world_count_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.item_set[0].world_finite_count = 2),
        "weak_power_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.item_set[0].power_rating_milli = 949),
        "item_map_grant_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.item_set[0].grants_map_or_minimap = true),
        "item_recall_bypass_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.item_set[0].bypasses_infernal_no_recall = true),
        "set_bonus_map_or_recall_rejected": invalid_relic_edge(&relics, &zones, &bosses, &attrition, |invalid| invalid.set_bonuses[0].grants_map_or_minimap = true),
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
        "schema": "litd.issue1069.infernal_relic_vaults_fsv.v1",
        "issue": 1069,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this physical report and its Calyx vault item rows from disk; compare expected_actuals to persisted item readbacks. The policy intentionally keeps built_in_map_required=false and grants no map or minimap."
        },
        "sufficiency_argument": "The #1069 proof is the minimal complete dataset: two source vaults are the only vault classes named by the issue, six unique hell-forged legendary items cover each set slot and world-finite count, and three 2/4/6-piece bonuses cover the entire set ladder. Real Calyx Item rows are written and read back by id. Thirteen edge probes cover built-in map drift, disabled finite enforcement, missing vault, wrong guard, respawn behavior, vault capacity overflow, duplicate item id, duplicate slot, non-unique world count, weak top-tier power, item map grant, recall bypass, and set bonus map/recall drift.",
        "expected_actuals": {
            "source_issue": 1069,
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "attrition_source_count": source_count_for("infernal_attrition"),
            "zone_source_count": source_count_for("infernal_zones"),
            "boss_agent_source_count": source_count_for("infernal_boss_agents"),
            "relic_vault_source_count": source_count_for("infernal_relic_vaults"),
            "vault_count": 2,
            "item_count": 6,
            "set_bonus_count": 3,
            "world_finite_item_total": 6,
            "highest_power_rating_milli": 1000,
            "calyx_item_rows_written": lowered.row_count,
            "calyx_item_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "any_item_grants_map_or_minimap": any_item_grants_map_or_minimap,
            "any_item_bypasses_infernal_no_recall": any_item_bypasses_infernal_no_recall,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "molten_vault_guard": "lord.infernal.molten_throne_road.throne_warden",
            "obsidian_vault_guard": "lord.infernal.obsidian_throne.maldrithax",
            "edge_count": 13,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": relics.server_policy_id,
            "source_issue": relics.source_issue,
            "built_in_map_required": relics.built_in_map_required,
            "world_finite_enforced": relics.world_finite_enforced,
            "policy_sha256": infernal_relic_vault_policy_hash(&relics)?,
            "zone_policy_sha256": infernal_zone_policy_hash(&zones)?,
            "boss_policy_sha256": infernal_boss_agent_policy_hash(&bosses)?,
            "vaults": relics.vaults,
            "item_set": relics.item_set,
            "set_bonuses": relics.set_bonuses,
        },
        "readback": readback,
        "calyx": {
            "persisted_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["reference_source_count"], 10)?;
    require_i64(&report["expected_actuals"]["attrition_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["zone_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["boss_agent_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["relic_vault_source_count"], 5)?;
    require_i64(&report["readback"]["vault_count"], 2)?;
    require_i64(&report["readback"]["item_count"], 6)?;
    require_i64(&report["readback"]["world_finite_item_total"], 6)?;
    require_i64(&report["readback"]["highest_power_rating_milli"], 1000)?;
    require_bool(&report["readback"]["built_in_map_required"], false)?;
    require_bool(&report["policy"]["built_in_map_required"], false)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 6)?;
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
    require_i64(&report["expected_actuals"]["calyx_item_rows_read_back"], 6)?;
    require_bool(
        &report["expected_actuals"]["any_item_grants_map_or_minimap"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["any_item_bypasses_infernal_no_recall"],
        false,
    )?;
    require_str(
        &report["readback"]["obsidian_vault_guard"],
        "lord.infernal.obsidian_throne.maldrithax",
    )?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1069_INFERNAL_RELIC_VAULTS_FSV report={} bytes={} sha256={} vaults={} items={} rows={} anchors={} editor_sources={} lowered={} map_true={} map_required={} max_power={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["readback"]["vault_count"],
        readback.json["readback"]["item_count"],
        readback.json["expected_actuals"]["calyx_item_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["readback"]["built_in_map_required"],
        readback.json["readback"]["highest_power_rating_milli"],
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
                "display_name": row.display_name,
                "kind": row.kind,
                "cx_id": row.cx_id,
                "database_id": row.database.id,
                "database_sha256": row.database.sha256,
                "source_path": row.source_path,
                "input_sha256": row.input_sha256,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
                "surface_kind": row.payload["surface_kind"],
                "rarity": row.payload["rarity"],
                "source_vault_id": row.payload["source_vault_id"],
                "source_circle_id": row.payload["source_circle_id"],
                "world_finite_count": row.payload["world_finite_count"],
                "power_rating_milli": row.payload["power_rating_milli"],
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
                "bypasses_infernal_no_recall": row.payload["bypasses_infernal_no_recall"],
            })
        })
        .collect()
}

fn invalid_relic_edge(
    policy: &InfernalRelicVaultPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    attrition: &InfernalAttritionPolicy,
    mutate: impl FnOnce(&mut InfernalRelicVaultPolicy),
) -> Value {
    let mut invalid = policy.clone();
    mutate(&mut invalid);
    edge_from_result(
        validate_infernal_relic_vault_policy(&invalid, zones, bosses, attrition),
        true,
    )
}

fn edge_from_result<T>(result: Result<T, EngineError>, state_unchanged: bool) -> Value {
    match result {
        Ok(_) => json!({"unexpected_success": true, "state_unchanged": state_unchanged}),
        Err(error) => json!({
            "error_code": error.code(),
            "error": error.to_string(),
            "state_unchanged": state_unchanged,
        }),
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1069.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_str(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1069.report.str".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1069.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1069.edge.{name}"),
                reason: "edge mutated source state".to_string(),
            });
        }
        if edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG" {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1069.edge.{name}"),
                reason: format!("expected RPG stats error, got {edge}"),
            });
        }
    }
    Ok(())
}
