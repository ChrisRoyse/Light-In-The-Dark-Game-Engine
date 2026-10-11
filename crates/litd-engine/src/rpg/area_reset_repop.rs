//! Reset/repop derivation, validation, coverage lint, and readback for the
//! authored world (#1668).
//!
//! `derive_area_reset_repop` turns one `AreaSpec` into its reset+repop profile: the
//! CADENCE is the CF-documented global 3/15 area_minute rule (helpfile 98), and the
//! POLICY is derived from the area's structural `AreaKind` (or read from the
//! CF-documented override table when one exists), failing loud (by name) on nothing
//! -- every kind maps to a policy. `validate_area_reset_repop` enforces the CF
//! invariants (object-regen-only-when-empty, capped-items-do-not-respawn-while-held)
//! and that the policy flags match the policy's affordances, so a fabricated profile
//! fails loud.
//!
//! `validate_reset_repop_coverage` is the corpus lint: it proves the profiled set is
//! EXACTLY the registry set, so no authored area is left without a reset profile and
//! no profile references a non-existent area. Every failure names the offenders.

use std::collections::{BTreeMap, BTreeSet};

use litd_hash::sha256_hex;

use crate::EngineError;

use super::area_reset_repop_table::cf_documented_repop_override;
use super::area_reset_repop_types::*;
use super::area_specs_types::{AreaKind, AreaSpec};

/// The repop policy each structural `AreaKind` implies. This is the by-kind
/// derivation (#1668, honestly marked `DerivedFromKind`).
pub const fn repop_policy_for_kind(kind: AreaKind) -> RepopPolicy {
    match kind {
        AreaKind::City | AreaKind::Town | AreaKind::CabalHall => RepopPolicy::CivilizedDense,
        AreaKind::Road => RepopPolicy::RoadPatrol,
        AreaKind::Wilderness | AreaKind::Region => RepopPolicy::WildRoaming,
        AreaKind::Dungeon => RepopPolicy::GuardedInterior,
        AreaKind::Lair => RepopPolicy::BossLair,
    }
}

/// Derive the reset + repop profile for one authored area (#1668). Cadence is the
/// CF-documented global rule (helpfile 98); policy is a CF-documented override if
/// one exists, else derived from the area's kind.
pub fn derive_area_reset_repop(spec: &AreaSpec) -> Result<AreaResetRepopProfile, EngineError> {
    let (policy, policy_derivation, policy_note) = match cf_documented_repop_override(&spec.area_id)
    {
        Some(over) => (
            over.policy,
            ResetDerivation::CfDocumented,
            over.note.to_string(),
        ),
        None => {
            let policy = repop_policy_for_kind(spec.area_kind);
            (
                policy,
                ResetDerivation::DerivedFromKind,
                format!(
                    "{:?} -> {} repop policy (helpfile 98 cadence).",
                    spec.area_kind,
                    policy.label()
                ),
            )
        }
    };

    let aff = policy.affordances();
    let entry = AreaResetRepopProfile {
        area_id: spec.area_id.clone(),
        repop_policy: policy,
        empty_reset_area_minutes: CF_EMPTY_RESET_AREA_MINUTES,
        occupied_reset_area_minutes: CF_OCCUPIED_RESET_AREA_MINUTES,
        regen_objects_only_when_empty: true,
        capped_items_respawn_while_held: false,
        rearms_doors_locks: aff.rearms_doors_locks,
        rearms_traps: aff.rearms_traps,
        has_boss_timer: aff.has_boss_timer,
        population_density_tier: aff.population_density_tier,
        cadence_derivation: ResetDerivation::CfDocumented,
        policy_derivation,
        note: policy_note,
    };
    validate_area_reset_repop(&entry)?;
    Ok(entry)
}

/// Validate one reset+repop profile. Fails loud (named) on an empty id/note, a zero
/// or inverted cadence, a violated CF invariant (object regen while occupied, or
/// capped items respawning while held), or policy flags that do not match the
/// policy's documented affordances.
pub fn validate_area_reset_repop(entry: &AreaResetRepopProfile) -> Result<(), EngineError> {
    if entry.area_id.trim().is_empty() {
        return Err(invalid("area_reset_repop.area_id", "must be non-empty"));
    }
    if entry.note.trim().is_empty() {
        return Err(invalid(
            "area_reset_repop.note",
            format!("area {} must carry a derivation note", entry.area_id),
        ));
    }
    if entry.empty_reset_area_minutes == 0 {
        return Err(invalid(
            "area_reset_repop.empty_reset_area_minutes",
            format!("area {} has a zero empty-reset cadence", entry.area_id),
        ));
    }
    if entry.occupied_reset_area_minutes == 0 {
        return Err(invalid(
            "area_reset_repop.occupied_reset_area_minutes",
            format!("area {} has a zero occupied-reset cadence", entry.area_id),
        ));
    }
    // CF helpfile 98: occupied areas reset LESS often than empty ones.
    if entry.occupied_reset_area_minutes < entry.empty_reset_area_minutes {
        return Err(invalid(
            "area_reset_repop.occupied_reset_area_minutes",
            format!(
                "area {} resets faster while occupied ({}) than empty ({}) -- CF helpfile 98 \
                 requires occupied >= empty",
                entry.area_id, entry.occupied_reset_area_minutes, entry.empty_reset_area_minutes
            ),
        ));
    }
    // CF helpfile 98 invariant.
    if !entry.regen_objects_only_when_empty {
        return Err(invalid(
            "area_reset_repop.regen_objects_only_when_empty",
            format!(
                "area {} claims ground objects regenerate while occupied -- violates CF helpfile 98",
                entry.area_id
            ),
        ));
    }
    // CF scarcity ledger invariant (helpfile 61 + #973/#974).
    if entry.capped_items_respawn_while_held {
        return Err(invalid(
            "area_reset_repop.capped_items_respawn_while_held",
            format!(
                "area {} claims capped items respawn while held -- violates the scarcity ledger \
                 (#973/#974, helpfile 61)",
                entry.area_id
            ),
        ));
    }
    // Policy flags must match the policy's documented affordances, so a fabricated
    // flag (e.g. a road with a boss timer) fails loud.
    let aff = entry.repop_policy.affordances();
    if entry.rearms_doors_locks != aff.rearms_doors_locks
        || entry.rearms_traps != aff.rearms_traps
        || entry.has_boss_timer != aff.has_boss_timer
        || entry.population_density_tier != aff.population_density_tier
    {
        return Err(invalid(
            "area_reset_repop.policy_flags",
            format!(
                "area {} flags disagree with its {} policy affordances",
                entry.area_id,
                entry.repop_policy.label()
            ),
        ));
    }
    Ok(())
}

/// Build the full reset+repop table for a set of authored specs. Every spec is
/// derived and validated; duplicate area ids fail loud.
pub fn build_area_reset_repop_table(
    specs: &[AreaSpec],
) -> Result<AreaResetRepopTable, EngineError> {
    let mut entries = Vec::with_capacity(specs.len());
    let mut seen = BTreeSet::new();
    for spec in specs {
        if !seen.insert(spec.area_id.clone()) {
            return Err(invalid(
                "area_reset_repop.duplicate_area",
                format!("area {} profiled twice", spec.area_id),
            ));
        }
        entries.push(derive_area_reset_repop(spec)?);
    }
    Ok(AreaResetRepopTable {
        schema: RPG_AREA_RESET_REPOP_SCHEMA.to_string(),
        table_id: AREA_RESET_REPOP_TABLE_ID.to_string(),
        entries,
    })
}

/// Deterministic readback with the by-policy histogram (#1668 blind-spot report).
pub fn area_reset_repop_readback(
    table: &AreaResetRepopTable,
) -> Result<AreaResetRepopReadback, EngineError> {
    let mut policy_histogram: BTreeMap<String, usize> = RepopPolicy::all()
        .iter()
        .map(|p| (p.label().to_string(), 0))
        .collect();
    let mut boss_timer_count = 0;
    let mut rearms_doors_locks_count = 0;
    let mut rearms_traps_count = 0;
    let mut cf_documented_policy_count = 0;
    let mut derived_from_kind_policy_count = 0;
    let mut min_empty = u16::MAX;
    let mut max_occupied = 0u16;

    for entry in &table.entries {
        validate_area_reset_repop(entry)?;
        *policy_histogram
            .entry(entry.repop_policy.label().to_string())
            .or_insert(0) += 1;
        boss_timer_count += usize::from(entry.has_boss_timer);
        rearms_doors_locks_count += usize::from(entry.rearms_doors_locks);
        rearms_traps_count += usize::from(entry.rearms_traps);
        match entry.policy_derivation {
            ResetDerivation::CfDocumented => cf_documented_policy_count += 1,
            ResetDerivation::DerivedFromKind => derived_from_kind_policy_count += 1,
        }
        min_empty = min_empty.min(entry.empty_reset_area_minutes);
        max_occupied = max_occupied.max(entry.occupied_reset_area_minutes);
    }
    if table.entries.is_empty() {
        min_empty = 0;
    }

    Ok(AreaResetRepopReadback {
        schema: table.schema.clone(),
        table_id: table.table_id.clone(),
        area_count: table.entries.len(),
        policy_histogram,
        boss_timer_count,
        rearms_doors_locks_count,
        rearms_traps_count,
        cf_documented_policy_count,
        derived_from_kind_policy_count,
        min_empty_reset_area_minutes: min_empty,
        max_occupied_reset_area_minutes: max_occupied,
        table_sha256: table_hash(table)?,
    })
}

/// The corpus coverage lint (#1668). Proves the profiled set is EXACTLY the registry
/// set: every authored area carries a reset profile, and no profile references an
/// area that is not authored. Fails loud listing every unprofiled and every orphan
/// area.
pub fn validate_reset_repop_coverage(
    registry_area_ids: &BTreeSet<String>,
    table: &AreaResetRepopTable,
) -> Result<AreaResetRepopCoverageReadback, EngineError> {
    let readback = area_reset_repop_readback(table)?;
    let profiled: BTreeSet<String> = table.entries.iter().map(|e| e.area_id.clone()).collect();
    if profiled.len() != table.entries.len() {
        return Err(invalid(
            "area_reset_repop.duplicate_area",
            "the reset+repop table contains duplicate area ids",
        ));
    }

    let unprofiled: Vec<&String> = registry_area_ids.difference(&profiled).collect();
    if !unprofiled.is_empty() {
        return Err(invalid(
            "area_reset_repop.unprofiled_areas",
            format!(
                "{} authored area(s) have no reset+repop profile: {}",
                unprofiled.len(),
                unprofiled
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }

    let orphans: Vec<&String> = profiled.difference(registry_area_ids).collect();
    if !orphans.is_empty() {
        return Err(invalid(
            "area_reset_repop.orphan_profiles",
            format!(
                "{} reset+repop profile(s) reference areas not in the registry: {}",
                orphans.len(),
                orphans
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }

    let distinct_policy_count = readback
        .policy_histogram
        .values()
        .filter(|&&count| count > 0)
        .count();

    Ok(AreaResetRepopCoverageReadback {
        registry_area_count: registry_area_ids.len(),
        profiled_area_count: profiled.len(),
        policy_histogram: readback.policy_histogram,
        distinct_policy_count,
        table_sha256: readback.table_sha256,
    })
}

fn table_hash(table: &AreaResetRepopTable) -> Result<String, EngineError> {
    serde_json::to_vec(table)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize area reset+repop table: {error}"),
        })
}

fn invalid(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    }
}
