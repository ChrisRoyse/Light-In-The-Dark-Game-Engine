//! The Carrion Fields item-type taxonomy as data-driven behaviours (#1684).
//!
//! Facts are transcribed, never invented (#984/#959): the type set is the CF
//! item-database "Item Type" filter (weapon/armor/clothing/treasure/instrument)
//! extended with the full CF object-type list from the game -- light plus the
//! consumables (potion/pill/scroll/wand/staff), sustenance (food/drink), and
//! world items (container/key/boat). Each kind carries a kind-specific payload;
//! [`validate_item_kind_payload`] fails LOUD on any kind/payload mismatch (a
//! potion without a spell payload, a wand whose remaining charges exceed its
//! total, a container with zero capacity), so a mistyped item can never persist.

use serde::{Deserialize, Serialize};

use crate::EngineError;

/// The discriminant tag for an item's type (stable snake_case id for payloads
/// and itemsearch-style filtering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Weapon,
    Armor,
    Clothing,
    Treasure,
    Instrument,
    Light,
    Potion,
    Pill,
    Scroll,
    Wand,
    Staff,
    Food,
    Drink,
    Container,
    Key,
    Boat,
}

impl ItemKind {
    pub const fn as_id(self) -> &'static str {
        match self {
            ItemKind::Weapon => "weapon",
            ItemKind::Armor => "armor",
            ItemKind::Clothing => "clothing",
            ItemKind::Treasure => "treasure",
            ItemKind::Instrument => "instrument",
            ItemKind::Light => "light",
            ItemKind::Potion => "potion",
            ItemKind::Pill => "pill",
            ItemKind::Scroll => "scroll",
            ItemKind::Wand => "wand",
            ItemKind::Staff => "staff",
            ItemKind::Food => "food",
            ItemKind::Drink => "drink",
            ItemKind::Container => "container",
            ItemKind::Key => "key",
            ItemKind::Boat => "boat",
        }
    }
}

/// A spell payload carried by consumables (potion/pill/scroll) and cast items
/// (wand/staff): the level the spells cast at, and the ability ids invoked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellPayload {
    pub spell_level: u16,
    pub spell_ability_ids: Vec<String>,
}

/// The charge economy for a targeted (wand) or room-AoE (staff) cast item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChargeEconomy {
    pub charges_total: u16,
    pub charges_remaining: u16,
}

/// The kind-specific payload. The tag mirrors [`ItemKind`]; [`kind`] recovers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
pub enum ItemKindPayload {
    /// Weapon dice live in [`super::WeaponProfile`] on the spec; the spec-level
    /// validator cross-checks that a `Weapon` payload has a weapon profile.
    Weapon,
    /// Armour carries the 5-channel AC on the spec; cross-checked at spec level.
    Armor,
    /// A stat item WITHOUT armour class (CF clothing).
    Clothing,
    /// Jewellery / wearable treasure.
    Treasure,
    /// Bard power modifier (#1024).
    Instrument { power_modifier: i16 },
    /// A worn/held light source; `fuel_hours` is `-1` for an eternal light.
    Light { fuel_hours: i32 },
    /// Quaffed instant spell payload.
    Potion(SpellPayload),
    /// Eaten spell payload.
    Pill(SpellPayload),
    /// Recited cast payload (needs the scrolls skill).
    Scroll(SpellPayload),
    /// Zapped, targeted, charge-limited.
    Wand {
        spell: SpellPayload,
        charges: ChargeEconomy,
    },
    /// Brandished, room AoE, charge-limited.
    Staff {
        spell: SpellPayload,
        charges: ChargeEconomy,
    },
    /// Sustenance for hunger (survival #953).
    Food { nourishment_hours: u16 },
    /// Thirst-quenching drink container (refillable at springs/fountains).
    Drink {
        liquid: String,
        servings: u16,
        thirst_hours: u16,
    },
    /// Container: carry capacity, weight reduction, optional lock (#941).
    Container {
        capacity_weight: u16,
        weight_reduction_percent: u16,
        lockable: bool,
    },
    /// A key that opens a specific lock (#941).
    Key { unlocks_ref: String },
    /// A boat for water travel (#1655).
    Boat,
}

impl ItemKindPayload {
    /// The [`ItemKind`] discriminant this payload represents.
    pub const fn kind(&self) -> ItemKind {
        match self {
            ItemKindPayload::Weapon => ItemKind::Weapon,
            ItemKindPayload::Armor => ItemKind::Armor,
            ItemKindPayload::Clothing => ItemKind::Clothing,
            ItemKindPayload::Treasure => ItemKind::Treasure,
            ItemKindPayload::Instrument { .. } => ItemKind::Instrument,
            ItemKindPayload::Light { .. } => ItemKind::Light,
            ItemKindPayload::Potion(_) => ItemKind::Potion,
            ItemKindPayload::Pill(_) => ItemKind::Pill,
            ItemKindPayload::Scroll(_) => ItemKind::Scroll,
            ItemKindPayload::Wand { .. } => ItemKind::Wand,
            ItemKindPayload::Staff { .. } => ItemKind::Staff,
            ItemKindPayload::Food { .. } => ItemKind::Food,
            ItemKindPayload::Drink { .. } => ItemKind::Drink,
            ItemKindPayload::Container { .. } => ItemKind::Container,
            ItemKindPayload::Key { .. } => ItemKind::Key,
            ItemKindPayload::Boat => ItemKind::Boat,
        }
    }
}

/// Validate the internal soundness of a kind payload. Fails LOUD (structured
/// `LITD-RPG-STATS-INVALID-CONFIG`) on any kind/payload mismatch. Cross-field
/// checks that need the whole spec (a `Weapon` payload requires a weapon
/// profile, an `Armor` payload requires armour class) are performed by the spec
/// validator; this function guards the payload's own invariants.
pub fn validate_item_kind_payload(payload: &ItemKindPayload) -> Result<(), EngineError> {
    match payload {
        ItemKindPayload::Potion(spell)
        | ItemKindPayload::Pill(spell)
        | ItemKindPayload::Scroll(spell) => validate_spell_payload(payload.kind().as_id(), spell),
        ItemKindPayload::Wand { spell, charges } | ItemKindPayload::Staff { spell, charges } => {
            validate_spell_payload(payload.kind().as_id(), spell)?;
            validate_charges(payload.kind().as_id(), charges)
        }
        ItemKindPayload::Food { nourishment_hours } => {
            if *nourishment_hours == 0 {
                return Err(invalid(
                    "food.nourishment_hours",
                    "must be >= 1".to_string(),
                ));
            }
            Ok(())
        }
        ItemKindPayload::Drink {
            liquid,
            servings,
            thirst_hours,
        } => {
            if liquid.trim().is_empty() {
                return Err(invalid("drink.liquid", "must be non-empty".to_string()));
            }
            if *servings == 0 {
                return Err(invalid("drink.servings", "must be >= 1".to_string()));
            }
            if *thirst_hours == 0 {
                return Err(invalid("drink.thirst_hours", "must be >= 1".to_string()));
            }
            Ok(())
        }
        ItemKindPayload::Container {
            capacity_weight,
            weight_reduction_percent,
            ..
        } => {
            if *capacity_weight == 0 {
                return Err(invalid(
                    "container.capacity_weight",
                    "must be >= 1".to_string(),
                ));
            }
            if *weight_reduction_percent > 100 {
                return Err(invalid(
                    "container.weight_reduction_percent",
                    "must be <= 100".to_string(),
                ));
            }
            Ok(())
        }
        ItemKindPayload::Key { unlocks_ref } => {
            if unlocks_ref.trim().is_empty() {
                return Err(invalid("key.unlocks_ref", "must be non-empty".to_string()));
            }
            Ok(())
        }
        ItemKindPayload::Light { fuel_hours } => {
            if *fuel_hours < -1 || *fuel_hours == 0 {
                return Err(invalid(
                    "light.fuel_hours",
                    "must be -1 (eternal) or a positive hour count".to_string(),
                ));
            }
            Ok(())
        }
        ItemKindPayload::Weapon
        | ItemKindPayload::Armor
        | ItemKindPayload::Clothing
        | ItemKindPayload::Treasure
        | ItemKindPayload::Instrument { .. }
        | ItemKindPayload::Boat => Ok(()),
    }
}

fn validate_spell_payload(kind: &str, spell: &SpellPayload) -> Result<(), EngineError> {
    if spell.spell_level == 0 {
        return Err(invalid(
            &format!("{kind}.spell_level"),
            "cast items must cast at level >= 1".to_string(),
        ));
    }
    if spell.spell_ability_ids.is_empty() {
        return Err(invalid(
            &format!("{kind}.spell_ability_ids"),
            "must invoke at least one spell".to_string(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for ability_id in &spell.spell_ability_ids {
        if ability_id.trim().is_empty() {
            return Err(invalid(
                &format!("{kind}.spell_ability_ids"),
                "spell ability id must be non-empty".to_string(),
            ));
        }
        if !seen.insert(ability_id.as_str()) {
            return Err(invalid(
                &format!("{kind}.spell_ability_ids"),
                format!("duplicate spell ability id {ability_id}"),
            ));
        }
    }
    Ok(())
}

fn validate_charges(kind: &str, charges: &ChargeEconomy) -> Result<(), EngineError> {
    if charges.charges_total == 0 {
        return Err(invalid(
            &format!("{kind}.charges_total"),
            "a charged item must have at least one total charge".to_string(),
        ));
    }
    if charges.charges_remaining > charges.charges_total {
        return Err(invalid(
            &format!("{kind}.charges_remaining"),
            format!(
                "remaining {} exceeds total {}",
                charges.charges_remaining, charges.charges_total
            ),
        ));
    }
    Ok(())
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
