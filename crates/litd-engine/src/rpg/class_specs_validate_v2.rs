//! Validators for the schema-v2 ClassSpec extensions (#1718): the point-buy
//! acquisition economy (gap 1), per-race alignment carve-outs (gap 2), the
//! many-to-many ability specialization membership set (gap 4), and the class
//! fount classification (gap 7). Split out of `class_specs.rs` to keep both
//! files under the 500-line ratchet; the shared primitive validators
//! (`invalid`, `validate_token`, `validate_text`, `validate_access`) live in
//! `class_specs.rs` and are re-used here.
use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::class_specs_primitives::{invalid, validate_access, validate_text, validate_token};
use super::class_specs_types::{
    AbilityAcquisitionCost, ClassFount, ClassLeveledAbility, ClassRaceAccess,
    RaceAlignmentException,
};

/// Validates the `specialization_ids` membership *set* (schema v2, gap 4):
/// every id is a valid token, references an authored specialization, and no id
/// repeats within the set.
pub(crate) fn validate_ability_specialization_ids(
    ids: &[String],
    specialization_ids: &BTreeSet<String>,
) -> Result<(), EngineError> {
    let mut local = BTreeSet::new();
    for specialization_id in ids {
        validate_token("leveled_abilities.specialization_ids", specialization_id)?;
        if !specialization_ids.contains(specialization_id) {
            return invalid(
                "leveled_abilities.specialization_ids",
                format!("references unknown specialization {specialization_id}"),
            );
        }
        if !local.insert(specialization_id.as_str()) {
            return invalid(
                "leveled_abilities.specialization_ids",
                format!("duplicate membership {specialization_id}"),
            );
        }
    }
    Ok(())
}

/// Per-ability shape checks for the acquisition economy (schema v2, gap 1):
/// non-zero cost, `cumulative_cost >= cost`, a costed ability must belong to a
/// specialization, and a prerequisite (if any) is a valid token that is not the
/// ability itself. Cross-ability consistency is handled by
/// [`validate_acquisition_graph`].
pub(crate) fn validate_ability_acquisition_shape(
    ability: &ClassLeveledAbility,
) -> Result<(), EngineError> {
    let Some(acquisition) = &ability.acquisition else {
        return Ok(());
    };
    if ability.specialization_ids.is_empty() {
        return invalid(
            "leveled_abilities.acquisition",
            format!(
                "ability {} has an acquisition cost but no specialization; \
                 point-buy costs apply only to specialist abilities",
                ability.ability_id
            ),
        );
    }
    if acquisition.cost == 0 {
        return invalid("leveled_abilities.acquisition.cost", "must be non-zero");
    }
    if acquisition.cumulative_cost < acquisition.cost {
        return invalid(
            "leveled_abilities.acquisition.cumulative_cost",
            "must be at least the ability's own cost",
        );
    }
    if let Some(prerequisite) = &acquisition.prerequisite_ability_id {
        validate_token(
            "leveled_abilities.acquisition.prerequisite_ability_id",
            prerequisite,
        )?;
        if prerequisite == &ability.ability_id {
            return invalid(
                "leveled_abilities.acquisition.prerequisite_ability_id",
                format!(
                    "ability {} cannot be its own prerequisite",
                    ability.ability_id
                ),
            );
        }
    }
    Ok(())
}

/// Cross-ability acquisition-graph checks (schema v2, gap 1): every prerequisite
/// resolves to an authored ability, sits at no higher level, and the running
/// `cumulative_cost` equals `prerequisite.cumulative_cost + cost` (or the bare
/// `cost` when there is no prerequisite). Also proves the prerequisite graph is
/// acyclic, so an authoring slip fails loud instead of looping at runtime.
pub(crate) fn validate_acquisition_graph(
    abilities: &[ClassLeveledAbility],
) -> Result<(), EngineError> {
    let by_id: BTreeMap<&str, &ClassLeveledAbility> = abilities
        .iter()
        .map(|ability| (ability.ability_id.as_str(), ability))
        .collect();
    for ability in abilities {
        let Some(acquisition) = &ability.acquisition else {
            continue;
        };
        let expected = expected_cumulative(ability, acquisition, &by_id)?;
        if acquisition.cumulative_cost != expected {
            return invalid(
                "leveled_abilities.acquisition.cumulative_cost",
                format!(
                    "ability {} cumulative cost {} does not equal expected {expected}",
                    ability.ability_id, acquisition.cumulative_cost
                ),
            );
        }
    }
    detect_acquisition_cycles(&by_id)
}

fn expected_cumulative(
    ability: &ClassLeveledAbility,
    acquisition: &AbilityAcquisitionCost,
    by_id: &BTreeMap<&str, &ClassLeveledAbility>,
) -> Result<u16, EngineError> {
    let Some(prerequisite_id) = &acquisition.prerequisite_ability_id else {
        return Ok(acquisition.cost);
    };
    let Some(prerequisite) = by_id.get(prerequisite_id.as_str()) else {
        return invalid(
            "leveled_abilities.acquisition.prerequisite_ability_id",
            format!("references unknown ability {prerequisite_id}"),
        );
    };
    let Some(prerequisite_cost) = &prerequisite.acquisition else {
        return invalid(
            "leveled_abilities.acquisition.prerequisite_ability_id",
            format!("prerequisite {prerequisite_id} has no acquisition cost"),
        );
    };
    if prerequisite.level > ability.level {
        return invalid(
            "leveled_abilities.acquisition.prerequisite_ability_id",
            format!(
                "prerequisite {prerequisite_id} unlocks after {}",
                ability.ability_id
            ),
        );
    }
    acquisition
        .cost
        .checked_add(prerequisite_cost.cumulative_cost)
        .ok_or_else(overflow_error)
}

fn detect_acquisition_cycles(
    by_id: &BTreeMap<&str, &ClassLeveledAbility>,
) -> Result<(), EngineError> {
    for start in by_id.keys() {
        let mut cursor = *start;
        // Bounded by the graph size; a chain longer than the node count must
        // contain a cycle.
        for _ in 0..=by_id.len() {
            let Some(ability) = by_id.get(cursor) else {
                break;
            };
            let Some(acquisition) = &ability.acquisition else {
                break;
            };
            let Some(prerequisite) = &acquisition.prerequisite_ability_id else {
                break;
            };
            cursor = prerequisite.as_str();
        }
        // If we can still advance after len steps, the chain looped.
        if let Some(ability) = by_id.get(cursor)
            && ability
                .acquisition
                .as_ref()
                .and_then(|acquisition| acquisition.prerequisite_ability_id.as_ref())
                .is_some()
        {
            return invalid(
                "leveled_abilities.acquisition.prerequisite_ability_id",
                format!("prerequisite chain from {start} contains a cycle"),
            );
        }
    }
    Ok(())
}

/// Validates per-race alignment carve-outs (schema v2, gap 2): each names a
/// distinct race, its `allowed` rule is well-formed, and - when the class gates
/// races - it may only except a race that can actually take the class.
pub(crate) fn validate_alignment_exceptions(
    exceptions: &[RaceAlignmentException],
    race_access: &ClassRaceAccess,
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for exception in exceptions {
        validate_token("alignment_exceptions.race_id", &exception.race_id)?;
        validate_access("alignment_exceptions.allowed", &exception.allowed)?;
        if !seen.insert(exception.race_id.as_str()) {
            return invalid(
                "alignment_exceptions.race_id",
                format!("duplicate {}", exception.race_id),
            );
        }
        if let ClassRaceAccess::Restricted { race_ids } = race_access
            && !race_ids.contains(&exception.race_id)
        {
            return invalid(
                "alignment_exceptions.race_id",
                format!(
                    "{} is excepted but is not allowed to take the class",
                    exception.race_id
                ),
            );
        }
    }
    Ok(())
}

/// Validates the class fount (schema v2, gap 7). The closed enum already rejects
/// unknown founts at the type level; the only ill-formed value is an
/// `Unresolved` variant with an empty reason, which would hide an open decision.
pub(crate) fn validate_fount(fount: &ClassFount) -> Result<(), EngineError> {
    if let ClassFount::Unresolved { reason } = fount {
        validate_text("fount.unresolved.reason", reason)?;
    }
    Ok(())
}

fn overflow_error() -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: "leveled_abilities.acquisition.cumulative_cost".to_string(),
        reason: "cumulative cost overflowed u16".to_string(),
    }
}
