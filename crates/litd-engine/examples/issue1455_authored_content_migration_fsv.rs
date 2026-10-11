use std::fs;

#[path = "fixtures/issue1061_undeath_fixture.rs"]
mod issue1061_undeath_fixture;
#[path = "fixtures/issue1066_infernal_access_fixture.rs"]
mod issue1066_infernal_access_fixture;
#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use issue1061_undeath_fixture::ashen_reach_undeath_policy;
use issue1066_infernal_access_fixture::ashen_reach_infernal_access_policy;
use issue1067_infernal_zones_fixture::ashen_reach_infernal_zone_policy;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;
use litd_engine::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        DeriveInfernalDeepsPackageArgs, InfernalBossAgentPolicy, InfernalDeepsPackagePolicy,
        InfernalRelicVaultPolicy, RpgMapFeature, RpgMapFeatureKind, RpgMapFidelity,
        RpgMapOpenRequest, RpgMapRuntimeState, RpgMapSource, RpgMapSourceKind, RpgMapSourceStatus,
        WitheringRaidPathPolicy, derive_infernal_deeps_package, infernal_deeps_content_records,
        persist_and_lower_rpg_authored_content, remove_rpg_map_source, resolve_rpg_map_access,
        validate_rpg_authored_content_records,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const MAP_ADR_PATH: &str = "docs/adr/adr-944-diegetic-rpg-maps.md";
const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const PANTHEON_PATH: &str = "docs/lore/ashen-reach-pantheon-factions.md";
const AUTHORED_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/authored_content.rs";
const AUTHORED_CONTENT_TEST_PATH: &str = "crates/litd-engine/src/rpg/authored_content_tests.rs";
const INFERNAL_TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps_types.rs";
const INFERNAL_POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps.rs";
const INFERNAL_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps_content.rs";
const INFERNAL_TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_deeps_tests.rs";
const MAP_ACCESS_TYPES_PATH: &str = "crates/litd-engine/src/rpg/map_access_types.rs";
const MAP_ACCESS_PATH: &str = "crates/litd-engine/src/rpg/map_access.rs";
const MAP_ACCESS_STORE_PATH: &str = "crates/litd-engine/src/rpg/map_access_store.rs";
const MAP_ACCESS_TEST_PATH: &str = "crates/litd-engine/src/rpg/map_access_tests.rs";
const CALYX_CONTENT_PATH: &str = "crates/litd-engine/src/calyx_content.rs";
const CALYX_CONTENT_TEST_PATH: &str = "crates/litd-engine/src/calyx_content_tests.rs";
const CALYX_LOWERING_PATH: &str = "crates/litd-engine/src/calyx_lowering.rs";
const CALYX_LOWERING_TEST_PATH: &str = "crates/litd-engine/src/calyx_lowering_tests.rs";
const UNDEATH_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1061_undeath_fixture.rs";
const ACCESS_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1066_infernal_access_fixture.rs";
const ATTRITION_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1070_infernal_attrition_fixture.rs";
const ZONES_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1067_infernal_zones_fixture.rs";
const EXAMPLE_PATH: &str =
    "crates/litd-engine/examples/issue1455_authored_content_migration_fsv.rs";
const SOURCE_PATHS: [&str; 23] = [
    MAP_ADR_PATH,
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    PANTHEON_PATH,
    EXAMPLE_PATH,
    AUTHORED_CONTENT_PATH,
    AUTHORED_CONTENT_TEST_PATH,
    INFERNAL_TYPES_PATH,
    INFERNAL_POLICY_PATH,
    INFERNAL_CONTENT_PATH,
    INFERNAL_TEST_PATH,
    MAP_ACCESS_TYPES_PATH,
    MAP_ACCESS_PATH,
    MAP_ACCESS_STORE_PATH,
    MAP_ACCESS_TEST_PATH,
    CALYX_CONTENT_PATH,
    CALYX_CONTENT_TEST_PATH,
    CALYX_LOWERING_PATH,
    CALYX_LOWERING_TEST_PATH,
    UNDEATH_EXAMPLE_FIXTURE_PATH,
    ACCESS_EXAMPLE_FIXTURE_PATH,
    ATTRITION_EXAMPLE_FIXTURE_PATH,
    ZONES_EXAMPLE_FIXTURE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1455_authored_content_migration")?;

    let records = infernal_deeps_records()?;
    let validation = validate_rpg_authored_content_records(&records)?;
    let vault_dir = run.artifact_dir("calyx-vault-original")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    let edited_records = edited_relic_records(&records);
    let edited_vault_dir = run.artifact_dir("calyx-vault-edited")?;
    let edited_database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&edited_vault_dir))?;
    let edited_store = EngineCalyxContentStore::new(edited_database);
    let edited_lowered = persist_and_lower_rpg_authored_content(&edited_store, &edited_records)?;
    let field_edit_changed_lowered =
        fingerprint_for(&lowered, "server.ashen_reach.infernal_deeps.relic_vaults")
            != fingerprint_for(
                &edited_lowered,
                "server.ashen_reach.infernal_deeps.relic_vaults",
            );

    let fresh_map = resolve_rpg_map_access(
        &RpgMapRuntimeState::fresh("player.issue1455"),
        &RpgMapOpenRequest::no_map_screen(),
    )?;
    let mut map_state = RpgMapRuntimeState::fresh("player.issue1455");
    map_state.sources.push(cartographer_source());
    let cartographer_map = resolve_rpg_map_access(
        &map_state,
        &RpgMapOpenRequest::open_source("map.vhael.vellum_of_ember_roads"),
    )?;
    remove_rpg_map_source(
        &mut map_state,
        "map.vhael.vellum_of_ember_roads",
        RpgMapSourceStatus::Lost,
    )?;
    let lost_map = resolve_rpg_map_access(
        &map_state,
        &RpgMapOpenRequest::open_source("map.vhael.vellum_of_ember_roads"),
    )?;

    let edges = json!({
        "compiled_rpg_source_rejected": compiled_source_edge(),
        "testpass_anchor_rejected": testpass_anchor_edge(),
        "free_builtin_map_rejected": free_map_edge(),
        "opaque_package_blob_rejected": package_blob_edge(),
        "acquired_map_minimap_hud_rejected": acquired_map_minimap_edge(),
        "cartographer_map_source_allowed": cartographer_map_content_edge(),
    });
    require_all_edges(&edges)?;

    let infernal_content_source = fs::read_to_string(INFERNAL_CONTENT_PATH)?;
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
        "schema": "litd.issue1455.authored_content_migration_fsv.v1",
        "issue": 1455,
        "scope": "migration guardrail slice; #1455 remains open until all hardcoded Ashen Reach packages are re-authored and compiled content is deleted",
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_original": vault_dir.display().to_string(),
            "calyx_vault_edited": edited_vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON plus the two Calyx vault directories from disk. Confirm the six editor-authored rows, lowered fingerprints, no default map readback, cartographer acquired-map readback, and negative edge probes."
        },
        "sufficiency_argument": "This #1455 slice installs the migration guardrail before deleting remaining compiled content: RPG content must come from editor/MCP source paths, cannot use TestPass anchors, cannot lower a package readback blob as one row, and cannot grant default map/minimap/GPS behavior. Infernal Deeps now writes six authored Calyx rows and lowers each through the existing valve. A one-field authored edit changes the lowered relic fingerprint without recompilation. Runtime map access proves a fresh player has no map surface, a cartographer source opens only the acquired scoped map, and losing that source removes map access.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "authored_guardrail_source_count": source_count_for("authored_content"),
            "infernal_deeps_source_count": source_count_for("infernal_deeps"),
            "undeath_fixture_source_count": source_count_for("issue1061_undeath_fixture"),
            "access_fixture_source_count": source_count_for("issue1066_infernal_access_fixture"),
            "attrition_fixture_source_count": source_count_for("issue1070_infernal_attrition_fixture"),
            "zone_fixture_source_count": source_count_for("issue1067_infernal_zones_fixture"),
            "map_access_source_count": source_count_for("map_access"),
            "calyx_content_source_count": source_count_for("calyx_content"),
            "calyx_lowering_source_count": source_count_for("calyx_lowering"),
            "authored_record_count": validation.record_count,
            "lowered_row_count": lowered.row_count,
            "edited_lowered_row_count": edited_lowered.row_count,
            "all_editor_authored_sources": validation.all_editor_authored_sources,
            "no_builtin_map_runtime": validation.no_builtin_map_runtime,
            "no_self_attesting_anchors": validation.no_self_attesting_anchors,
            "field_edit_changed_lowered": field_edit_changed_lowered,
            "original_package_fingerprint": lowered.package_fingerprint,
            "edited_package_fingerprint": edited_lowered.package_fingerprint,
            "default_map_surface_visible": fresh_map.readback.map_surface_visible,
            "default_map_access": fresh_map.readback.access,
            "cartographer_map_surface_visible": cartographer_map.readback.map_surface_visible,
            "cartographer_source_kind": cartographer_map.readback.source_kind,
            "cartographer_gps_disabled": cartographer_map.readback.gps_indicators_disabled,
            "lost_map_surface_visible": lost_map.readback.map_surface_visible,
            "lost_map_access": lost_map.readback.access,
            "infernal_content_old_blob_literal_present": infernal_content_source.contains("\"infernal_deeps_package\""),
            "edge_count": edges.as_object().map(|edges| edges.len()).unwrap_or_default(),
        },
        "authored_validation": validation,
        "lowered_original": {
            "package_fingerprint": lowered.package_fingerprint,
            "entities": lowered.entities,
        },
        "lowered_edited": {
            "package_fingerprint": edited_lowered.package_fingerprint,
            "entities": edited_lowered.entities,
        },
        "map_runtime": {
            "fresh": fresh_map.readback,
            "cartographer": cartographer_map.readback,
            "lost": lost_map.readback,
        },
        "edges": edges,
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(
        &report["expected_actuals"]["authored_guardrail_source_count"],
        3,
    )?;
    require_i64(
        &report["expected_actuals"]["infernal_deeps_source_count"],
        4,
    )?;
    require_i64(
        &report["expected_actuals"]["undeath_fixture_source_count"],
        1,
    )?;
    require_i64(
        &report["expected_actuals"]["access_fixture_source_count"],
        1,
    )?;
    require_i64(
        &report["expected_actuals"]["attrition_fixture_source_count"],
        1,
    )?;
    require_i64(&report["expected_actuals"]["zone_fixture_source_count"], 1)?;
    require_i64(&report["expected_actuals"]["map_access_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["calyx_content_source_count"], 2)?;
    require_i64(
        &report["expected_actuals"]["calyx_lowering_source_count"],
        2,
    )?;
    require_i64(&report["expected_actuals"]["authored_record_count"], 6)?;
    require_i64(&report["expected_actuals"]["lowered_row_count"], 6)?;
    require_bool(
        &report["expected_actuals"]["all_editor_authored_sources"],
        true,
    )?;
    require_bool(&report["expected_actuals"]["no_builtin_map_runtime"], true)?;
    require_bool(
        &report["expected_actuals"]["field_edit_changed_lowered"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["default_map_surface_visible"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["cartographer_map_surface_visible"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["cartographer_gps_disabled"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["lost_map_surface_visible"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["infernal_content_old_blob_literal_present"],
        false,
    )?;

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1455_AUTHORED_CONTENT_FSV report={} bytes={} sha256={} rows={} edited_rows={} no_default_map={} cartographer_map={} lost_map={} edit_changed={} edges={}",
        read.path.display(),
        read.bytes.len(),
        read.sha256,
        read.json["expected_actuals"]["lowered_row_count"],
        read.json["expected_actuals"]["edited_lowered_row_count"],
        !read.json["expected_actuals"]["default_map_surface_visible"]
            .as_bool()
            .unwrap_or(true),
        read.json["expected_actuals"]["cartographer_map_surface_visible"],
        !read.json["expected_actuals"]["lost_map_surface_visible"]
            .as_bool()
            .unwrap_or(true),
        read.json["expected_actuals"]["field_edit_changed_lowered"],
        read.json["expected_actuals"]["edge_count"],
    );
    Ok(())
}

fn infernal_deeps_records() -> Result<Vec<EngineContentRecord>, Box<dyn std::error::Error>> {
    let package = InfernalDeepsPackagePolicy::ashen_reach_defaults();
    let readback = derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
        package: &package,
        access: &ashen_reach_infernal_access_policy(),
        attrition: &ashen_reach_infernal_attrition_policy(),
        zones: &ashen_reach_infernal_zone_policy(),
        bosses: &InfernalBossAgentPolicy::ashen_reach_defaults(),
        relics: &InfernalRelicVaultPolicy::ashen_reach_defaults(),
        withering: &WitheringRaidPathPolicy::ashen_reach_defaults(),
        undeath: &ashen_reach_undeath_policy(),
    })?;
    Ok(infernal_deeps_content_records(&package, &readback)?)
}

fn edited_relic_records(records: &[EngineContentRecord]) -> Vec<EngineContentRecord> {
    let mut edited = records.to_vec();
    let relic = edited
        .iter_mut()
        .find(|record| record.id == "server.ashen_reach.infernal_deeps.relic_vaults")
        .expect("relic vault record");
    relic.payload["highest_power_rating_milli"] = json!(975);
    relic.source.source_hash = payload_sha256(&relic.payload);
    edited
}

fn cartographer_source() -> RpgMapSource {
    RpgMapSource {
        source_id: "map.vhael.vellum_of_ember_roads".to_string(),
        kind: RpgMapSourceKind::CartographerPurchase,
        status: RpgMapSourceStatus::Available,
        scope_id: "region.vhael.galdmere_east_road".to_string(),
        display_name: "Vellum of Ember Roads".to_string(),
        acquired_from: "Sera Vell, cartographer of the Galdmere East Gate stair".to_string(),
        quality_tier: 2,
        fidelity: RpgMapFidelity::Incomplete,
        features: vec![RpgMapFeature {
            feature_id: "landmark.old_ash_road".to_string(),
            kind: RpgMapFeatureKind::Landmark,
            label: "Old Ash Road".to_string(),
            world_position: [32.0, 0.0, -18.0],
        }],
    }
}

fn fingerprint_for(
    readback: &litd_engine::rpg::RpgAuthoredContentLoweringReadback,
    id: &str,
) -> String {
    readback
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .expect("entity readback")
        .lowered_package_fingerprint
        .clone()
}

fn compiled_source_edge() -> Value {
    let mut record = authored_record("edge.compiled_source", json!({"known_value": 1}));
    record.source.path = "rpg://infernal_deeps/edge.compiled_source".to_string();
    edge_from_result(validate_rpg_authored_content_records(&[record]), true)
}

fn testpass_anchor_edge() -> Value {
    let mut record = authored_record("edge.testpass_anchor", json!({"known_value": 1}));
    record.anchors.push(Anchor {
        kind: AnchorKind::TestPass,
        value: AnchorValue::Bool(true),
        source: "self".to_string(),
        observed_at: 1_786_000_000,
        confidence: 1.0,
    });
    edge_from_result(validate_rpg_authored_content_records(&[record]), true)
}

fn free_map_edge() -> Value {
    let record = authored_record("edge.free_map", json!({"built_in_map_required": true}));
    edge_from_result(validate_rpg_authored_content_records(&[record]), true)
}

fn package_blob_edge() -> Value {
    let record = authored_record(
        "edge.package_blob",
        json!({
            "surface_kind": "infernal_deeps_package",
            "readback": {"zone_count": 7},
        }),
    );
    edge_from_result(validate_rpg_authored_content_records(&[record]), true)
}

fn acquired_map_minimap_edge() -> Value {
    let record = authored_record(
        "edge.acquired_map_minimap",
        json!({
            "acquired_map_source": {
                "source_kind": "cartographer_purchase",
                "scope_id": "region.vhael",
                "requires_possession_or_knowledge": true,
                "default_available": false,
                "player_position_indicator": false,
                "quest_markers": false,
                "route_lines": false,
                "minimap_hud": true
            },
        }),
    );
    edge_from_result(validate_rpg_authored_content_records(&[record]), true)
}

fn cartographer_map_content_edge() -> Value {
    let record = authored_record(
        "edge.cartographer_map",
        json!({
            "acquired_map_source": {
                "source_kind": "cartographer_purchase",
                "scope_id": "region.vhael",
                "requires_possession_or_knowledge": true,
                "default_available": false,
                "player_position_indicator": false,
                "quest_markers": false,
                "route_lines": false,
                "minimap_hud": false
            },
        }),
    );
    edge_from_result(validate_rpg_authored_content_records(&[record]), false)
}

fn authored_record(id: &str, fields: Value) -> EngineContentRecord {
    let mut payload = json!({
        "schema": "litd.rpg.issue1455.edge.v1",
        "surface_kind": "issue1455_edge_entity",
        "source_issue": 1455,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
    });
    let object = payload.as_object_mut().expect("payload object");
    for (key, value) in fields.as_object().expect("fields object") {
        object.insert(key.clone(), value.clone());
    }
    EngineContentRecord {
        kind: EngineContentKind::Lore,
        id: id.to_string(),
        display_name: id.to_string(),
        payload: payload.clone(),
        source: EngineContentSource {
            path: format!("editor://rpg/issue1455/{id}"),
            author: "editor_authoring.issue1455.edge".to_string(),
            source_hash: payload_sha256(&payload),
        },
        anchors: Vec::new(),
    }
}

fn edge_from_result<T, E: std::fmt::Display>(result: Result<T, E>, should_reject: bool) -> Value {
    match (result, should_reject) {
        (Ok(_), false) => json!({"passed": true, "rejected": false}),
        (Err(error), true) => json!({"passed": true, "rejected": true, "error": error.to_string()}),
        (Ok(_), true) => {
            json!({"passed": false, "rejected": false, "error": "unexpectedly accepted"})
        }
        (Err(error), false) => {
            json!({"passed": false, "rejected": true, "error": error.to_string()})
        }
    }
}

fn require_all_edges(edges: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let object = edges.as_object().ok_or("edges must be an object")?;
    for (name, edge) in object {
        require_bool(&edge["passed"], true).map_err(|error| format!("{name}: {error}"))?;
    }
    Ok(())
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({"path": path, "bytes": bytes.len(), "sha256": sha256_hex(&bytes)}))
}

fn payload_sha256(payload: &Value) -> String {
    sha256_hex(&serde_json::to_vec(payload).expect("serialize payload"))
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
