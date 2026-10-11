//! Referential-integrity validation and readback for the #1623 stealth
//! acquisition matrix, plus the #1732 catalog<->matrix level-consistency
//! cross-check. Split from `stealth_acquisition.rs` to keep both files under the
//! 500-line ratchet.
//!
//! `validate_stealth_acquisition_matrix` proves every `ability_id` resolves in the
//! #1604 catalog and every non-provisional `grantor_id` matches an authored
//! class/race spec id. `validate_catalog_matrix_level_consistency` is the #1732
//! guard: it fails loud if the level the #1604 catalog authors for an ability's
//! primary gating class ever drifts from the level the #1623 matrix records for
//! that same class -- naming the ability, the class, and both levels.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::abilities_stealth::stealth_ability_catalog;
use super::abilities_types::AbilityCatalog;
use super::racial_stealth_validate::known_race_ids;
use super::serialize_sha256;
use super::stealth_acquisition::{
    ASSASSIN, DARK_ELF, DRUID, INVOKER, RANGER, THIEF, TRANSMUTER, known_class_ids,
    stealth_acquisition_matrix,
};
use super::stealth_acquisition_types::{
    GrantorKind, RPG_STEALTH_ACQUISITION_SCHEMA, StealthAcquisitionMatrix,
    StealthAcquisitionReadback,
};

use crate::EngineError;

// --- Referential-integrity validation ---------------------------------------

fn invalid<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::RpgAbilityInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    })
}

/// Validate the matrix. Every ability id must resolve in the #1604 catalog; every
/// non-provisional grantor id must resolve in the authored class/race rosters;
/// levels must be in 1..=51; no duplicate (ability, grantor) cell. Fails loud with
/// the exact dangling reference.
pub fn validate_stealth_acquisition_matrix(
    matrix: &StealthAcquisitionMatrix,
) -> Result<StealthAcquisitionReadback, EngineError> {
    if matrix.entries.is_empty() {
        return invalid(
            "stealth_acquisition.entries",
            "matrix must have at least one entry",
        );
    }
    let abilities: BTreeSet<String> = stealth_ability_catalog()
        .abilities
        .into_iter()
        .map(|ability| ability.ability_id)
        .collect();
    let classes = known_class_ids();
    let races = known_race_ids();

    let mut seen_cells = BTreeSet::new();
    let mut acquisition_counts = BTreeMap::new();
    let mut owners_per_ability = BTreeMap::new();
    let mut referenced = BTreeSet::new();
    let mut distinct_grantors = BTreeSet::new();
    let mut provisional_grantors = BTreeSet::new();
    let mut class_entry_count = 0;
    let mut race_entry_count = 0;
    let mut provisional_entry_count = 0;
    let mut abilities_validated = 0;
    let mut grantors_validated = 0;

    for e in &matrix.entries {
        if !abilities.contains(&e.ability_id) {
            return invalid(
                "stealth_acquisition.ability_id",
                format!(
                    "dangling ability id {} (grantor {}) is not in the #1604 stealth catalog",
                    e.ability_id, e.grantor_id
                ),
            );
        }
        abilities_validated += 1;
        if e.level == 0 || e.level > 51 {
            return invalid(
                "stealth_acquisition.level",
                format!(
                    "level {} for {} on {} is outside 1..=51",
                    e.level, e.ability_id, e.grantor_id
                ),
            );
        }
        if !seen_cells.insert((e.ability_id.clone(), e.grantor_id.clone())) {
            return invalid(
                "stealth_acquisition.cell",
                format!("duplicate cell {} x {}", e.ability_id, e.grantor_id),
            );
        }
        match e.grantor_kind {
            GrantorKind::Class => class_entry_count += 1,
            GrantorKind::Race => race_entry_count += 1,
        }
        if e.provisional {
            provisional_entry_count += 1;
            provisional_grantors.insert(e.grantor_id.clone());
        } else {
            let roster = match e.grantor_kind {
                GrantorKind::Class => &classes,
                GrantorKind::Race => &races,
            };
            if !roster.contains(&e.grantor_id) {
                return invalid(
                    "stealth_acquisition.grantor_id",
                    format!(
                        "dangling {} grantor id {} for {} matches no authored spec id",
                        e.grantor_kind.label(),
                        e.grantor_id,
                        e.ability_id
                    ),
                );
            }
            grantors_validated += 1;
        }
        *acquisition_counts
            .entry(e.acquisition.label().to_string())
            .or_insert(0) += 1;
        *owners_per_ability.entry(e.ability_id.clone()).or_insert(0) += 1;
        referenced.insert(e.ability_id.clone());
        distinct_grantors.insert(e.grantor_id.clone());
    }

    let mut readback = StealthAcquisitionReadback {
        schema: RPG_STEALTH_ACQUISITION_SCHEMA.to_string(),
        table_id: matrix.table_id.clone(),
        entry_count: matrix.entries.len(),
        class_entry_count,
        race_entry_count,
        acquisition_counts,
        distinct_ability_count: referenced.len(),
        distinct_grantor_count: distinct_grantors.len(),
        provisional_entry_count,
        provisional_grantors: provisional_grantors.into_iter().collect(),
        abilities_validated,
        grantors_validated,
        referenced_ability_ids: referenced.into_iter().collect(),
        owners_per_ability,
        matrix_sha256: String::new(),
    };
    readback.matrix_sha256 = serialize_sha256(matrix)?;
    Ok(readback)
}

/// Build the readback for the canonical authored matrix.
pub fn build_stealth_acquisition_readback() -> Result<StealthAcquisitionReadback, EngineError> {
    validate_stealth_acquisition_matrix(&stealth_acquisition_matrix())
}

// --- #1732 catalog<->matrix level consistency -------------------------------

/// Deterministic proof that the #1604 catalog and #1623 matrix agree on levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogMatrixConsistencyReadback {
    pub catalog_ability_count: usize,
    pub matrix_entry_count: usize,
    /// How many catalog abilities were cross-checked against a matrix cell for
    /// their primary gating class (abilities whose gate has no matrix row skip).
    pub cross_checked_count: usize,
    pub consistency_sha256: String,
}

/// Map a catalog ability's primary gate id (`category.family_id`) to the
/// originalized grantor id the #1623 matrix uses for that same class/race. Returns
/// `None` for gates the matrix does not track as a primary owner (the cross-check
/// then skips that ability rather than inventing a cell).
fn catalog_gate_to_matrix_grantor(gate: &str) -> Option<&'static str> {
    Some(match gate {
        "class.thief" => THIEF,
        "class.ranger" => RANGER,
        "class.assassin" => ASSASSIN,
        "class.transmuter" => TRANSMUTER,
        "class.invoker" => INVOKER,
        "domain.wild" => DRUID,
        "race.dark_elf" => DARK_ELF,
        _ => return None,
    })
}

/// #1732 single-source cross-validation: for every catalog ability, the level the
/// catalog authors (`requirements.min_level`) must equal the level the acquisition
/// matrix records for that ability under its primary gating class. Any drift fails
/// loud naming the ability, the class, and BOTH levels, so the catalog and matrix
/// can never silently disagree again.
pub fn validate_catalog_matrix_level_consistency(
    catalog: &AbilityCatalog,
    matrix: &StealthAcquisitionMatrix,
) -> Result<CatalogMatrixConsistencyReadback, EngineError> {
    let mut cell_level: BTreeMap<(&str, &str), u16> = BTreeMap::new();
    for e in &matrix.entries {
        cell_level.insert((e.ability_id.as_str(), e.grantor_id.as_str()), e.level);
    }

    let mut cross_checked_count = 0usize;
    for ability in &catalog.abilities {
        let Some(grantor) = catalog_gate_to_matrix_grantor(&ability.category.family_id) else {
            continue;
        };
        let Some(&matrix_level) = cell_level.get(&(ability.ability_id.as_str(), grantor)) else {
            continue;
        };
        let catalog_level = ability.requirements.min_level;
        if catalog_level != matrix_level {
            return invalid(
                "stealth_acquisition.level_consistency",
                format!(
                    "level drift for {} granted by {}: #1604 catalog min_level={} but #1623 acquisition matrix level={}",
                    ability.ability_id, grantor, catalog_level, matrix_level
                ),
            );
        }
        cross_checked_count += 1;
    }

    let mut readback = CatalogMatrixConsistencyReadback {
        catalog_ability_count: catalog.abilities.len(),
        matrix_entry_count: matrix.entries.len(),
        cross_checked_count,
        consistency_sha256: String::new(),
    };
    readback.consistency_sha256 = serialize_sha256(&json!({
        "catalog_id": catalog.catalog_id,
        "table_id": matrix.table_id,
        "cross_checked_count": cross_checked_count,
    }))?;
    Ok(readback)
}

/// Cross-check the canonical authored catalog against the canonical authored
/// matrix (#1732 convenience entrypoint for tests and the FSV harness).
pub fn build_catalog_matrix_consistency_readback()
-> Result<CatalogMatrixConsistencyReadback, EngineError> {
    validate_catalog_matrix_level_consistency(
        &stealth_ability_catalog(),
        &stealth_acquisition_matrix(),
    )
}
