//! #1788 phase-1 faction content data tables (asymmetric unit/structure/hero/ability/item/
//! tech/upgrade rows). Kept separate from the authoring logic to respect the <500-line ratchet.

use crate::factions::{EMBERHOLD, FactionSpec};

// (suffix, name, hp, move_speed, gold, lumber, food, damage_base)
pub(crate) type UnitRow = (&'static str, &'static str, u32, u32, u32, u32, u32, u32);
// (suffix, name, hp, gold, footprint, food_provided, &[train unit suffixes])
pub(crate) type StructRow = (
    &'static str,
    &'static str,
    u32,
    u32,
    u32,
    u32,
    &'static [&'static str],
);
// (suffix, name, primary_attr, hp, move_speed, &[3 ability suffixes])
pub(crate) type HeroRow = (
    &'static str,
    &'static str,
    &'static str,
    u32,
    u32,
    &'static [&'static str],
);
// (suffix, name)
pub(crate) type AbilityRow = (&'static str, &'static str);
// (suffix, name, class, gold, bonus_key, bonus_value)
pub(crate) type ItemRow = (
    &'static str,
    &'static str,
    &'static str,
    u32,
    &'static str,
    u32,
);
// (suffix, name, tier, &[require tech suffixes])
pub(crate) type TechRow = (&'static str, &'static str, u32, &'static [&'static str]);
// (suffix, name, effect_kind, &[level deltas])
pub(crate) type UpgradeRow = (&'static str, &'static str, &'static str, &'static [i32]);

const EMBERHOLD_UNITS: &[UnitRow] = &[
    ("delver", "Ashen Delver", 220, 200, 65, 0, 1, 2),
    ("shieldsworn", "Shieldsworn", 520, 180, 135, 10, 2, 14),
    ("halberdier", "Halberdier", 480, 190, 150, 10, 2, 17),
    ("marksman", "Ember Marksman", 340, 200, 145, 20, 2, 15),
    ("chaplain", "Field Chaplain", 360, 190, 160, 30, 2, 9),
    ("sentinel", "Ember Sentinel", 560, 170, 170, 10, 3, 16),
    ("bastion", "Bastion Golem", 900, 150, 245, 60, 4, 30),
    ("ballista", "Siege Ballista", 420, 160, 230, 40, 4, 38),
];
const EMBERHOLD_STRUCTS: &[StructRow] = &[
    (
        "keep",
        "Emberhold Keep",
        1500,
        200,
        3,
        20,
        &["delver", "shieldsworn"],
    ),
    ("granary", "Emberhold Granary", 600, 120, 2, 12, &[]),
    (
        "barracks",
        "Emberhold Barracks",
        900,
        160,
        3,
        0,
        &["halberdier", "marksman", "sentinel"],
    ),
    ("forge", "Emberhold Forge", 700, 150, 2, 0, &[]),
    ("bulwark", "Emberhold Bulwark", 800, 110, 2, 0, &[]),
    (
        "citadel",
        "Emberhold Citadel",
        1200,
        220,
        3,
        0,
        &["bastion", "ballista", "chaplain"],
    ),
];
const EMBERHOLD_HEROES: &[HeroRow] = &[
    (
        "doran",
        "Doran Ashheart",
        "STR",
        900,
        190,
        &["emberbrand", "bulwarkstance", "moltenwrath"],
    ),
    (
        "miren",
        "Sister Miren",
        "INT",
        620,
        200,
        &["wardinglight", "cindermend", "radiantbastion"],
    ),
];
const EMBERHOLD_ABILITIES: &[AbilityRow] = &[
    ("emberbrand", "Emberbrand Strike"),
    ("bulwarkstance", "Bulwark Stance"),
    ("moltenwrath", "Molten Wrath"),
    ("wardinglight", "Warding Light"),
    ("cindermend", "Cinder Mend"),
    ("radiantbastion", "Radiant Bastion"),
];
const EMBERHOLD_ITEMS: &[ItemRow] = &[
    (
        "ashsteel",
        "Ashsteel Blade",
        "Charged",
        250,
        "bonus.attack",
        8,
    ),
    (
        "charm",
        "Coal-Warden Charm",
        "Permanent",
        180,
        "bonus.hit_points",
        150,
    ),
];
const EMBERHOLD_TECH: &[TechRow] = &[
    ("stonecraft", "Stonecraft", 1, &[]),
    ("drill", "Field Drill", 1, &[]),
    ("forgemastery", "Forge Mastery", 2, &["stonecraft"]),
    ("wardoctrine", "War Doctrine", 2, &["drill"]),
    (
        "ascendancy",
        "Ashen Ascendancy",
        2,
        &["forgemastery", "wardoctrine"],
    ),
];
const EMBERHOLD_UPGRADES: &[UpgradeRow] = &[
    ("plating", "Ember Plating", "armor", &[1, 2, 3]),
    ("emberedge", "Ember Edge", "attack", &[2, 4, 6]),
];

const VEILSPUN_UNITS: &[UnitRow] = &[
    ("gleaner", "Veil Gleaner", 180, 230, 60, 0, 1, 2),
    ("duskblade", "Duskblade", 360, 260, 120, 10, 2, 16),
    ("nightstalker", "Nightstalker", 330, 270, 130, 10, 2, 18),
    ("thorncaster", "Thorncaster", 280, 250, 140, 20, 2, 17),
    ("weaver", "Veil Weaver", 260, 250, 155, 30, 2, 8),
    ("skirmisher", "Umbral Skirmisher", 300, 280, 125, 10, 2, 15),
    ("shadowmaw", "Shadowmaw Beast", 620, 300, 235, 60, 4, 34),
    ("gloomwing", "Gloomwing Raider", 340, 320, 225, 40, 3, 28),
];
const VEILSPUN_STRUCTS: &[StructRow] = &[
    (
        "sanctum",
        "Veilspun Sanctum",
        1200,
        190,
        3,
        20,
        &["gleaner", "duskblade"],
    ),
    ("waystone", "Veilspun Waystone", 480, 110, 2, 12, &[]),
    (
        "covenant",
        "Veilspun Covenant",
        720,
        150,
        3,
        0,
        &["nightstalker", "thorncaster", "skirmisher"],
    ),
    ("loom", "Veilspun Loom", 560, 140, 2, 0, &[]),
    ("bramble", "Veilspun Bramble", 640, 100, 2, 0, &[]),
    (
        "nexus",
        "Veilspun Nexus",
        960,
        210,
        3,
        0,
        &["shadowmaw", "gloomwing", "weaver"],
    ),
];
const VEILSPUN_HEROES: &[HeroRow] = &[
    (
        "yvenne",
        "Yvenne Duskblade",
        "AGI",
        680,
        270,
        &["shadowstep", "venomkiss", "umbralflurry"],
    ),
    (
        "caul",
        "Caul the Whisperer",
        "INT",
        600,
        240,
        &["gloombolt", "veilofnight", "whispereddoom"],
    ),
];
const VEILSPUN_ABILITIES: &[AbilityRow] = &[
    ("shadowstep", "Shadowstep"),
    ("venomkiss", "Venom Kiss"),
    ("umbralflurry", "Umbral Flurry"),
    ("gloombolt", "Gloom Bolt"),
    ("veilofnight", "Veil of Night"),
    ("whispereddoom", "Whispered Doom"),
];
const VEILSPUN_ITEMS: &[ItemRow] = &[
    (
        "cloak",
        "Nightsilk Cloak",
        "Permanent",
        200,
        "bonus.agility",
        6,
    ),
    ("vial", "Gloom Vial", "Charged", 160, "bonus.mana", 120),
];
const VEILSPUN_TECH: &[TechRow] = &[
    ("shrouding", "Shrouding", 1, &[]),
    ("swiftness", "Swift Ways", 1, &[]),
    ("hexcraft", "Hexcraft", 2, &["shrouding"]),
    ("nightpact", "Night Pact", 2, &["swiftness"]),
    ("eclipse", "Eclipse Rite", 2, &["hexcraft", "nightpact"]),
];
const VEILSPUN_UPGRADES: &[UpgradeRow] = &[
    ("shadowweave", "Shadow Weave", "armor", &[1, 2, 3]),
    ("venomcoat", "Venom Coat", "attack", &[2, 4, 6]),
];

/// All seven per-faction record tables (units, structures, heroes, abilities, items, tech,
/// upgrades) borrowed for the whole program lifetime.
pub(crate) type FactionTables = (
    &'static [UnitRow],
    &'static [StructRow],
    &'static [HeroRow],
    &'static [AbilityRow],
    &'static [ItemRow],
    &'static [TechRow],
    &'static [UpgradeRow],
);

pub(crate) fn tables(f: &FactionSpec) -> FactionTables {
    if f.key == EMBERHOLD.key {
        (
            EMBERHOLD_UNITS,
            EMBERHOLD_STRUCTS,
            EMBERHOLD_HEROES,
            EMBERHOLD_ABILITIES,
            EMBERHOLD_ITEMS,
            EMBERHOLD_TECH,
            EMBERHOLD_UPGRADES,
        )
    } else {
        (
            VEILSPUN_UNITS,
            VEILSPUN_STRUCTS,
            VEILSPUN_HEROES,
            VEILSPUN_ABILITIES,
            VEILSPUN_ITEMS,
            VEILSPUN_TECH,
            VEILSPUN_UPGRADES,
        )
    }
}
