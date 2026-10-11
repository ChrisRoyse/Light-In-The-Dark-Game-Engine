//! Core Carrion Fields item data model (#1682).
//!
//! CF-exact schema transcribed from the CF item database
//! (`carrionfields.net/itemsearch`) and the identify helpfile 430
//! (`carrionfields.net/helpsearch.php?id=430`), captured 2026-07-12. Mirrors the
//! proven `RaceSpec`/`ClassSpec` house pattern: a plain-data spec (this file),
//! a validator + readback (`item_specs.rs`), and an editor-authored content
//! emitter (`item_specs_content.rs`) routed through the shared
//! `persist_and_lower_rpg_authored_content` valve.
//!
//! Names/lore are originalized per #984; every mechanic (5-channel AC, weapon
//! dice, the affect list, materials, flags, the identify reveal split) is
//! factual.

use serde::{Deserialize, Serialize};

use super::item_affects::ItemAffect;
use super::item_kinds::ItemKindPayload;
use super::item_weapons::WeaponProfile;
use super::item_wear_slots::WearGating;

pub const RPG_ITEM_SPEC_SCHEMA: &str = "litd.rpg.item_spec.v1";

// Pilot item ids (real CF itemsearch records; names/lore originalized per #984).
pub const CRUSADER_LONGSWORD_ITEM_ID: &str = "item.weapon.crusader_longsword";
pub const BLACKENED_LEATHER_GLOVES_ITEM_ID: &str = "item.armor.blackened_leather_gloves";
pub const FELISAR_WARCLAWS_ITEM_ID: &str = "item.racial.felisar_warclaws";
pub const HEALING_DRAUGHT_ITEM_ID: &str = "item.potion.healing_draught";
pub const OAK_STAFF_LIGHTNING_ITEM_ID: &str = "item.staff.oak_lightning";
pub const TRAVELLERS_WATERSKIN_ITEM_ID: &str = "item.drink.travellers_waterskin";
pub const IRONBOUND_STRONGBOX_ITEM_ID: &str = "item.container.ironbound_strongbox";

/// Item weight in CF pounds-and-ounces (feeds encumbrance #981/#1679).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemWeight {
    pub pounds: u16,
    pub ounces: u16,
}

impl ItemWeight {
    /// Total weight expressed in ounces (16 oz per pound) for encumbrance math.
    pub const fn total_ounces(self) -> u32 {
        (self.pounds as u32) * 16 + (self.ounces as u32)
    }
}

/// The FIVE-channel armor class model (itemsearch "Amor Class" block:
/// `Pierce / Bash / Slash / Magic / Element`). Lower is better in CF; the
/// damage-type-vs-channel matrix lives in the combat core (#1673).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmorClass {
    pub pierce: i16,
    pub bash: i16,
    pub slash: i16,
    pub magic: i16,
    pub element: i16,
}

/// Item condition: breakage (outfit gear breaks, helpfile 314) and repair hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCondition {
    pub breakage_prone: bool,
    pub repairable: bool,
}

/// Cosmetic restring layer (rename/redescribe without stat change -- the RP
/// reward economy #979). Absent for un-restrung items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRestring {
    pub restrung_name: String,
    pub restrung_description: String,
    pub reward_source: String,
}

/// One property that the identify / lore / appraise reveal model can gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemRevealProperty {
    Level,
    Weight,
    Value,
    Material,
    ArmorClass,
    WeaponDamage,
    StatAffects,
    Flags,
    SpecialProc,
    AlignmentRestriction,
}

impl ItemRevealProperty {
    pub const fn as_id(self) -> &'static str {
        match self {
            ItemRevealProperty::Level => "level",
            ItemRevealProperty::Weight => "weight",
            ItemRevealProperty::Value => "value",
            ItemRevealProperty::Material => "material",
            ItemRevealProperty::ArmorClass => "armor_class",
            ItemRevealProperty::WeaponDamage => "weapon_damage",
            ItemRevealProperty::StatAffects => "stat_affects",
            ItemRevealProperty::Flags => "flags",
            ItemRevealProperty::SpecialProc => "special_proc",
            ItemRevealProperty::AlignmentRestriction => "alignment_restriction",
        }
    }
}

/// The identify / lore reveal split (helpfile 430): stats hidden until
/// identified, and certain abilities beyond identify that only a conjurer's
/// CONTACT OTHER PLANES (deep-lore tier) can surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifyReveal {
    /// Properties the identify spell / lore skill / appraise reveals.
    pub identify_reveals: Vec<ItemRevealProperty>,
    /// Properties beyond identify (conjurer contact-other-planes / deep lore).
    pub deep_lore_reveals: Vec<ItemRevealProperty>,
}

/// The core CF item record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSpec {
    pub schema: String,
    pub item_id: String,
    /// Short description (the name shown in inventory).
    pub display_name: String,
    /// Keywords used to target the item in commands.
    pub keywords: Vec<String>,
    /// Every item knows its home area (ties world issues #1636-#1663).
    pub source_area_id: String,
    pub source_area_name: String,
    /// The kind taxonomy tag + kind-specific payload (#1684).
    pub kind_payload: ItemKindPayload,
    /// Where the item is worn / that it is carry-only (#1683).
    pub wear: WearGating,
    pub level: u16,
    pub weight: ItemWeight,
    /// Value in coin.
    pub value_coin: u32,
    /// Material (itemsearch material vocabulary; validated as a token).
    pub material: String,
    /// CF item flags (glow/hum/magic/evil/anti_good/noremove/...).
    pub flags: Vec<String>,
    /// Five-channel armor class, present for armour.
    pub armor: Option<ArmorClass>,
    /// Weapon profile (class/dice/damage type/noun), present for weapons.
    pub weapon: Option<WeaponProfile>,
    /// Stat-adjusting affects (the full CF affect list).
    pub affects: Vec<ItemAffect>,
    pub condition: ItemCondition,
    pub restring: Option<ItemRestring>,
    pub identify: IdentifyReveal,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub implementation_notes: Vec<String>,
}

/// The FSV readback surface: counts + the deterministic spec hash. Mirrors
/// `RaceSpecReadback` -- an independent projection an assertion can compare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSpecReadback {
    pub schema: String,
    pub item_id: String,
    pub display_name: String,
    pub source_area_id: String,
    pub kind: String,
    pub wear_slot: String,
    pub gating_race_count: usize,
    pub level: u16,
    pub weight_total_ounces: u32,
    pub value_coin: u32,
    pub material: String,
    pub flag_count: usize,
    pub has_armor: bool,
    pub has_weapon: bool,
    pub affect_count: usize,
    pub breakage_prone: bool,
    pub restrung: bool,
    pub identify_reveal_count: usize,
    pub deep_lore_reveal_count: usize,
    pub source_issue: u32,
    pub item_spec_sha256: String,
}
