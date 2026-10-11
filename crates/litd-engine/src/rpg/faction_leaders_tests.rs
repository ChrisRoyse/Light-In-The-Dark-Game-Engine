use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        FactionLeaderActionKind, FactionLeaderCatalog, FactionLeaderEvaluationInput,
        FactionLeaderRules, FactionLeaderSignal, FactionLeaderSignalKind,
        FactionOrganizationDefinition, FactionOrganizationKind, FactionRankDefinition,
        RPG_FACTION_LEADER_SCHEMA, evaluate_faction_leader_signal, faction_leader_content_records,
        persist_and_lower_rpg_authored_content, validate_rpg_authored_content_records,
    },
    rpg_themes_test_fixture::{happy_catalog, happy_input, happy_rules},
};

#[test]
fn issue966_accepts_all_faction_leader_action_kinds() {
    let decisions = [
        eval(signal(
            "signal.application",
            FactionLeaderSignalKind::Application,
            FactionLeaderActionKind::Recruit,
        )),
        eval(signal(
            "signal.induct",
            FactionLeaderSignalKind::SponsorSupport,
            FactionLeaderActionKind::Induct,
        )),
        eval(signal(
            "signal.objective",
            FactionLeaderSignalKind::WorldThreat,
            FactionLeaderActionKind::SetObjective,
        )),
        eval(signal(
            "signal.raid",
            FactionLeaderSignalKind::ItemOfPowerThreat,
            FactionLeaderActionKind::WageWarfare,
        )),
        eval(signal(
            "signal.reward",
            FactionLeaderSignalKind::LoyaltyReport,
            FactionLeaderActionKind::RewardLoyalty,
        )),
        eval(signal(
            "signal.rank",
            FactionLeaderSignalKind::RankReview,
            FactionLeaderActionKind::AdvanceRank,
        )),
    ]
    .map(Result::unwrap);

    assert!(decisions.iter().all(|decision| decision.accepted));
    assert!(
        decisions[0].lowered_effects[0]
            .recruit_invitation_id
            .is_some()
    );
    assert_eq!(decisions[1].lowered_effects[0].rank_after, Some(1));
    assert!(decisions[2].lowered_effects[0].objective_id.is_some());
    assert!(decisions[3].lowered_effects[0].raid_order_id.is_some());
    assert!(decisions[4].lowered_effects[0].reward_id.is_some());
    assert_eq!(decisions[5].lowered_effects[0].rank_after, Some(2));
    assert_eq!(decisions[5].lowered_effects[0].unlocked_power_ids.len(), 2);
    assert_eq!(
        decisions[3].lowered_effects[0]
            .leader_order
            .lowered_agent_id,
        966
    );
}

#[test]
fn issue966_refuses_valid_candidate_outside_organization_dogma() {
    let mut input = base_input(signal(
        "signal.theme_mismatch",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    input
        .theme_input
        .declared_theme_tags
        .push("greed".to_string());

    let decision =
        evaluate_faction_leader_signal(&input, &FactionLeaderRules::mud_defaults()).unwrap();
    assert!(!decision.accepted);
    assert_eq!(decision.grade, "theme_mismatch");
    assert_eq!(decision.effect_count, 0);
    assert_eq!(
        decision.organization_theme_fit.forbidden_tags_present,
        vec!["greed".to_string()]
    );
}

#[test]
fn issue966_fails_closed_on_invalid_edges() {
    let mut duplicate = base_input(signal(
        "signal.duplicate",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    duplicate
        .catalog
        .organizations
        .push(duplicate.catalog.organizations[0].clone());
    assert_code(
        evaluate_faction_leader_signal(&duplicate, &FactionLeaderRules::mud_defaults()),
        "LITD-RPG-FACTION-INVALID-CONFIG",
    );

    let mut duplicate_rank = base_input(signal(
        "signal.duplicate_rank",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    let rank_clone = duplicate_rank.catalog.organizations[0].ranks[0].clone();
    duplicate_rank.catalog.organizations[0]
        .ranks
        .push(rank_clone);
    assert_code(
        evaluate_faction_leader_signal(&duplicate_rank, &FactionLeaderRules::mud_defaults()),
        "LITD-RPG-FACTION-INVALID-CONFIG",
    );

    let mut outside_ward = base_input(signal(
        "signal.outside",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    outside_ward.signal.tags.push("forbidden".to_string());
    assert_code(
        evaluate_faction_leader_signal(&outside_ward, &FactionLeaderRules::mud_defaults()),
        "LITD-RPG-FACTION-INVALID-CONFIG",
    );

    let mut missing_item = base_input(signal(
        "signal.missing_item",
        FactionLeaderSignalKind::ItemOfPowerThreat,
        FactionLeaderActionKind::WageWarfare,
    ));
    missing_item.signal.target_item_id = None;
    assert_code(
        evaluate_faction_leader_signal(&missing_item, &FactionLeaderRules::mud_defaults()),
        "LITD-RPG-FACTION-INVALID-CONFIG",
    );

    let mut disallowed = base_input(signal(
        "signal.disallowed",
        FactionLeaderSignalKind::WorldThreat,
        FactionLeaderActionKind::SetObjective,
    ));
    disallowed.catalog.organizations[0].allowed_actions = vec![FactionLeaderActionKind::Recruit];
    assert_code(
        evaluate_faction_leader_signal(&disallowed, &FactionLeaderRules::mud_defaults()),
        "LITD-RPG-FACTION-INVALID-CONFIG",
    );
}

#[test]
fn issue1484_loyalty_report_without_current_rank_fails_closed() {
    // Before #1484-2 a loyalty report with no current_rank resolved a fabricated
    // rank 1 and granted rank-1 powers from invented state. It must now fail loud.
    let mut unranked = signal(
        "signal.unranked_loyalty",
        FactionLeaderSignalKind::LoyaltyReport,
        FactionLeaderActionKind::RewardLoyalty,
    );
    unranked.current_rank = None;
    assert_code(eval(unranked), "LITD-RPG-FACTION-INVALID-CONFIG");

    // With a real current rank the reward resolves that exact rank (rank 2 here),
    // never rank 1 by default — proving the value comes from state, not a fallback.
    let mut ranked = signal(
        "signal.ranked_loyalty",
        FactionLeaderSignalKind::LoyaltyReport,
        FactionLeaderActionKind::RewardLoyalty,
    );
    ranked.current_rank = Some(2);
    let decision = eval(ranked).unwrap();
    assert_eq!(decision.target_rank, Some(2));
    assert_eq!(decision.lowered_effects[0].rank_after, Some(2));
    assert_eq!(decision.lowered_effects[0].unlocked_power_ids.len(), 2);
}

#[test]
fn issue966_issue1455_persists_faction_leader_policy_as_authored_calyx_content() {
    let input = base_input(signal(
        "signal.application",
        FactionLeaderSignalKind::Application,
        FactionLeaderActionKind::Recruit,
    ));
    let rules = FactionLeaderRules::mud_defaults();
    let records = faction_leader_content_records(&input.catalog, &rules).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("faction-leaders");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rules_row = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "faction_leader.rules.mud_defaults",
        )
        .unwrap();
    let catalog_row = store
        .get_content_for_editor(
            EngineContentKind::Agent,
            "catalog.issue966.faction_leaders.summary",
        )
        .unwrap();
    let organization_row = store
        .get_content_for_editor(EngineContentKind::Agent, "organization.dawnward")
        .unwrap();
    let rank_row = store
        .get_content_for_editor(EngineContentKind::Lore, "organization.dawnward.rank.2")
        .unwrap();
    let action_row = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "organization.dawnward.action.wage_warfare",
        )
        .unwrap();
    let payload_json = serde_json::to_string(&records).unwrap();

    assert_eq!(records.len(), 20);
    assert_eq!(validation.record_count, 20);
    assert_eq!(validation.acquired_map_record_count, 0);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert_eq!(rules_row.payload["surface_kind"], "faction_leader_rules");
    assert_eq!(rules_row.payload["max_rank"], 5);
    assert_eq!(
        catalog_row.payload["surface_kind"],
        "faction_leader_catalog"
    );
    assert_eq!(catalog_row.payload["organization_count"], 2);
    assert_eq!(
        organization_row.payload["surface_kind"],
        "faction_leader_organization"
    );
    assert_eq!(organization_row.payload["leader_agent_id"], 966);
    assert_eq!(rank_row.payload["surface_kind"], "faction_rank");
    assert_eq!(rank_row.payload["rank"], 2);
    assert_eq!(
        rank_row
            .payload
            .get("power_ids")
            .and_then(|value| value.as_array())
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        action_row.payload["surface_kind"],
        "faction_leader_action_policy"
    );
    assert_eq!(action_row.payload["action_kind"], "wage_warfare");
    assert!(
        action_row
            .payload
            .get("accepted_signal_kinds")
            .and_then(|value| value.as_array())
            .unwrap()
            .iter()
            .any(|value| value == "item_of_power_threat")
    );
    assert_ne!(rules_row.payload["built_in_map_required"], true);
    assert_ne!(rules_row.payload["grants_map_or_minimap"], true);
    assert_ne!(rules_row.payload["grants_gps_markers"], true);
    assert_eq!(lowered.row_count, 20);
    assert!(!lowered.calyx_live_query_allowed_in_hot_path);
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Trigger)
            .count(),
        13
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Agent)
            .count(),
        3
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Lore)
            .count(),
        4
    );
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/faction_leaders/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    for surface in [
        "faction_leader_rules",
        "faction_leader_catalog",
        "faction_leader_organization",
        "faction_rank",
        "faction_leader_action_policy",
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

fn eval(
    signal: FactionLeaderSignal,
) -> Result<crate::rpg::FactionLeaderDecisionReadback, EngineError> {
    evaluate_faction_leader_signal(&base_input(signal), &FactionLeaderRules::mud_defaults())
}

fn base_input(signal: FactionLeaderSignal) -> FactionLeaderEvaluationInput {
    FactionLeaderEvaluationInput {
        catalog: catalog(),
        theme_catalog: happy_catalog(),
        theme_rules: happy_rules(),
        theme_input: happy_input(),
        signal,
    }
}

fn catalog() -> FactionLeaderCatalog {
    FactionLeaderCatalog {
        schema: RPG_FACTION_LEADER_SCHEMA.to_string(),
        catalog_id: "catalog.issue966.faction_leaders".to_string(),
        organizations: vec![
            organization(
                "organization.dawnward",
                "Dawnward",
                966,
                Some("religion.dawn_court"),
                Some("item.dawn_standard"),
                vec!["organization.onyx"].as_slice(),
            ),
            organization(
                "organization.onyx",
                "Onyx Compact",
                1966,
                None,
                Some("item.onyx_crown"),
                vec!["organization.dawnward"].as_slice(),
            ),
        ],
    }
}

fn organization(
    id: &str,
    name: &str,
    leader_agent_id: u32,
    religion_id: Option<&str>,
    item_of_power_id: Option<&str>,
    enemies: &[&str],
) -> FactionOrganizationDefinition {
    FactionOrganizationDefinition {
        organization_id: id.to_string(),
        display_name: name.to_string(),
        organization_kind: FactionOrganizationKind::Cabal,
        leader_agent_id,
        leader_display_name: format!("{name} Voice"),
        religion_id: religion_id.map(str::to_string),
        home_region_id: format!("region.{id}.sanctum"),
        item_of_power_id: item_of_power_id.map(str::to_string),
        allowed_actions: vec![
            FactionLeaderActionKind::Recruit,
            FactionLeaderActionKind::Induct,
            FactionLeaderActionKind::SetObjective,
            FactionLeaderActionKind::WageWarfare,
            FactionLeaderActionKind::RewardLoyalty,
            FactionLeaderActionKind::AdvanceRank,
        ],
        ward_tags: ids(&[
            "compassion",
            "oathbound",
            "recruit",
            "objective",
            "warfare",
            "loyalty",
            "rank",
        ]),
        required_theme_tags: ids(&["compassion", "oathbound"]),
        forbidden_theme_tags: ids(&["greed"]),
        ranks: vec![
            rank(1, "Initiate", 700, 0, &["cabal.power.sanctuary"]),
            rank(
                2,
                "Standard Bearer",
                800,
                700,
                &["cabal.power.rally", "cabal.power.siege_blessing"],
            ),
        ],
        enemy_organization_ids: ids(enemies),
    }
}

fn rank(rank: u16, title: &str, rp: u16, loyalty: u16, powers: &[&str]) -> FactionRankDefinition {
    FactionRankDefinition {
        rank,
        title: title.to_string(),
        min_rp_quality_milli: rp,
        min_loyalty_milli: loyalty,
        power_ids: ids(powers),
    }
}

fn signal(
    id: &str,
    kind: FactionLeaderSignalKind,
    action: FactionLeaderActionKind,
) -> FactionLeaderSignal {
    let mut tags = ids(&["compassion", "oathbound"]);
    match action {
        FactionLeaderActionKind::Recruit | FactionLeaderActionKind::Induct => {
            tags.push("recruit".to_string());
        }
        FactionLeaderActionKind::SetObjective => tags.push("objective".to_string()),
        FactionLeaderActionKind::WageWarfare => tags.push("warfare".to_string()),
        FactionLeaderActionKind::RewardLoyalty => tags.push("loyalty".to_string()),
        FactionLeaderActionKind::AdvanceRank => tags.push("rank".to_string()),
    }
    FactionLeaderSignal {
        signal_id: id.to_string(),
        organization_id: "organization.dawnward".to_string(),
        tick: 77,
        kind,
        requested_action: action,
        strength_milli: 900,
        rp_quality_milli: 850,
        loyalty_milli: 760,
        source_cx_id: format!("cx.issue966.{id}"),
        target_character_id: "char.ashen.mender".to_string(),
        target_organization_id: (action == FactionLeaderActionKind::WageWarfare)
            .then(|| "organization.onyx".to_string()),
        target_item_id: (action == FactionLeaderActionKind::WageWarfare)
            .then(|| "item.onyx_crown".to_string()),
        current_rank: matches!(
            action,
            FactionLeaderActionKind::RewardLoyalty | FactionLeaderActionKind::AdvanceRank
        )
        .then_some(1),
        requested_rank: (action == FactionLeaderActionKind::AdvanceRank).then_some(2),
        tags,
        evidence_refs: vec![format!("evidence://issue966/{id}")],
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
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
    crate::test_temp::TestTempDir::new(&format!("issue966-{label}"))
}
