use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        DeriveInfernalDeepsPackageArgs, InfernalAccessPolicy, InfernalAttritionPolicy,
        InfernalBossAgentPolicy, InfernalDeepsPackagePolicy, InfernalRelicVaultPolicy,
        InfernalZonePolicy, UndeathPolicy, WitheringRaidPathPolicy, derive_infernal_deeps_package,
        infernal_deeps_content_records, persist_and_lower_rpg_authored_content,
        validate_infernal_deeps_package_policy, validate_rpg_authored_content_records,
    },
    rpg_infernal_access_test_fixture::ashen_reach_infernal_access_policy,
    rpg_infernal_attrition_test_fixture::ashen_reach_infernal_attrition_policy,
    rpg_infernal_zones_test_fixture::ashen_reach_infernal_zone_policy,
    rpg_undeath_test_fixture::ashen_reach_undeath_policy,
};

#[test]
fn issue1063_aggregate_covers_infernal_deeps_parent_contract() {
    let (package, access, attrition, zones, bosses, relics, withering, undeath) = policies();
    let readback = derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
        package: &package,
        access: &access,
        attrition: &attrition,
        zones: &zones,
        bosses: &bosses,
        relics: &relics,
        withering: &withering,
        undeath: &undeath,
    })
    .unwrap();

    assert_eq!(readback.source_issue, 1063);
    assert!(readback.ordered_hell_not_outer_dark);
    assert!(readback.group_required);
    assert!(readback.no_recall_inside);
    assert_eq!(readback.access_path_count, 5);
    assert_eq!(readback.denied_escape_kind_count, 5);
    assert_eq!(readback.zone_count, 7);
    assert_eq!(readback.hazard_kind_count, 3);
    assert_eq!(readback.boss_count, 7);
    assert_eq!(
        readback.apex_agent_id,
        "agent.infernal.obsidian_throne.maldrithax"
    );
    assert_eq!(readback.relic_vault_count, 2);
    assert_eq!(readback.hellforged_item_count, 6);
    assert_eq!(readback.world_finite_item_total, 6);
    assert_eq!(readback.highest_power_rating_milli, 1000);
    assert_eq!(readback.withering_raid_phase_count, 6);
    assert_eq!(readback.withering_minimum_party_size, 8);
    assert!(readback.wailing_tombs_reagent_source);
    assert!(readback.throne_road_relic_vault_source);
    assert!(!readback.built_in_map_required);
    assert!(!readback.grants_map_or_minimap);
    assert!(!readback.grants_gps_markers);
}

#[test]
fn issue1063_persists_parent_package_as_calyx_content() {
    let (package, access, attrition, zones, bosses, relics, withering, undeath) = policies();
    let readback = derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
        package: &package,
        access: &access,
        attrition: &attrition,
        zones: &zones,
        bosses: &bosses,
        relics: &relics,
        withering: &withering,
        undeath: &undeath,
    })
    .unwrap();
    let records = infernal_deeps_content_records(&package, &readback).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("infernal-deeps");
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
    assert_eq!(lowered.row_count, 6);
    assert_eq!(rows.len(), 6);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_deeps/")
    }));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_deeps/")
            && row.lowered_field_count > 0
    }));
    assert!(rows.iter().all(|row| row.payload.get("readback").is_none()));
    let parent = rows
        .iter()
        .find(|row| row.id == "server.ashen_reach.infernal_deeps.package")
        .unwrap();
    assert_eq!(parent.kind, EngineContentKind::Lore);
    assert_eq!(
        parent.payload["surface_kind"],
        "infernal_deeps_parent_summary"
    );
    assert_eq!(parent.payload["child_hashes"].as_object().unwrap().len(), 6);
    let zones = rows
        .iter()
        .find(|row| row.id == "server.ashen_reach.infernal_deeps.zone_topology")
        .unwrap();
    assert_eq!(zones.payload["zone_count"], 7);
    let relics = rows
        .iter()
        .find(|row| row.id == "server.ashen_reach.infernal_deeps.relic_vaults")
        .unwrap();
    assert_eq!(relics.payload["hellforged_item_count"], 6);
    assert!(rows.iter().all(|row| {
        row.payload["built_in_map_required"] != true
            && row.payload["grants_map_or_minimap"] != true
            && row.payload["grants_gps_markers"] != true
    }));
}

#[test]
fn issue1063_rejects_parent_and_child_contract_drift() {
    let mut package = InfernalDeepsPackagePolicy::ashen_reach_defaults();
    package.built_in_map_required = true;
    assert_code(
        validate_infernal_deeps_package_policy(&package),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let (package, mut access, attrition, zones, bosses, relics, withering, undeath) = policies();
    access.no_recall_inside = false;
    assert_code(
        derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
            package: &package,
            access: &access,
            attrition: &attrition,
            zones: &zones,
            bosses: &bosses,
            relics: &relics,
            withering: &withering,
            undeath: &undeath,
        }),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let (package, access, attrition, mut zones, bosses, relics, withering, undeath) = policies();
    zones.built_in_map_required = true;
    assert_code(
        derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
            package: &package,
            access: &access,
            attrition: &attrition,
            zones: &zones,
            bosses: &bosses,
            relics: &relics,
            withering: &withering,
            undeath: &undeath,
        }),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let (package, access, attrition, zones, bosses, mut relics, withering, undeath) = policies();
    relics.item_set[0].grants_map_or_minimap = true;
    assert_code(
        derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
            package: &package,
            access: &access,
            attrition: &attrition,
            zones: &zones,
            bosses: &bosses,
            relics: &relics,
            withering: &withering,
            undeath: &undeath,
        }),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let (package, access, attrition, zones, bosses, relics, mut withering, undeath) = policies();
    withering.questline.revealed_by_default_ui = true;
    assert_code(
        derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
            package: &package,
            access: &access,
            attrition: &attrition,
            zones: &zones,
            bosses: &bosses,
            relics: &relics,
            withering: &withering,
            undeath: &undeath,
        }),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn policies() -> (
    InfernalDeepsPackagePolicy,
    InfernalAccessPolicy,
    InfernalAttritionPolicy,
    InfernalZonePolicy,
    InfernalBossAgentPolicy,
    InfernalRelicVaultPolicy,
    WitheringRaidPathPolicy,
    UndeathPolicy,
) {
    (
        InfernalDeepsPackagePolicy::ashen_reach_defaults(),
        ashen_reach_infernal_access_policy(),
        ashen_reach_infernal_attrition_policy(),
        ashen_reach_infernal_zone_policy(),
        InfernalBossAgentPolicy::ashen_reach_defaults(),
        InfernalRelicVaultPolicy::ashen_reach_defaults(),
        WitheringRaidPathPolicy::ashen_reach_defaults(),
        ashen_reach_undeath_policy(),
    )
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
    crate::test_temp::TestTempDir::new(&format!("issue1063-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
