use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    math::FixedVec3,
    rpg::{
        ImmortalActionKind, ImmortalCatalog, ImmortalCoembodimentSession, ImmortalDefinition,
        ImmortalEmbodiedDecision, ImmortalEmbodimentDriver, ImmortalEvaluationInput, ImmortalRules,
        ImmortalSignal, ImmortalSignalKind, RPG_IMMORTAL_SCHEMA,
        compose_immortal_coembodiment_session, evaluate_immortal_signal,
        immortal_coembodiment_content_records, persist_and_lower_rpg_authored_content,
        validate_immortal_coembodiment_policy, validate_rpg_authored_content_records,
    },
    rpg_immortal_coembodiment_test_fixture::ashen_reach_immortal_coembodiment_policy,
    rpg_themes_test_fixture::{happy_catalog, happy_input, happy_rules},
};

#[test]
fn issue1046_composes_ai_and_human_into_one_public_immortal() {
    let policy = ashen_reach_immortal_coembodiment_policy();
    let readback = compose_immortal_coembodiment_session(&policy, &session()).unwrap();

    assert_eq!(readback.source_issue, 1046);
    assert_eq!(readback.public_identity, "Luminara");
    assert!(readback.player_visible_single_identity);
    assert!(readback.ai_driver_present);
    assert!(readback.human_driver_present);
    assert!(readback.human_override_authoritative);
    assert_eq!(readback.handoff_count, 1);
    assert_eq!(readback.human_override_count, 1);
    assert_eq!(readback.accepted_decision_count, 2);
    assert_eq!(readback.lowered_effect_count, 2);
    assert!(!readback.ai_hot_path_mutation_allowed);
    assert!(!readback.built_in_map_required);
    assert!(!readback.grants_map_or_minimap);
    assert!(!readback.grants_gps_markers);
}

#[test]
fn issue1046_issue1455_persists_coembodiment_session_as_calyx_agent_content() {
    let policy = ashen_reach_immortal_coembodiment_policy();
    let readback = compose_immortal_coembodiment_session(&policy, &session()).unwrap();
    let records = immortal_coembodiment_content_records(&policy, &readback).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("immortal-coembodiment");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert_eq!(lowered.row_count, 1);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert_eq!(lowered.entities[0].kind, EngineContentKind::Agent);
    assert_eq!(
        lowered.entities[0].id,
        "session.immortal.luminara.coembody.1"
    );
    assert!(
        lowered.entities[0]
            .source_path
            .starts_with("editor://rpg/ashen_reach/immortal_coembodiment/")
    );
    assert_eq!(
        lowered.entities[0].surface_kind,
        "immortal_coembodiment_session"
    );
    assert!(lowered.entities[0].lowered_field_count > 0);
    assert_eq!(
        records[0].payload["readback"]["public_identity"],
        "Luminara"
    );
    assert_eq!(records[0].payload["readback"]["handoff_count"], 1);
    assert_ne!(records[0].payload["built_in_map_required"], true);
    assert_ne!(records[0].payload["grants_map_or_minimap"], true);
    assert_ne!(records[0].payload["grants_gps_markers"], true);
}

#[test]
fn issue1046_rejects_identity_driver_override_and_map_drift() {
    let mut map = ashen_reach_immortal_coembodiment_policy();
    map.grants_gps_markers = true;
    assert_code(
        validate_immortal_coembodiment_policy(&map),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let policy = ashen_reach_immortal_coembodiment_policy();
    let mut only_ai = session();
    only_ai.embodied_decisions.truncate(1);
    assert_code(
        compose_immortal_coembodiment_session(&policy, &only_ai),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut hidden_identity = session();
    hidden_identity.public_identity = "AdminChris".to_string();
    assert_code(
        compose_immortal_coembodiment_session(&policy, &hidden_identity),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut missing_admin = session();
    missing_admin.embodied_decisions[1].human_admin_id = None;
    assert_code(
        compose_immortal_coembodiment_session(&policy, &missing_admin),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut no_override = session();
    no_override.embodied_decisions[1].override_ai_routine = false;
    assert_code(
        compose_immortal_coembodiment_session(&policy, &no_override),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut mismatch = session();
    mismatch.embodied_decisions[1].decision.immortal_id = "immortal.vorran".to_string();
    assert_code(
        compose_immortal_coembodiment_session(&policy, &mismatch),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );
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
                ),
            },
        ],
    }
}

fn decision(
    signal_id: &str,
    kind: ImmortalSignalKind,
    action: ImmortalActionKind,
) -> crate::rpg::ImmortalDecisionReadback {
    evaluate_immortal_signal(
        &base_input(signal(signal_id, kind, action)),
        &ImmortalRules::mud_defaults(),
    )
    .unwrap()
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

fn content_store(
    label: &str,
) -> (
    EngineCalyxContentStore,
    EngineCalyxDatabase,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database.clone(), label),
        database,
        vault_dir,
    )
}

// #1535: TestTempDir auto-cleans on Drop.
fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1046-{label}"))
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
