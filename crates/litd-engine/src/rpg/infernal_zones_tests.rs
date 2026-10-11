use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        InfernalCircleId, InfernalHazardKind, derive_infernal_zone_topology,
        infernal_zone_content_records, infernal_zone_policy_hash,
        persist_and_lower_rpg_authored_content, validate_infernal_zone_policy,
        validate_rpg_authored_content_records, zone_for_circle,
    },
    rpg_infernal_attrition_test_fixture::ashen_reach_infernal_attrition_policy,
    rpg_infernal_zones_test_fixture::ashen_reach_infernal_zone_policy,
};

#[test]
fn issue1067_fixture_defines_seven_descending_circles_without_builtin_maps() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let policy = ashen_reach_infernal_zone_policy();
    let topology = derive_infernal_zone_topology(&policy, &attrition).unwrap();

    assert_eq!(topology.zone_count, 7);
    assert!(!topology.built_in_map_required);
    assert_eq!(topology.entry_circle, InfernalCircleId::Threshold);
    assert_eq!(topology.terminal_circle, InfernalCircleId::ObsidianThrone);
    assert_eq!(
        topology.chain.first().unwrap().circle_id,
        InfernalCircleId::Threshold
    );
    assert_eq!(
        topology.chain.last().unwrap().circle_id,
        InfernalCircleId::ObsidianThrone
    );
    assert_eq!(topology.chain.last().unwrap().descends_to, None);
    assert!(topology.wailing_tombs_reagent_source);
    assert!(topology.throne_road_relic_vault_source);
    assert_eq!(topology.chain[4].hazard_kind, InfernalHazardKind::SoulDrain);
    assert_eq!(
        topology.policy_sha256,
        infernal_zone_policy_hash(&policy).unwrap()
    );
}

#[test]
fn issue1067_each_zone_binds_to_attrition_hazards_mobs_rulers_and_resets() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let policy = ashen_reach_infernal_zone_policy();
    let topology = derive_infernal_zone_topology(&policy, &attrition).unwrap();

    for (index, step) in topology.chain.iter().enumerate() {
        assert_eq!(step.depth, u8::try_from(index + 1).unwrap());
        assert!(step.attrition_health_damage_per_tick > 0);
        assert!(step.attrition_constitution_strain_per_tick > 0);
        assert!(step.mob_population_count >= 3);
        assert!(step.ruler_ref.starts_with("lord.infernal."));
        assert!(step.reset_id.starts_with("reset.infernal."));
        assert!(step.repop_id.starts_with("repop.infernal."));
    }
    assert_eq!(
        topology.chain[3].hazard_kind,
        InfernalHazardKind::DarkPressure
    );
    assert_eq!(topology.chain[6].hazard_kind, InfernalHazardKind::SoulDrain);
}

#[test]
fn issue1067_zones_persist_as_authored_calyx_terrain_content() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let policy = ashen_reach_infernal_zone_policy();
    let records = infernal_zone_content_records(&policy, &attrition).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault_dir) = content_store("zones");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rows = records
        .iter()
        .map(|record| {
            store
                .get_content_for_editor(record.kind, &record.id)
                .unwrap()
        })
        .collect::<Vec<_>>();

    assert_eq!(validation.record_count, 8);
    assert_eq!(lowered.row_count, 8);
    assert_eq!(rows.len(), 8);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(
        rows.iter()
            .all(|row| row.kind == EngineContentKind::Terrain)
    );
    assert!(rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_zones/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|entity| entity.lowered_field_count > 0)
    );
    assert!(
        rows.iter()
            .filter(|row| row.payload["surface_kind"] == "infernal_zone_node")
            .count()
            == 7
    );
    assert!(
        rows.iter()
            .any(|row| row.payload["surface_kind"] == "infernal_zone_topology_summary")
    );
    assert!(rows.iter().all(|row| {
        row.payload["built_in_map_required"] != true
            && row.payload["grants_map_or_minimap"] != true
            && row.payload["grants_gps_markers"] != true
    }));
}

#[test]
fn issue1067_zone_policy_fails_closed_on_invalid_edges() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let policy = ashen_reach_infernal_zone_policy();

    let mut map_required = policy.clone();
    map_required.built_in_map_required = true;
    assert_code(
        validate_infernal_zone_policy(&map_required, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut missing = policy.clone();
    missing.zones.pop();
    assert_code(
        validate_infernal_zone_policy(&missing, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut duplicate_depth = policy.clone();
    duplicate_depth.zones[1].depth = 1;
    assert_code(
        validate_infernal_zone_policy(&duplicate_depth, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut duplicate_circle = policy.clone();
    duplicate_circle.zones[1].circle_id = InfernalCircleId::Threshold;
    assert_code(
        validate_infernal_zone_policy(&duplicate_circle, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut broken_exit = policy.clone();
    broken_exit.zones[2].exits[0].to_circle_id = InfernalCircleId::WailingTombs;
    assert_code(
        validate_infernal_zone_policy(&broken_exit, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1067_zone_policy_rejects_content_drift_without_fallbacks() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let policy = ashen_reach_infernal_zone_policy();

    let mut empty_mobs = policy.clone();
    empty_mobs.zones[0].mob_population_ids.clear();
    assert_code(
        validate_infernal_zone_policy(&empty_mobs, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut wrong_hazard = policy.clone();
    wrong_hazard.zones[0].hazard_kind = InfernalHazardKind::SoulDrain;
    assert_code(
        validate_infernal_zone_policy(&wrong_hazard, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut no_reset = policy.clone();
    no_reset.zones[0].reset_profile.interval_minutes = 0;
    assert_code(
        validate_infernal_zone_policy(&no_reset, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut no_repop = policy.clone();
    no_repop.zones[0].repop_profile.baseline_mob_budget = 0;
    assert_code(
        validate_infernal_zone_policy(&no_repop, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut wrong_reagents = policy.clone();
    wrong_reagents.zones[0].withering_reagent_source = true;
    assert_code(
        validate_infernal_zone_policy(&wrong_reagents, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1067_zone_lookup_fails_closed_for_missing_circle() {
    let mut policy = ashen_reach_infernal_zone_policy();
    policy
        .zones
        .retain(|zone| zone.circle_id != InfernalCircleId::WailingTombs);
    assert_code(
        zone_for_circle(&policy, InfernalCircleId::WailingTombs),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
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
    crate::test_temp::TestTempDir::new(&format!("issue1067-{label}"))
}
