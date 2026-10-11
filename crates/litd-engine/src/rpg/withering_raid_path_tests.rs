use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AttributeScores, InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalZonePolicy,
        MortalityCharacterState, RPG_MORTALITY_SCHEMA, UndeadForm, UndeathPolicy,
        WitheringRaidPathPolicy, apply_withering, derive_withering_raid_path_readback,
        persist_and_lower_rpg_authored_content, validate_rpg_authored_content_records,
        validate_withering_raid_path_policy, withering_raid_content_records,
        withering_request_from_raid_path,
    },
    rpg_infernal_attrition_test_fixture::ashen_reach_infernal_attrition_policy,
    rpg_infernal_zones_test_fixture::ashen_reach_infernal_zone_policy,
    rpg_mortality_test_fixture::ashen_reach_mortality_policy,
    rpg_undeath_test_fixture::ashen_reach_undeath_policy,
};

#[test]
fn issue1064_defaults_unlock_withering_through_elite_secret_raid() {
    let mortality = ashen_reach_mortality_policy();
    let undeath = ashen_reach_undeath_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let path = WitheringRaidPathPolicy::ashen_reach_defaults();
    let readback =
        derive_withering_raid_path_readback(&path, &undeath, &zones, &bosses, &attrition).unwrap();
    let request = withering_request_from_raid_path(&path, &undeath, UndeadForm::Lich).unwrap();
    let result = apply_withering(&mortal_state(), &mortality, &undeath, &request).unwrap();

    assert_eq!(readback.phase_count, 6);
    assert_eq!(readback.minimum_party_size, 8);
    assert_eq!(readback.pressure_tier_milli, 1000);
    assert!(readback.elite_group_required);
    assert!(readback.source_zone_has_reagents);
    assert!(readback.sample_withering_request_valid);
    assert!(!readback.built_in_map_required);
    assert_eq!(
        readback.final_guard_agent_id,
        "agent.infernal.wailing_tombs.choirmaster"
    );
    assert_eq!(result.constitution_after, 6);
    assert_eq!(result.discovery_ref_count, 3);
}

#[test]
fn issue1064_issue1455_persists_raid_path_as_authored_calyx_content() {
    let undeath = ashen_reach_undeath_policy();
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let path = WitheringRaidPathPolicy::ashen_reach_defaults();
    let records =
        withering_raid_content_records(&path, &undeath, &zones, &bosses, &attrition).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("path");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 9);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert_eq!(lowered.row_count, 9);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/withering_raid_path/")
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
            .any(|row| row.kind == EngineContentKind::Story)
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.id == "item.hollow_pact.rift_shard")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "withering_questline")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "withering_raid")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| { row.surface_kind == "withering_item_of_power" })
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| { row.surface_kind == "withering_raid_phase" })
    );
    assert!(
        records
            .iter()
            .all(|row| row.payload["built_in_map_required"] != true)
    );
    assert!(
        records
            .iter()
            .all(|row| row.payload["grants_map_or_minimap"] != true)
    );
    assert!(
        records
            .iter()
            .all(|row| row.payload["grants_gps_markers"] != true)
    );
}

#[test]
fn issue1064_rejects_map_ui_shortcuts_and_bypasses() {
    let (undeath, attrition, zones, bosses, path) = deps();

    let mut map = path.clone();
    map.built_in_map_required = true;
    assert_code(
        validate_withering_raid_path_policy(&map, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut ui = path.clone();
    ui.questline.revealed_by_default_ui = true;
    assert_code(
        validate_withering_raid_path_policy(&ui, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut phase = path.clone();
    phase.raid.phases[0].grants_gps_markers = true;
    assert_code(
        validate_withering_raid_path_policy(&phase, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut bypass = path.clone();
    bypass.raid.phases[5].bypasses_withering_requirements = true;
    assert_code(
        validate_withering_raid_path_policy(&bypass, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1064_rejects_weak_raid_and_secret_drift() {
    let (undeath, attrition, zones, bosses, path) = deps();

    let mut solo = path.clone();
    solo.raid.minimum_party_size = 1;
    assert_code(
        validate_withering_raid_path_policy(&solo, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut pressure = path.clone();
    pressure.raid.pressure_tier_milli = 990;
    assert_code(
        validate_withering_raid_path_policy(&pressure, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut discovery = path.clone();
    discovery.questline.required_discovery_refs.clear();
    assert_code(
        validate_withering_raid_path_policy(&discovery, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut component = path.clone();
    component.questline.required_components.pop();
    assert_code(
        validate_withering_raid_path_policy(&component, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1064_rejects_wrong_zone_guard_or_phase_order() {
    let (undeath, attrition, zones, bosses, path) = deps();

    let mut zone = path.clone();
    zone.raid.source_zone_id = "zone.infernal.threshold.ashgate".to_string();
    assert_code(
        validate_withering_raid_path_policy(&zone, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut guard = path.clone();
    guard.raid.final_guard_agent_id = "agent.infernal.fake".to_string();
    assert_code(
        validate_withering_raid_path_policy(&guard, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut phase = path.clone();
    phase.raid.phases.swap(0, 1);
    assert_code(
        validate_withering_raid_path_policy(&phase, &undeath, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn deps() -> (
    UndeathPolicy,
    InfernalAttritionPolicy,
    InfernalZonePolicy,
    InfernalBossAgentPolicy,
    WitheringRaidPathPolicy,
) {
    (
        ashen_reach_undeath_policy(),
        ashen_reach_infernal_attrition_policy(),
        ashen_reach_infernal_zone_policy(),
        InfernalBossAgentPolicy::ashen_reach_defaults(),
        WitheringRaidPathPolicy::ashen_reach_defaults(),
    )
}

fn mortal_state() -> MortalityCharacterState {
    MortalityCharacterState {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        character_id: "char.varn_black".to_string(),
        character_name: "Varn Black".to_string(),
        race_id: "race.ashen_human".to_string(),
        level: 47,
        age_years: 64,
        base_attributes: AttributeScores::new(9, 18, 15, 10, 16, 8),
        current_constitution: 16,
        deaths_since_constitution_loss: 0,
        total_deaths: 0,
        permanently_dead: false,
        mourning_wall_entry_id: None,
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
    crate::test_temp::TestTempDir::new(&format!("issue1064-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
