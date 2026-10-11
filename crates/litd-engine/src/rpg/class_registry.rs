//! Referential-integrity registry for race `class_access` (#1631, gap 2).
//!
//! A [`RaceSpec`]'s `class_access` names class ids as bare strings. Per-spec
//! validation (`validate_race_spec`) checks only token FORM, so a well-formed
//! but non-existent id (e.g. `class.does_not_exist_anywhere`) silently passes.
//! This module closes that gap the same way the #1704 area registry closes the
//! dangling-connection gap: it derives a class-id registry from the authored
//! [`ClassSpec`] corpus (single source of truth) and validates every race's
//! class-access ids against it, failing loud and naming race + bad id.

use std::collections::BTreeSet;

use crate::EngineError;

use super::{
    class_specs_types::ClassSpec,
    race_specs_types::{RaceClassAccess, RaceSpec},
};

/// The set of authored class ids, derived from the [`ClassSpec`] corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassRegistry {
    ids: BTreeSet<String>,
}

impl ClassRegistry {
    /// Build the registry from the authored class corpus. Fails loud on a
    /// duplicate `class_id` (an ambiguous registry would mask a real collision).
    pub fn from_specs(specs: &[ClassSpec]) -> Result<Self, EngineError> {
        let mut ids = BTreeSet::new();
        for spec in specs {
            if !ids.insert(spec.class_id.clone()) {
                return Err(invalid(
                    "class_registry",
                    format!("duplicate class id {}", spec.class_id),
                ));
            }
        }
        Ok(Self { ids })
    }

    /// True when `class_id` resolves to an authored class spec.
    pub fn contains(&self, class_id: &str) -> bool {
        self.ids.contains(class_id)
    }

    /// The authored class ids, sorted.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.ids.iter().map(String::as_str)
    }

    /// Number of authored class ids.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// True when no class is authored.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Count-oriented result of a full corpus referential-integrity pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaceClassAccessReadback {
    /// Races validated.
    pub race_count: usize,
    /// Authored class ids in the registry.
    pub class_count: usize,
    /// Total class-access entries checked across all races (the union of every
    /// `Restricted`/`AllExcept` id; `All` contributes none).
    pub checked_class_access_entry_count: usize,
    /// Races whose access is `Restricted` or `AllExcept` (i.e. carry a list).
    pub races_with_class_lists: usize,
}

/// The class ids named by one race's `class_access` (empty for `All`).
fn access_ids(access: &RaceClassAccess) -> &[String] {
    match access {
        RaceClassAccess::All => &[],
        RaceClassAccess::Restricted { class_ids } => class_ids,
        RaceClassAccess::AllExcept { class_ids } => class_ids,
    }
}

/// Validate that EVERY class id named in EVERY race's `class_access` resolves to
/// an authored class in `registry`. Fails loud, naming the race and the exact
/// unresolved class id, on the first dangling reference.
pub fn validate_race_class_access_against_registry(
    races: &[RaceSpec],
    registry: &ClassRegistry,
) -> Result<RaceClassAccessReadback, EngineError> {
    let mut checked = 0usize;
    let mut with_lists = 0usize;
    for race in races {
        let ids = access_ids(&race.class_access);
        if !ids.is_empty() {
            with_lists += 1;
        }
        for class_id in ids {
            checked += 1;
            if !registry.contains(class_id) {
                return Err(invalid(
                    "class_access",
                    format!(
                        "race {} references unauthored class id {} (not in the {}-class registry)",
                        race.race_id,
                        class_id,
                        registry.len()
                    ),
                ));
            }
        }
    }
    Ok(RaceClassAccessReadback {
        race_count: races.len(),
        class_count: registry.len(),
        checked_class_access_entry_count: checked,
        races_with_class_lists: with_lists,
    })
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
