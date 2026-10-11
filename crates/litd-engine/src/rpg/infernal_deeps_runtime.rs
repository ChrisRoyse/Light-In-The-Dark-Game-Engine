//! Runtime consumer of editor-authored Infernal Deeps tuning (#1455).
//!
//! Doctrine #1348/#1455: mechanics that legitimately stay in Rust must read their tuning constants
//! from the lowered Calyx data, NEVER from Rust literals. This module is that proof for the flagship
//! Infernal Deeps corpus. The withering-raid admission gate reads its minimum party size / character
//! level out of the FROZEN content table (the editor-authored, Calyx-ingested, lowered rows that
//! [`crate::calyx_content_runtime::FrozenContentTable`] freezes once at startup), so editing those
//! numbers in the editor moves the in-game gate with no recompile.
//!
//! There are no content literals here - every tuning value is read back from the frozen table, and a
//! missing row or non-integer field is a loud, structured error, never a silent default.

use crate::EngineError;
use crate::calyx_content::EngineContentKind;
use crate::calyx_content_runtime::FrozenContentTable;

/// Story row id: the withering-raid path entity (minimum party/level, phase count).
pub const INFERNAL_DEEPS_WITHERING_ID: &str =
    "server.ashen_reach.infernal_deeps.withering_raid_path";
/// Terrain row id: the zone-topology entity (zone count).
pub const INFERNAL_DEEPS_ZONE_TOPOLOGY_ID: &str = "server.ashen_reach.infernal_deeps.zone_topology";
/// Agent row id: the boss-hierarchy entity (boss count).
pub const INFERNAL_DEEPS_BOSS_HIERARCHY_ID: &str =
    "server.ashen_reach.infernal_deeps.boss_hierarchy";
/// Item row id: the relic-vaults entity (relic vault count).
pub const INFERNAL_DEEPS_RELIC_VAULTS_ID: &str = "server.ashen_reach.infernal_deeps.relic_vaults";

/// The Infernal Deeps tuning a sim system consults at runtime, read entirely from frozen lowered
/// rows. Constructed once (see [`InfernalDeepsRuntimeTuning::from_frozen`]); never re-queries Calyx.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfernalDeepsRuntimeTuning {
    pub withering_minimum_party_size: i64,
    pub withering_minimum_character_level: i64,
    pub withering_raid_phase_count: i64,
    pub zone_count: i64,
    pub boss_count: i64,
    pub relic_vault_count: i64,
}

impl InfernalDeepsRuntimeTuning {
    /// Read every tuning constant out of the frozen table. Each field comes from the authored
    /// payload of its entity row; a missing row or absent/non-integer pointer is a hard error so a
    /// mechanic can never silently fall back to a compiled-in literal.
    pub fn from_frozen(table: &FrozenContentTable) -> Result<Self, EngineError> {
        Ok(Self {
            withering_minimum_party_size: read_i64(
                table,
                EngineContentKind::Story,
                INFERNAL_DEEPS_WITHERING_ID,
                "/withering_minimum_party_size",
            )?,
            withering_minimum_character_level: read_i64(
                table,
                EngineContentKind::Story,
                INFERNAL_DEEPS_WITHERING_ID,
                "/withering_minimum_character_level",
            )?,
            withering_raid_phase_count: read_i64(
                table,
                EngineContentKind::Story,
                INFERNAL_DEEPS_WITHERING_ID,
                "/withering_raid_phase_count",
            )?,
            zone_count: read_i64(
                table,
                EngineContentKind::Terrain,
                INFERNAL_DEEPS_ZONE_TOPOLOGY_ID,
                "/zone_count",
            )?,
            boss_count: read_i64(
                table,
                EngineContentKind::Agent,
                INFERNAL_DEEPS_BOSS_HIERARCHY_ID,
                "/boss_count",
            )?,
            relic_vault_count: read_i64(
                table,
                EngineContentKind::Item,
                INFERNAL_DEEPS_RELIC_VAULTS_ID,
                "/relic_vault_count",
            )?,
        })
    }
}

/// The outcome of the withering-raid admission gate. The denied variants carry the frozen threshold
/// that produced the decision so the boundary is auditable (and demonstrably no-recompile mutable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitheringRaidAdmission {
    Admitted,
    DeniedPartyTooSmall { required: i64, actual: i64 },
    DeniedUnderLeveled { required: i64, actual: i64 },
}

/// A sim mechanic that stays in Rust but reads its thresholds from lowered data. It admits a party
/// to the Withering raid only if it meets the frozen minimum party size AND character level; the
/// numbers are the editor-authored ones, so re-authoring them in the editor moves this boundary
/// with no recompile.
pub fn admit_withering_raid(
    tuning: &InfernalDeepsRuntimeTuning,
    party_size: i64,
    character_level: i64,
) -> WitheringRaidAdmission {
    if party_size < tuning.withering_minimum_party_size {
        return WitheringRaidAdmission::DeniedPartyTooSmall {
            required: tuning.withering_minimum_party_size,
            actual: party_size,
        };
    }
    if character_level < tuning.withering_minimum_character_level {
        return WitheringRaidAdmission::DeniedUnderLeveled {
            required: tuning.withering_minimum_character_level,
            actual: character_level,
        };
    }
    WitheringRaidAdmission::Admitted
}

fn read_i64(
    table: &FrozenContentTable,
    kind: EngineContentKind,
    id: &str,
    pointer: &str,
) -> Result<i64, EngineError> {
    let row = table
        .row(kind, id)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("infernal_deeps_runtime.{id}"),
            reason: format!(
                "frozen content table is missing row {id}; runtime tuning must come from the lowered Calyx data, never a literal"
            ),
        })?;
    row.payload_i64(pointer)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("infernal_deeps_runtime.{id}{pointer}"),
            reason: "authored payload pointer is absent or non-integer".to_string(),
        })
}

#[cfg(test)]
#[path = "infernal_deeps_runtime_tests.rs"]
mod tests;
