#[path = "fixtures/issue1066_infernal_access_fixture.rs"]
mod issue1066_infernal_access_fixture;

use std::fs;

use issue1066_infernal_access_fixture::ashen_reach_infernal_access_policy;
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        CindermarkSource, InfernalAccessPolicy, InfernalAccessState, InfernalEntryPath,
        InfernalEntryRequest, InfernalEscapeAttempt, InfernalEscapeKind,
        RPG_INFERNAL_ACCESS_SCHEMA, infernal_access_content_records, infernal_access_state_hash,
        persist_and_lower_rpg_authored_content, resolve_infernal_entry,
        resolve_infernal_escape_attempt, validate_infernal_access_policy,
        validate_infernal_access_state, validate_infernal_entry_request,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const COSMOLOGY_PATH: &str = "docs/lore/ashen-reach-cosmology.md";
const GAZETTEER_PATH: &str = "docs/lore/ashen-reach-ages-gazetteer.md";
const INFERNAL_REF_PATH: &str = "docs/readme/S06-infernal-deeps.png";
const GAMEPLAY_REF_PATH: &str = "docs/readme/GP14-infernal.png";
const TYPES_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_types.rs";
const POLICY_PATH: &str = "crates/litd-engine/src/rpg/infernal_access.rs";
const VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_validate.rs";
const CONTENT_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_content.rs";
const TEST_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_test_fixture.rs";
const EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue1066_infernal_access_fixture.rs";
const TEST_PATH: &str = "crates/litd-engine/src/rpg/infernal_access_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1066_infernal_access_fsv.rs";
const SOURCE_PATHS: [&str; 15] = [
    WORLD_BIBLE_PATH,
    COSMOLOGY_PATH,
    GAZETTEER_PATH,
    INFERNAL_REF_PATH,
    GAMEPLAY_REF_PATH,
    TYPES_PATH,
    POLICY_PATH,
    VALIDATE_PATH,
    CONTENT_PATH,
    TEST_FIXTURE_PATH,
    EXAMPLE_FIXTURE_PATH,
    TEST_PATH,
    EXAMPLE_PATH,
    "crates/litd-engine/src/rpg/map_access.rs",
    "crates/litd-engine/src/rpg/map_access_tests.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1066_infernal_access")?;
    let policy = ashen_reach_infernal_access_policy();
    validate_infernal_access_policy(&policy)?;
    let records = infernal_access_content_records(&policy)?;
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
            .starts_with("editor://rpg/ashen_reach/infernal_access/")
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

    let earned = resolve_infernal_entry(&outside_state(None), &policy, &earned_request())?;
    let devil_brand =
        resolve_infernal_entry(&outside_state(None), &policy, &devil_brand_request())?;
    let pact = resolve_infernal_entry(&outside_state(None), &policy, &lesser_devil_pact_request())?;
    let hollow = resolve_infernal_entry(
        &outside_state(Some("faction.hollow_pact")),
        &policy,
        &hollow_pact_request(),
    )?;
    let dragged = resolve_infernal_entry(&outside_state(None), &policy, &dragged_under_request())?;
    let recall = resolve_infernal_escape_attempt(
        &earned.state_after,
        &policy,
        &escape("escape.recall.fsv", InfernalEscapeKind::Recall),
    )?;
    let teleport = resolve_infernal_escape_attempt(
        &earned.state_after,
        &policy,
        &escape("escape.teleport.fsv", InfernalEscapeKind::Teleport),
    )?;
    let gate_portal = resolve_infernal_escape_attempt(
        &earned.state_after,
        &policy,
        &escape("escape.portal.fsv", InfernalEscapeKind::GatePortal),
    )?;

    let edges = json!({
        "duplicate_entry_rule_rejected": duplicate_rule_edge(&policy),
        "no_recall_policy_rejected": no_recall_policy_edge(&policy),
        "missing_cindermark_rejected": missing_cindermark_edge(&policy),
        "wrong_faction_seam_rejected": wrong_faction_edge(&policy),
        "chosen_dragged_under_rejected": chosen_dragged_under_edge(&policy),
        "outside_recall_lock_rejected": outside_recall_lock_edge(&policy),
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
        "schema": "litd.issue1066.infernal_access_fsv.v1",
        "issue": 1066,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this persisted JSON through Synapse/computer control, inspect source hashes and expected actuals, and separately inspect a saved bitmap from the same browser target."
        },
        "sufficiency_argument": "The #1066 proof covers every allowed Infernal Deeps entry family exactly once: earned Cindermark after the Long Descent at the Gate of Cinders, devil-branded Cindermark at a hidden cost, lesser-devil pact at a hidden cost, Hollow Pact seam by faction power, and dragged-under accident at a thin-place death. Escape probes deny recall, teleport, and portal from the entered state while preserving its hash. Six edge probes cover duplicate policy drift, disabled no-recall policy, missing Cindermark, wrong faction seam, player-chosen dragged-under travel, and an invalid outside no-recall lock.",
        "expected_actuals": {
            "source_issue": 1066,
            "entry_rule_count": 5,
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "access_source_count": source_count_for("infernal_access"),
            "map_access_source_count": source_count_for("map_access"),
            "calyx_content_rows": lowered.row_count,
            "calyx_content_rows_read_back": content_readbacks.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "destination_circle": "threshold",
            "no_recall_inside": true,
            "built_in_map_required": false,
            "earned_granted": true,
            "earned_debt_after": 0,
            "devil_brand_debt_after": 1,
            "pact_debt_after": 1,
            "hollow_debt_after": 1,
            "dragged_debt_after": 1,
            "recall_denied": true,
            "teleport_denied": true,
            "portal_denied": true,
            "escape_denial_reason": "infernal_no_recall_lock",
            "edge_count": 6,
            "edge_error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "policy": {
            "server_policy_id": policy.server_policy_id,
            "source_issue": policy.source_issue,
            "destination_circle": policy.destination_circle,
            "no_recall_inside": policy.no_recall_inside,
            "required_gate_id": policy.required_gate_id,
            "required_long_descent_ref": policy.required_long_descent_ref,
            "required_earned_cindermark_ref": policy.required_earned_cindermark_ref,
            "required_hollow_pact_faction_id": policy.required_hollow_pact_faction_id,
            "required_hollow_pact_power_id": policy.required_hollow_pact_power_id,
            "entry_rules": policy.entry_rules,
            "denied_escape_kinds": policy.denied_escape_kinds,
        },
        "happy_path": {
            "earned_cindermark": earned,
            "devil_branded_cindermark": devil_brand,
            "lesser_devil_pact": pact,
            "hollow_pact_seam": hollow,
            "dragged_under_accident": dragged,
        },
        "escape_denials": {
            "recall": recall,
            "teleport": teleport,
            "gate_portal": gate_portal,
        },
        "calyx": {
            "content_rows": content_rows(&content_readbacks),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_eq(
        &report["expected_actuals"]["source_readback_count"],
        json!(SOURCE_PATHS.len()),
    )?;
    require_eq(
        &report["expected_actuals"]["reference_source_count"],
        json!(5),
    )?;
    require_eq(&report["expected_actuals"]["access_source_count"], json!(8))?;
    require_eq(
        &report["expected_actuals"]["map_access_source_count"],
        json!(2),
    )?;
    require_eq(&report["expected_actuals"]["calyx_content_rows"], json!(7))?;
    require_eq(
        &report["expected_actuals"]["calyx_content_rows_read_back"],
        json!(7),
    )?;
    require_eq(&report["expected_actuals"]["source_anchor_count"], json!(0))?;
    require_eq(
        &report["expected_actuals"]["lowered_artifact_count"],
        json!(7),
    )?;
    require_eq(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        json!(true),
    )?;
    require_eq(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        json!(false),
    )?;
    require_eq(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        json!(false),
    )?;
    require_eq(
        &report["happy_path"]["earned_cindermark"]["granted"],
        json!(true),
    )?;
    require_eq(
        &report["happy_path"]["earned_cindermark"]["infernal_debt_after"],
        json!(0),
    )?;
    require_eq(
        &report["happy_path"]["devil_branded_cindermark"]["infernal_debt_after"],
        json!(1),
    )?;
    require_eq(
        &report["happy_path"]["lesser_devil_pact"]["infernal_debt_after"],
        json!(1),
    )?;
    require_eq(&report["escape_denials"]["recall"]["denied"], json!(true))?;
    require_eq(
        &report["escape_denials"]["teleport"]["state_unchanged"],
        json!(true),
    )?;
    require_eq(
        &report["escape_denials"]["gate_portal"]["denial_reason"],
        json!("infernal_no_recall_lock"),
    )?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1066_INFERNAL_ACCESS_FSV report={} bytes={} sha256={} entries={} rows={} anchors={} editor_sources={} lowered={} map_true={} recall_denied={} teleport_unchanged={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["policy"]["entry_rules"]
            .as_array()
            .map(|rules| rules.len())
            .unwrap_or_default(),
        readback.json["expected_actuals"]["calyx_content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["escape_denials"]["recall"]["denied"],
        readback.json["escape_denials"]["teleport"]["state_unchanged"],
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
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn duplicate_rule_edge(policy: &InfernalAccessPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.entry_rules[1].path = InfernalEntryPath::LongDescentEarnedCindermark;
    edge_from_result(validate_infernal_access_policy(&invalid), true)
}

fn no_recall_policy_edge(policy: &InfernalAccessPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.no_recall_inside = false;
    edge_from_result(validate_infernal_access_policy(&invalid), true)
}

fn missing_cindermark_edge(policy: &InfernalAccessPolicy) -> Value {
    let mut request = earned_request();
    request.cindermark_ref = None;
    edge_from_result(validate_infernal_entry_request(&request, policy), true)
}

fn wrong_faction_edge(policy: &InfernalAccessPolicy) -> Value {
    let state = outside_state(None);
    let before = infernal_access_state_hash(&state).unwrap();
    let result = resolve_infernal_entry(&state, policy, &hollow_pact_request()).map(|_| ());
    let after = infernal_access_state_hash(&state).unwrap();
    edge_from_result(result, before == after)
}

fn chosen_dragged_under_edge(policy: &InfernalAccessPolicy) -> Value {
    let mut request = dragged_under_request();
    request.accident_not_player_chosen = false;
    edge_from_result(validate_infernal_entry_request(&request, policy), true)
}

fn outside_recall_lock_edge(policy: &InfernalAccessPolicy) -> Value {
    let mut state = outside_state(None);
    state.no_recall_lock_active = true;
    let before = infernal_access_state_hash(&state).unwrap();
    let result = validate_infernal_access_state(&state, policy);
    let after = infernal_access_state_hash(&state).unwrap();
    edge_from_result(result, before == after)
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

fn outside_state(faction_id: Option<&str>) -> InfernalAccessState {
    InfernalAccessState {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        character_id: "char.hellrunner".to_string(),
        party_id: "party.vanguard".to_string(),
        location_id: "zone.sunken_vaults.gatewalk".to_string(),
        faction_id: faction_id.map(str::to_string),
        inside_infernal_deeps: false,
        current_circle: None,
        no_recall_lock_active: false,
        infernal_debt_count: 0,
        access_history: Vec::new(),
    }
}

fn earned_request() -> InfernalEntryRequest {
    request(InfernalEntryPath::LongDescentEarnedCindermark)
}

fn devil_brand_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::LongDescentDevilBrandedCindermark);
    request.cindermark_ref = Some("sigil.cindermark.devil_brand".to_string());
    request.cindermark_source = Some(CindermarkSource::DevilBrand);
    request.lesser_devil_id = Some("devil.vorthax_gatebroker".to_string());
    request.accepted_hidden_cost = true;
    request
}

fn lesser_devil_pact_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::LesserDevilPact);
    clear_gate_fields(&mut request);
    request.lesser_devil_id = Some("devil.vorthax_gatebroker".to_string());
    request.pact_contract_id = Some("contract.infernal_passage.1".to_string());
    request.accepted_hidden_cost = true;
    request
}

fn hollow_pact_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::HollowPactSeam);
    clear_gate_fields(&mut request);
    request.faction_power_id = Some("power.hollow_pact.infernal_seam".to_string());
    request.accepted_hidden_cost = true;
    request
}

fn dragged_under_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::DraggedUnderAccident);
    clear_gate_fields(&mut request);
    request.thin_place_death_id = Some("death.thin_place.ember_crack.1".to_string());
    request.current_location_thin_place = true;
    request.accident_not_player_chosen = true;
    request
}

fn clear_gate_fields(request: &mut InfernalEntryRequest) {
    request.at_gate_id = None;
    request.completed_long_descent_ref = None;
    request.cindermark_ref = None;
    request.cindermark_source = None;
}

fn request(path: InfernalEntryPath) -> InfernalEntryRequest {
    InfernalEntryRequest {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        request_id: format!("infernal.entry.{}", path.label()),
        character_id: "char.hellrunner".to_string(),
        path,
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

fn escape(attempt_id: &str, kind: InfernalEscapeKind) -> InfernalEscapeAttempt {
    InfernalEscapeAttempt {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        attempt_id: attempt_id.to_string(),
        character_id: "char.hellrunner".to_string(),
        kind,
        declared_destination_id: "zone.galdmere.square".to_string(),
    }
}

fn require_eq(value: &Value, expected: Value) -> Result<(), EngineError> {
    if value != &expected {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1066.report.value".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}

fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1066.edge.{name}"),
                reason: "edge mutated source state".to_string(),
            });
        }
        if edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG" {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1066.edge.{name}"),
                reason: format!("expected RPG stats error, got {edge}"),
            });
        }
    }
    Ok(())
}
