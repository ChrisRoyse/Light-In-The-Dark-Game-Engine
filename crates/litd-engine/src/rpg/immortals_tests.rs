use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    math::FixedVec3,
    rpg::{
        ImmortalActionKind, ImmortalDefinition, ImmortalEvaluationInput, ImmortalRules,
        ImmortalSignal, ImmortalSignalKind, RPG_IMMORTAL_SCHEMA, evaluate_immortal_signal,
        immortal_content_records, persist_and_lower_rpg_authored_content,
        validate_rpg_authored_content_records,
    },
    rpg_themes_test_fixture::{happy_catalog, happy_input, happy_rules, mismatch_input},
    sim_order::SimOrderSourceKind,
};

#[test]
fn issue965_accepts_all_immortal_action_kinds() {
    let decisions = [
        eval(signal(
            "signal.prayer",
            ImmortalSignalKind::Prayer,
            ImmortalActionKind::GrantEmpowerment,
        )),
        eval(signal(
            "signal.quest",
            ImmortalSignalKind::QuestOpportunity,
            ImmortalActionKind::IssueQuest,
        )),
        eval(signal(
            "signal.event",
            ImmortalSignalKind::EventOpportunity,
            ImmortalActionKind::RunEvent,
        )),
        eval(signal(
            "signal.rule_break",
            ImmortalSignalKind::RuleBreak,
            ImmortalActionKind::EnforceRoleplay,
        )),
        eval(signal(
            "signal.possess",
            ImmortalSignalKind::PossessionRequest,
            ImmortalActionKind::PossessMob,
        )),
    ]
    .map(Result::unwrap);

    assert!(decisions.iter().all(|decision| decision.accepted));
    assert_eq!(decisions[0].lowered_effects[0].empowerment_tier, Some(51));
    assert!(decisions[1].lowered_effects[0].quest_id.is_some());
    assert!(decisions[2].lowered_effects[0].event_id.is_some());
    assert_eq!(
        decisions[3].lowered_effects[0]
            .enforcement_severity
            .as_deref(),
        Some("warning")
    );
    let order = decisions[4].lowered_effects[0].sim_order.unwrap();
    assert_eq!(order.source.kind, SimOrderSourceKind::LoweredAgent);
    assert_eq!(order.source.id, 965);
}

#[test]
fn issue965_refuses_empowerment_without_theme_fit() {
    let mut input = base_input(signal(
        "signal.mismatch",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    input.theme_input = mismatch_input();

    let decision = evaluate_immortal_signal(&input, &ImmortalRules::mud_defaults()).unwrap();
    assert!(!decision.accepted);
    assert_eq!(decision.grade, "religion_mismatch");
    assert_eq!(decision.effect_count, 0);
}

#[test]
fn issue965_fails_closed_on_invalid_edges() {
    let mut duplicate = base_input(signal(
        "signal.duplicate",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    duplicate
        .catalog
        .immortals
        .push(duplicate.catalog.immortals[0].clone());
    assert_code(
        evaluate_immortal_signal(&duplicate, &ImmortalRules::mud_defaults()),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut unknown = base_input(signal(
        "signal.unknown",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    unknown.signal.immortal_id = "immortal.missing".to_string();
    assert_code(
        evaluate_immortal_signal(&unknown, &ImmortalRules::mud_defaults()),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut outside_ward = base_input(signal(
        "signal.outside",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    outside_ward.signal.tags.push("forbidden".to_string());
    assert_code(
        evaluate_immortal_signal(&outside_ward, &ImmortalRules::mud_defaults()),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );

    let mut missing_body = base_input(signal(
        "signal.no_body",
        ImmortalSignalKind::PossessionRequest,
        ImmortalActionKind::PossessMob,
    ));
    missing_body.signal.target_body_id = None;
    assert_code(
        evaluate_immortal_signal(&missing_body, &ImmortalRules::mud_defaults()),
        "LITD-RPG-IMMORTAL-INVALID-CONFIG",
    );
}

#[test]
fn issue965_issue1455_persists_immortal_policy_as_authored_calyx_content() {
    let input = base_input(signal(
        "signal.prayer",
        ImmortalSignalKind::Prayer,
        ImmortalActionKind::GrantEmpowerment,
    ));
    let rules = ImmortalRules::mud_defaults();
    let records = immortal_content_records(&input.catalog, &rules).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("immortals");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rules_row = store
        .get_content_for_editor(EngineContentKind::Trigger, "immortal.rules.mud_defaults")
        .unwrap();
    let catalog_row = store
        .get_content_for_editor(
            EngineContentKind::Agent,
            "catalog.issue965.immortals.summary",
        )
        .unwrap();
    let immortal_row = store
        .get_content_for_editor(EngineContentKind::Agent, "immortal.luminara")
        .unwrap();
    let action_row = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "immortal.luminara.action.grant_empowerment",
        )
        .unwrap();
    let payload_json = serde_json::to_string(&records).unwrap();

    assert_eq!(records.len(), 8);
    assert_eq!(validation.record_count, 8);
    assert_eq!(validation.acquired_map_record_count, 0);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert_eq!(rules_row.payload["surface_kind"], "immortal_rules");
    assert_eq!(rules_row.payload["max_empowerment_tier"], 51);
    assert_eq!(catalog_row.payload["surface_kind"], "immortal_catalog");
    assert_eq!(catalog_row.payload["immortal_count"], 1);
    assert_eq!(immortal_row.payload["surface_kind"], "immortal_definition");
    assert_eq!(immortal_row.payload["lowered_agent_id"], 965);
    assert_eq!(action_row.payload["surface_kind"], "immortal_action_policy");
    assert_eq!(action_row.payload["action_kind"], "grant_empowerment");
    assert!(
        action_row
            .payload
            .get("accepted_signal_kinds")
            .and_then(|value| value.as_array())
            .unwrap()
            .iter()
            .any(|value| value == "prayer")
    );
    assert_ne!(rules_row.payload["built_in_map_required"], true);
    assert_ne!(rules_row.payload["grants_map_or_minimap"], true);
    assert_ne!(rules_row.payload["grants_gps_markers"], true);
    assert_eq!(lowered.row_count, 8);
    assert!(!lowered.calyx_live_query_allowed_in_hot_path);
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Trigger)
            .count(),
        6
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Agent)
            .count(),
        2
    );
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/immortals/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    for surface in [
        "immortal_rules",
        "immortal_catalog",
        "immortal_definition",
        "immortal_action_policy",
    ] {
        assert!(
            lowered
                .entities
                .iter()
                .any(|row| row.surface_kind == surface),
            "missing {surface}"
        );
    }
    assert!(!payload_json.contains("\"built_in_map_required\":true"));
    assert!(!payload_json.contains("\"grants_map_or_minimap\":true"));
    assert!(!payload_json.contains("\"grants_gps_markers\":true"));
}

fn eval(signal: ImmortalSignal) -> Result<crate::rpg::ImmortalDecisionReadback, EngineError> {
    evaluate_immortal_signal(&base_input(signal), &ImmortalRules::mud_defaults())
}

fn base_input(signal: ImmortalSignal) -> ImmortalEvaluationInput {
    ImmortalEvaluationInput {
        catalog: crate::rpg::ImmortalCatalog {
            schema: RPG_IMMORTAL_SCHEMA.to_string(),
            catalog_id: "catalog.issue965.immortals".to_string(),
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
        ward_tags: vec![
            "compassion".to_string(),
            "oathbound".to_string(),
            "rp_break".to_string(),
            "quest".to_string(),
            "event".to_string(),
            "possession".to_string(),
        ],
        lowered_agent_id: 965,
        max_empowerment_tier: 51,
    }
}

fn signal(id: &str, kind: ImmortalSignalKind, action: ImmortalActionKind) -> ImmortalSignal {
    let mut tags = vec!["compassion".to_string(), "oathbound".to_string()];
    match action {
        ImmortalActionKind::IssueQuest => tags.push("quest".to_string()),
        ImmortalActionKind::RunEvent => tags.push("event".to_string()),
        ImmortalActionKind::EnforceRoleplay => tags.push("rp_break".to_string()),
        ImmortalActionKind::PossessMob => tags.push("possession".to_string()),
        ImmortalActionKind::GrantEmpowerment => {}
    }
    ImmortalSignal {
        signal_id: id.to_string(),
        immortal_id: "immortal.luminara".to_string(),
        tick: 77,
        kind,
        requested_action: action,
        strength_milli: 900,
        rp_quality_milli: 850,
        source_cx_id: format!("cx.issue965.{id}"),
        target_character_id: "char.ashen.mender".to_string(),
        target_body_id: Some(42),
        possession_velocity: Some(FixedVec3::from_i32s(1, 0, 0)),
        tags,
        evidence_refs: vec![format!("evidence://issue965/{id}")],
    }
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}

// #1535: TestTempDir auto-cleans on Drop.
fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue965-{label}"))
}
