//! #1683 wear-slot equipment model + #1684 type-taxonomy tests: racial slots,
//! attachment points, referential integrity against the authored race corpus,
//! and kind-specific payload validation failing loud on mismatch.

use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        ChargeEconomy, ItemKindPayload, RaceRegistry, SpellPayload, WearGating, WearSlot,
        item_spec_content_records, item_spec_readback, persist_and_lower_rpg_authored_content,
        validate_item_kind_payload, validate_item_racial_gating_against_registry,
        validate_item_spec, validate_wear_gating,
    },
    rpg_item_specs_test_fixture::{
        felisar_warclaws_spec, healing_draught_spec, ironbound_strongbox_spec,
        oak_lightning_staff_spec, travellers_waterskin_spec,
    },
    rpg_race_specs_batch_1001_1018::all_batch_race_specs,
};

// -- #1683 wear-slot model ----------------------------------------------------

#[test]
fn issue1683_racial_pilot_occupies_claws_slot() {
    let spec = felisar_warclaws_spec();
    let readback = item_spec_readback(&spec).unwrap();
    assert_eq!(readback.wear_slot, "claws");
    assert_eq!(readback.gating_race_count, 1);
    assert!(spec.wear.slot.is_racial());
    assert_eq!(spec.wear.gating_race_ids, vec!["race.felisar".to_string()]);
}

#[test]
fn issue1683_every_wear_slot_maps_to_an_attachment_point() {
    // #782 render contract: every worn slot has a model attachment point; only
    // the carry-only Take slot has none.
    for slot in WearSlot::ALL {
        match slot {
            WearSlot::Take => assert!(slot.attachment_point().is_none()),
            other => assert!(
                other.attachment_point().is_some(),
                "slot {} must map to an attachment point",
                other.as_id()
            ),
        }
    }
    assert_eq!(WearSlot::Wield.attachment_point(), Some("attach.hand.main"));
    assert_eq!(WearSlot::Claws.attachment_point(), Some("attach.claws"));
    assert_eq!(WearSlot::Wings.attachment_point(), Some("attach.wings"));
}

#[test]
fn issue1683_slot_id_round_trips_and_rejects_bogus() {
    for slot in WearSlot::ALL {
        assert_eq!(WearSlot::from_id(slot.as_id()), Some(slot));
    }
    // FSV edge: a bogus wear slot does not resolve.
    assert_eq!(WearSlot::from_id("greaves"), None);
    assert_eq!(WearSlot::from_id(""), None);
}

#[test]
fn issue1683_only_racial_slots_are_gated() {
    let racial = [
        WearSlot::Claws,
        WearSlot::Foreclaws,
        WearSlot::Forepaws,
        WearSlot::Hindpaws,
        WearSlot::Hooves,
        WearSlot::Horns,
        WearSlot::Tail,
        WearSlot::Wings,
    ];
    for slot in WearSlot::ALL {
        assert_eq!(slot.is_racial(), racial.contains(&slot));
    }
}

#[test]
fn issue1683_wear_gating_form_edges_fail_closed() {
    // A racial slot MUST name at least one gating race id.
    let empty_racial = WearGating {
        slot: WearSlot::Hooves,
        gating_race_ids: Vec::new(),
    };
    assert_code(
        validate_wear_gating(&empty_racial),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // A standard slot MUST NOT carry gating race ids.
    let gated_standard = WearGating {
        slot: WearSlot::Body,
        gating_race_ids: vec!["race.centaur".to_string()],
    };
    assert_code(
        validate_wear_gating(&gated_standard),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // A malformed gating token fails loud.
    let malformed = WearGating {
        slot: WearSlot::Tail,
        gating_race_ids: vec!["race.not a token!".to_string()],
    };
    assert_code(
        validate_wear_gating(&malformed),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1683_racial_gating_referential_integrity_against_race_corpus() {
    // The registry is DERIVED from the authored RaceSpec corpus (single source of
    // truth), mirroring the #1631 class registry / #1704 area registry.
    let races = all_batch_race_specs();
    let registry = RaceRegistry::from_specs(&races).unwrap();
    assert!(registry.contains("race.felisar"));

    // The felar warclaws gate to race.felisar, which IS authored -> passes.
    let good = felisar_warclaws_spec();
    let readback = validate_item_racial_gating_against_registry(&[good], &registry).unwrap();
    assert_eq!(readback.racial_slot_item_count, 1);
    assert_eq!(readback.checked_gating_reference_count, 1);
    assert_eq!(readback.race_count, races.len());

    // FSV edge: a dangling gating race id fails loud, naming item + bad id.
    let mut ghost = felisar_warclaws_spec();
    ghost.wear.gating_race_ids = vec!["race.does_not_exist".to_string()];
    let error = validate_item_racial_gating_against_registry(&[ghost], &registry).unwrap_err();
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    assert!(error.to_string().contains("race.does_not_exist"));
}

#[test]
fn issue1683_registry_rejects_duplicate_race_id() {
    let mut races = all_batch_race_specs();
    let dup = races[0].clone();
    races.push(dup);
    assert_code(
        RaceRegistry::from_specs(&races).map(|_| ()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

// -- #1684 type taxonomy + consumables ----------------------------------------

#[test]
fn issue1684_consumable_pilots_carry_their_kind() {
    let cases = [
        (healing_draught_spec(), "potion"),
        (oak_lightning_staff_spec(), "staff"),
        (travellers_waterskin_spec(), "drink"),
        (ironbound_strongbox_spec(), "container"),
    ];
    for (spec, kind) in cases {
        let readback = item_spec_readback(&spec).unwrap();
        assert_eq!(readback.kind, kind, "item {} kind", spec.item_id);
        // Consumables are carry-only, never armour/weapon.
        assert!(!readback.has_armor);
        assert!(!readback.has_weapon);
    }
}

#[test]
fn issue1684_staff_charge_economy_is_sound() {
    let spec = oak_lightning_staff_spec();
    match &spec.kind_payload {
        ItemKindPayload::Staff { spell, charges } => {
            assert_eq!(spell.spell_level, 30);
            assert_eq!(charges.charges_total, 12);
            assert_eq!(charges.charges_remaining, 12);
        }
        other => panic!("expected a staff payload, got {other:?}"),
    }
}

#[test]
fn issue1684_potion_persists_and_reads_back_kind() {
    let spec = healing_draught_spec();
    let records = item_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("potion");
    persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(EngineContentKind::Item, &spec.item_id)
        .unwrap();
    assert_eq!(row.payload["kind"], "potion");
    assert_eq!(row.payload["kind_payload"]["kind"], "potion");
    assert_eq!(row.payload["kind_payload"]["payload"]["spell_level"], 18);
}

#[test]
fn issue1684_payload_validation_fails_loud_on_bad_payloads() {
    // Potion with a zero spell level (never casts).
    let zero_level = ItemKindPayload::Potion(SpellPayload {
        spell_level: 0,
        spell_ability_ids: vec!["spell.cure_light".to_string()],
    });
    assert_code(
        validate_item_kind_payload(&zero_level),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Wand whose remaining charges exceed its total.
    let over_charged = ItemKindPayload::Wand {
        spell: SpellPayload {
            spell_level: 20,
            spell_ability_ids: vec!["spell.magic_missile".to_string()],
        },
        charges: ChargeEconomy {
            charges_total: 3,
            charges_remaining: 9,
        },
    };
    assert_code(
        validate_item_kind_payload(&over_charged),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Container with zero capacity.
    let empty_container = ItemKindPayload::Container {
        capacity_weight: 0,
        weight_reduction_percent: 10,
        lockable: false,
    };
    assert_code(
        validate_item_kind_payload(&empty_container),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Drink with no liquid.
    let dry_drink = ItemKindPayload::Drink {
        liquid: "  ".to_string(),
        servings: 5,
        thirst_hours: 3,
    };
    assert_code(
        validate_item_kind_payload(&dry_drink),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1684_kind_payload_mismatch_fails_at_spec_level() {
    // FSV edge: kind/payload mismatch -- a non-combat kind carrying weapon dice.
    let mut potion_with_weapon = healing_draught_spec();
    potion_with_weapon.weapon = crate::rpg_item_specs_test_fixture::steel_maul_weapon_spec().weapon;
    assert_code(
        validate_item_spec(&potion_with_weapon),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // A non-armour kind carrying armour class also fails loud.
    let mut drink_with_ac = travellers_waterskin_spec();
    drink_with_ac.armor = crate::rpg_item_specs_test_fixture::blackened_leather_gloves_spec().armor;
    assert_code(
        validate_item_spec(&drink_with_ac),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1684-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
