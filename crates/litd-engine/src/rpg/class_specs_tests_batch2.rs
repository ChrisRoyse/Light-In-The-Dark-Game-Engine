// Lib tests for issues #1025 (Berserker), #1026 (Raider), #1027 (Paladin), #1028
// (Anti-Paladin). Mirrors the Warrior pilot's class_specs_tests.rs pattern.
use crate::{
    rpg::{
        ClassRaceAccess, PrimeStat, RaceAccessRule, class_spec_content_records,
        class_spec_readback, persist_and_lower_rpg_authored_content, validate_class_race_access,
        validate_class_spec,
    },
    rpg_class_specs_fixture_anti_paladin::{
        ANTI_PALADIN_CLASS_ID, anti_paladin_class_spec, known_races as anti_paladin_races,
    },
    rpg_class_specs_fixture_berserker::{
        BERSERKER_CLASS_ID, berserker_class_spec, known_races as berserker_races,
    },
    rpg_class_specs_fixture_paladin::{
        PALADIN_CLASS_ID, known_races as paladin_races, paladin_class_spec,
    },
    rpg_class_specs_fixture_raider::{
        RAIDER_CLASS_ID, known_races as raider_races, raider_class_spec,
    },
    rpg_class_specs_tests_support::{assert_code, content_store},
};

#[test]
fn issue1025_berserker_class_spec_matches_source_facts_and_persists() {
    let spec = berserker_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, BERSERKER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Strength);
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["evil".to_string()]
        }
    );
    assert!(!readback.empowerment);
    assert_eq!(
        readback.race_access,
        ClassRaceAccess::Restricted {
            race_ids: vec!["race.orc".to_string()]
        }
    );
    assert_eq!(readback.allowed_weapon_count, 9);
    assert_eq!(readback.allowed_armor_count, 3);
    assert!(readback.leveled_ability_count >= 70);
    assert_eq!(readback.specialization_count, 4);
    assert_eq!(readback.specialization_choice_count, 1);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("berserker");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, BERSERKER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &berserker_races()).unwrap();
}

#[test]
fn issue1025_berserker_class_spec_fail_closed_edges() {
    let mut wrong_schema = berserker_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = berserker_class_spec();
    dangling.leveled_abilities[0].specialization_ids =
        vec!["spec.berserker.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = berserker_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &berserker_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1026_raider_class_spec_matches_source_facts_and_persists() {
    let spec = raider_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, RAIDER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Dexterity);
    assert_eq!(readback.max_level, 40);
    assert!(!readback.empowerment);
    assert_eq!(
        readback.race_access,
        ClassRaceAccess::Restricted {
            race_ids: vec!["race.goblin".to_string()]
        }
    );
    assert_eq!(readback.allowed_weapon_count, 5);
    assert_eq!(readback.allowed_armor_count, 2);
    assert!(readback.leveled_ability_count >= 35);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("raider");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, RAIDER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &raider_races()).unwrap();
}

#[test]
fn issue1026_raider_class_spec_fail_closed_edges() {
    let mut wrong_schema = raider_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = raider_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &raider_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1027_paladin_class_spec_matches_source_facts_and_persists() {
    let spec = paladin_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, PALADIN_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Strength);
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["good".to_string()]
        }
    );
    assert!(readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 4);
    assert_eq!(readback.allowed_armor_count, 2);
    assert!(readback.leveled_ability_count >= 50);
    assert_eq!(readback.specialization_count, 4);
    assert_eq!(readback.specialization_choice_count, 1);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("paladin");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, PALADIN_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &paladin_races()).unwrap();
}

#[test]
fn issue1027_paladin_class_spec_fail_closed_edges() {
    let mut wrong_schema = paladin_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = paladin_class_spec();
    dangling.leveled_abilities[0].specialization_ids = vec!["spec.paladin.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = paladin_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &paladin_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1028_anti_paladin_class_spec_matches_source_facts_and_persists() {
    let spec = anti_paladin_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, ANTI_PALADIN_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Strength);
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["evil".to_string()]
        }
    );
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 8);
    assert_eq!(readback.allowed_armor_count, 3);
    assert!(readback.leveled_ability_count >= 65);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("anti_paladin");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, ANTI_PALADIN_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &anti_paladin_races()).unwrap();
}

#[test]
fn issue1028_anti_paladin_class_spec_fail_closed_edges() {
    let mut wrong_schema = anti_paladin_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = anti_paladin_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &anti_paladin_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
