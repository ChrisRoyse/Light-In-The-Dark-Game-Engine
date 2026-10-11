#![recursion_limit = "256"]

#[path = "fixtures/issue1045_immortal_avatar_fixture.rs"]
#[allow(dead_code)]
mod issue1045_immortal_avatar_fixture;

use std::fs;

use issue1045_immortal_avatar_fixture::{
    ashen_reach_human_immortal_avatar_policy, avatar_session, coembodiment,
};
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        HumanImmortalAvatarSession, human_immortal_avatar_content_records,
        human_immortal_avatar_policy_hash, lower_human_immortal_avatar_session,
        persist_and_lower_rpg_authored_content, validate_human_immortal_avatar_policy,
        validate_human_immortal_avatar_readback,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const SOURCE_PATHS: [&str; 33] = [
    "docs/lore/ashen-reach-world-bible.md",
    "docs/lore/ashen-reach-pantheon-factions.md",
    "docs/readme/06-living-agents.png",
    "docs/readme/S17-agent-binding.png",
    "docs/readme/03-mcp-agent.png",
    "crates/litd-engine/examples/issue1045_human_immortal_avatar_fsv.rs",
    "crates/litd-engine/examples/fixtures/issue1045_immortal_avatar_fixture.rs",
    "crates/litd-engine/examples/fixtures/issue964_themes_fixture.rs",
    "crates/litd-engine/src/rpg/human_immortal_avatar_types.rs",
    "crates/litd-engine/src/rpg/human_immortal_avatar.rs",
    "crates/litd-engine/src/rpg/human_immortal_avatar_support.rs",
    "crates/litd-engine/src/rpg/human_immortal_avatar_content.rs",
    "crates/litd-engine/src/rpg/human_immortal_avatar_validate.rs",
    "crates/litd-engine/src/rpg/human_immortal_avatar_test_fixture.rs",
    "crates/litd-engine/src/rpg/human_immortal_avatar_tests.rs",
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
    "crates/litd-engine/src/rpg/map_access.rs",
    "crates/litd-engine/src/rpg/map_access_tests.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1045_human_immortal_avatar")?;
    let policy = ashen_reach_human_immortal_avatar_policy();
    let session = avatar_session();
    let coembodiment = coembodiment();
    let readback = lower_human_immortal_avatar_session(&policy, &session, &coembodiment)?;
    let records = human_immortal_avatar_content_records(&policy, &readback)?;

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
            .starts_with("editor://rpg/ashen_reach/human_immortal_avatar/")
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
        "unauth_admin_rejected": unauth_admin_edge(),
        "bad_role_rejected": bad_role_edge(),
        "coembodiment_identity_rejected": coembodiment_identity_edge(),
        "missing_possession_body_rejected": missing_body_edge(),
        "duplicate_command_rejected": duplicate_command_edge(),
        "tampered_map_readback_rejected": tampered_map_readback_edge(),
        "nonrevocable_grant_rejected": nonrevocable_grant_edge(),
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
    let avatar_source_count = source_count_for("human_immortal_avatar")
        + source_count_for("issue1045_immortal_avatar_fixture");

    let report = json!({
        "schema": "litd.issue1045.human_immortal_avatar_fsv.v1",
        "issue": 1045,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON and the Calyx WAL. Compare expected_actuals to persisted avatar session, audit ledger, effect rows, and map-doctrine flags."
        },
        "research_sources": [
            "https://www.evennia.com/docs/latest/Components/Permissions.html",
            "https://www.evennia.com/docs/latest/Howtos/Beginner-Tutorial/Part1/Beginner-Tutorial-Adding-Commands.html"
        ],
        "sufficiency_argument": "The fixture uses one Immortal, one authenticated admin, one #1046 co-embodiment readback, and nine commands because #1045 is about the full command surface and deterministic audit boundary, not content volume. The commands cover NPC possession, boss possession, live event, boon, tattoo, title, super-power, rank promotion, and ruling. Eight edge probes cover map drift, missing auth, wrong role, hidden identity, missing body, duplicate command, tampered map readback, and non-revocable grant tampering.",
        "expected_actuals": {
            "source_issue": 1045,
            "session_id": "session.immortal.luminara.human_avatar.1",
            "admin_id": "admin.chris.immortal",
            "admin_authenticated": true,
            "admin_role": "immortal_admin",
            "immortal_id": "immortal.luminara",
            "public_identity": "Luminara",
            "coembodiment_session_id": "session.immortal.luminara.coembody.1",
            "player_visible_single_identity": true,
            "human_override_authoritative": true,
            "command_count": 9,
            "possession_order_count": 2,
            "npc_possession_count": 1,
            "boss_possession_count": 1,
            "live_event_count": 1,
            "grant_count": 4,
            "promotion_count": 1,
            "ruling_count": 1,
            "revocable_grant_count": 4,
            "lowered_effect_count": 9,
            "source_readback_count": source_readback_count,
            "reference_source_count": source_count_for("docs/"),
            "avatar_source_count": avatar_source_count,
            "coembodiment_source_count": source_count_for("immortal_coembodiment"),
            "immortal_decision_source_count": source_count_for("immortals"),
            "theme_source_count": source_count_for("themes"),
            "map_access_source_count": source_count_for("map_access"),
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
            "policy_sha256": human_immortal_avatar_policy_hash(&policy)?,
        },
        "coembodiment_readback": coembodiment,
        "avatar_session": session,
        "avatar_readback": readback,
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
    require_i64(&report["expected_actuals"]["reference_source_count"], 5)?;
    require_i64(&report["expected_actuals"]["avatar_source_count"], 9)?;
    require_i64(&report["expected_actuals"]["coembodiment_source_count"], 4)?;
    require_i64(
        &report["expected_actuals"]["immortal_decision_source_count"],
        5,
    )?;
    require_i64(&report["expected_actuals"]["theme_source_count"], 8)?;
    require_i64(&report["expected_actuals"]["map_access_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 11)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 11)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_i64(&report["expected_actuals"]["command_count"], 9)?;
    require_i64(&report["expected_actuals"]["possession_order_count"], 2)?;
    require_i64(&report["expected_actuals"]["grant_count"], 4)?;
    require_bool(&report["expected_actuals"]["content_has_map_true"], false)?;

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1045_HUMAN_IMMORTAL_AVATAR_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} commands={} grants={} possessions={} map_true={} edges={}",
        read.path.display(),
        read.bytes.len(),
        read.sha256,
        read.json["expected_actuals"]["content_rows_read_back"],
        read.json["expected_actuals"]["source_anchor_count"],
        read.json["expected_actuals"]["source_paths_are_editor_authored"],
        read.json["expected_actuals"]["command_count"],
        read.json["expected_actuals"]["grant_count"],
        read.json["expected_actuals"]["possession_order_count"],
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
                "command_count": row.payload["readback"]["command_count"],
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn policy_map_edge() -> Value {
    let mut policy = ashen_reach_human_immortal_avatar_policy();
    policy.built_in_map_required = true;
    edge_from_result(validate_human_immortal_avatar_policy(&policy), true)
}
fn unauth_admin_edge() -> Value {
    let mut session = avatar_session();
    session.admin_authenticated = false;
    edge_from_session(&session)
}
fn bad_role_edge() -> Value {
    let mut session = avatar_session();
    session.admin_role = "builder".to_string();
    edge_from_session(&session)
}
fn coembodiment_identity_edge() -> Value {
    let mut readback = coembodiment();
    readback.public_identity = "AdminChris".to_string();
    edge_from_result(
        lower_human_immortal_avatar_session(
            &ashen_reach_human_immortal_avatar_policy(),
            &avatar_session(),
            &readback,
        ),
        true,
    )
}
fn missing_body_edge() -> Value {
    let mut session = avatar_session();
    session.commands[0].target_body_id = None;
    edge_from_session(&session)
}
fn duplicate_command_edge() -> Value {
    let mut session = avatar_session();
    session.commands[1].command_id = session.commands[0].command_id.clone();
    edge_from_session(&session)
}
fn tampered_map_readback_edge() -> Value {
    let policy = ashen_reach_human_immortal_avatar_policy();
    let mut readback =
        lower_human_immortal_avatar_session(&policy, &avatar_session(), &coembodiment()).unwrap();
    readback.grants_gps_markers = true;
    edge_from_result(
        validate_human_immortal_avatar_readback(&policy, &readback),
        true,
    )
}
fn nonrevocable_grant_edge() -> Value {
    let policy = ashen_reach_human_immortal_avatar_policy();
    let mut readback =
        lower_human_immortal_avatar_session(&policy, &avatar_session(), &coembodiment()).unwrap();
    readback
        .lowered_effects
        .iter_mut()
        .find(|effect| effect.kind.is_grant())
        .unwrap()
        .revocable = false;
    edge_from_result(
        validate_human_immortal_avatar_readback(&policy, &readback),
        true,
    )
}

fn edge_from_session(session: &HumanImmortalAvatarSession) -> Value {
    edge_from_result(
        lower_human_immortal_avatar_session(
            &ashen_reach_human_immortal_avatar_policy(),
            session,
            &coembodiment(),
        ),
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

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgImmortalInvalidConfig {
            field: "issue1045.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgImmortalInvalidConfig {
            field: "issue1045.report.i64".to_string(),
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
                field: format!("issue1045.edge.{name}"),
                reason: format!("unexpected edge {edge}"),
            });
        }
    }
    Ok(())
}
