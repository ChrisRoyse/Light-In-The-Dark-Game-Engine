#[path = "fixtures/issue1061_undeath_fixture.rs"]
mod issue1061_undeath_fixture;
#[path = "fixtures/issue1063_infernal_deeps_edges.rs"]
mod issue1063_infernal_deeps_edges;
#[path = "fixtures/issue1066_infernal_access_fixture.rs"]
mod issue1066_infernal_access_fixture;
#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;
#[path = "fixtures/workspace_source_readback.rs"]
mod workspace_source_readback;

use issue1061_undeath_fixture::ashen_reach_undeath_policy;
use issue1063_infernal_deeps_edges::infernal_deeps_edges;
use issue1066_infernal_access_fixture::ashen_reach_infernal_access_policy;
use issue1067_infernal_zones_fixture::ashen_reach_infernal_zone_policy;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        CindermarkSource, DeriveInfernalDeepsPackageArgs, InfernalAccessPolicy,
        InfernalAccessState, InfernalAttritionPolicy, InfernalBossAgentPolicy,
        InfernalDeepsPackagePolicy, InfernalEntryPath, InfernalEntryRequest, InfernalEscapeAttempt,
        InfernalEscapeKind, InfernalRelicVaultPolicy, InfernalZonePolicy,
        RPG_INFERNAL_ACCESS_SCHEMA, UndeathPolicy, WitheringRaidPathPolicy,
        derive_infernal_deeps_package, infernal_deeps_content_records,
        infernal_deeps_package_policy_hash, persist_and_lower_rpg_authored_content,
        resolve_infernal_entry, resolve_infernal_escape_attempt,
    },
};
use serde_json::{Value, json};
use workspace_source_readback::source_readback;

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const PANTHEON_PATH: &str = "docs/lore/ashen-reach-pantheon-factions.md";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps_content.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps_tests.rs";
const ACCESS_TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_types.rs";
const ACCESS_POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_access.rs";
const ACCESS_VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_validate.rs";
const ACCESS_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_content.rs";
const ACCESS_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_test_fixture.rs";
const ACCESS_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1066_infernal_access_fixture.rs";
const ACCESS_TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_tests.rs";
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
const RELIC_TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults_types.rs";
const RELIC_POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults.rs";
const RELIC_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults_content.rs";
const RELIC_TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_relic_vaults_tests.rs";
const WITHERING_TYPES_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path_types.rs";
const WITHERING_POLICY_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path.rs";
const WITHERING_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path_content.rs";
const WITHERING_TEST_PATH: &str = "crates/litd-engine/src/rpg/withering_raid_path_tests.rs";
const UNDEATH_TYPES_PATH: &str = "crates/litd-engine/src/rpg/undeath_types.rs";
const UNDEATH_POLICY_PATH: &str = "crates/litd-engine/src/rpg/undeath.rs";
const UNDEATH_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/undeath_content.rs";
const UNDEATH_TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/undeath_test_fixture.rs";
const UNDEATH_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1061_undeath_fixture.rs";
const UNDEATH_TEST_PATH: &str = "crates/litd-engine/src/rpg/undeath_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1063_infernal_deeps_fsv.rs";
const EDGE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1063_infernal_deeps_edges.rs";
const SOURCE_PATHS: [&str; 46] = [
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    PANTHEON_PATH,
    TYPES_PATH,
    POLICY_PATH,
    CONTENT_PATH,
    TEST_PATH,
    ACCESS_TYPES_PATH,
    ACCESS_POLICY_PATH,
    ACCESS_VALIDATE_PATH,
    ACCESS_CONTENT_PATH,
    ACCESS_TEST_FIXTURE_PATH,
    ACCESS_EXAMPLE_FIXTURE_PATH,
    ACCESS_TEST_PATH,
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
    RELIC_TYPES_PATH,
    RELIC_POLICY_PATH,
    RELIC_CONTENT_PATH,
    RELIC_TEST_PATH,
    WITHERING_TYPES_PATH,
    WITHERING_POLICY_PATH,
    WITHERING_CONTENT_PATH,
    WITHERING_TEST_PATH,
    UNDEATH_TYPES_PATH,
    UNDEATH_POLICY_PATH,
    UNDEATH_CONTENT_PATH,
    UNDEATH_TEST_FIXTURE_PATH,
    UNDEATH_EXAMPLE_FIXTURE_PATH,
    UNDEATH_TEST_PATH,
    EXAMPLE_PATH,
    EDGE_FIXTURE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1063_infernal_deeps")?;
    let policies = policies();
    let readback = derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
        package: &policies.package,
        access: &policies.access,
        attrition: &policies.attrition,
        zones: &policies.zones,
        bosses: &policies.bosses,
        relics: &policies.relics,
        withering: &policies.withering,
        undeath: &policies.undeath,
    })?;
    let entry = resolve_infernal_entry(&outside_state(), &policies.access, &entry_request())?;
    let escape = resolve_infernal_escape_attempt(
        &entry.state_after,
        &policies.access,
        &escape_attempt(InfernalEscapeKind::Recall),
    )?;
    let records = infernal_deeps_content_records(&policies.package, &readback)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_forbidden_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true")
        || persisted_json.contains("\"bypasses_infernal_no_recall\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_deeps/")
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

    let edges = infernal_deeps_edges(
        &policies.package,
        &policies.access,
        &policies.attrition,
        &policies.zones,
        &policies.bosses,
        &policies.relics,
        &policies.withering,
        &policies.undeath,
    );
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

    let source_expected_actuals = json!({
        "source_issue": 1063,
        "source_readback_count": source_readback_count,
        "reference_source_count": source_count_for("docs/"),
        "parent_source_count": source_count_for("infernal_deeps"),
        "access_source_count": source_count_for("infernal_access"),
        "attrition_source_count": source_count_for("infernal_attrition"),
        "zone_source_count": source_count_for("infernal_zones"),
        "boss_agent_source_count": source_count_for("infernal_boss_agents"),
        "relic_vault_source_count": source_count_for("infernal_relic_vaults"),
        "withering_source_count": source_count_for("withering_raid_path"),
        "undeath_source_count": source_count_for("undeath"),
    });
    let mut expected_actuals = json!({
        "package_id": readback.package_id,
        "ordered_hell_not_outer_dark": true,
        "group_required": true,
        "no_recall_inside": true,
        "entry_circle": "threshold",
        "terminal_circle": "obsidian_throne",
        "access_path_count": 5,
        "denied_escape_kind_count": 5,
        "zone_count": 7,
        "hazard_kind_count": 3,
        "boss_count": 7,
        "apex_agent_id": "agent.infernal.obsidian_throne.maldrithax",
        "relic_vault_count": 2,
        "hellforged_item_count": 6,
        "world_finite_item_total": 6,
        "highest_power_rating_milli": 1000,
        "withering_raid_phase_count": 6,
        "withering_minimum_party_size": 8,
        "wailing_tombs_reagent_source": true,
        "throne_road_relic_vault_source": true,
        "entry_granted": entry.granted,
        "escape_denied": escape.denied,
        "escape_state_unchanged": escape.state_unchanged,
        "content_rows_written": lowered.row_count,
        "content_rows_read_back": persisted_rows.len(),
        "source_anchor_count": source_anchor_count,
        "source_paths_are_editor_authored": source_paths_are_editor_authored,
        "content_has_compiled_source_path": content_has_compiled_source_path,
        "lowered_artifact_count": lowered_artifact_count,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
        "content_has_forbidden_true": content_has_forbidden_true,
        "edge_count": 8,
        "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
    })
    .as_object()
    .cloned()
    .expect("expected actuals must be a JSON object");
    expected_actuals.extend(
        source_expected_actuals
            .as_object()
            .cloned()
            .expect("source expected actuals must be a JSON object"),
    );
    let expected_actuals = Value::Object(expected_actuals);

    let report = json!({
        "schema": "litd.issue1063.infernal_deeps_fsv.v1",
        "issue": 1063,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON and the Calyx WAL. Compare expected_actuals to the persisted parent package row and the access/escape readbacks."
        },
        "sufficiency_argument": "The parent proof composes the already-verified child contracts through live validators instead of restating them: #1066 access/no-recall, #1070 attrition, #1067 seven-circle topology, #1068 Calyx boss hierarchy, #1069 world-finite relic vaults, and #1064 Withering raid path. One known-I/O entry request proves the Gate of Cinders path enters Threshold and one escape request proves recall is denied with unchanged state. Content is persisted as six editor-authored Calyx rows instead of one opaque package blob: parent summary, access contract, zone topology, boss hierarchy, relic vaults, and Withering raid path. Eight edge probes cover parent map drift, child issue drift, no-recall drift, missing circle, hot-path Calyx drift, relic map drift, Withering UI reveal drift, and escape outside the locked Deeps.",
        "expected_actuals": expected_actuals,
        "package_policy": {
            "package_id": policies.package.package_id,
            "policy_sha256": infernal_deeps_package_policy_hash(&policies.package)?,
            "required_child_issues": policies.package.required_child_issues,
        },
        "infernal_deeps_readback": readback,
        "sample_access": {
            "entry": entry,
            "escape": escape,
        },
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
    require_i64(&report["expected_actuals"]["reference_source_count"], 3)?;
    require_i64(&report["expected_actuals"]["parent_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["access_source_count"], 7)?;
    require_i64(&report["expected_actuals"]["attrition_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["zone_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["boss_agent_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["relic_vault_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["withering_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["undeath_source_count"], 6)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 6)?;
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
    require_i64(&report["expected_actuals"]["zone_count"], 7)?;
    require_i64(&report["expected_actuals"]["hellforged_item_count"], 6)?;
    require_i64(
        &report["expected_actuals"]["highest_power_rating_milli"],
        1000,
    )?;
    require_bool(&report["expected_actuals"]["escape_denied"], true)?;
    require_bool(
        &report["expected_actuals"]["content_has_forbidden_true"],
        false,
    )?;

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1063_INFERNAL_DEEPS_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} lowered={} zones={} bosses={} relics={} no_recall={} map_true={} edges={}",
        read.path.display(),
        read.bytes.len(),
        read.sha256,
        read.json["expected_actuals"]["content_rows_read_back"],
        read.json["expected_actuals"]["source_anchor_count"],
        read.json["expected_actuals"]["source_paths_are_editor_authored"],
        read.json["expected_actuals"]["lowered_artifact_count"],
        read.json["expected_actuals"]["zone_count"],
        read.json["expected_actuals"]["boss_count"],
        read.json["expected_actuals"]["hellforged_item_count"],
        read.json["expected_actuals"]["escape_denied"],
        read.json["expected_actuals"]["content_has_forbidden_true"],
        read.json["edges"]
            .as_object()
            .map(|edges| edges.len())
            .unwrap_or_default(),
    );
    Ok(())
}

struct Policies {
    package: InfernalDeepsPackagePolicy,
    access: InfernalAccessPolicy,
    attrition: InfernalAttritionPolicy,
    zones: InfernalZonePolicy,
    bosses: InfernalBossAgentPolicy,
    relics: InfernalRelicVaultPolicy,
    withering: WitheringRaidPathPolicy,
    undeath: UndeathPolicy,
}

fn policies() -> Policies {
    Policies {
        package: InfernalDeepsPackagePolicy::ashen_reach_defaults(),
        access: ashen_reach_infernal_access_policy(),
        attrition: ashen_reach_infernal_attrition_policy(),
        zones: ashen_reach_infernal_zone_policy(),
        bosses: InfernalBossAgentPolicy::ashen_reach_defaults(),
        relics: InfernalRelicVaultPolicy::ashen_reach_defaults(),
        withering: WitheringRaidPathPolicy::ashen_reach_defaults(),
        undeath: ashen_reach_undeath_policy(),
    }
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
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn outside_state() -> InfernalAccessState {
    InfernalAccessState {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        character_id: "char.morvane_ash".to_string(),
        party_id: "party.black_reliquary.1".to_string(),
        location_id: "gate.cinders".to_string(),
        faction_id: None,
        inside_infernal_deeps: false,
        current_circle: None,
        no_recall_lock_active: false,
        infernal_debt_count: 0,
        access_history: Vec::new(),
    }
}

fn entry_request() -> InfernalEntryRequest {
    InfernalEntryRequest {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        request_id: "entry.long_descent.morvane.1".to_string(),
        character_id: "char.morvane_ash".to_string(),
        path: InfernalEntryPath::LongDescentEarnedCindermark,
        at_gate_id: Some("gate.cinders".to_string()),
        completed_long_descent_ref: Some("quest.long_descent.sunken_vaults".to_string()),
        cindermark_ref: Some("sigil.cindermark.earned".to_string()),
        cindermark_source: Some(CindermarkSource::EarnedQuestline),
        lesser_devil_id: None,
        pact_contract_id: None,
        accepted_hidden_cost: false,
        faction_power_id: None,
        thin_place_death_id: None,
        current_location_thin_place: false,
        accident_not_player_chosen: false,
    }
}

fn escape_attempt(kind: InfernalEscapeKind) -> InfernalEscapeAttempt {
    InfernalEscapeAttempt {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        attempt_id: "escape.recall.morvane.1".to_string(),
        character_id: "char.morvane_ash".to_string(),
        kind,
        declared_destination_id: "safe.galdmere.shrine".to_string(),
    }
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1063.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1063.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true || edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG"
        {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1063.edge.{name}"),
                reason: format!("unexpected edge {edge}"),
            });
        }
    }
    Ok(())
}
