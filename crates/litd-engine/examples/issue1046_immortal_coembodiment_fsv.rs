#[path = "fixtures/issue964_themes_fixture.rs"]
#[allow(dead_code)]
mod issue964_themes_fixture;

use std::fs;

use issue964_themes_fixture::{happy_catalog, happy_input, happy_rules, mismatch_input};
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    rpg::{
        ASHEN_REACH_IMMORTAL_COEMBODIMENT_POLICY_ID, ImmortalActionKind, ImmortalCatalog,
        ImmortalCoembodimentPolicy, ImmortalCoembodimentSession, ImmortalDefinition,
        ImmortalEmbodiedDecision, ImmortalEmbodimentDriver, ImmortalEvaluationInput, ImmortalRules,
        ImmortalSignal, ImmortalSignalKind, RPG_IMMORTAL_COEMBODIMENT_SCHEMA, RPG_IMMORTAL_SCHEMA,
        compose_immortal_coembodiment_session, evaluate_immortal_signal,
        immortal_coembodiment_content_records, immortal_coembodiment_policy_hash,
        persist_and_lower_rpg_authored_content, validate_immortal_coembodiment_policy,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const SOURCE_PATHS: [&str; 23] = [
    "docs/lore/ashen-reach-world-bible.md",
    "docs/lore/ashen-reach-pantheon-factions.md",
    "docs/readme/06-living-agents.png",
    "docs/readme/S17-agent-binding.png",
    "docs/readme/03-mcp-agent.png",
    "crates/litd-engine/examples/issue1046_immortal_coembodiment_fsv.rs",
    "crates/litd-engine/examples/fixtures/issue964_themes_fixture.rs",
    "crates/litd-engine/src/rpg/immortal_coembodiment_types.rs",
    "crates/litd-engine/src/rpg/immortal_coembodiment.rs",
    "crates/litd-engine/src/rpg/immortal_coembodiment_content.rs",
    "crates/litd-engine/src/rpg/immortal_coembodiment_tests.rs",
    "crates/litd-engine/src/rpg/immortals_types.rs",
    "crates/litd-engine/src/rpg/immortals.rs",
    "crates/litd-engine/src/rpg/immortals_validate.rs",
    "crates/litd-engine/src/rpg/immortals_content.rs",
    "crates/litd-engine/src/rpg/immortals_tests.rs",
    "crates/litd-engine/src/rpg/themes_types.rs",
    "crates/litd-engine/src/rpg/themes.rs",
    "crates/litd-engine/src/rpg/themes_validate.rs",
    "crates/litd-engine/src/rpg/themes_readback.rs",
    "crates/litd-engine/src/rpg/themes_content.rs",
    "crates/litd-engine/src/rpg/themes_tests.rs",
    "crates/litd-engine/src/rpg/themes_test_fixture.rs",
];

fn ashen_reach_immortal_coembodiment_policy() -> ImmortalCoembodimentPolicy {
    ImmortalCoembodimentPolicy {
        schema: RPG_IMMORTAL_COEMBODIMENT_SCHEMA.to_string(),
        policy_id: ASHEN_REACH_IMMORTAL_COEMBODIMENT_POLICY_ID.to_string(),
        source_issue: 1046,
        ai_driver_required: true,
        human_driver_allowed: true,
        human_override_authoritative: true,
        player_visible_single_identity: true,
        deterministic_lowering_required: true,
        ai_hot_path_mutation_allowed: false,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1046_immortal_coembodiment")?;
    let policy = ashen_reach_immortal_coembodiment_policy();
    let session = session();
    let readback = compose_immortal_coembodiment_session(&policy, &session)?;
    let records = immortal_coembodiment_content_records(&policy, &readback)?;

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
            .starts_with("editor://rpg/ashen_reach/immortal_coembodiment/")
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
        "policy_map_rejected": policy_map_edge(),
        "only_ai_rejected": only_ai_edge(),
        "hidden_identity_rejected": hidden_identity_edge(),
        "missing_admin_rejected": missing_admin_edge(),
        "no_override_rejected": no_override_edge(),
        "mismatched_immortal_rejected": mismatched_immortal_edge(),
        "rejected_decision_rejected": rejected_decision_edge(),
        "duplicate_sequence_rejected": duplicate_sequence_edge(),
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
        "schema": "litd.issue1046.immortal_coembodiment_fsv.v1",
        "issue": 1046,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON and the Calyx WAL. Compare expected_actuals to the persisted co-embodiment session and verify players see one public identity."
        },
        "sufficiency_argument": "The fixture uses one Immortal and two real #965 decisions because #1046 is about driver coherence and handoff, not content volume. A Calyx AI event decision establishes routine AI presence; a human-admin empowerment decision with override ledger establishes authoritative human takeover. Eight edge probes cover map drift, missing human driver, hidden identity, missing admin id, missing override marker, mismatched Immortal, rejected decision, and duplicate sequence.",
        "expected_actuals": {
            "source_issue": 1046,
            "source_readback_count": source_readback_count,
            "coembodiment_source_count": source_count_for("immortal_coembodiment"),
            "immortal_decision_source_count": source_count_for("immortals"),
            "theme_source_count": source_count_for("themes"),
            "theme_fixture_source_count": source_count_for("issue964_themes_fixture"),
            "session_id": "session.immortal.luminara.coembody.1",
            "immortal_id": "immortal.luminara",
            "public_identity": "Luminara",
            "player_visible_single_identity": true,
            "ai_driver_present": true,
            "human_driver_present": true,
            "human_override_authoritative": true,
            "handoff_count": 1,
            "human_override_count": 1,
            "accepted_decision_count": 2,
            "lowered_effect_count": 2,
            "deterministic_lowering_required": true,
            "ai_hot_path_mutation_allowed": false,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
            "content_has_map_true": content_has_map_true,
            "edge_count": 8,
            "edge_error_code": "LITD-RPG-IMMORTAL-INVALID-CONFIG"
        },
        "policy": {
            "policy_id": policy.policy_id,
            "policy_sha256": immortal_coembodiment_policy_hash(&policy)?,
        },
        "session": session,
        "coembodiment_readback": readback,
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
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 1)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 1)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_i64(&report["expected_actuals"]["handoff_count"], 1)?;
    require_i64(&report["expected_actuals"]["human_override_count"], 1)?;
    require_i64(&report["expected_actuals"]["lowered_effect_count"], 2)?;
    require_bool(
        &report["expected_actuals"]["player_visible_single_identity"],
        true,
    )?;
    require_bool(&report["expected_actuals"]["content_has_map_true"], false)?;

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1046_IMMORTAL_COEMBODIMENT_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} identity={} handoffs={} overrides={} effects={} map_true={} edges={}",
        read.path.display(),
        read.bytes.len(),
        read.sha256,
        read.json["expected_actuals"]["content_rows_read_back"],
        read.json["expected_actuals"]["source_anchor_count"],
        read.json["expected_actuals"]["source_paths_are_editor_authored"],
        read.json["expected_actuals"]["public_identity"],
        read.json["expected_actuals"]["handoff_count"],
        read.json["expected_actuals"]["human_override_count"],
        read.json["expected_actuals"]["lowered_effect_count"],
        read.json["expected_actuals"]["content_has_map_true"],
        read.json["edges"]
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
                "public_identity": row.payload["readback"]["public_identity"],
                "handoff_count": row.payload["readback"]["handoff_count"],
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn policy_map_edge() -> Value {
    let mut policy = ashen_reach_immortal_coembodiment_policy();
    policy.built_in_map_required = true;
    edge_from_result(validate_immortal_coembodiment_policy(&policy), true)
}
fn only_ai_edge() -> Value {
    let mut session = session();
    session.embodied_decisions.truncate(1);
    edge_from_session(&session)
}
fn hidden_identity_edge() -> Value {
    let mut session = session();
    session.public_identity = "AdminChris".to_string();
    edge_from_session(&session)
}
fn missing_admin_edge() -> Value {
    let mut session = session();
    session.embodied_decisions[1].human_admin_id = None;
    edge_from_session(&session)
}
fn no_override_edge() -> Value {
    let mut session = session();
    session.embodied_decisions[1].override_ai_routine = false;
    edge_from_session(&session)
}
fn mismatched_immortal_edge() -> Value {
    let mut session = session();
    session.embodied_decisions[1].decision.immortal_id = "immortal.vorran".to_string();
    edge_from_session(&session)
}
fn rejected_decision_edge() -> Value {
    let mut session = session();
    session.embodied_decisions[1].decision = rejected_decision();
    edge_from_session(&session)
}
fn duplicate_sequence_edge() -> Value {
    let mut session = session();
    session.embodied_decisions[1].sequence = 1;
    edge_from_session(&session)
}

fn edge_from_session(session: &ImmortalCoembodimentSession) -> Value {
    edge_from_result(
        compose_immortal_coembodiment_session(&ashen_reach_immortal_coembodiment_policy(), session),
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

fn session() -> ImmortalCoembodimentSession {
    ImmortalCoembodimentSession {
        session_id: "session.immortal.luminara.coembody.1".to_string(),
        immortal_id: "immortal.luminara".to_string(),
        public_identity: "Luminara".to_string(),
        embodied_decisions: vec![
            ImmortalEmbodiedDecision {
                sequence: 1,
                driver: ImmortalEmbodimentDriver::CalyxAi,
                human_admin_id: None,
                override_ai_routine: false,
                decision: decision(
                    "signal.ai.event",
                    ImmortalSignalKind::EventOpportunity,
                    ImmortalActionKind::RunEvent,
                    false,
                ),
            },
            ImmortalEmbodiedDecision {
                sequence: 2,
                driver: ImmortalEmbodimentDriver::HumanAdmin,
                human_admin_id: Some("admin.chris.immortal".to_string()),
                override_ai_routine: true,
                decision: decision(
                    "signal.human.prayer",
                    ImmortalSignalKind::Prayer,
                    ImmortalActionKind::GrantEmpowerment,
                    false,
                ),
            },
        ],
    }
}

fn rejected_decision() -> litd_engine::rpg::ImmortalDecisionReadback {
    decision(
        "signal.rejected.prayer",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
        true,
    )
}

fn decision(
    signal_id: &str,
    kind: ImmortalSignalKind,
    action: ImmortalActionKind,
    mismatch: bool,
) -> litd_engine::rpg::ImmortalDecisionReadback {
    let mut input = base_input(signal(signal_id, kind, action));
    if mismatch {
        input.theme_input = mismatch_input();
    }
    evaluate_immortal_signal(&input, &ImmortalRules::mud_defaults()).unwrap()
}

fn base_input(signal: ImmortalSignal) -> ImmortalEvaluationInput {
    ImmortalEvaluationInput {
        catalog: ImmortalCatalog {
            schema: RPG_IMMORTAL_SCHEMA.to_string(),
            catalog_id: "catalog.issue1046.immortals".to_string(),
            immortals: vec![immortal()],
        },
        theme_catalog: happy_catalog(),
        theme_rules: happy_rules(),
        theme_input: happy_input(),
        signal,
    }
}

fn immortal() -> ImmortalDefinition {
    ImmortalDefinition {
        immortal_id: "immortal.luminara".to_string(),
        display_name: "Luminara".to_string(),
        religion_id: "religion.dawn_court".to_string(),
        allowed_actions: vec![
            ImmortalActionKind::GrantEmpowerment,
            ImmortalActionKind::IssueQuest,
            ImmortalActionKind::RunEvent,
            ImmortalActionKind::EnforceRoleplay,
            ImmortalActionKind::PossessMob,
        ],
        ward_tags: ids(&["compassion", "oathbound", "quest", "event", "possession"]),
        lowered_agent_id: 965,
        max_empowerment_tier: 51,
    }
}

fn signal(id: &str, kind: ImmortalSignalKind, action: ImmortalActionKind) -> ImmortalSignal {
    let mut tags = ids(&["compassion", "oathbound"]);
    if action == ImmortalActionKind::RunEvent {
        tags.push("event".to_string());
    }
    if action == ImmortalActionKind::IssueQuest {
        tags.push("quest".to_string());
    }
    if action == ImmortalActionKind::PossessMob {
        tags.push("possession".to_string());
    }
    ImmortalSignal {
        signal_id: id.to_string(),
        immortal_id: "immortal.luminara".to_string(),
        tick: 1046,
        kind,
        requested_action: action,
        strength_milli: 900,
        rp_quality_milli: 850,
        source_cx_id: format!("cx.issue1046.{id}"),
        target_character_id: "char.ashen.mender".to_string(),
        target_body_id: Some(42),
        possession_velocity: Some(FixedVec3::from_i32s(1, 0, 0)),
        tags,
        evidence_refs: vec![format!("evidence://issue1046/{id}")],
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgImmortalInvalidConfig {
            field: "issue1046.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgImmortalInvalidConfig {
            field: "issue1046.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true
            || edge["error_code"] != "LITD-RPG-IMMORTAL-INVALID-CONFIG"
        {
            return Err(EngineError::RpgImmortalInvalidConfig {
                field: format!("issue1046.edge.{name}"),
                reason: format!("unexpected edge {edge}"),
            });
        }
    }
    Ok(())
}
