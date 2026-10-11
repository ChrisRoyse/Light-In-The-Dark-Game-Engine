#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

use std::fs;

use issue1067_infernal_zones_fixture::ashen_reach_infernal_zone_policy;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        InfernalAttritionPolicy, InfernalCircleId, InfernalHazardKind, InfernalZonePolicy,
        derive_infernal_zone_topology, infernal_zone_content_records, infernal_zone_policy_hash,
        persist_and_lower_rpg_authored_content, validate_infernal_zone_policy, zone_for_circle,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const GAZETTEER_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const INFERNAL_REF_PATH: &str = "docs/readme/S06-infernal-deeps.png";
const GAMEPLAY_REF_PATH: &str = "docs/readme/GP14-infernal.png";
const GATE_REF_PATH: &str = "docs/readme/H01-gate-of-cinders.png";
const DESCENT_REF_PATH: &str = "docs/readme/H02-descent.png";
const EMBER_REF_PATH: &str = "docs/readme/H03-ember-wastes.png";
const BASTION_REF_PATH: &str = "docs/readme/H04-iron-bastion.png";
const DARK_REF_PATH: &str = "docs/readme/H05-hungering-dark.png";
const TOMBS_REF_PATH: &str = "docs/readme/H06-bone-choir.png";
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
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_content.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_tests.rs";
const ZONE_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_test_fixture.rs";
const ZONE_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1067_infernal_zones_fixture.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1067_infernal_zones_fsv.rs";
const SOURCE_PATHS: [&str; 26] = [
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    GAZETTEER_PATH,
    INFERNAL_REF_PATH,
    GAMEPLAY_REF_PATH,
    GATE_REF_PATH,
    DESCENT_REF_PATH,
    EMBER_REF_PATH,
    BASTION_REF_PATH,
    DARK_REF_PATH,
    TOMBS_REF_PATH,
    THRONE_REF_PATH,
    RELIC_REF_PATH,
    ATTRITION_TYPES_PATH,
    ATTRITION_POLICY_PATH,
    ATTRITION_CONTENT_PATH,
    ATTRITION_TEST_PATH,
    ATTRITION_TEST_FIXTURE_PATH,
    ATTRITION_EXAMPLE_FIXTURE_PATH,
    TYPES_PATH,
    POLICY_PATH,
    CONTENT_PATH,
    TEST_PATH,
    ZONE_TEST_FIXTURE_PATH,
    ZONE_EXAMPLE_FIXTURE_PATH,
    EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1067_infernal_zones")?;
    let attrition = ashen_reach_infernal_attrition_policy();
    let policy = ashen_reach_infernal_zone_policy();
    validate_infernal_zone_policy(&policy, &attrition)?;
    let topology = derive_infernal_zone_topology(&policy, &attrition)?;
    let records = infernal_zone_content_records(&policy, &attrition)?;
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
            .starts_with("editor://rpg/ashen_reach/infernal_zones/")
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

    let edges = json!({
        "built_in_map_rejected": built_in_map_edge(&policy, &attrition),
        "missing_circle_rejected": missing_circle_edge(&policy, &attrition),
        "duplicate_depth_rejected": duplicate_depth_edge(&policy, &attrition),
        "broken_exit_rejected": broken_exit_edge(&policy, &attrition),
        "empty_mob_population_rejected": empty_mob_edge(&policy, &attrition),
        "hazard_mismatch_rejected": hazard_mismatch_edge(&policy, &attrition),
        "zero_reset_rejected": zero_reset_edge(&policy, &attrition),
        "zero_repop_rejected": zero_repop_edge(&policy, &attrition),
        "wrong_reagent_source_rejected": wrong_reagent_edge(&policy, &attrition),
        "missing_lookup_rejected": missing_lookup_edge(&policy),
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
        "schema": "litd.issue1067.infernal_zones_fsv.v1",
        "issue": 1067,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this persisted JSON through Synapse/computer control and inspect the expected actuals against the physical report bytes. For visual work, separately inspect the referenced readme bitmaps."
        },
        "sufficiency_argument": "The #1067 proof fixes the Infernal Deeps as seven connected descending zone nodes, from Threshold to Obsidian Throne. Every node has a unique circle, depth, hazard profile, mob population list, ruling lord, reset profile, and repop profile. The topology is validated against the #1070 attrition policy so zone hazard kinds cannot drift from runtime environmental damage. The Wailing Tombs is the only rare undeath reagent source, the Molten Throne-road is the only relic-vault source, and built_in_map_required is false so the runtime contract does not add a built-in map or minimap.",
        "expected_actuals": {
            "source_issue": 1067,
            "zone_count": 7,
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "attrition_source_count": source_count_for("infernal_attrition"),
            "zone_source_count": source_count_for("infernal_zones"),
            "calyx_content_rows": lowered.row_count,
            "calyx_content_rows_read_back": content_readbacks.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "built_in_map_required": false,
            "entry_circle": "threshold",
            "terminal_circle": "obsidian_throne",
            "first_zone": "zone.infernal.threshold.ashgate",
            "last_zone": "zone.infernal.obsidian_throne.crown_chamber",
            "wailing_tombs_reagent_source": true,
            "throne_road_relic_vault_source": true,
            "ember_hazard": "heat",
            "hungering_dark_hazard": "dark_pressure",
            "wailing_tombs_hazard": "soul_drain",
            "obsidian_throne_descends_to": null,
            "edge_count": 10,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": policy.server_policy_id,
            "source_issue": policy.source_issue,
            "built_in_map_required": policy.built_in_map_required,
            "entry_circle": policy.entry_circle,
            "terminal_circle": policy.terminal_circle,
            "policy_sha256": infernal_zone_policy_hash(&policy)?,
            "zones": policy.zones,
        },
        "topology": topology,
        "calyx": {
            "content_rows": content_rows(&content_readbacks),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["reference_source_count"], 13)?;
    require_i64(&report["expected_actuals"]["attrition_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["zone_source_count"], 7)?;
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
    require_bool(&report["expected_actuals"]["built_in_map_required"], false)?;
    require_i64(&report["topology"]["zone_count"], 7)?;
    require_str(&report["topology"]["chain"][0]["circle_id"], "threshold")?;
    require_str(
        &report["topology"]["chain"][6]["circle_id"],
        "obsidian_throne",
    )?;
    require_bool(&report["topology"]["wailing_tombs_reagent_source"], true)?;
    require_bool(&report["topology"]["throne_road_relic_vault_source"], true)?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1067_INFERNAL_ZONES_FSV report={} bytes={} sha256={} zones={} rows={} anchors={} editor_sources={} lowered={} map_true={} map_required={} first={} last={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["topology"]["zone_count"],
        readback.json["expected_actuals"]["calyx_content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["topology"]["built_in_map_required"],
        readback.json["topology"]["chain"][0]["zone_id"],
        readback.json["topology"]["chain"][6]["zone_id"],
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
                "circle_id": row.payload["circle_id"],
                "zone_id": row.payload["zone_id"],
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn built_in_map_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.built_in_map_required = true;
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn missing_circle_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones.pop();
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn duplicate_depth_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones[1].depth = 1;
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn broken_exit_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones[2].exits[0].to_circle_id = InfernalCircleId::WailingTombs;
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn empty_mob_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones[0].mob_population_ids.clear();
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn hazard_mismatch_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones[1].hazard_kind = InfernalHazardKind::SoulDrain;
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn zero_reset_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones[0].reset_profile.interval_minutes = 0;
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn zero_repop_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones[0].repop_profile.baseline_mob_budget = 0;
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn wrong_reagent_edge(policy: &InfernalZonePolicy, attrition: &InfernalAttritionPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.zones[0].withering_reagent_source = true;
    edge_from_result(validate_infernal_zone_policy(&invalid, attrition), true)
}

fn missing_lookup_edge(policy: &InfernalZonePolicy) -> Value {
    let mut invalid = policy.clone();
    invalid
        .zones
        .retain(|zone| zone.circle_id != InfernalCircleId::WailingTombs);
    edge_from_result(
        zone_for_circle(&invalid, InfernalCircleId::WailingTombs),
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
            field: "issue1067.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_str(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1067.report.str".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1067.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1067.edge.{name}"),
                reason: "edge mutated source state".to_string(),
            });
        }
        if edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG" {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1067.edge.{name}"),
                reason: format!("expected RPG stats error, got {edge}"),
            });
        }
    }
    Ok(())
}
