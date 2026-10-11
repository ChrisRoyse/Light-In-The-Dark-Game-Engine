// Lib tests for issues #1033 (Transmuter), #1034 (Conjurer), #1035 (Necromancer), #1036
// (Shapeshifter). Mirrors the Warrior pilot's class_specs_tests.rs pattern.
use crate::{
    rpg::{
        ClassRaceAccess, PrimeStat, RaceAccessRule, class_spec_content_records,
        class_spec_readback, persist_and_lower_rpg_authored_content, validate_class_race_access,
        validate_class_spec,
    },
    rpg_class_specs_fixture_conjurer::{
        CONJURER_CLASS_ID, conjurer_class_spec, known_races as conjurer_races,
    },
    rpg_class_specs_fixture_necromancer::{
        NECROMANCER_CLASS_ID, known_races as necromancer_races, necromancer_class_spec,
    },
    rpg_class_specs_fixture_shapeshifter::{
        SHAPESHIFTER_CLASS_ID, known_races as shapeshifter_races, shapeshifter_class_spec,
    },
    rpg_class_specs_fixture_transmuter::{
        TRANSMUTER_CLASS_ID, known_races as transmuter_races, transmuter_class_spec,
    },
    rpg_class_specs_tests_support::{assert_code, content_store},
};

#[test]
fn issue1033_transmuter_class_spec_matches_source_facts_and_persists() {
    let spec = transmuter_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, TRANSMUTER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Intelligence);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 3);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 50);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("transmuter");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, TRANSMUTER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &transmuter_races()).unwrap();
}

#[test]
fn issue1033_transmuter_class_spec_fail_closed_edges() {
    let mut wrong_schema = transmuter_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = transmuter_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &transmuter_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1034_conjurer_class_spec_matches_source_facts_and_persists() {
    let spec = conjurer_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, CONJURER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Charisma);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 4);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 45);
    assert_eq!(readback.specialization_count, 3);
    assert_eq!(readback.specialization_choice_count, 1);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("conjurer");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, CONJURER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &conjurer_races()).unwrap();
}

#[test]
fn issue1034_conjurer_class_spec_fail_closed_edges() {
    let mut wrong_schema = conjurer_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = conjurer_class_spec();
    dangling.leveled_abilities[0].specialization_ids =
        vec!["spec.conjurer.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = conjurer_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &conjurer_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1035_necromancer_class_spec_matches_source_facts_and_persists() {
    let spec = necromancer_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, NECROMANCER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Intelligence);
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["evil".to_string()]
        }
    );
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 3);
    assert_eq!(readback.allowed_armor_count, 2);
    assert!(readback.leveled_ability_count >= 40);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 1);
    assert_eq!(readback.legacy_choice_count, 1);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("necromancer");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, NECROMANCER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &necromancer_races()).unwrap();
}

#[test]
fn issue1035_necromancer_class_spec_fail_closed_edges() {
    let mut wrong_schema = necromancer_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = necromancer_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &necromancer_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1036_shapeshifter_class_spec_matches_source_facts_and_persists() {
    let spec = shapeshifter_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, SHAPESHIFTER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Intelligence);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 3);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 50);
    assert_eq!(readback.specialization_count, 5);
    assert_eq!(readback.specialization_choice_count, 1);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("shapeshifter");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, SHAPESHIFTER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &shapeshifter_races()).unwrap();
}

#[test]
fn issue1036_shapeshifter_class_spec_fail_closed_edges() {
    let mut wrong_schema = shapeshifter_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = shapeshifter_class_spec();
    dangling.leveled_abilities[0].specialization_ids =
        vec!["spec.shapeshifter.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = shapeshifter_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &shapeshifter_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
