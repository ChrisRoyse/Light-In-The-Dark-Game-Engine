use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    math::FixedVec3,
    rpg::{
        HumanImmortalAvatarCommand, HumanImmortalAvatarCommandKind, HumanImmortalAvatarSession,
        ImmortalActionKind, ImmortalCatalog, ImmortalCoembodimentReadback,
        ImmortalCoembodimentSession, ImmortalDefinition, ImmortalEmbodiedDecision,
        ImmortalEmbodimentDriver, ImmortalEvaluationInput, ImmortalRules, ImmortalSignal,
        ImmortalSignalKind, RPG_IMMORTAL_SCHEMA, compose_immortal_coembodiment_session,
        evaluate_immortal_signal, human_immortal_avatar_content_records,
        lower_human_immortal_avatar_session, persist_and_lower_rpg_authored_content,
        validate_human_immortal_avatar_policy, validate_human_immortal_avatar_readback,
        validate_rpg_authored_content_records,
    },
    rpg_human_immortal_avatar_test_fixture::ashen_reach_human_immortal_avatar_policy,
    rpg_immortal_coembodiment_test_fixture::ashen_reach_immortal_coembodiment_policy,
    rpg_themes_test_fixture::{happy_catalog, happy_input, happy_rules},
    sim_order::SimOrderSourceKind,
};

#[test]
fn issue1045_lowers_full_human_admin_immortal_avatar_session() {
    let policy = ashen_reach_human_immortal_avatar_policy();
    let readback =
        lower_human_immortal_avatar_session(&policy, &avatar_session(), &coembodiment()).unwrap();

    assert_eq!(readback.source_issue, 1045);
    assert_eq!(readback.public_identity, "Luminara");
    assert!(readback.admin_authenticated);
    assert!(readback.player_visible_single_identity);
    assert!(readback.human_override_authoritative);
    assert_eq!(readback.command_count, 9);
    assert_eq!(readback.possession_order_count, 2);
    assert_eq!(readback.npc_possession_count, 1);
    assert_eq!(readback.boss_possession_count, 1);
    assert_eq!(readback.live_event_count, 1);
    assert_eq!(readback.grant_count, 4);
    assert_eq!(readback.promotion_count, 1);
    assert_eq!(readback.ruling_count, 1);
    assert_eq!(readback.revocable_grant_count, 4);
    assert_eq!(readback.lowered_effect_count, 9);
    assert!(!readback.ai_hot_path_mutation_allowed);
    assert!(!readback.built_in_map_required);
    assert!(!readback.grants_map_or_minimap);
    assert!(!readback.grants_gps_markers);
    let order = readback.lowered_effects[0].sim_order.unwrap();
    assert_eq!(order.source.kind, SimOrderSourceKind::LoweredAgent);
    assert_eq!(order.source.id, 1045);
}

#[test]
fn issue1045_issue1455_persists_avatar_audit_and_effect_rows_without_map_hud() {
    let policy = ashen_reach_human_immortal_avatar_policy();
    let readback =
        lower_human_immortal_avatar_session(&policy, &avatar_session(), &coembodiment()).unwrap();
    let records = human_immortal_avatar_content_records(&policy, &readback).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("human-immortal-avatar");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let payload_json = serde_json::to_string(&records).unwrap();

    assert_eq!(validation.record_count, 11);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert_eq!(lowered.row_count, 11);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/human_immortal_avatar/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Agent)
            .count(),
        1
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Story)
            .count(),
        1
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Trigger)
            .count(),
        9
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| { row.surface_kind == "human_immortal_avatar_session" })
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| { row.surface_kind == "human_immortal_avatar_audit_ledger" })
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| { row.surface_kind == "human_immortal_avatar_effect" })
    );
    assert!(!payload_json.contains("\"built_in_map_required\":true"));
    assert!(!payload_json.contains("\"grants_map_or_minimap\":true"));
    assert!(!payload_json.contains("\"grants_gps_markers\":true"));
}

#[test]
fn issue1045_rejects_auth_identity_command_and_map_drift() {
    let mut map = ashen_reach_human_immortal_avatar_policy();
    map.grants_map_or_minimap = true;
    assert_code(
        validate_human_immortal_avatar_policy(&map),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let policy = ashen_reach_human_immortal_avatar_policy();
    let mut unauth = avatar_session();
    unauth.admin_authenticated = false;
    assert_code(
        lower_human_immortal_avatar_session(&policy, &unauth, &coembodiment()),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut mismatch = coembodiment();
    mismatch.public_identity = "AdminChris".to_string();
    assert_code(
        lower_human_immortal_avatar_session(&policy, &avatar_session(), &mismatch),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut missing_body = avatar_session();
    missing_body.commands[0].target_body_id = None;
    assert_code(
        lower_human_immortal_avatar_session(&policy, &missing_body, &coembodiment()),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut duplicate = avatar_session();
    duplicate.commands[1].command_id = duplicate.commands[0].command_id.clone();
    assert_code(
        lower_human_immortal_avatar_session(&policy, &duplicate, &coembodiment()),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut tampered =
        lower_human_immortal_avatar_session(&policy, &avatar_session(), &coembodiment()).unwrap();
    tampered.built_in_map_required = true;
    assert_code(
        validate_human_immortal_avatar_readback(&policy, &tampered),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );
}

fn avatar_session() -> HumanImmortalAvatarSession {
    HumanImmortalAvatarSession {
        session_id: "session.immortal.luminara.human_avatar.1".to_string(),
        admin_id: "admin.chris.immortal".to_string(),
        admin_role: "immortal_admin".to_string(),
        immortal_id: "immortal.luminara".to_string(),
        public_identity: "Luminara".to_string(),
        coembodiment_session_id: "session.immortal.luminara.coembody.1".to_string(),
        admin_authenticated: true,
        commands: vec![
            command(
                "cmd.possess.npc",
                HumanImmortalAvatarCommandKind::PossessNpc,
                "npc.ashen_mender",
                Some(42),
                Some(FixedVec3::from_i32s(1, 0, 0)),
                None,
            ),
            command(
                "cmd.possess.boss",
                HumanImmortalAvatarCommandKind::PossessBoss,
                "boss.soot_crowned_judge",
                Some(77),
                Some(FixedVec3::from_i32s(0, 1, 0)),
                None,
            ),
            command(
                "cmd.event.dawn_trial",
                HumanImmortalAvatarCommandKind::RunLiveEvent,
                "scene.ashen_bridge",
                None,
                None,
                Some("event.dawn_trial"),
            ),
            command(
                "cmd.grant.boon",
                HumanImmortalAvatarCommandKind::GrantBoon,
                "char.ashen_mender",
                None,
                None,
                Some("boon.luminara.mercy"),
            ),
            command(
                "cmd.grant.tattoo",
                HumanImmortalAvatarCommandKind::GrantTattoo,
                "char.ashen_mender",
                None,
                None,
                Some("tattoo.dawn.sigils"),
            ),
            command(
                "cmd.grant.title",
                HumanImmortalAvatarCommandKind::GrantTitle,
                "char.ashen_mender",
                None,
                None,
                Some("title.lightbearer"),
            ),
            command(
                "cmd.grant.power",
                HumanImmortalAvatarCommandKind::GrantSuperPower,
                "char.ashen_mender",
                None,
                None,
                Some("power.luminara.flame"),
            ),
            command(
                "cmd.promote.rank",
                HumanImmortalAvatarCommandKind::PromoteRank,
                "char.ashen_mender",
                None,
                None,
                Some("rank.dawn_court.initiate"),
            ),
            command(
                "cmd.ruling.mercy",
                HumanImmortalAvatarCommandKind::MakeRuling,
                "char.ashen_mender",
                None,
                None,
                Some("ruling.dawn_court.mercy"),
            ),
        ],
    }
}

fn command(
    command_id: &str,
    kind: HumanImmortalAvatarCommandKind,
    subject_ref: &str,
    target_body_id: Option<u32>,
    possession_velocity: Option<FixedVec3>,
    artifact_ref: Option<&str>,
) -> HumanImmortalAvatarCommand {
    HumanImmortalAvatarCommand {
        command_id: command_id.to_string(),
        kind,
        subject_ref: subject_ref.to_string(),
        target_body_id,
        possession_velocity,
        artifact_ref: artifact_ref.map(str::to_string),
        summary: format!("audit {command_id}"),
        evidence_refs: vec![format!("evidence://issue1045/{command_id}")],
    }
}

fn coembodiment() -> ImmortalCoembodimentReadback {
    compose_immortal_coembodiment_session(
        &ashen_reach_immortal_coembodiment_policy(),
        &ImmortalCoembodimentSession {
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
        },
    )
    .unwrap()
}

fn decision(
    signal_id: &str,
    kind: ImmortalSignalKind,
    action: ImmortalActionKind,
) -> crate::rpg::ImmortalDecisionReadback {
    evaluate_immortal_signal(
        &ImmortalEvaluationInput {
            catalog: ImmortalCatalog {
                schema: RPG_IMMORTAL_SCHEMA.to_string(),
                catalog_id: "catalog.issue1045.immortals".to_string(),
                immortals: vec![immortal()],
            },
            theme_catalog: happy_catalog(),
            theme_rules: happy_rules(),
            theme_input: happy_input(),
            signal: signal(signal_id, kind, action),
        },
        &ImmortalRules::mud_defaults(),
    )
    .unwrap()
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
        ward_tags: ids(&["compassion", "oathbound", "event", "possession"]),
        lowered_agent_id: 965,
        max_empowerment_tier: 51,
    }
}

fn signal(id: &str, kind: ImmortalSignalKind, action: ImmortalActionKind) -> ImmortalSignal {
    let mut tags = ids(&["compassion", "oathbound"]);
    if action == ImmortalActionKind::RunEvent {
        tags.push("event".to_string());
    }
    if action == ImmortalActionKind::PossessMob {
        tags.push("possession".to_string());
    }
    ImmortalSignal {
        signal_id: id.to_string(),
        immortal_id: "immortal.luminara".to_string(),
        tick: 1045,
        kind,
        requested_action: action,
        strength_milli: 900,
        rp_quality_milli: 850,
        source_cx_id: format!("cx.issue1045.{id}"),
        target_character_id: "char.ashen_mender".to_string(),
        target_body_id: Some(42),
        possession_velocity: Some(FixedVec3::from_i32s(1, 0, 0)),
        tags,
        evidence_refs: vec![format!("evidence://issue1045/{id}")],
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
    crate::test_temp::TestTempDir::new(&format!("issue1045-{label}"))
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
