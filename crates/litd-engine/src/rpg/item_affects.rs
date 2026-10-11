//! The full Carrion Fields item-affect vocabulary (#1682).
//!
//! Facts are transcribed, never invented (#984/#959): the affect set is the
//! authoritative "Affects" filter list published on the CF item database
//! (`carrionfields.net/itemsearch`, captured 2026-07-12) -- the six primary
//! attributes, the hp/mana/move pools plus their regeneration channels, hit and
//! damage roll, armor class, the four saves, the fourteen resistance channels,
//! and the AGE (aging system #952) and MORALE affects. An item's `power to
//! adjust statistics` is exactly what the identify spell reveals (helpfile 430).
//!
//! This vocabulary is decoupled from the nine combat damage channels wired into
//! the derived-stat table (mirrors the #1631 `RaceAffinity` decision): an item
//! affect can name AGE or MORALE, which no combat channel expresses.

use serde::{Deserialize, Serialize};

use crate::EngineError;

/// One controlled affect channel an item may modify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemAffectKind {
    // Six primary attributes.
    Strength,
    Intelligence,
    Wisdom,
    Dexterity,
    Constitution,
    Charisma,
    // Resource pools + regeneration channels.
    Hp,
    Mana,
    Moves,
    HpRegeneration,
    ManaRegeneration,
    MovementRegeneration,
    // Combat rolls + armor class.
    HitRoll,
    DamageRoll,
    ArmorClass,
    // Saves.
    SaveVsBreath,
    SaveVsMental,
    SaveVsParalysis,
    SaveVsSpell,
    // Fourteen resistance channels.
    AcidResistance,
    BashResistance,
    ColdResistance,
    DiseaseResistance,
    DrowningResistance,
    EnergyResistance,
    FireResistance,
    HolyResistance,
    LightResistance,
    LightningResistance,
    MentalResistance,
    NegativeResistance,
    PierceResistance,
    PoisonResistance,
    SlashResistance,
    // Meta affects with no combat-channel slot.
    Age,
    Morale,
}

impl ItemAffectKind {
    /// Every affect in the vocabulary, in declaration order.
    pub const ALL: [ItemAffectKind; 36] = [
        ItemAffectKind::Strength,
        ItemAffectKind::Intelligence,
        ItemAffectKind::Wisdom,
        ItemAffectKind::Dexterity,
        ItemAffectKind::Constitution,
        ItemAffectKind::Charisma,
        ItemAffectKind::Hp,
        ItemAffectKind::Mana,
        ItemAffectKind::Moves,
        ItemAffectKind::HpRegeneration,
        ItemAffectKind::ManaRegeneration,
        ItemAffectKind::MovementRegeneration,
        ItemAffectKind::HitRoll,
        ItemAffectKind::DamageRoll,
        ItemAffectKind::ArmorClass,
        ItemAffectKind::SaveVsBreath,
        ItemAffectKind::SaveVsMental,
        ItemAffectKind::SaveVsParalysis,
        ItemAffectKind::SaveVsSpell,
        ItemAffectKind::AcidResistance,
        ItemAffectKind::BashResistance,
        ItemAffectKind::ColdResistance,
        ItemAffectKind::DiseaseResistance,
        ItemAffectKind::DrowningResistance,
        ItemAffectKind::EnergyResistance,
        ItemAffectKind::FireResistance,
        ItemAffectKind::HolyResistance,
        ItemAffectKind::LightResistance,
        ItemAffectKind::LightningResistance,
        ItemAffectKind::MentalResistance,
        ItemAffectKind::NegativeResistance,
        ItemAffectKind::PierceResistance,
        ItemAffectKind::PoisonResistance,
        ItemAffectKind::SlashResistance,
        ItemAffectKind::Age,
        ItemAffectKind::Morale,
    ];

    /// The canonical snake_case id stored in serialized payloads.
    pub const fn as_id(self) -> &'static str {
        match self {
            ItemAffectKind::Strength => "strength",
            ItemAffectKind::Intelligence => "intelligence",
            ItemAffectKind::Wisdom => "wisdom",
            ItemAffectKind::Dexterity => "dexterity",
            ItemAffectKind::Constitution => "constitution",
            ItemAffectKind::Charisma => "charisma",
            ItemAffectKind::Hp => "hp",
            ItemAffectKind::Mana => "mana",
            ItemAffectKind::Moves => "moves",
            ItemAffectKind::HpRegeneration => "hp_regeneration",
            ItemAffectKind::ManaRegeneration => "mana_regeneration",
            ItemAffectKind::MovementRegeneration => "movement_regeneration",
            ItemAffectKind::HitRoll => "hit_roll",
            ItemAffectKind::DamageRoll => "damage_roll",
            ItemAffectKind::ArmorClass => "armor_class",
            ItemAffectKind::SaveVsBreath => "save_vs_breath",
            ItemAffectKind::SaveVsMental => "save_vs_mental",
            ItemAffectKind::SaveVsParalysis => "save_vs_paralysis",
            ItemAffectKind::SaveVsSpell => "save_vs_spell",
            ItemAffectKind::AcidResistance => "acid_resistance",
            ItemAffectKind::BashResistance => "bash_resistance",
            ItemAffectKind::ColdResistance => "cold_resistance",
            ItemAffectKind::DiseaseResistance => "disease_resistance",
            ItemAffectKind::DrowningResistance => "drowning_resistance",
            ItemAffectKind::EnergyResistance => "energy_resistance",
            ItemAffectKind::FireResistance => "fire_resistance",
            ItemAffectKind::HolyResistance => "holy_resistance",
            ItemAffectKind::LightResistance => "light_resistance",
            ItemAffectKind::LightningResistance => "lightning_resistance",
            ItemAffectKind::MentalResistance => "mental_resistance",
            ItemAffectKind::NegativeResistance => "negative_resistance",
            ItemAffectKind::PierceResistance => "pierce_resistance",
            ItemAffectKind::PoisonResistance => "poison_resistance",
            ItemAffectKind::SlashResistance => "slash_resistance",
            ItemAffectKind::Age => "age",
            ItemAffectKind::Morale => "morale",
        }
    }

    /// Resolve a stored id back to its affect, or `None` if unknown.
    pub fn from_id(id: &str) -> Option<Self> {
        ItemAffectKind::ALL.into_iter().find(|a| a.as_id() == id)
    }
}

/// One authored affect entry on an item: which channel and by how much.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemAffect {
    pub affect: ItemAffectKind,
    pub modifier: i32,
}

/// Validate a list of item affects. Fails loud (structured
/// `LITD-RPG-STATS-INVALID-CONFIG`) on a zero-magnitude affect (a no-op affect
/// is authoring noise) or an out-of-range modifier (guards against transcription
/// slips such as a stray extra digit).
pub fn validate_item_affects(field: &str, affects: &[ItemAffect]) -> Result<(), EngineError> {
    for affect in affects {
        if affect.modifier == 0 {
            return Err(invalid(
                field,
                format!(
                    "affect {} has a zero modifier (no-op)",
                    affect.affect.as_id()
                ),
            ));
        }
        if !(-1000..=1000).contains(&affect.modifier) {
            return Err(invalid(
                field,
                format!(
                    "affect {} modifier {} is outside -1000..=1000",
                    affect.affect.as_id(),
                    affect.modifier
                ),
            ));
        }
    }
    Ok(())
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
