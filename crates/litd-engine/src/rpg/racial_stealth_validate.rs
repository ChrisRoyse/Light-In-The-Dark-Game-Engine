//! Referential-integrity validation and readback for the #1603 racial stealth
//! profile table. Split from `racial_stealth.rs` to keep both files under the
//! 500-line ratchet. Every race id must match an authored `RaceSpec` id and every
//! innate trait's ability id must resolve in the #1604 stealth catalog; dangling
//! references fail loud.

use std::collections::{BTreeMap, BTreeSet};

use super::abilities_stealth::stealth_ability_catalog;
use super::race_specs_types::{
    ASHEN_HUMAN_RACE_ID, ASHKIN_DEEP_RACE_ID, ASHKIN_MIXED_RACE_ID, CLOUD_GIANT_RACE_ID,
    DEEP_KIN_RACE_ID, DWARF_RACE_ID, ELF_RACE_ID, FELISAR_RACE_ID, FIRE_GIANT_RACE_ID,
    FROST_GIANT_RACE_ID, GNOME_RACE_ID, GOBLIN_RACE_ID, HALF_ELF_RACE_ID, MINOTAUR_RACE_ID,
    ONYX_KIN_RACE_ID, ORC_RACE_ID, SAUR_RACE_ID, STORM_GIANT_RACE_ID, SYLPHKIN_RACE_ID,
    WOOD_ELF_RACE_ID,
};
use super::racial_stealth::racial_stealth_profile_table;
use super::racial_stealth_types::{
    RPG_RACIAL_STEALTH_SCHEMA, RacialStealthProfileTable, RacialStealthTableReadback,
};
use super::serialize_sha256;

use crate::EngineError;

/// The 20 authored race ids (#999/#1000-#1018) the profile keys must match.
pub fn known_race_ids() -> BTreeSet<String> {
    [
        ASHEN_HUMAN_RACE_ID,
        ELF_RACE_ID,
        HALF_ELF_RACE_ID,
        ASHKIN_MIXED_RACE_ID,
        ASHKIN_DEEP_RACE_ID,
        WOOD_ELF_RACE_ID,
        DWARF_RACE_ID,
        ONYX_KIN_RACE_ID,
        GNOME_RACE_ID,
        DEEP_KIN_RACE_ID,
        ORC_RACE_ID,
        GOBLIN_RACE_ID,
        MINOTAUR_RACE_ID,
        STORM_GIANT_RACE_ID,
        FIRE_GIANT_RACE_ID,
        FROST_GIANT_RACE_ID,
        CLOUD_GIANT_RACE_ID,
        SYLPHKIN_RACE_ID,
        FELISAR_RACE_ID,
        SAUR_RACE_ID,
    ]
    .iter()
    .map(|id| (*id).to_string())
    .collect()
}

/// The ability ids the #1604 stealth catalog defines, used to reject dangling
/// innate references.
pub fn stealth_catalog_ability_ids() -> BTreeSet<String> {
    stealth_ability_catalog()
        .abilities
        .into_iter()
        .map(|ability| ability.ability_id)
        .collect()
}

fn invalid<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    })
}

/// Validate every profile: unique race key, race id in the authored roster, and
/// every innate trait's ability id resolvable in the #1604 catalog. Fails loud
/// with the exact dangling reference.
pub fn validate_racial_stealth_table(
    table: &RacialStealthProfileTable,
) -> Result<RacialStealthTableReadback, EngineError> {
    if table.profiles.is_empty() {
        return invalid(
            "racial_stealth.profiles",
            "table must have at least one race",
        );
    }
    let races = known_race_ids();
    let abilities = stealth_catalog_ability_ids();

    let mut seen_races = BTreeSet::new();
    let mut seen_traits = BTreeSet::new();
    let mut noise_class_counts = BTreeMap::new();
    let mut trait_role_counts = BTreeMap::new();
    let mut referenced_abilities = BTreeSet::new();
    let mut race_ids = Vec::new();
    let mut innate_trait_total = 0;
    let mut races_with_innate_stealth = 0;
    let mut creep_penalized = 0;

    for p in &table.profiles {
        if !races.contains(&p.race_id) {
            return invalid(
                "racial_stealth.race_id",
                format!(
                    "dangling race id {} is not an authored RaceSpec id",
                    p.race_id
                ),
            );
        }
        if !seen_races.insert(p.race_id.clone()) {
            return invalid(
                "racial_stealth.race_id",
                format!("duplicate race id {}", p.race_id),
            );
        }
        if p.footstep_radius_scale_percent != p.movement_noise.footstep_radius_scale_percent() {
            return invalid(
                "racial_stealth.footstep_radius_scale_percent",
                format!(
                    "race {} footstep scale {} disagrees with its {} noise class",
                    p.race_id,
                    p.footstep_radius_scale_percent,
                    p.movement_noise.label()
                ),
            );
        }
        *noise_class_counts
            .entry(p.movement_noise.label().to_string())
            .or_insert(0) += 1;
        creep_penalized += usize::from(p.creep_size_penalty);
        if !p.innate_traits.is_empty() {
            races_with_innate_stealth += 1;
        }
        for t in &p.innate_traits {
            if t.requires_training {
                return invalid(
                    "racial_stealth.trait.requires_training",
                    format!(
                        "trait {} on {} is trained, not innate; it does not belong in the innate table",
                        t.trait_id, p.race_id
                    ),
                );
            }
            if !abilities.contains(&t.ability_id) {
                return invalid(
                    "racial_stealth.trait.ability_id",
                    format!(
                        "dangling ability id {} on race {} is not in the #1604 stealth catalog",
                        t.ability_id, p.race_id
                    ),
                );
            }
            if !seen_traits.insert((p.race_id.clone(), t.trait_id.clone())) {
                return invalid(
                    "racial_stealth.trait.trait_id",
                    format!("duplicate trait {} on race {}", t.trait_id, p.race_id),
                );
            }
            *trait_role_counts
                .entry(t.role.label().to_string())
                .or_insert(0) += 1;
            referenced_abilities.insert(t.ability_id.clone());
            innate_trait_total += 1;
        }
        race_ids.push(p.race_id.clone());
    }

    let mut readback = RacialStealthTableReadback {
        schema: RPG_RACIAL_STEALTH_SCHEMA.to_string(),
        table_id: table.table_id.clone(),
        profile_count: table.profiles.len(),
        noise_class_counts,
        trait_role_counts,
        innate_trait_total,
        races_with_innate_stealth,
        creep_penalized_races: creep_penalized,
        referenced_ability_ids: referenced_abilities.into_iter().collect(),
        race_ids,
        table_sha256: String::new(),
    };
    readback.table_sha256 = serialize_sha256(table)?;
    Ok(readback)
}

/// Build the readback for the canonical authored table.
pub fn build_racial_stealth_readback() -> Result<RacialStealthTableReadback, EngineError> {
    validate_racial_stealth_table(&racial_stealth_profile_table())
}
