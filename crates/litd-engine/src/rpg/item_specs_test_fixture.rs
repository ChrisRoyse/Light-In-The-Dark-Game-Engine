//! Pilot item fixtures: REAL Carrion Fields items transcribed from the CF item
//! database (`carrionfields.net/itemsearch`, 2026-07-12) and the CF object-type
//! conventions, with names/lore originalized per #984 and every mechanic kept
//! factual. These are the proof fixtures for #1682 (weapon + armor), #1683
//! (racial-slot claws), and #1684 (potion/staff/drink/container consumables).

use crate::rpg::{
    ArmorClass, ChargeEconomy, DamageDice, DamageType, IdentifyReveal, ItemAffect, ItemAffectKind,
    ItemCondition, ItemKindPayload, ItemRevealProperty, ItemSpec, ItemWeight, RPG_ITEM_SPEC_SCHEMA,
    SpellPayload, WeaponClass, WeaponProfile, WearGating, WearSlot,
};
use crate::rpg::{
    BLACKENED_LEATHER_GLOVES_ITEM_ID, CRUSADER_LONGSWORD_ITEM_ID, FELISAR_WARCLAWS_ITEM_ID,
    HEALING_DRAUGHT_ITEM_ID, IRONBOUND_STRONGBOX_ITEM_ID, OAK_STAFF_LIGHTNING_ITEM_ID,
    TRAVELLERS_WATERSKIN_ITEM_ID,
};

// -- #1682 weapon pilot -------------------------------------------------------
// CF source item: "a durable steel ball peen hammer" (Subterranean Spire) --
// Item Type weapon, Wear wield, Material steel, Level 51, Weight 15 lb 13 oz,
// Weapon Class mace, Weapon Damage average 18 (pound), +1 damage roll, +1 hit
// roll, +1 strength. The itemsearch AVERAGE (18) and NOUN (pound) are the
// transcribed facts; 4d8 is the dice decomposition reproducing average 18
// (4*(8+1)/2), and "pound" is a Bash noun per helpfile 2304.
pub(crate) fn steel_maul_weapon_spec() -> ItemSpec {
    ItemSpec {
        schema: RPG_ITEM_SPEC_SCHEMA.to_string(),
        item_id: CRUSADER_LONGSWORD_ITEM_ID.to_string(),
        display_name: "a Reach-forged ball-peen maul".to_string(),
        keywords: vec!["maul".to_string(), "hammer".to_string()],
        source_area_id: "area.ashen_reach.subterranean_spire".to_string(),
        source_area_name: "The Subterranean Spire".to_string(),
        kind_payload: ItemKindPayload::Weapon,
        wear: standard(WearSlot::Wield),
        level: 51,
        weight: weight(15, 13),
        value_coin: 3200,
        material: "steel".to_string(),
        flags: Vec::new(),
        armor: None,
        weapon: Some(WeaponProfile {
            weapon_class: WeaponClass::Mace,
            dice: DamageDice { count: 4, sides: 8 },
            damage_type: DamageType::Bash,
            damage_noun: "pound".to_string(),
            two_handed: false,
        }),
        affects: vec![
            affect(ItemAffectKind::DamageRoll, 1),
            affect(ItemAffectKind::HitRoll, 1),
            affect(ItemAffectKind::Strength, 1),
        ],
        condition: ItemCondition {
            breakage_prone: false,
            repairable: true,
        },
        restring: None,
        identify: full_identify(ItemRevealProperty::WeaponDamage),
        source_issue: 1682,
        canon_issues: vec![936, 989, 1682],
        mechanics_source_url: "https://carrionfields.net/itemsearch".to_string(),
        implementation_notes: vec![
            "CF source: 'a durable steel ball peen hammer' (Subterranean Spire), weapon/wield, \
             steel, L51, 15 lb 13 oz, mace, Weapon Damage average 18 (pound)."
                .to_string(),
            "itemsearch publishes the AVERAGE (18) and NOUN (pound); 4d8 reproduces average 18 \
             (4*(8+1)/2) and 'pound' is a Bash damage noun per helpfile 2304."
                .to_string(),
            "Name originalized per #984; mechanics unchanged.".to_string(),
        ],
    }
}

// -- #1682 armor pilot --------------------------------------------------------
// CF source item: "a pair of blackened leather gloves" (Pine Forest) -- armor,
// Wear hands, Material leather, Level 51, Weight 1 lb 10 oz, Flags giant_only,
// Armor Class Pierce 13 Bash 7 Slash 13 Magic 3 Element 14, +10 armor class,
// +2 damage roll, -5 save vs breath.
pub(crate) fn blackened_leather_gloves_spec() -> ItemSpec {
    ItemSpec {
        schema: RPG_ITEM_SPEC_SCHEMA.to_string(),
        item_id: BLACKENED_LEATHER_GLOVES_ITEM_ID.to_string(),
        display_name: "a pair of soot-blackened reaver's gloves".to_string(),
        keywords: vec!["gloves".to_string(), "pair".to_string()],
        source_area_id: "area.ashen_reach.pine_forest".to_string(),
        source_area_name: "The Pine Forest".to_string(),
        kind_payload: ItemKindPayload::Armor,
        wear: standard(WearSlot::Hands),
        level: 51,
        weight: weight(1, 10),
        value_coin: 1500,
        material: "leather".to_string(),
        flags: vec!["giant_only".to_string()],
        armor: Some(ArmorClass {
            pierce: 13,
            bash: 7,
            slash: 13,
            magic: 3,
            element: 14,
        }),
        weapon: None,
        affects: vec![
            affect(ItemAffectKind::ArmorClass, 10),
            affect(ItemAffectKind::DamageRoll, 2),
            affect(ItemAffectKind::SaveVsBreath, -5),
        ],
        condition: ItemCondition {
            breakage_prone: false,
            repairable: true,
        },
        restring: None,
        identify: full_identify(ItemRevealProperty::ArmorClass),
        source_issue: 1682,
        canon_issues: vec![936, 989, 1682],
        mechanics_source_url: "https://carrionfields.net/itemsearch".to_string(),
        implementation_notes: vec![
            "CF source: 'a pair of blackened leather gloves' (Pine Forest), armor/hands, leather, \
             L51, 1 lb 10 oz, giant_only, AC Pierce 13 Bash 7 Slash 13 Magic 3 Element 14."
                .to_string(),
            "Five-channel AC captured verbatim; affects +10 AC, +2 damage roll, -5 save vs breath."
                .to_string(),
            "Name originalized per #984; mechanics unchanged.".to_string(),
        ],
    }
}

// -- #1683 racial-slot pilot --------------------------------------------------
// The itemsearch Slot filter lists the racial slots (claws/foreclaws/forepaws/
// hindpaws/hooves/horns/tail/wings). This pilot occupies the felar `claws`
// anatomy surface (#946) and is gated to race.felisar.
pub(crate) fn felisar_warclaws_spec() -> ItemSpec {
    ItemSpec {
        schema: RPG_ITEM_SPEC_SCHEMA.to_string(),
        item_id: FELISAR_WARCLAWS_ITEM_ID.to_string(),
        display_name: "a set of honed steel claw-sheaths".to_string(),
        keywords: vec!["sheaths".to_string(), "claws".to_string()],
        source_area_id: "area.ashen_reach.felar_warren".to_string(),
        source_area_name: "The Felar Warren".to_string(),
        kind_payload: ItemKindPayload::Armor,
        wear: WearGating {
            slot: WearSlot::Claws,
            gating_race_ids: vec!["race.felisar".to_string()],
        },
        level: 40,
        weight: weight(0, 12),
        value_coin: 900,
        material: "steel".to_string(),
        flags: Vec::new(),
        armor: Some(ArmorClass {
            pierce: 8,
            bash: 4,
            slash: 8,
            magic: 0,
            element: 2,
        }),
        weapon: None,
        affects: vec![
            affect(ItemAffectKind::DamageRoll, 2),
            affect(ItemAffectKind::HitRoll, 1),
        ],
        condition: ItemCondition {
            breakage_prone: true,
            repairable: true,
        },
        restring: None,
        identify: full_identify(ItemRevealProperty::ArmorClass),
        source_issue: 1683,
        canon_issues: vec![936, 946, 989, 1683],
        mechanics_source_url: "https://carrionfields.net/itemsearch".to_string(),
        implementation_notes: vec![
            "Occupies the CF racial `claws` slot (itemsearch Slot filter) -- felar anatomy (#946)."
                .to_string(),
            "Gated to race.felisar; referential integrity checked against the authored RaceSpec \
             corpus by race_registry (#1683, reuses the #1631 registry pattern)."
                .to_string(),
            "Representative racial-slot demonstrator; mass item transcription is #1688/#1690 scope."
                .to_string(),
        ],
    }
}

// -- #1684 consumable pilots --------------------------------------------------
pub(crate) fn healing_draught_spec() -> ItemSpec {
    consumable(
        HEALING_DRAUGHT_ITEM_ID,
        "a crimson healing draught",
        &["draught", "potion"],
        "glass",
        weight(0, 4),
        ItemKindPayload::Potion(SpellPayload {
            spell_level: 18,
            spell_ability_ids: vec!["spell.cure_serious".to_string()],
        }),
        1684,
        "CF potion object: quaffed for an instant spell payload (helpfile POTION); casts at \
         spell level 18.",
    )
}

pub(crate) fn oak_lightning_staff_spec() -> ItemSpec {
    let mut spec = consumable(
        OAK_STAFF_LIGHTNING_ITEM_ID,
        "a gnarled oaken storm-staff",
        &["staff", "oak"],
        "wood",
        weight(3, 0),
        ItemKindPayload::Staff {
            spell: SpellPayload {
                spell_level: 30,
                spell_ability_ids: vec!["spell.lightning_bolt".to_string()],
            },
            charges: ChargeEconomy {
                charges_total: 12,
                charges_remaining: 12,
            },
        },
        1684,
        "CF staff object: brandished for a room-AoE cast payload with a charge economy \
         (helpfile STAVES); casts lightning bolt at spell level 30, 12 charges.",
    );
    spec.wear = standard(WearSlot::Hold);
    spec
}

pub(crate) fn travellers_waterskin_spec() -> ItemSpec {
    consumable(
        TRAVELLERS_WATERSKIN_ITEM_ID,
        "a well-worn traveller's waterskin",
        &["waterskin", "skin"],
        "leather",
        weight(1, 0),
        ItemKindPayload::Drink {
            liquid: "water".to_string(),
            servings: 10,
            thirst_hours: 4,
        },
        1684,
        "CF drink-container object: quenches thirst (survival #953), refillable at springs and \
         fountains; 10 servings of water.",
    )
}

pub(crate) fn ironbound_strongbox_spec() -> ItemSpec {
    consumable(
        IRONBOUND_STRONGBOX_ITEM_ID,
        "an iron-bound oak strongbox",
        &["strongbox", "box"],
        "iron",
        weight(8, 0),
        ItemKindPayload::Container {
            capacity_weight: 200,
            weight_reduction_percent: 25,
            lockable: true,
        },
        1684,
        "CF container object: holds up to 200 lb with 25% weight reduction, lockable (#941).",
    )
}

// Shared positional fixture builder for the consumable/staff/container specs
// above; each arg is a distinct, clearly-named field of the built ItemSpec, so a
// parameter struct would add indirection without clarifying the test callers.
#[allow(clippy::too_many_arguments)]
fn consumable(
    item_id: &str,
    display_name: &str,
    keywords: &[&str],
    material: &str,
    weight: ItemWeight,
    kind_payload: ItemKindPayload,
    source_issue: u32,
    note: &str,
) -> ItemSpec {
    ItemSpec {
        schema: RPG_ITEM_SPEC_SCHEMA.to_string(),
        item_id: item_id.to_string(),
        display_name: display_name.to_string(),
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
        source_area_id: "area.ashen_reach.galdmere_market".to_string(),
        source_area_name: "Galdmere Market".to_string(),
        kind_payload,
        wear: standard(WearSlot::Take),
        level: 20,
        weight,
        value_coin: 250,
        material: material.to_string(),
        flags: Vec::new(),
        armor: None,
        weapon: None,
        affects: Vec::new(),
        condition: ItemCondition {
            breakage_prone: false,
            repairable: false,
        },
        restring: None,
        identify: IdentifyReveal {
            identify_reveals: vec![
                ItemRevealProperty::Level,
                ItemRevealProperty::Weight,
                ItemRevealProperty::Value,
                ItemRevealProperty::Material,
            ],
            deep_lore_reveals: vec![ItemRevealProperty::SpecialProc],
        },
        source_issue,
        canon_issues: vec![821, 989, source_issue],
        mechanics_source_url: "https://carrionfields.net/helpsearch.php?keywords=object"
            .to_string(),
        implementation_notes: vec![
            note.to_string(),
            "Representative type-taxonomy demonstrator for #1684; mass item transcription is \
             #1688/#1690 scope."
                .to_string(),
        ],
    }
}

fn standard(slot: WearSlot) -> WearGating {
    WearGating {
        slot,
        gating_race_ids: Vec::new(),
    }
}

fn affect(affect: ItemAffectKind, modifier: i32) -> ItemAffect {
    ItemAffect { affect, modifier }
}

fn weight(pounds: u16, ounces: u16) -> ItemWeight {
    ItemWeight { pounds, ounces }
}

fn full_identify(combat_property: ItemRevealProperty) -> IdentifyReveal {
    IdentifyReveal {
        identify_reveals: vec![
            ItemRevealProperty::Level,
            ItemRevealProperty::Weight,
            ItemRevealProperty::Value,
            ItemRevealProperty::Material,
            combat_property,
            ItemRevealProperty::StatAffects,
            ItemRevealProperty::Flags,
        ],
        deep_lore_reveals: vec![
            ItemRevealProperty::SpecialProc,
            ItemRevealProperty::AlignmentRestriction,
        ],
    }
}
