use crate::{
    EngineError,
    calyx_agent_runtime::{
        EngineWorldSignal, EngineWorldSignalKind, activate_calyx_agents, read_calyx_agent_runtime,
        store_calyx_agent_runtime,
    },
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        InfernalBossAgentPolicy, InfernalCircleId, derive_infernal_boss_agent_hierarchy,
        infernal_boss_agent_content_records, persist_and_lower_rpg_authored_content,
        validate_infernal_boss_agent_policy, validate_rpg_authored_content_records,
    },
    rpg_infernal_attrition_test_fixture::ashen_reach_infernal_attrition_policy,
    rpg_infernal_zones_test_fixture::ashen_reach_infernal_zone_policy,
};

#[test]
fn issue1068_defaults_define_seven_calyx_boss_agents() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let readback = derive_infernal_boss_agent_hierarchy(&bosses, &zones, &attrition).unwrap();
    let records = infernal_boss_agent_content_records(&bosses, &zones, &attrition).unwrap();

    assert_eq!(readback.boss_count, 7);
    assert_eq!(records.len(), 7);
    assert!(readback.calyx_agent_runtime_required);
    assert_eq!(readback.hot_path_policy, "lowered_orders_only");
    assert!(!readback.calyx_live_query_allowed_in_hot_path);
    assert!(!readback.built_in_map_required);
    assert_eq!(
        readback.apex_agent_id,
        "agent.infernal.obsidian_throne.maldrithax"
    );
    assert_eq!(readback.chain[2].display_name, "Iron Praetor");
    assert_eq!(readback.chain[3].display_name, "The Maw");
    assert_eq!(readback.chain[4].display_name, "The Choirmaster");
    assert_eq!(readback.chain[6].lethality_tier, 10);
}

#[test]
fn issue1068_issue1455_generated_records_activate_through_calyx_runtime() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let records = infernal_boss_agent_content_records(&bosses, &zones, &attrition).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, database, _vault_dir) = content_store("runtime");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let content = records
        .iter()
        .map(|record| {
            store
                .get_content_for_editor(record.kind, &record.id)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let signals = bosses
        .boss_agents
        .iter()
        .enumerate()
        .map(|(index, boss)| {
            signal(
                &format!("signal.infernal.{}", boss.circle_id.label()),
                u64::try_from(index + 1).unwrap() * 10,
                boss.tactic_profile.trigger_terms[0].as_str(),
                boss.display_name.as_str(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(validation.record_count, 7);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert_eq!(lowered.row_count, 7);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_boss_agents/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| { row.surface_kind == "infernal_boss_agent" })
    );
    assert!(
        content
            .iter()
            .all(|row| row.payload["built_in_map_required"] == false)
    );
    assert!(
        content
            .iter()
            .all(|row| row.payload["grants_map_or_minimap"] == false)
    );
    assert!(
        content
            .iter()
            .all(|row| row.payload["grants_gps_markers"] == false)
    );

    let runtime = activate_calyx_agents("issue1068-runtime", &content, &signals).unwrap();
    let stored = store_calyx_agent_runtime(&database, &runtime).unwrap();
    let readback = read_calyx_agent_runtime(&database, "issue1068-runtime").unwrap();

    assert_eq!(runtime.agent_count, 7);
    assert_eq!(runtime.signal_count, 7);
    assert_eq!(runtime.activation_count, 7);
    assert_eq!(runtime.order_count, 7);
    assert!(!runtime.calyx_live_query_allowed_in_hot_path);
    assert!(
        runtime
            .activations
            .iter()
            .any(|activation| activation.prompt.contains("Maldrithax"))
    );
    assert_eq!(
        stored.runtime.runtime_fingerprint,
        runtime.runtime_fingerprint
    );
    assert_eq!(readback.runtime.activation_count, 7);
}

#[test]
fn issue1068_policy_fails_closed_on_hierarchy_and_zone_drift() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();

    let mut missing = bosses.clone();
    missing.boss_agents.pop();
    assert_code(
        validate_infernal_boss_agent_policy(&missing, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut duplicate = bosses.clone();
    duplicate.boss_agents[1].circle_id = InfernalCircleId::Threshold;
    assert_code(
        validate_infernal_boss_agent_policy(&duplicate, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut zone_drift = zones.clone();
    zone_drift.zones[2].ruler_ref = "lord.infernal.iron_bastion.false_praetor".to_string();
    assert_code(
        validate_infernal_boss_agent_policy(&bosses, &zone_drift, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1068_policy_rejects_non_calyx_or_weak_bosses() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();

    let mut disabled = bosses.clone();
    disabled.calyx_agent_runtime_required = false;
    assert_code(
        validate_infernal_boss_agent_policy(&disabled, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut live_query = bosses.clone();
    live_query.calyx_live_query_allowed_in_hot_path = true;
    assert_code(
        validate_infernal_boss_agent_policy(&live_query, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut weak = bosses.clone();
    weak.boss_agents[0].lethality_tier = 6;
    assert_code(
        validate_infernal_boss_agent_policy(&weak, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1068_policy_rejects_tactic_and_dialogue_drift() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();

    let mut no_terms = bosses.clone();
    no_terms.boss_agents[0].tactic_profile.trigger_terms.clear();
    assert_code(
        validate_infernal_boss_agent_policy(&no_terms, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut no_dialogue = bosses.clone();
    no_dialogue.boss_agents[0]
        .dialogue_profile
        .phase_lines
        .clear();
    assert_code(
        validate_infernal_boss_agent_policy(&no_dialogue, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut no_velocity = bosses.clone();
    no_velocity.boss_agents[0].tactic_profile.order_velocity = [0, 0, 0];
    assert_code(
        validate_infernal_boss_agent_policy(&no_velocity, &zones, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
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
    crate::test_temp::TestTempDir::new(&format!("issue1068-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
