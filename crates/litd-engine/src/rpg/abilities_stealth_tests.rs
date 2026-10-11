use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AbilityCost, AbilityPrerequisite, RPG_ABILITY_SCHEMA, ResourceCostTotal,
        ability_content_records, build_ability_catalog_readback,
        persist_and_lower_rpg_authored_content, stealth_ability_catalog,
        validate_rpg_authored_content_records,
    },
};

#[test]
fn issue1604_stealth_catalog_counts_every_ability() {
    let readback = build_ability_catalog_readback(&stealth_ability_catalog()).unwrap();

    assert_eq!(readback.schema, RPG_ABILITY_SCHEMA);
    assert_eq!(readback.catalog_id, "catalog.ashen_reach.stealth");
    assert_eq!(readback.ability_count, 33);
    assert_eq!(readback.kind_counts["skill"], 22);
    assert_eq!(readback.kind_counts["spell"], 7);
    assert_eq!(readback.kind_counts["supplication"], 3);
    assert_eq!(readback.kind_counts["racial_innate"], 1);
    assert_eq!(readback.kind_counts["song"], 0);
    assert_eq!(readback.kind_counts["cabal_power"], 0);
    assert_eq!(readback.activation_counts["active"], 26);
    assert_eq!(readback.activation_counts["passive"], 6);
    assert_eq!(readback.activation_counts["automatic"], 1);
    assert_eq!(readback.prerequisite_edge_count, 9);
    assert_eq!(readback.max_prerequisite_depth, 2);
    assert_eq!(readback.school_count, 1);
    assert_eq!(readback.branch_count, 1);
    assert_eq!(readback.capstone_count, 0);
    assert_eq!(readback.ui_slot_count, 4);
    assert_eq!(readback.trigger_tag_count, 1);
    assert_eq!(readback.ability_ids.len(), 33);
    assert_eq!(readback.input_sha256.len(), 64);
    assert_eq!(readback.result_sha256.len(), 64);
}

#[test]
fn issue1604_stealth_cost_target_and_effect_readbacks_are_deterministic() {
    let readback = build_ability_catalog_readback(&stealth_ability_catalog()).unwrap();

    assert_eq!(
        resource_total(&readback.cost_resource_totals, "movement"),
        18
    );
    assert_eq!(resource_total(&readback.cost_resource_totals, "mana"), 580);
    assert_eq!(
        resource_total(&readback.cost_resource_totals, "devotion"),
        140
    );

    assert_eq!(readback.target_mode_counts["self_only"], 13);
    assert_eq!(readback.target_mode_counts["none"], 6);
    assert_eq!(readback.target_mode_counts["direction"], 2);
    assert_eq!(readback.target_mode_counts["enemy"], 7);
    assert_eq!(readback.target_mode_counts["ally"], 3);
    assert_eq!(readback.target_mode_counts["room"], 1);
    assert_eq!(readback.target_mode_counts["area"], 1);

    assert_eq!(readback.effect_kind_counts["apply_affect"], 22);
    assert_eq!(readback.effect_kind_counts["state_tag"], 8);
    assert_eq!(readback.effect_kind_counts["damage"], 3);
    assert_eq!(readback.effect_kind_counts["movement"], 2);
    assert_eq!(readback.effect_kind_counts["dispel"], 1);
}

#[test]
fn issue1604_stealth_catalog_fails_closed_on_invalid_edges() {
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
fn issue1604_stealth_catalog_persists_as_authored_calyx_content() {
    let catalog = stealth_ability_catalog();
    let records = ability_content_records(&catalog).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("stealth-catalog");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    // 33 abilities + 1 catalog summary row.
    assert_eq!(records.len(), 34);
    assert_eq!(validation.record_count, 34);
    assert_eq!(lowered.row_count, 34);
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

    let summary = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "catalog.ashen_reach.stealth.summary",
        )
        .unwrap();
    assert_eq!(summary.payload["surface_kind"], "ability_catalog");
    assert_eq!(summary.payload["ability_count"], 33);

    let invis = store
        .get_content_for_editor(EngineContentKind::Ability, "ability.invisibility")
        .unwrap();
    assert_eq!(invis.payload["surface_kind"], "ability_definition");
    assert_eq!(invis.payload["kind"], "spell");
    assert!(!invis.payload["costs"].as_array().unwrap().is_empty());
    assert_eq!(invis.payload["built_in_map_required"], false);

    let hide = store
        .get_content_for_editor(EngineContentKind::Ability, "ability.hide")
        .unwrap();
    assert_eq!(hide.payload["kind"], "skill");
    assert_eq!(hide.payload["activation"], "active");
    // Hide-level drift fixed at the root: catalog now agrees with CF/#1623 at L1.
    assert_eq!(hide.payload["requirements"]["min_level"], 1);

    // #1732: every one of the five previously-missing abilities reads back from
    // Calyx exactly as authored, with its CF-factual gate level.
    let readback_new = |id: &str| {
        store
            .get_content_for_editor(EngineContentKind::Ability, id)
            .unwrap()
            .payload
    };
    let gentle = readback_new("ability.gentle_walk");
    assert_eq!(gentle["kind"], "skill");
    assert_eq!(gentle["requirements"]["min_level"], 13);
    assert_eq!(gentle["requirements"]["class_ids"][0], "class.thief");

    let heightened = readback_new("ability.heightened_awareness");
    assert_eq!(heightened["kind"], "skill");
    assert_eq!(heightened["activation"], "passive");
    assert_eq!(heightened["requirements"]["min_level"], 7);

    let dark = readback_new("ability.dark_vision");
    assert_eq!(dark["activation"], "passive");
    assert_eq!(dark["requirements"]["min_level"], 5);
    assert_eq!(dark["requirements"]["class_ids"][0], "class.assassin");
    assert_eq!(dark["requirements"]["class_ids"][1], "class.ranger");

    let mass = readback_new("ability.mass_invis");
    assert_eq!(mass["kind"], "spell");
    assert_eq!(mass["requirements"]["min_level"], 11);
    assert_eq!(mass["targeting"]["mode"], "area");

    let vanish = readback_new("ability.vanish");
    assert_eq!(vanish["kind"], "skill");
    assert_eq!(vanish["requirements"]["min_level"], 16);
    assert_eq!(vanish["requirements"]["class_ids"][0], "class.assassin");
}

// --- Negative-case constructors (mutate the authored catalog) ---------------

fn duplicate_ability() -> Result<(), EngineError> {
    let mut catalog = stealth_ability_catalog();
    catalog.abilities.push(catalog.abilities[0].clone());
    build_ability_catalog_readback(&catalog).map(|_| ())
}

fn unknown_prerequisite() -> Result<(), EngineError> {
    let mut catalog = stealth_ability_catalog();
    // improved_hide (index 3) requires hide; break the reference.
    catalog.abilities[3].requirements.prerequisites = vec![AbilityPrerequisite {
        ability_id: "ability.missing".to_string(),
        min_rank: 1,
    }];
    build_ability_catalog_readback(&catalog).map(|_| ())
}

fn passive_with_cost() -> Result<(), EngineError> {
    let mut catalog = stealth_ability_catalog();
    // improved_hide is passive; passives may not carry costs.
    catalog.abilities[3].costs = vec![AbilityCost {
        resource_id: "mana".to_string(),
        amount: 1,
    }];
    build_ability_catalog_readback(&catalog).map(|_| ())
}

fn automatic_without_trigger() -> Result<(), EngineError> {
    let mut catalog = stealth_ability_catalog();
    // quiet_movement (index 1) is automatic and requires trigger tags.
    catalog.abilities[1].trigger_tags.clear();
    build_ability_catalog_readback(&catalog).map(|_| ())
}

fn active_without_cost_or_wait() -> Result<(), EngineError> {
    let mut catalog = stealth_ability_catalog();
    // sneak (index 0) is active on a movement cost alone; strip every gate.
    catalog.abilities[0].costs.clear();
    catalog.abilities[0].timing.lag_ticks = 0;
    catalog.abilities[0].timing.cooldown_ticks = 0;
    catalog.abilities[0].timing.cooldown_group = None;
    build_ability_catalog_readback(&catalog).map(|_| ())
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

fn resource_total(totals: &[ResourceCostTotal], resource_id: &str) -> i64 {
    totals
        .iter()
        .find(|total| total.resource_id == resource_id)
        .unwrap()
        .amount
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

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1604-{label}"))
}
