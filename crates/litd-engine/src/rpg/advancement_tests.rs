use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        RPG_ADVANCEMENT_SCHEMA, advancement_content_records, apply_advancement_plan,
        apply_class_spec_advancement_plan, persist_advancement_readback,
        persist_and_lower_rpg_authored_content, read_persisted_advancement_readback,
        validate_rpg_authored_content_records,
    },
    rpg_advancement_test_fixture::{
        duplicate_learning_state, happy_input, happy_rules, insufficient_practices,
        resource_ability, study_insufficient_points, train_attribute_cap_exceeded,
        unknown_ability_use,
    },
    rpg_class_specs_fixture_raider::{RAIDER_CLASS_ID, raider_class_spec},
    rpg_class_specs_fixture_thief::thief_class_spec,
};

#[test]
fn issue962_advancement_awards_and_applies_known_actions() {
    let readback = apply_advancement_plan(&happy_input(), &happy_rules()).unwrap();

    assert_eq!(readback.schema, RPG_ADVANCEMENT_SCHEMA);
    assert_eq!(readback.awards.levels_gained, 2);
    assert_eq!(readback.awards.practice_per_level, 4);
    assert_eq!(readback.awards.practice_awarded, 8);
    assert_eq!(readback.awards.trains_awarded, 2);
    assert_eq!(readback.awards.study_points_awarded, 4);
    assert_eq!(readback.level_proficiency_cap, 66);
    assert_eq!(readback.final_state.pools.practices, 7);
    assert_eq!(readback.final_state.pools.trains, 0);
    assert_eq!(readback.final_state.pools.study_points, 1);
    assert_eq!(readback.final_state.attributes.dexterity, 18);
    assert_eq!(readback.final_state.vitals.mana, 112);
    assert_eq!(
        resource_ability(&readback.final_state.abilities, "ability.cinder_bash").proficiency,
        66
    );
    assert_eq!(readback.actions.len(), 5);
    assert_eq!(readback.input_sha256.len(), 64);
    assert_eq!(readback.result_sha256.len(), 64);
}

#[test]
fn issue962_use_improvement_reports_focus_bonus_and_level_cap() {
    let readback = apply_advancement_plan(&happy_input(), &happy_rules()).unwrap();

    let practice = &readback.actions[1];
    assert_eq!(practice.action_kind, "practice");
    assert_eq!(practice.before_value, 0);
    assert_eq!(practice.after_value, 50);
    assert!(!practice.cap_applied);

    let use_action = &readback.actions[2];
    assert_eq!(use_action.action_kind, "use_ability");
    assert_eq!(readback.focused_skill_learn_bonus, 2);
    assert_eq!(use_action.before_value, 50);
    assert_eq!(use_action.after_value, 66);
    assert_eq!(use_action.delta, 16);
    assert!(use_action.cap_applied);
}

#[test]
fn issue962_advancement_fails_closed_on_invalid_edges() {
    assert_code(
        duplicate_learning_state(),
        "LITD-RPG-ADVANCEMENT-INVALID-CONFIG",
    );
    assert_code(
        insufficient_practices(),
        "LITD-RPG-ADVANCEMENT-INVALID-CONFIG",
    );
    assert_code(
        train_attribute_cap_exceeded(),
        "LITD-RPG-ADVANCEMENT-INVALID-CONFIG",
    );
    assert_code(unknown_ability_use(), "LITD-RPG-ADVANCEMENT-INVALID-CONFIG");
    assert_code(
        study_insufficient_points(),
        "LITD-RPG-ADVANCEMENT-INVALID-CONFIG",
    );
}

#[test]
fn issue1750_class_spec_boundary_rejects_cross_class_ability() {
    let error =
        apply_class_spec_advancement_plan(&happy_input(), &happy_rules(), &thief_class_spec())
            .expect_err("an ability absent from the authored ClassSpec must fail closed");
    assert_eq!(error.code(), "LITD-RPG-ADVANCEMENT-INVALID-CONFIG");
    assert!(error.to_string().contains("ability.cinder_bash"));
}

#[test]
fn issue1753_class_spec_max_level_stops_raider_and_allows_exact_cap() {
    let class_spec = raider_class_spec();
    let mut exact_cap = happy_input();
    exact_cap.class_id = RAIDER_CLASS_ID.to_string();
    exact_cap.from_level = 39;
    exact_cap.to_level = 40;
    exact_cap.actions.clear();
    apply_class_spec_advancement_plan(&exact_cap, &happy_rules(), &class_spec)
        .expect("Raider may advance through its authored exact cap");

    let mut beyond_cap = exact_cap;
    beyond_cap.from_level = 40;
    beyond_cap.to_level = 41;
    let error = apply_class_spec_advancement_plan(&beyond_cap, &happy_rules(), &class_spec)
        .expect_err("Raider advancement beyond rank 40 must fail closed");
    assert_eq!(error.code(), "LITD-RPG-ADVANCEMENT-INVALID-CONFIG");
    assert!(error.to_string().contains("stops at level 40"));
}

#[test]
fn issue1750_advancement_readback_persists_and_reads_from_calyx() {
    let readback = apply_advancement_plan(&happy_input(), &happy_rules()).unwrap();
    let (_store, vault) = content_store("advancement-state");

    let written = persist_advancement_readback(&vault, &readback).unwrap();
    let loaded = read_persisted_advancement_readback(&vault, &readback.character_id).unwrap();

    assert_eq!(written.readback, readback);
    assert_eq!(loaded.readback, readback);
    assert_eq!(loaded.sha256, written.sha256);
    assert_eq!(loaded.character_id, readback.character_id);
    assert!(loaded.bytes > 0);
}

#[test]
fn issue962_issue1455_persists_advancement_tuning_as_authored_calyx_content() {
    let input = happy_input();
    let rules = happy_rules();
    let records = advancement_content_records(&rules, &input.ability_catalog).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("advancement");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rules_row = store
        .get_content_for_editor(EngineContentKind::Lore, "advancement.rules.mud_defaults")
        .unwrap();
    let cost_row = store
        .get_content_for_editor(
            EngineContentKind::Ability,
            "advancement.study_cost.ability.cinder_bash",
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
    assert_eq!(rules_row.payload["surface_kind"], "advancement_rules");
    assert_eq!(rules_row.payload["practice_band_count"], 5);
    assert_eq!(rules_row.payload["train_mana"], 12);
    assert_eq!(cost_row.payload["surface_kind"], "advancement_study_cost");
    assert_eq!(cost_row.payload["ability_id"], "ability.cinder_bash");
    assert_eq!(cost_row.payload["points"], 3);
    assert_ne!(rules_row.payload["built_in_map_required"], true);
    assert_ne!(rules_row.payload["grants_map_or_minimap"], true);
    assert_ne!(rules_row.payload["grants_gps_markers"], true);
    assert_ne!(cost_row.payload["built_in_map_required"], true);
    assert_ne!(cost_row.payload["grants_map_or_minimap"], true);
    assert_ne!(cost_row.payload["grants_gps_markers"], true);
    assert_eq!(lowered.row_count, 8);
    assert!(!lowered.calyx_live_query_allowed_in_hot_path);
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Lore)
            .count(),
        7
    );
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Ability)
            .count(),
        1
    );
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/advancement/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    for surface in [
        "advancement_rules",
        "advancement_practice_band",
        "advancement_study_grant",
        "advancement_study_cost",
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
    crate::test_temp::TestTempDir::new(&format!("issue962-{label}"))
}
