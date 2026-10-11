//! Carrion Fields weapon model: weapon class, damage type, and dice (#1682).
//!
//! Facts are transcribed, never invented (#984/#959):
//! - The eleven weapon classes are the CF item-database "Weapon Class" filter
//!   (`carrionfields.net/itemsearch`, 2026-07-12): axe, bow, dagger, exotic,
//!   flail, mace, polearm, spear, staff, sword, whip.
//! - The fifteen damage types and their flavour nouns are helpfile 2304
//!   (`carrionfields.net/helpsearch.php?id=2304`, 'DAMAGE TYPE'), grouped
//!   PHYSICAL (bash/pierce/slash), MAGIC (energy/light/holy/negative/poison/
//!   mental) and ELEMENTAL (acid/lightning/fire/cold/water=drowning) plus the
//!   disease type carried by the itemsearch damage-type filter.
//! - Weapon dice are the classic ROM/Merc `<count>d<sides>` roll the identify
//!   spell reports as an average (helpfile 430 "ability to inflict damage"): the
//!   average of NdS is `N * (S + 1) / 2`.

use serde::{Deserialize, Serialize};

use crate::EngineError;

/// The CF weapon-class taxonomy (itemsearch "Weapon Class" filter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponClass {
    Axe,
    Bow,
    Dagger,
    Exotic,
    Flail,
    Mace,
    Polearm,
    Spear,
    Staff,
    Sword,
    Whip,
}

impl WeaponClass {
    pub const ALL: [WeaponClass; 11] = [
        WeaponClass::Axe,
        WeaponClass::Bow,
        WeaponClass::Dagger,
        WeaponClass::Exotic,
        WeaponClass::Flail,
        WeaponClass::Mace,
        WeaponClass::Polearm,
        WeaponClass::Spear,
        WeaponClass::Staff,
        WeaponClass::Sword,
        WeaponClass::Whip,
    ];

    pub const fn as_id(self) -> &'static str {
        match self {
            WeaponClass::Axe => "axe",
            WeaponClass::Bow => "bow",
            WeaponClass::Dagger => "dagger",
            WeaponClass::Exotic => "exotic",
            WeaponClass::Flail => "flail",
            WeaponClass::Mace => "mace",
            WeaponClass::Polearm => "polearm",
            WeaponClass::Spear => "spear",
            WeaponClass::Staff => "staff",
            WeaponClass::Sword => "sword",
            WeaponClass::Whip => "whip",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        WeaponClass::ALL.into_iter().find(|c| c.as_id() == id)
    }
}

/// The CF damage-type taxonomy (itemsearch "Damage Type" filter + helpfile 2304).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Acid,
    Bash,
    Cold,
    Disease,
    Drowning,
    Energy,
    Fire,
    Holy,
    Light,
    Lightning,
    Mental,
    Negative,
    Pierce,
    Poison,
    Slash,
}

impl DamageType {
    pub const ALL: [DamageType; 15] = [
        DamageType::Acid,
        DamageType::Bash,
        DamageType::Cold,
        DamageType::Disease,
        DamageType::Drowning,
        DamageType::Energy,
        DamageType::Fire,
        DamageType::Holy,
        DamageType::Light,
        DamageType::Lightning,
        DamageType::Mental,
        DamageType::Negative,
        DamageType::Pierce,
        DamageType::Poison,
        DamageType::Slash,
    ];

    pub const fn as_id(self) -> &'static str {
        match self {
            DamageType::Acid => "acid",
            DamageType::Bash => "bash",
            DamageType::Cold => "cold",
            DamageType::Disease => "disease",
            DamageType::Drowning => "drowning",
            DamageType::Energy => "energy",
            DamageType::Fire => "fire",
            DamageType::Holy => "holy",
            DamageType::Light => "light",
            DamageType::Lightning => "lightning",
            DamageType::Mental => "mental",
            DamageType::Negative => "negative",
            DamageType::Pierce => "pierce",
            DamageType::Poison => "poison",
            DamageType::Slash => "slash",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        DamageType::ALL.into_iter().find(|t| t.as_id() == id)
    }

    /// Resolve a CF weapon damage NOUN (the parenthesized word the itemsearch
    /// weapon line publishes, e.g. `slice`, `pound`, `poisonous bite`) to its
    /// damage type, per helpfile 2304 ('DAMAGE TYPE'). This is the bridge the
    /// #1688 itemsearch ingestion uses: itemsearch reports the noun, not the
    /// channel, so the noun table is the single factual source for the mapping.
    /// Returns `None` for a noun outside the helpfile-2304 vocabulary (the
    /// loud-fail path -- an unknown noun is an itemized ingestion failure, never
    /// a silent guess).
    pub fn from_damage_noun(noun: &str) -> Option<Self> {
        Some(match noun.trim().to_ascii_lowercase().as_str() {
            // PHYSICAL -- Bash.
            "blast" | "pound" | "crush" | "suction" | "beating" | "slap" | "punch" | "smash"
            | "stinging lash" | "crushing force" | "charge" | "peck" => DamageType::Bash,
            // PHYSICAL -- Pierce.
            "stab" | "bite" | "pierce" | "scratch" | "sting" | "chomp" | "gore" | "thrust" => {
                DamageType::Pierce
            }
            // PHYSICAL -- Slash.
            "slice" | "slash" | "whip" | "claw" | "cleave" | "chop" | "cut" | "rending gale"
            | "flogging" | "hack" | "rend" | "rake" | "tear" => DamageType::Slash,
            // MAGIC -- Energy.
            "magic" | "chaotic blast" | "asphyxiation" | "burst of energy" => DamageType::Energy,
            // MAGIC -- Light.
            "searing light" | "brilliant radiance" | "blinding slash" | "blast of light"
            | "piercing ray" | "searing beam" => DamageType::Light,
            // MAGIC -- Holy.
            "wrath" | "divine power" | "heavenly cut" | "crushing wrath" | "righteous wrath"
            | "holy assault" => DamageType::Holy,
            // MAGIC -- Negative.
            "defilement" | "infernal power" | "black light" | "unholy cut" | "malefic attack"
            | "defiling smash" | "piercing evil" => DamageType::Negative,
            // MAGIC -- Poison.
            "poisonous bite" | "venomous slash" | "venomous cloud" | "blast of poison"
            | "noxious force" | "piercing venoms" => DamageType::Poison,
            // MAGIC -- Mental.
            "mental assault" => DamageType::Mental,
            // ELEMENTAL -- Acid.
            "acidic bite" | "caustic slime" | "corrosive slice" | "wave of acid"
            | "caustic smash" | "acrid stab" | "penetrating acid" | "digestion" => DamageType::Acid,
            // ELEMENTAL -- Lightning.
            "shocking bite" | "electrical discharge" => DamageType::Lightning,
            // ELEMENTAL -- Fire.
            "flaming bite" | "searing cut" | "burn" | "blazing slash" | "fiery blast"
            | "burning thrust" | "arcing flames" | "piercing fire" | "oppressive heat"
            | "raging flames" | "whirling flames" | "molten smash" => DamageType::Fire,
            // ELEMENTAL -- Cold.
            "icy grip" | "frigid chop" | "freezing cut" | "icy blast" | "piercing cold"
            | "frigid smash" | "blast of ice" | "biting cold" => DamageType::Cold,
            // ELEMENTAL -- Water (drowning channel).
            "drowning" | "surge of water" | "slashing wave" | "crashing wave"
            | "blast of water" | "jet of water" | "wave of water" => DamageType::Drowning,
            _ => return None,
        })
    }
}

/// A `<count>d<sides>` weapon damage roll (ROM/Merc lineage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageDice {
    pub count: u16,
    pub sides: u16,
}

impl DamageDice {
    /// The average damage the identify spell reports: `count * (sides + 1) / 2`,
    /// scaled by 10 so the half is preserved without floating point (e.g. 3d4 =>
    /// average 7.5 => `average_milli`... reported here as tenths => 75).
    pub const fn average_tenths(self) -> u32 {
        (self.count as u32) * ((self.sides as u32) + 1) * 5
    }

    /// The minimum roll (`count`) and maximum roll (`count * sides`).
    pub const fn min_roll(self) -> u32 {
        self.count as u32
    }

    pub const fn max_roll(self) -> u32 {
        (self.count as u32) * (self.sides as u32)
    }
}

/// A weapon's full CF profile: class, dice, damage type, and flavour noun.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponProfile {
    pub weapon_class: WeaponClass,
    pub dice: DamageDice,
    pub damage_type: DamageType,
    /// The flavour verb the hit renders as (helpfile 2304 nouns: slice, chop,
    /// pound, gore, ...). Drives hit VFX/SFX flavour.
    pub damage_noun: String,
    /// True for a two-handed weapon (itemsearch renders the class as
    /// `<class> (two-handed)`): it occupies both hands and cannot pair a shield.
    pub two_handed: bool,
}

/// Validate a weapon profile. Fails loud on degenerate dice (a zero count or
/// zero sides can never roll damage) or an empty damage noun.
pub fn validate_weapon_profile(profile: &WeaponProfile) -> Result<(), EngineError> {
    if profile.dice.count == 0 {
        return Err(invalid("weapon.dice.count", "must be >= 1"));
    }
    if profile.dice.sides == 0 {
        return Err(invalid("weapon.dice.sides", "must be >= 1"));
    }
    if profile.damage_noun.trim().is_empty() {
        return Err(invalid("weapon.damage_noun", "must be non-empty"));
    }
    Ok(())
}

fn invalid(field: &str, reason: &str) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason: reason.to_string(),
    }
}
