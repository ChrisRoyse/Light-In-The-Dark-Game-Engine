//! Referential-integrity registry for item racial-slot gating (#1683).
//!
//! A racial-slot item (claws/paws/hooves/horns/tail/wings) names the race ids
//! that possess that anatomy in `WearGating.gating_race_ids`. Per-item
//! validation (`validate_wear_gating`) checks only token FORM, so a well-formed
//! but non-existent id (e.g. `race.does_not_exist`) silently passes. This module
//! closes that gap exactly as the #1631 class registry and #1704 area registry
//! do: it derives a race-id registry from the authored [`RaceSpec`] corpus
//! (single source of truth -- NOT a hand-maintained parallel list) and validates
//! every item's gating ids against it, failing LOUD and naming item + bad id.

use std::collections::BTreeSet;

use crate::EngineError;

use super::item_specs_types::ItemSpec;
use super::race_specs_types::RaceSpec;

/// The set of authored race ids, derived from the [`RaceSpec`] corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaceRegistry {
    ids: BTreeSet<String>,
}

impl RaceRegistry {
    /// Build the registry from the authored race corpus. Fails LOUD on a
    /// duplicate `race_id` (an ambiguous registry would mask a real collision).
    pub fn from_specs(specs: &[RaceSpec]) -> Result<Self, EngineError> {
        let mut ids = BTreeSet::new();
        for spec in specs {
            if !ids.insert(spec.race_id.clone()) {
                return Err(invalid(
                    "race_registry",
                    format!("duplicate race id {}", spec.race_id),
                ));
            }
        }
        Ok(Self { ids })
    }

    /// True when `race_id` resolves to an authored race spec.
    pub fn contains(&self, race_id: &str) -> bool {
        self.ids.contains(race_id)
    }

    /// The authored race ids, sorted.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.ids.iter().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Count-oriented result of a full corpus referential-integrity pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRacialGatingReadback {
    /// Items validated.
    pub item_count: usize,
    /// Authored race ids in the registry.
    pub race_count: usize,
    /// Items that occupy a racial slot (and therefore carry gating race ids).
    pub racial_slot_item_count: usize,
    /// Total gating race-id references checked across all items.
    pub checked_gating_reference_count: usize,
}

/// Validate that EVERY gating race id named by EVERY racial-slot item resolves
/// to an authored race in `registry`. Fails LOUD, naming the item and the exact
/// unresolved race id, on the first dangling reference.
pub fn validate_item_racial_gating_against_registry(
    items: &[ItemSpec],
    registry: &RaceRegistry,
) -> Result<ItemRacialGatingReadback, EngineError> {
    let mut racial_items = 0usize;
    let mut checked = 0usize;
    for item in items {
        // Only racial slots carry gating ids; a Take/standard slot has none
        // (enforced by validate_wear_gating), so the loop over gating ids below
        // is a no-op there.
        if item.wear.slot.is_racial() {
            racial_items += 1;
        }
        for race_id in &item.wear.gating_race_ids {
            checked += 1;
            if !registry.contains(race_id) {
                return Err(invalid(
                    "wear.gating_race_ids",
                    format!(
                        "item {} references unauthored race id {} (not in the {}-race registry)",
                        item.item_id,
                        race_id,
                        registry.len()
                    ),
                ));
            }
        }
    }
    Ok(ItemRacialGatingReadback {
        item_count: items.len(),
        race_count: registry.len(),
        racial_slot_item_count: racial_items,
        checked_gating_reference_count: checked,
    })
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
