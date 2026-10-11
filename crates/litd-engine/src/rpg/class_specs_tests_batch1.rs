// Lib tests for issues #1021 (Thief), #1022 (Assassin), #1023 (Ranger), #1024 (Bard). Mirrors
// the Warrior pilot's class_specs_tests.rs pattern: readback facts, persist+readback identity,
// fail-closed edges, and race-access cross-check, one pair of tests per class.
use crate::{
    rpg::{
        ClassRaceAccess, PrimeStat, RaceAccessRule, class_spec_content_records,
        class_spec_readback, persist_and_lower_rpg_authored_content, validate_class_race_access,
        validate_class_spec,
    },
    rpg_class_specs_fixture_assassin::{
        ASSASSIN_CLASS_ID, assassin_class_spec, known_races as assassin_races,
    },
    rpg_class_specs_fixture_bard::{BARD_CLASS_ID, bard_class_spec, known_races as bard_races},
    rpg_class_specs_fixture_ranger::{
        RANGER_CLASS_ID, known_races as ranger_races, ranger_class_spec,
    },
    rpg_class_specs_fixture_thief::{THIEF_CLASS_ID, known_races as thief_races, thief_class_spec},
    rpg_class_specs_tests_support::{assert_code, content_store},
};

#[test]
fn issue1021_thief_class_spec_matches_source_facts_and_persists() {
    let spec = thief_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, THIEF_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Dexterity);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 4);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 90);
    assert_eq!(readback.specialization_count, 8);
    assert_eq!(readback.specialization_choice_count, 5);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    // Schema v2 (#1718): Binder point-buy chain migrated (11 costed abilities),
    // martial fount.
    assert_eq!(readback.costed_ability_count, 11);
    assert_eq!(readback.fount, litd_engine::rpg::ClassFount::Martial);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("thief");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, THIEF_CLASS_ID);
    assert_eq!(
        row.payload["class_spec_sha256"],
        serde_json::json!(readback.class_spec_sha256)
    );
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &thief_races()).unwrap();
}

#[test]
fn issue1021_thief_class_spec_fail_closed_edges() {
    let mut wrong_schema = thief_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = thief_class_spec();
    dangling.leveled_abilities[0].specialization_ids = vec!["spec.thief.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = thief_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &thief_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1022_assassin_class_spec_matches_source_facts_and_persists() {
    let spec = assassin_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, ASSASSIN_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Dexterity);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 5);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 40);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("assassin");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, ASSASSIN_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &assassin_races()).unwrap();
}

#[test]
fn issue1022_assassin_class_spec_fail_closed_edges() {
    let mut wrong_schema = assassin_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = assassin_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &assassin_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1023_ranger_class_spec_matches_source_facts_and_persists() {
    let spec = ranger_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, RANGER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Strength);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 7);
    assert_eq!(readback.allowed_armor_count, 2);
    assert!(readback.leveled_ability_count >= 70);
    assert_eq!(readback.specialization_count, 13);
    assert_eq!(readback.specialization_choice_count, 1);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    // Schema v2 (#1718): the 3 dual-tagged terrain skills (draw breath, sure
    // footing, pursuit) now carry both terrain memberships.
    assert_eq!(readback.dual_tagged_ability_count, 3);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("ranger");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, RANGER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &ranger_races()).unwrap();
}

#[test]
fn issue1023_ranger_class_spec_fail_closed_edges() {
    let mut wrong_schema = ranger_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = ranger_class_spec();
    dangling.leveled_abilities[0].specialization_ids = vec!["spec.ranger.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = ranger_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &ranger_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1024_bard_class_spec_matches_source_facts_and_persists() {
    let spec = bard_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, BARD_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Charisma);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert_eq!(readback.ethos, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 4);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 35);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("bard");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, BARD_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &bard_races()).unwrap();
}

#[test]
fn issue1024_bard_class_spec_fail_closed_edges() {
    let mut wrong_schema = bard_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = bard_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &bard_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
