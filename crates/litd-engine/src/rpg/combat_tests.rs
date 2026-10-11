use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        RPG_COMBAT_SCHEMA, combat_content_records, persist_and_lower_rpg_authored_content,
        resolve_combat_plan, validate_rpg_authored_content_records,
    },
    rpg_combat_test_fixture::{
        command_out_of_range, duplicate_defender_skill, duplicate_defense_order,
        exhausted_roll_stream, happy_input, happy_rules, invalid_weapon_damage,
        missing_armor_mitigation, missing_weapon_matchup,
    },
};

#[test]
fn issue963_combat_resolves_rounds_multi_attack_and_lag() {
    let readback = resolve_combat_plan(&happy_input(), &happy_rules()).unwrap();

    assert_eq!(readback.schema, RPG_COMBAT_SCHEMA);
    assert_eq!(readback.round_millis, 1500);
    assert_eq!(readback.requested_rounds, 3);
    assert_eq!(readback.resolved_rounds, 3);
    assert_eq!(readback.total_damage, 43);
    assert_eq!(readback.final_defender.hit_points, 137);
    assert_eq!(readback.rolls_consumed, 28);
    assert_eq!(readback.rounds[0].attacks.len(), 3);
    assert_eq!(readback.rounds[1].action_kind, "bash");
    assert_eq!(readback.rounds[1].attacker_wait_after, 3);
    assert!(readback.rounds[2].skipped_by_wait_state);
    assert_eq!(readback.rounds[2].attacker_wait_before, 3);
    assert_eq!(readback.rounds[2].attacker_wait_after, 0);
    assert_eq!(readback.input_sha256.len(), 64);
    assert_eq!(readback.result_sha256.len(), 64);
}

#[test]
fn issue963_combat_reports_defense_order_and_armor_damage() {
    let readback = resolve_combat_plan(&happy_input(), &happy_rules()).unwrap();

    assert_eq!(
        readback.defense_order,
        [
            "distance",
            "spin",
            "evade",
            "parry",
            "shield_block",
            "dodge"
        ]
    );
    assert_eq!(
        readback.rounds[0].attacks[0].defended_by.as_deref(),
        Some("spin")
    );
    assert_eq!(
        readback.rounds[0].attacks[1].defended_by.as_deref(),
        Some("shield_block")
    );
    let first_damage = &readback.rounds[0].attacks[2];
    assert_eq!(first_damage.raw_damage, 25);
    assert_eq!(first_damage.mitigated_damage, 20);
    let bash_damage = &readback.rounds[1].attacks[0];
    assert_eq!(bash_damage.raw_damage, 29);
    assert_eq!(bash_damage.mitigated_damage, 23);
}

#[test]
fn issue963_combat_reports_multi_attack_checks() {
    let readback = resolve_combat_plan(&happy_input(), &happy_rules()).unwrap();

    assert_eq!(readback.multi_attack_checks.len(), 3);
    assert_eq!(readback.multi_attack_checks[0].attack_slot, 2);
    assert!(readback.multi_attack_checks[0].granted);
    assert_eq!(readback.multi_attack_checks[1].attack_slot, 3);
    assert!(readback.multi_attack_checks[1].granted);
    assert_eq!(readback.multi_attack_checks[2].attack_slot, 4);
    assert!(!readback.multi_attack_checks[2].granted);
}

#[test]
fn issue963_combat_fails_closed_on_invalid_edges() {
    assert_code(duplicate_defender_skill(), "LITD-RPG-COMBAT-INVALID-CONFIG");
    assert_code(exhausted_roll_stream(), "LITD-RPG-COMBAT-INVALID-CONFIG");
    assert_code(duplicate_defense_order(), "LITD-RPG-COMBAT-INVALID-CONFIG");
    assert_code(invalid_weapon_damage(), "LITD-RPG-COMBAT-INVALID-CONFIG");
    assert_code(missing_weapon_matchup(), "LITD-RPG-COMBAT-INVALID-CONFIG");
    assert_code(missing_armor_mitigation(), "LITD-RPG-COMBAT-INVALID-CONFIG");
    assert_code(command_out_of_range(), "LITD-RPG-COMBAT-INVALID-CONFIG");
}

#[test]
fn issue963_issue1455_persists_combat_tuning_as_authored_calyx_content() {
    let rules = happy_rules();
    let records = combat_content_records(&rules).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("combat");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rules_row = store
        .get_content_for_editor(EngineContentKind::Lore, "combat.rules.mud_defaults")
        .unwrap();
    let bash_row = store
        .get_content_for_editor(EngineContentKind::Lore, "combat.action_profile.bash")
        .unwrap();
    let plate_row = store
        .get_content_for_editor(EngineContentKind::Lore, "combat.armor_mitigation.plate")
        .unwrap();
    let payload_json = serde_json::to_string(&records).unwrap();

    assert_eq!(records.len(), 26);
    assert_eq!(validation.record_count, 26);
    assert_eq!(validation.acquired_map_record_count, 0);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert_eq!(rules_row.payload["surface_kind"], "combat_rules");
    assert_eq!(rules_row.payload["round_ticks"], 3);
    assert_eq!(rules_row.payload["defense_order_count"], 6);
    assert_eq!(rules_row.payload["armor_mitigation_count"], 8);
    assert_eq!(bash_row.payload["surface_kind"], "combat_action_profile");
    assert_eq!(bash_row.payload["skill_id"], "skill.bash");
    assert_eq!(bash_row.payload["lag_ticks"], 3);
    assert_eq!(plate_row.payload["surface_kind"], "combat_armor_mitigation");
    assert_eq!(plate_row.payload["damage_reduction_percent"], 30);
    assert_ne!(rules_row.payload["built_in_map_required"], true);
    assert_ne!(rules_row.payload["grants_map_or_minimap"], true);
    assert_ne!(rules_row.payload["grants_gps_markers"], true);
    assert_eq!(lowered.row_count, 26);
    assert!(!lowered.calyx_live_query_allowed_in_hot_path);
    assert!(lowered.entities.iter().all(|row| {
        row.kind == EngineContentKind::Lore
            && row
                .source_path
                .starts_with("editor://rpg/ashen_reach/combat/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    for surface in [
        "combat_rules",
        "combat_defense_order",
        "combat_multi_attack_slot",
        "combat_weapon_matchup",
        "combat_armor_mitigation",
        "combat_action_profile",
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
    crate::test_temp::TestTempDir::new(&format!("issue963-{label}"))
}
