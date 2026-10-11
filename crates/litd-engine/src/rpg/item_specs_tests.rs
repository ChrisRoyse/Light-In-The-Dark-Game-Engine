//! #1682 core item data model tests: readback matches transcribed CF facts,
//! Calyx persist + readback per pilot, and loud-fail edges.

use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        BLACKENED_LEATHER_GLOVES_ITEM_ID, CRUSADER_LONGSWORD_ITEM_ID, DamageDice,
        item_spec_content_records, item_spec_readback, persist_and_lower_rpg_authored_content,
        validate_item_spec, validate_rpg_authored_content_records,
    },
    rpg_item_specs_test_fixture::{blackened_leather_gloves_spec, steel_maul_weapon_spec},
};

#[test]
fn issue1682_weapon_pilot_matches_source_facts() {
    let spec = steel_maul_weapon_spec();
    let readback = item_spec_readback(&spec).unwrap();

    assert_eq!(readback.item_id, CRUSADER_LONGSWORD_ITEM_ID);
    assert_eq!(readback.kind, "weapon");
    assert_eq!(readback.wear_slot, "wield");
    assert_eq!(readback.level, 51);
    // CF weight 15 lb 13 oz = 253 oz.
    assert_eq!(readback.weight_total_ounces, 15 * 16 + 13);
    assert_eq!(readback.material, "steel");
    assert!(readback.has_weapon);
    assert!(!readback.has_armor);
    // CF +1 damage roll, +1 hit roll, +1 strength.
    assert_eq!(readback.affect_count, 3);
    assert_eq!(readback.item_spec_sha256.len(), 64);

    // CF Weapon Damage average 18 (pound): 4d8 average = 4*(8+1)/2 = 18.
    let dice = DamageDice { count: 4, sides: 8 };
    assert_eq!(dice.average_tenths(), 180);
    assert_eq!(dice.min_roll(), 4);
    assert_eq!(dice.max_roll(), 32);
    let weapon = spec.weapon.unwrap();
    assert_eq!(weapon.damage_noun, "pound");
}

#[test]
fn issue1682_armor_pilot_matches_five_channel_ac() {
    let spec = blackened_leather_gloves_spec();
    let readback = item_spec_readback(&spec).unwrap();

    assert_eq!(readback.item_id, BLACKENED_LEATHER_GLOVES_ITEM_ID);
    assert_eq!(readback.kind, "armor");
    assert_eq!(readback.wear_slot, "hands");
    assert!(readback.has_armor);
    assert!(!readback.has_weapon);
    assert_eq!(readback.flag_count, 1);

    // CF Armor Class Pierce 13 Bash 7 Slash 13 Magic 3 Element 14 (all 5 channels).
    let ac = spec.armor.unwrap();
    assert_eq!(ac.pierce, 13);
    assert_eq!(ac.bash, 7);
    assert_eq!(ac.slash, 13);
    assert_eq!(ac.magic, 3);
    assert_eq!(ac.element, 14);
    // Identify reveals the AC (helpfile 430); deep-lore hides special proc + align.
    assert_eq!(readback.identify_reveal_count, 7);
    assert_eq!(readback.deep_lore_reveal_count, 2);
}

#[test]
fn issue1682_pilots_persist_and_read_back_from_calyx_store() {
    for spec in [steel_maul_weapon_spec(), blackened_leather_gloves_spec()] {
        let records = item_spec_content_records(&spec).unwrap();
        let validation = validate_rpg_authored_content_records(&records).unwrap();
        let (store, _vault) = content_store(&spec.item_id);
        let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
        // Source-of-truth read-back: fetch from the persisted store.
        let row = store
            .get_content_for_editor(records[0].kind, &records[0].id)
            .unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(validation.record_count, 1);
        assert!(validation.all_editor_authored_sources);
        assert_eq!(row.kind, EngineContentKind::Item);
        assert_eq!(row.id, spec.item_id);
        assert_eq!(row.payload["surface_kind"], "item_spec");
        assert_eq!(row.payload["item_id"], spec.item_id);
        assert_eq!(row.payload["level"], spec.level);
        assert_ne!(row.payload["built_in_map_required"], true);
        assert_ne!(row.payload["grants_map_or_minimap"], true);
        assert!(
            row.source_path
                .starts_with("editor://rpg/ashen_reach/item_specs/")
        );
        assert_eq!(lowered.row_count, 1);
        assert_eq!(lowered.entities[0].surface_kind, "item_spec");
        assert!(lowered.entities[0].lowered_field_count > 0);
    }
}

#[test]
fn issue1682_five_channel_ac_survives_round_trip() {
    let spec = blackened_leather_gloves_spec();
    let records = item_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("armor-ac-roundtrip");
    persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();

    // Every one of the five AC channels survives the store round-trip intact.
    assert_eq!(row.payload["armor"]["pierce"], 13);
    assert_eq!(row.payload["armor"]["bash"], 7);
    assert_eq!(row.payload["armor"]["slash"], 13);
    assert_eq!(row.payload["armor"]["magic"], 3);
    assert_eq!(row.payload["armor"]["element"], 14);
    // Weapon dice absent for armor (kind cross-check holds through the store).
    assert!(row.payload.get("weapon").is_none());
}

#[test]
fn issue1682_armor_without_ac_fails_closed() {
    // FSV edge: an armor item MUST carry the 5-channel AC.
    let mut no_ac = blackened_leather_gloves_spec();
    no_ac.armor = None;
    assert_code(validate_item_spec(&no_ac), "LITD-RPG-STATS-INVALID-CONFIG");

    // FSV edge: a weapon must not carry armor class.
    let mut weapon_with_ac = steel_maul_weapon_spec();
    weapon_with_ac.armor = blackened_leather_gloves_spec().armor;
    assert_code(
        validate_item_spec(&weapon_with_ac),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1682_weapon_without_profile_fails_closed() {
    // FSV edge: kind/payload mismatch -- a weapon item without weapon dice.
    let mut no_dice = steel_maul_weapon_spec();
    no_dice.weapon = None;
    assert_code(
        validate_item_spec(&no_dice),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Degenerate dice (zero sides can never roll) fail loud.
    let mut zero_dice = steel_maul_weapon_spec();
    if let Some(weapon) = zero_dice.weapon.as_mut() {
        weapon.dice = DamageDice { count: 2, sides: 0 };
    }
    assert_code(
        validate_item_spec(&zero_dice),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1682_identify_split_edges_fail_closed() {
    // Identify must reveal at least one property (helpfile 430).
    let mut empty_identify = blackened_leather_gloves_spec();
    empty_identify.identify.identify_reveals.clear();
    assert_code(
        validate_item_spec(&empty_identify),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // A property cannot be both identify-visible AND deep-lore-hidden.
    let mut overlap = blackened_leather_gloves_spec();
    overlap.identify.deep_lore_reveals = overlap.identify.identify_reveals.clone();
    assert_code(
        validate_item_spec(&overlap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1682_wrong_schema_and_bad_source_fail_closed() {
    let mut wrong_schema = steel_maul_weapon_spec();
    wrong_schema.schema = "litd.rpg.item_spec.bad".to_string();
    assert_code(
        validate_item_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut bad_source = steel_maul_weapon_spec();
    bad_source.mechanics_source_url = "https://example.com/item".to_string();
    assert_code(
        validate_item_spec(&bad_source),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut zero_affect = blackened_leather_gloves_spec();
    zero_affect.affects[0].modifier = 0;
    assert_code(
        validate_item_spec(&zero_affect),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
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

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    let safe: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    crate::test_temp::TestTempDir::new(&format!("issue1682-{safe}"))
}
