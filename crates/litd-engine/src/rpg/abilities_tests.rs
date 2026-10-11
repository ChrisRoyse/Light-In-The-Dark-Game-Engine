use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        RPG_ABILITY_SCHEMA, ability_content_records, build_ability_catalog_readback,
        persist_and_lower_rpg_authored_content, validate_rpg_authored_content_records,
    },
    rpg_abilities_test_fixture::{
        active_without_cost_or_wait, automatic_without_trigger, duplicate_ability, happy_catalog,
        passive_with_cost, resource_total, unknown_prerequisite,
    },
};

#[test]
fn issue961_universal_catalog_counts_all_ability_kinds() {
    let readback = build_ability_catalog_readback(&happy_catalog()).unwrap();

    assert_eq!(readback.schema, RPG_ABILITY_SCHEMA);
    assert_eq!(readback.ability_count, 7);
    assert_eq!(readback.kind_counts["skill"], 2);
    assert_eq!(readback.kind_counts["spell"], 1);
    assert_eq!(readback.kind_counts["supplication"], 1);
    assert_eq!(readback.kind_counts["song"], 1);
    assert_eq!(readback.kind_counts["cabal_power"], 1);
    assert_eq!(readback.kind_counts["racial_innate"], 1);
    assert_eq!(readback.activation_counts["active"], 5);
    assert_eq!(readback.activation_counts["passive"], 1);
    assert_eq!(readback.activation_counts["automatic"], 1);
    assert_eq!(readback.prerequisite_edge_count, 5);
    assert_eq!(readback.max_prerequisite_depth, 2);
    assert_eq!(readback.school_count, 5);
    assert_eq!(readback.branch_count, 5);
    assert_eq!(readback.capstone_count, 1);
    assert_eq!(readback.ui_slot_count, 5);
    assert_eq!(readback.input_sha256.len(), 64);
    assert_eq!(readback.result_sha256.len(), 64);
}

#[test]
fn issue961_cost_target_and_effect_readbacks_are_deterministic() {
    let readback = build_ability_catalog_readback(&happy_catalog()).unwrap();

    assert_eq!(resource_total(&readback.cost_resource_totals, "mana"), 240);
    assert_eq!(
        resource_total(&readback.cost_resource_totals, "movement"),
        12
    );
    assert_eq!(
        resource_total(&readback.cost_resource_totals, "devotion"),
        45
    );
    assert_eq!(resource_total(&readback.cost_resource_totals, "focus"), 20);
    assert_eq!(
        resource_total(&readback.cost_resource_totals, "cabal_charge"),
        3
    );
    assert_eq!(readback.target_mode_counts["enemy"], 2);
    assert_eq!(readback.target_mode_counts["area"], 1);
    assert_eq!(readback.target_mode_counts["ally"], 1);
    assert_eq!(readback.target_mode_counts["room"], 1);
    assert_eq!(readback.target_mode_counts["self_only"], 1);
    assert_eq!(readback.target_mode_counts["none"], 1);
    assert_eq!(readback.effect_kind_counts["damage"], 3);
    assert_eq!(readback.effect_kind_counts["apply_affect"], 3);
    assert_eq!(readback.effect_kind_counts["state_tag"], 2);
    assert_eq!(readback.effect_kind_counts["heal"], 1);
    assert_eq!(readback.effect_kind_counts["dispel"], 1);
}

#[test]
fn issue961_ability_catalog_fails_closed_on_invalid_edges() {
    assert_code(duplicate_ability(), "LITD-RPG-ABILITY-INVALID-CONFIG");
    assert_code(unknown_prerequisite(), "LITD-RPG-ABILITY-INVALID-CONFIG");
    assert_code(passive_with_cost(), "LITD-RPG-ABILITY-INVALID-CONFIG");
    assert_code(
        automatic_without_trigger(),
        "LITD-RPG-ABILITY-INVALID-CONFIG",
    );
    assert_code(
        active_without_cost_or_wait(),
        "LITD-RPG-ABILITY-INVALID-CONFIG",
    );
}

#[test]
fn issue961_ability_catalog_persists_as_authored_calyx_content() {
    let records = ability_content_records(&happy_catalog()).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("ability-catalog");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(records.len(), 8);
    assert_eq!(validation.record_count, 8);
    assert_eq!(lowered.row_count, 8);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(records.iter().all(|record| {
        record
            .source
            .path
            .starts_with("editor://rpg/ashen_reach/abilities/")
    }));
    assert!(records.iter().all(|record| {
        record.payload["built_in_map_required"] == false
            && record.payload["grants_map_or_minimap"] == false
            && record.payload["grants_gps_markers"] == false
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );

    let summary = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "catalog.ashen_reach.issue961.summary",
        )
        .unwrap();
    assert_eq!(summary.payload["surface_kind"], "ability_catalog");
    assert_eq!(summary.payload["ability_count"], 7);

    let chain = store
        .get_content_for_editor(EngineContentKind::Ability, "ability.chain_lightning")
        .unwrap();
    assert_eq!(chain.payload["surface_kind"], "ability_definition");
    assert_eq!(chain.payload["kind"], "spell");
    assert!(!chain.payload["costs"].as_array().unwrap().is_empty());
    assert_eq!(chain.payload["built_in_map_required"], false);
    assert_eq!(chain.payload["grants_map_or_minimap"], false);
    assert_eq!(chain.payload["grants_gps_markers"], false);
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
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
    crate::test_temp::TestTempDir::new(&format!("issue961-{label}"))
}
