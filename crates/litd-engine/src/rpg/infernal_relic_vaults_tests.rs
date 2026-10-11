use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        InfernalBossAgentPolicy, InfernalRelicVaultPolicy, derive_infernal_relic_vault_readback,
        infernal_relic_item_content_records, persist_and_lower_rpg_authored_content,
        validate_infernal_relic_vault_policy, validate_rpg_authored_content_records,
    },
    rpg_infernal_attrition_test_fixture::ashen_reach_infernal_attrition_policy,
    rpg_infernal_zones_test_fixture::ashen_reach_infernal_zone_policy,
};

#[test]
fn issue1069_defaults_define_world_finite_top_tier_relics() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let relics = InfernalRelicVaultPolicy::ashen_reach_defaults();
    let readback =
        derive_infernal_relic_vault_readback(&relics, &zones, &bosses, &attrition).unwrap();

    assert_eq!(readback.vault_count, 2);
    assert_eq!(readback.item_count, 6);
    assert_eq!(readback.set_bonus_count, 3);
    assert_eq!(readback.world_finite_item_total, 6);
    assert_eq!(readback.highest_power_rating_milli, 1000);
    assert!(!readback.built_in_map_required);
    assert!(readback.world_finite_enforced);
    assert_eq!(
        readback.obsidian_vault_guard,
        "lord.infernal.obsidian_throne.maldrithax"
    );
}

#[test]
fn issue1069_issue1455_items_persist_as_authored_calyx_item_content() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let relics = InfernalRelicVaultPolicy::ashen_reach_defaults();
    let records =
        infernal_relic_item_content_records(&relics, &zones, &bosses, &attrition).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault_dir) = content_store("items");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rows = records
        .iter()
        .map(|record| {
            store
                .get_content_for_editor(record.kind, &record.id)
                .unwrap()
        })
        .collect::<Vec<_>>();

    assert_eq!(validation.record_count, 6);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert_eq!(lowered.row_count, 6);
    assert_eq!(rows.len(), 6);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(rows.iter().all(|row| row.kind == EngineContentKind::Item));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_relic_vaults/")
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
            .all(|row| { row.surface_kind == "infernal_relic_item" })
    );
    assert!(
        rows.iter()
            .all(|row| row.payload["rarity"] == "hellforged_legendary")
    );
    assert!(
        rows.iter()
            .all(|row| row.payload["grants_map_or_minimap"] == false)
    );
    assert!(
        rows.iter()
            .all(|row| row.payload["bypasses_infernal_no_recall"] == false)
    );
}

#[test]
fn issue1069_rejects_vault_and_guard_drift() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let relics = InfernalRelicVaultPolicy::ashen_reach_defaults();

    let mut missing = relics.clone();
    missing.vaults.pop();
    assert_code(
        validate_infernal_relic_vault_policy(&missing, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut bad_guard = relics.clone();
    bad_guard.vaults[0].guarded_by_agent_id = "agent.infernal.fake".to_string();
    assert_code(
        validate_infernal_relic_vault_policy(&bad_guard, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut respawn = relics.clone();
    respawn.vaults[0].respawns_world_finite_items = true;
    assert_code(
        validate_infernal_relic_vault_policy(&respawn, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1069_rejects_non_finite_low_power_or_missing_perks() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let relics = InfernalRelicVaultPolicy::ashen_reach_defaults();

    let mut count = relics.clone();
    count.item_set[0].world_finite_count = 2;
    assert_code(
        validate_infernal_relic_vault_policy(&count, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut weak = relics.clone();
    weak.item_set[0].power_rating_milli = 949;
    assert_code(
        validate_infernal_relic_vault_policy(&weak, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut perks = relics.clone();
    perks.item_set[0].perks.clear();
    assert_code(
        validate_infernal_relic_vault_policy(&perks, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1069_rejects_map_or_no_recall_bypass_drift() {
    let attrition = ashen_reach_infernal_attrition_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let relics = InfernalRelicVaultPolicy::ashen_reach_defaults();

    let mut map = relics.clone();
    map.item_set[0].grants_map_or_minimap = true;
    assert_code(
        validate_infernal_relic_vault_policy(&map, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut recall = relics.clone();
    recall.item_set[0].bypasses_infernal_no_recall = true;
    assert_code(
        validate_infernal_relic_vault_policy(&recall, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut bonus = relics.clone();
    bonus.set_bonuses[2].bypasses_infernal_no_recall = true;
    assert_code(
        validate_infernal_relic_vault_policy(&bonus, &zones, &bosses, &attrition),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
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
    crate::test_temp::TestTempDir::new(&format!("issue1069-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
