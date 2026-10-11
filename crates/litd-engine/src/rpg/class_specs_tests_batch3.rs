// Lib tests for issues #1029 (Shaman), #1030 (Healer), #1031 (Druid), #1032 (Invoker). Mirrors
// the Warrior pilot's class_specs_tests.rs pattern.
use crate::{
    rpg::{
        ClassRaceAccess, PrimeStat, RaceAccessRule, class_spec_content_records,
        class_spec_readback, persist_and_lower_rpg_authored_content, validate_class_race_access,
        validate_class_spec,
    },
    rpg_class_specs_fixture_druid::{DRUID_CLASS_ID, druid_class_spec, known_races as druid_races},
    rpg_class_specs_fixture_healer::{
        HEALER_CLASS_ID, healer_class_spec, known_races as healer_races,
    },
    rpg_class_specs_fixture_invoker::{
        INVOKER_CLASS_ID, invoker_class_spec, known_races as invoker_races,
    },
    rpg_class_specs_fixture_shaman::{
        SHAMAN_CLASS_ID, known_races as shaman_races, shaman_class_spec,
    },
    rpg_class_specs_tests_support::{assert_code, content_store},
};

#[test]
fn issue1029_shaman_class_spec_matches_source_facts_and_persists() {
    let spec = shaman_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, SHAMAN_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Wisdom);
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["good".to_string(), "evil".to_string()]
        }
    );
    assert!(readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 4);
    assert_eq!(readback.allowed_armor_count, 2);
    assert!(readback.leveled_ability_count >= 90);
    assert_eq!(readback.specialization_count, 15);
    assert_eq!(readback.specialization_choice_count, 2);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("shaman");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, SHAMAN_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &shaman_races()).unwrap();
}

#[test]
fn issue1029_shaman_class_spec_fail_closed_edges() {
    let mut wrong_schema = shaman_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = shaman_class_spec();
    dangling.leveled_abilities[0].specialization_ids = vec!["spec.shaman.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = shaman_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &shaman_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1030_healer_class_spec_matches_source_facts_and_persists() {
    let spec = healer_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, HEALER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Wisdom);
    // Schema v2 (#1718): the description's good-or-neutral rule is now encoded,
    // with a per-race evil carve-out for humans/dark-elves/saurians.
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["good".to_string(), "neutral".to_string()],
        }
    );
    assert_eq!(readback.alignment_exception_count, 3);
    assert_eq!(readback.fount, litd_engine::rpg::ClassFount::Supplication);
    assert!(readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 4);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 55);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("healer");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, HEALER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &healer_races()).unwrap();
}

#[test]
fn issue1030_healer_class_spec_fail_closed_edges() {
    let mut wrong_schema = healer_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = healer_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &healer_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1031_druid_class_spec_matches_source_facts_and_persists() {
    let spec = druid_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, DRUID_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Wisdom);
    assert_eq!(
        readback.alignment,
        RaceAccessRule::Restricted {
            values: vec!["neutral".to_string()]
        }
    );
    assert!(readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 4);
    assert_eq!(readback.allowed_armor_count, 2);
    assert!(readback.leveled_ability_count >= 55);
    assert_eq!(readback.specialization_count, 0);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("druid");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, DRUID_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &druid_races()).unwrap();
}

#[test]
fn issue1031_druid_class_spec_fail_closed_edges() {
    let mut wrong_schema = druid_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = druid_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &druid_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1032_invoker_class_spec_matches_source_facts_and_persists() {
    let spec = invoker_class_spec();
    let readback = class_spec_readback(&spec).unwrap();
    assert_eq!(readback.class_id, INVOKER_CLASS_ID);
    assert_eq!(readback.prime_stat, PrimeStat::Intelligence);
    assert_eq!(readback.alignment, RaceAccessRule::Any);
    assert!(!readback.empowerment);
    assert_eq!(readback.allowed_weapon_count, 3);
    assert_eq!(readback.allowed_armor_count, 1);
    assert!(readback.leveled_ability_count >= 55);
    assert_eq!(readback.specialization_count, 7);
    assert_eq!(readback.specialization_choice_count, 1);
    assert_eq!(readback.legacy_count, 0);
    assert_eq!(readback.signature_vfx_count, 4);
    assert_eq!(readback.class_spec_sha256.len(), 64);

    let records = class_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("invoker");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(row.id, INVOKER_CLASS_ID);
    assert_eq!(lowered.row_count, 1);

    validate_class_race_access(&spec, &invoker_races()).unwrap();
}

#[test]
fn issue1032_invoker_class_spec_fail_closed_edges() {
    let mut wrong_schema = invoker_class_spec();
    wrong_schema.schema = "litd.rpg.class_spec.bad".to_string();
    assert_code(
        validate_class_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling = invoker_class_spec();
    dangling.leveled_abilities[0].specialization_ids = vec!["spec.invoker.nonexistent".to_string()];
    assert_code(
        validate_class_spec(&dangling),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut dangling_race = invoker_class_spec();
    dangling_race.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    assert_code(
        validate_class_race_access(&dangling_race, &invoker_races()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
