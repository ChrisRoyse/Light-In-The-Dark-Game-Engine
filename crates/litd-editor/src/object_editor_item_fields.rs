//! WC3-style RTS item record field set for #821, lowered through the #829 object-data valve.
//!
//! This is the EDITOR authoring surface for World-Editor Item tabs (class, gold/lumber cost,
//! stock, charges, inventory flags, stat bonuses, item-ability refs, pickup/drop, art). It is
//! deliberately DISTINCT from the engine CF-RPG loot model in `litd_engine::rpg::item_flags`
//! / itemspec (#1685/#1686/#1687): that model is CF wear-locations/materials/limited-item
//! economy for the RPG track; this is the WC3 RTS object-editor record. The two taxonomies do
//! not overlap (WC3 inventory flags droppable/sellable/pawnable/perishable vs CF item flags),
//! so reusing the engine enum here would misrepresent the data -- they are not duplicated,
//! they are different things.
//!
//! Item-ability refs contract (#821): the abilities editor track has not merged as authorable
//! ability *records* wired to this document, so an item-ability ref is modelled as a validated
//! identifier string (shape-checked + deduped at persist AND lower time). Refs are resolved
//! best-effort against `Ability`-kind objects in the same document and the resolution count is
//! surfaced in the lower_item tool output, but an unresolved ref is NOT a lowering failure --
//! the abilities may live in a separate track/document. (Item refs in recipes/shops/drop-tables
//! DO hard-fail on dangling refs; see #822 -- those are same-track economy records.)

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::EditorObject;

use super::field_extract::{
    check_enum, check_flags_subset, check_id_list, check_range, enum_field, id_list_field,
    opt_flag_list_field, string_field, u32_field,
};

/// Allowed WC3 item classes (issue #821 taxonomy).
pub const ITEM_CLASSES: &[&str] = &[
    "Permanent",
    "Charged",
    "PowerUp",
    "Artifact",
    "Purchasable",
    "Campaign",
];
/// Allowed WC3 inventory flags (a bounded subset; an item may carry any/none of these).
pub const ITEM_INVENTORY_FLAGS: &[&str] = &["droppable", "sellable", "pawnable", "perishable"];
/// Allowed drop-on-death behaviours for pickup/drop metadata.
pub const ITEM_DROP_ON_DEATH: &[&str] = &["drop", "destroy", "keep"];

// Range bounds shared by persist-time (`mcp_object_tools_validation`) and lowering-time
// validation so the two can never drift (single source of truth, mirrored from the enum/flag
// discipline the #818 unit field set established).
pub const ITEM_COST_MAX: u32 = 1_000_000;
pub const ITEM_STOCK_MAX: u32 = 100;
pub const ITEM_INTERVAL_MAX: u32 = 100_000;
pub const ITEM_CHARGES_MAX: u32 = 1_000;
pub const ITEM_BONUS_ATTACK_MAX: u32 = 100_000;
pub const ITEM_BONUS_ARMOR_MAX: u32 = 1_000;
pub const ITEM_BONUS_HP_MAX: u32 = 1_000_000;
pub const ITEM_BONUS_MANA_MAX: u32 = 1_000_000;
pub const ITEM_BONUS_ATTR_MAX: u32 = 1_000;
pub const ITEM_PICKUP_RANGE_MAX: u32 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemWc3Fields {
    pub classification: ItemClassification,
    pub economy: ItemEconomy,
    pub stock: ItemStock,
    pub charges: ItemCharges,
    pub bonuses: ItemBonuses,
    pub abilities: ItemAbilities,
    pub pickup: ItemPickupDrop,
    pub art: ItemArt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemClassification {
    pub class: String,
    pub inventory_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemEconomy {
    pub gold_cost: u32,
    pub lumber_cost: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStock {
    pub stock_max: u32,
    pub replenish_interval_secs: u32,
    pub start_delay_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCharges {
    pub initial: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemBonuses {
    pub attack: u32,
    pub armor: u32,
    pub hit_points: u32,
    pub mana: u32,
    pub strength: u32,
    pub agility: u32,
    pub intelligence: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemAbilities {
    pub active: Vec<String>,
    pub passive: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPickupDrop {
    pub pickup_range: u32,
    pub drop_on_death: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemArt {
    pub icon: String,
    pub model: String,
}

/// The overrideable WC3 item property keys (used by tests/FSV to exercise the full set).
pub const ITEM_PROPERTY_KEYS: &[&str] = &[
    "item.class",
    "item.inventory_flags",
    "economy.gold_cost",
    "economy.lumber_cost",
    "stock.stock_max",
    "stock.replenish_interval_secs",
    "stock.start_delay_secs",
    "charges.initial",
    "charges.max",
    "bonus.attack",
    "bonus.armor",
    "bonus.hit_points",
    "bonus.mana",
    "bonus.strength",
    "bonus.agility",
    "bonus.intelligence",
    "ability.active",
    "ability.passive",
    "pickup.range",
    "pickup.drop_on_death",
    "art.icon",
    "art.model",
];

/// Deterministically extract the full WC3 item field set from an authored item object.
/// Absent optional fields resolve to WC3-authentic defaults; present fields are
/// range/enum/flag/id validated so a lowering can never emit an out-of-band artifact.
pub(crate) fn build_item_fields(object: &EditorObject) -> Result<ItemWc3Fields, EngineError> {
    let charges = ItemCharges {
        initial: u32_field(object, "charges.initial", 0, ITEM_CHARGES_MAX, 0)?,
        max: u32_field(object, "charges.max", 0, ITEM_CHARGES_MAX, 0)?,
    };
    if charges.initial > charges.max {
        return Err(super::validation::object_editor_error(
            "charges",
            format!(
                "initial charges {} cannot exceed max charges {}",
                charges.initial, charges.max
            ),
        ));
    }
    Ok(ItemWc3Fields {
        classification: ItemClassification {
            class: enum_field(object, "item.class", ITEM_CLASSES, "Purchasable")?,
            inventory_flags: opt_flag_list_field(
                object,
                "item.inventory_flags",
                ITEM_INVENTORY_FLAGS,
            )?,
        },
        economy: ItemEconomy {
            gold_cost: u32_field(object, "economy.gold_cost", 0, ITEM_COST_MAX, 0)?,
            lumber_cost: u32_field(object, "economy.lumber_cost", 0, ITEM_COST_MAX, 0)?,
        },
        stock: ItemStock {
            stock_max: u32_field(object, "stock.stock_max", 0, ITEM_STOCK_MAX, 1)?,
            replenish_interval_secs: u32_field(
                object,
                "stock.replenish_interval_secs",
                0,
                ITEM_INTERVAL_MAX,
                0,
            )?,
            start_delay_secs: u32_field(object, "stock.start_delay_secs", 0, ITEM_INTERVAL_MAX, 0)?,
        },
        charges,
        bonuses: ItemBonuses {
            attack: u32_field(object, "bonus.attack", 0, ITEM_BONUS_ATTACK_MAX, 0)?,
            armor: u32_field(object, "bonus.armor", 0, ITEM_BONUS_ARMOR_MAX, 0)?,
            hit_points: u32_field(object, "bonus.hit_points", 0, ITEM_BONUS_HP_MAX, 0)?,
            mana: u32_field(object, "bonus.mana", 0, ITEM_BONUS_MANA_MAX, 0)?,
            strength: u32_field(object, "bonus.strength", 0, ITEM_BONUS_ATTR_MAX, 0)?,
            agility: u32_field(object, "bonus.agility", 0, ITEM_BONUS_ATTR_MAX, 0)?,
            intelligence: u32_field(object, "bonus.intelligence", 0, ITEM_BONUS_ATTR_MAX, 0)?,
        },
        abilities: ItemAbilities {
            active: id_list_field(object, "ability.active")?,
            passive: id_list_field(object, "ability.passive")?,
        },
        pickup: ItemPickupDrop {
            pickup_range: u32_field(object, "pickup.range", 0, ITEM_PICKUP_RANGE_MAX, 0)?,
            drop_on_death: enum_field(object, "pickup.drop_on_death", ITEM_DROP_ON_DEATH, "drop")?,
        },
        art: ItemArt {
            icon: string_field(object, "art.icon", ""),
            model: string_field(object, "art.model", ""),
        },
    })
}

/// Validate that a lowered `ItemWc3Fields` still satisfies every range/enum/flag/id bound.
/// Called on TOML readback so a corrupted or hand-edited artifact fails loud.
pub(crate) fn validate_item_fields(fields: &ItemWc3Fields) -> Result<(), EngineError> {
    check_enum("item.class", &fields.classification.class, ITEM_CLASSES)?;
    check_flags_subset(
        "item.inventory_flags",
        &fields.classification.inventory_flags,
        ITEM_INVENTORY_FLAGS,
    )?;
    check_range(
        "economy.gold_cost",
        fields.economy.gold_cost,
        0,
        ITEM_COST_MAX,
    )?;
    check_range(
        "economy.lumber_cost",
        fields.economy.lumber_cost,
        0,
        ITEM_COST_MAX,
    )?;
    check_range("stock.stock_max", fields.stock.stock_max, 0, ITEM_STOCK_MAX)?;
    check_range(
        "stock.replenish_interval_secs",
        fields.stock.replenish_interval_secs,
        0,
        ITEM_INTERVAL_MAX,
    )?;
    check_range(
        "stock.start_delay_secs",
        fields.stock.start_delay_secs,
        0,
        ITEM_INTERVAL_MAX,
    )?;
    check_range(
        "charges.initial",
        fields.charges.initial,
        0,
        ITEM_CHARGES_MAX,
    )?;
    check_range("charges.max", fields.charges.max, 0, ITEM_CHARGES_MAX)?;
    if fields.charges.initial > fields.charges.max {
        return Err(super::validation::object_editor_error(
            "charges",
            "initial charges cannot exceed max charges",
        ));
    }
    check_range(
        "bonus.attack",
        fields.bonuses.attack,
        0,
        ITEM_BONUS_ATTACK_MAX,
    )?;
    check_range("bonus.armor", fields.bonuses.armor, 0, ITEM_BONUS_ARMOR_MAX)?;
    check_range(
        "bonus.hit_points",
        fields.bonuses.hit_points,
        0,
        ITEM_BONUS_HP_MAX,
    )?;
    check_range("bonus.mana", fields.bonuses.mana, 0, ITEM_BONUS_MANA_MAX)?;
    check_range(
        "bonus.strength",
        fields.bonuses.strength,
        0,
        ITEM_BONUS_ATTR_MAX,
    )?;
    check_range(
        "bonus.agility",
        fields.bonuses.agility,
        0,
        ITEM_BONUS_ATTR_MAX,
    )?;
    check_range(
        "bonus.intelligence",
        fields.bonuses.intelligence,
        0,
        ITEM_BONUS_ATTR_MAX,
    )?;
    check_id_list("ability.active", &fields.abilities.active)?;
    check_id_list("ability.passive", &fields.abilities.passive)?;
    check_range(
        "pickup.range",
        fields.pickup.pickup_range,
        0,
        ITEM_PICKUP_RANGE_MAX,
    )?;
    check_enum(
        "pickup.drop_on_death",
        &fields.pickup.drop_on_death,
        ITEM_DROP_ON_DEATH,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "object_editor_item_fields_tests.rs"]
mod tests;
