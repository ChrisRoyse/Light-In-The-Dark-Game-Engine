#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1068_infernal_boss_agents_edges.rs"]
mod issue1068_infernal_boss_agents_edges;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

use std::fs;

use issue1067_infernal_zones_fixture::ashen_reach_infernal_zone_policy;
use issue1068_infernal_boss_agents_edges::boss_agent_edges;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;
use litd_engine::{
    EngineError,
    calyx_agent_runtime::{
        EngineWorldSignal, EngineWorldSignalKind, activate_calyx_agents, read_calyx_agent_runtime,
        store_calyx_agent_runtime,
    },
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        InfernalBossAgentPolicy, derive_infernal_boss_agent_hierarchy,
        infernal_boss_agent_content_records, infernal_boss_agent_policy_hash,
        infernal_zone_policy_hash, persist_and_lower_rpg_authored_content,
        validate_infernal_boss_agent_policy,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const GAZETTEER_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const INFERNAL_REF_PATH: &str = "docs/readme/S06-infernal-deeps.png";
const GAMEPLAY_REF_PATH: &str = "docs/readme/GP14-infernal.png";
const ARCHDEVIL_REF_PATH: &str = "docs/readme/B01-archdevil.png";
const GATE_REF_PATH: &str = "docs/readme/H01-gate-of-cinders.png";
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
const ZONES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_types.rs";
const ZONES_POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones.rs";
const ZONES_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_content.rs";
const ZONES_TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_tests.rs";
const ZONES_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/infernal_zones_test_fixture.rs";
const ZONES_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1067_infernal_zones_fixture.rs";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents_content.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_boss_agents_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1068_infernal_boss_agents_fsv.rs";
const EDGE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1068_infernal_boss_agents_edges.rs";
const SOURCE_PATHS: [&str; 31] = [
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    GAZETTEER_PATH,
    INFERNAL_REF_PATH,
    GAMEPLAY_REF_PATH,
    ARCHDEVIL_REF_PATH,
    GATE_REF_PATH,
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
    ZONES_TYPES_PATH,
    ZONES_POLICY_PATH,
    ZONES_CONTENT_PATH,
    ZONES_TEST_PATH,
    ZONES_TEST_FIXTURE_PATH,
    ZONES_EXAMPLE_FIXTURE_PATH,
    TYPES_PATH,
    POLICY_PATH,
    CONTENT_PATH,
    TEST_PATH,
    EXAMPLE_PATH,
    EDGE_FIXTURE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1068_infernal_boss_agents")?;
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    validate_infernal_boss_agent_policy(&bosses, &zones, &attrition)?;
    let hierarchy = derive_infernal_boss_agent_hierarchy(&bosses, &zones, &attrition)?;
    let records = infernal_boss_agent_content_records(&bosses, &zones, &attrition)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database.clone());
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
            .starts_with("editor://rpg/ashen_reach/infernal_boss_agents/")
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
    let signals = bosses
        .boss_agents
        .iter()
        .enumerate()
        .map(|(index, boss)| {
            signal(
                &format!("signal.infernal.{}", boss.circle_id.label()),
                u64::try_from(index + 1).unwrap() * 100,
                boss.tactic_profile.trigger_terms[0].as_str(),
                boss.display_name.as_str(),
            )
        })
        .collect::<Vec<_>>();
    let runtime = activate_calyx_agents(
        "issue1068.infernal_boss_agents",
        &content_readbacks,
        &signals,
    )?;
    let stored_runtime = store_calyx_agent_runtime(&database, &runtime)?;
    let runtime_readback = read_calyx_agent_runtime(&database, "issue1068.infernal_boss_agents")?;

    let edges = boss_agent_edges(&bosses, &zones, &attrition);
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
        "schema": "litd.issue1068.infernal_boss_agents_fsv.v1",
        "issue": 1068,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this physical report and its Calyx vault row hashes from disk; compare expected_actuals to the persisted content/runtime readbacks."
        },
        "sufficiency_argument": "The #1068 proof covers all seven Infernal rulers from #1067, including Maldrithax as the apex tier-10 tyrant. Every boss-agent is persisted as real Calyx Agent content, then activated by synthetic known combat signals through the existing Calyx agent runtime, producing deterministic lowered orders and a persisted runtime ledger. Ten edge probes cover missing/duplicate hierarchy, zone ruler drift, disabled Calyx runtime, illegal live Calyx hot-path query, weak lethality, empty triggers, empty dialogue, zero deterministic order velocity, and wrong apex role.",
        "expected_actuals": {
            "source_issue": 1068,
            "boss_count": 7,
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "attrition_source_count": source_count_for("infernal_attrition"),
            "zone_source_count": source_count_for("infernal_zones"),
            "boss_agent_source_count": source_count_for("infernal_boss_agents"),
            "calyx_content_rows": 7,
            "runtime_agent_count": 7,
            "runtime_signal_count": 7,
            "runtime_activation_count": 7,
            "runtime_order_count": 7,
            "hot_path_policy": "lowered_orders_only",
            "calyx_live_query_allowed_in_hot_path": false,
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "apex_agent_id": "agent.infernal.obsidian_throne.maldrithax",
            "maldrithax_lethality_tier": 10,
            "edge_count": 10,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": bosses.server_policy_id,
            "source_issue": bosses.source_issue,
            "policy_sha256": infernal_boss_agent_policy_hash(&bosses)?,
            "zone_policy_sha256": infernal_zone_policy_hash(&zones)?,
            "boss_agents": bosses.boss_agents,
        },
        "hierarchy": hierarchy,
        "calyx": {
            "content_rows": content_rows(&content_readbacks),
            "lowered_entities": &lowered.entities,
            "runtime_fingerprint": runtime.runtime_fingerprint,
            "stored_runtime_sha256": stored_runtime.database.sha256,
            "readback_runtime_sha256": runtime_readback.database.sha256,
            "runtime": runtime_readback.runtime,
        },
        "edges": edges,
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["reference_source_count"], 13)?;
    require_i64(&report["expected_actuals"]["attrition_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["zone_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["boss_agent_source_count"], 6)?;
    require_i64(&report["hierarchy"]["boss_count"], 7)?;
    require_bool(&report["hierarchy"]["built_in_map_required"], false)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 7)?;
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
    require_str(
        &report["hierarchy"]["apex_agent_id"],
        "agent.infernal.obsidian_throne.maldrithax",
    )?;
    require_i64(&report["calyx"]["runtime"]["activation_count"], 7)?;
    require_bool(
        &report["calyx"]["runtime"]["calyx_live_query_allowed_in_hot_path"],
        false,
    )?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1068_INFERNAL_BOSS_AGENTS_FSV report={} bytes={} sha256={} bosses={} content_rows={} anchors={} editor_sources={} lowered={} map_true={} activations={} orders={} apex={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["hierarchy"]["boss_count"],
        readback.json["calyx"]["content_rows"]
            .as_array()
            .map(|rows| rows.len())
            .unwrap_or_default(),
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["calyx"]["runtime"]["activation_count"],
        readback.json["calyx"]["runtime"]["order_count"],
        readback.json["hierarchy"]["apex_agent_id"],
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

fn content_rows(readbacks: &[EngineContentConstellationReadback]) -> Vec<Value> {
    readbacks
        .iter()
        .map(|readback| {
            json!({
                "id": readback.id,
                "display_name": readback.display_name,
                "cx_id": readback.cx_id,
                "database_id": readback.database.id,
                "database_sha256": readback.database.sha256,
                "source_path": readback.source_path,
                "input_sha256": readback.input_sha256,
                "lowering_hot_path_policy": readback.lowering.hot_path_policy,
                "surface_kind": readback.payload["surface_kind"],
                "role": readback.payload["role"],
                "lethality_tier": readback.payload["lethality_tier"],
                "built_in_map_required": readback.payload["built_in_map_required"],
                "grants_map_or_minimap": readback.payload["grants_map_or_minimap"],
                "grants_gps_markers": readback.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn signal(signal_id: &str, tick: u64, trigger_term: &str, display_name: &str) -> EngineWorldSignal {
    EngineWorldSignal {
        signal_id: signal_id.to_string(),
        world_id: "world.ashen_reach.infernal_deeps".to_string(),
        tick,
        kind: EngineWorldSignalKind::Combat,
        strength_milli: 1000,
        terms: vec![trigger_term.to_string()],
        summary: format!("party challenges {display_name}"),
        provenance_refs: vec![format!("issue1068://signal/{signal_id}")],
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1068.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_str(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1068.report.str".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1068.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1068.edge.{name}"),
                reason: "edge mutated source state".to_string(),
            });
        }
        if edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG" {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1068.edge.{name}"),
                reason: format!("expected RPG stats error, got {edge}"),
            });
        }
    }
    Ok(())
}
