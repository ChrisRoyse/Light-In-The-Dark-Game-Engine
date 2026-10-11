use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        SoulKillEvent, SoulVictimKind, SoulWeaponBindRequest, SoulWeaponTransferRequest,
        UNHALLOWED_KNIGHT_CLASS_ID, UnhallowedKnightCharacter, bind_soul_weapon,
        derive_unhallowed_knight_readback, destroy_soul_bound_weapon,
        persist_and_lower_rpg_authored_content, record_soul_weapon_kill,
        transfer_soul_weapon_power, unhallowed_knight_content_records,
        validate_rpg_authored_content_records, validate_unhallowed_knight_policy,
    },
    rpg_unhallowed_knight_test_fixture::ashen_reach_unhallowed_knight_policy,
};

#[test]
fn issue1062_defaults_define_evil_strength_soul_weapon_class() {
    let policy = ashen_reach_unhallowed_knight_policy();
    let readback = derive_unhallowed_knight_readback(&policy).unwrap();

    assert_eq!(readback.class_id, UNHALLOWED_KNIGHT_CLASS_ID);
    assert_eq!(readback.prime_stat, "strength");
    assert_eq!(readback.required_alignment, "evil");
    assert_eq!(readback.ability_count, 6);
    assert_eq!(readback.ability_readback.capstone_count, 1);
    assert!(readback.every_life_claimed_strengthens);
    assert!(readback.accumulated_power_transferable);
    assert!(readback.destroying_weapon_wounds_master);
    assert!(!readback.built_in_map_required);
    assert!(!readback.grants_map_or_minimap);
    assert!(!readback.grants_gps_markers);
}

#[test]
fn issue1062_every_witnessed_kill_permanently_grows_bound_weapon() {
    let policy = ashen_reach_unhallowed_knight_policy();
    let bound = bind_soul_weapon(&policy, &character(), &bind_request()).unwrap();
    let first = record_soul_weapon_kill(&policy, &bound.after_state, &kill("kill.1", 3)).unwrap();
    let second = record_soul_weapon_kill(&policy, &first.after_state, &kill("kill.2", 1)).unwrap();

    assert_eq!(bound.power_after_milli, 350);
    assert_eq!(first.souls_after, 3);
    assert_eq!(first.power_after_milli, 425);
    assert_eq!(second.souls_after, 4);
    assert_eq!(second.power_after_milli, 450);
    assert_eq!(second.after_state.kill_event_ids.len(), 2);
    assert!(!second.built_in_map_required);
}

#[test]
fn issue1062_accumulated_power_transfers_to_another_weapon() {
    let policy = ashen_reach_unhallowed_knight_policy();
    let bound = bind_soul_weapon(&policy, &character(), &bind_request()).unwrap();
    let grown = record_soul_weapon_kill(&policy, &bound.after_state, &kill("kill.1", 4)).unwrap();
    let transfer =
        transfer_soul_weapon_power(&policy, &grown.after_state, &transfer_request()).unwrap();

    assert_eq!(grown.power_after_milli, 450);
    assert_eq!(
        transfer.weapon_id_before.as_deref(),
        Some("item.blade.black_oath")
    );
    assert_eq!(
        transfer.weapon_id_after.as_deref(),
        Some("item.blade.throne_iron")
    );
    assert_eq!(transfer.power_after_milli, 405);
    assert_eq!(transfer.after_state.souls_siphoned, 4);
    assert_eq!(transfer.after_state.transfer_history.len(), 1);
}

#[test]
fn issue1062_destroying_bound_weapon_wounds_master() {
    let policy = ashen_reach_unhallowed_knight_policy();
    let bound = bind_soul_weapon(&policy, &character(), &bind_request()).unwrap();
    let grown = record_soul_weapon_kill(&policy, &bound.after_state, &kill("kill.1", 5)).unwrap();
    let destroyed =
        destroy_soul_bound_weapon(&policy, &grown.after_state, "destroy.black_oath.1").unwrap();

    assert!(destroyed.after_state.destroyed);
    assert_eq!(destroyed.power_before_milli, 475);
    assert_eq!(destroyed.power_after_milli, 0);
    assert_eq!(destroyed.master_wound_after_milli, 300);
}

#[test]
fn issue1062_persists_class_weapon_and_abilities_as_calyx_content() {
    let policy = ashen_reach_unhallowed_knight_policy();
    let records = unhallowed_knight_content_records(&policy).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("unhallowed");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 8);
    assert_eq!(lowered.row_count, 8);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Ability)
            .count(),
        6
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.kind == EngineContentKind::Lore)
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.kind == EngineContentKind::Item)
    );
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/unhallowed_knight/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    assert_eq!(
        records
            .iter()
            .filter(|row| row.kind == EngineContentKind::Ability)
            .count(),
        6
    );
    assert!(
        records
            .iter()
            .all(|record| record.payload["built_in_map_required"] != true)
    );
    assert!(
        records
            .iter()
            .all(|record| record.payload["grants_gps_markers"] != true)
    );
}

#[test]
fn issue1062_fails_closed_on_alignment_strength_duplicate_and_map_drift() {
    let policy = ashen_reach_unhallowed_knight_policy();

    let mut good = character();
    good.alignment = "neutral".to_string();
    assert_code(
        bind_soul_weapon(&policy, &good, &bind_request()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut weak = character();
    weak.strength = 12;
    assert_code(
        bind_soul_weapon(&policy, &weak, &bind_request()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let bound = bind_soul_weapon(&policy, &character(), &bind_request()).unwrap();
    let first = record_soul_weapon_kill(&policy, &bound.after_state, &kill("kill.1", 1)).unwrap();
    assert_code(
        record_soul_weapon_kill(&policy, &first.after_state, &kill("kill.1", 1)),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut map = policy.clone();
    map.grants_map_or_minimap = true;
    assert_code(
        validate_unhallowed_knight_policy(&map),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut transfer = transfer_request();
    transfer.accepted_soul_scarring_cost = false;
    assert_code(
        transfer_soul_weapon_power(&policy, &first.after_state, &transfer),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn character() -> UnhallowedKnightCharacter {
    UnhallowedKnightCharacter {
        character_id: "char.morvane_ash".to_string(),
        character_name: "Morvane Ash".to_string(),
        class_id: UNHALLOWED_KNIGHT_CLASS_ID.to_string(),
        alignment: "evil".to_string(),
        strength: 18,
    }
}

fn bind_request() -> SoulWeaponBindRequest {
    SoulWeaponBindRequest {
        weapon_id: "item.blade.black_oath".to_string(),
        weapon_display_name: "Black-Oath Brand".to_string(),
        rite_id: "rite.unhallowed.black_oath_binding".to_string(),
        accepted_irreversible_binding: true,
    }
}

fn transfer_request() -> SoulWeaponTransferRequest {
    SoulWeaponTransferRequest {
        transfer_id: "transfer.black_oath.to_throne_iron".to_string(),
        new_weapon_id: "item.blade.throne_iron".to_string(),
        new_weapon_display_name: "Throne-Iron Reaver".to_string(),
        rite_id: "rite.unhallowed.iron_to_iron_transfer".to_string(),
        accepted_soul_scarring_cost: true,
    }
}

fn kill(event_id: &str, soul_value: u16) -> SoulKillEvent {
    SoulKillEvent {
        event_id: event_id.to_string(),
        victim_id: format!("victim.{event_id}"),
        victim_kind: SoulVictimKind::PlayerCharacter,
        soul_value,
        witnessed: true,
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
    crate::test_temp::TestTempDir::new(&format!("issue1062-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
