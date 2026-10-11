//! Runtime consumer of editor-authored Withering raid-path tuning (#1455 / child #1876).
//!
//! Reads per-entity frozen rows (raid Trigger + phase Triggers), not package-summary scalars.
//! Admission thresholds come from FROZEN payloads so editor re-authoring moves the gate
//! without recompile.

use crate::EngineError;
use crate::calyx_content::EngineContentKind;
use crate::calyx_content_runtime::{FrozenContentRow, FrozenContentTable};

pub const WITHERING_RAID_SURFACE_KIND: &str = "withering_raid";
pub const WITHERING_RAID_PHASE_SURFACE_KIND: &str = "withering_raid_phase";

/// Raid-level thresholds from the frozen `withering_raid` Trigger row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitheringRaidRuntimeSpec {
    pub raid_id: String,
    pub minimum_party_size: i64,
    pub minimum_character_level: i64,
    pub pressure_tier_milli: i64,
    pub final_guard_agent_id: String,
    pub target_item_of_power_id: String,
    pub requires_faction_leader_order: bool,
}

/// One phase of the withering path from a frozen `withering_raid_phase` Trigger row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitheringPhaseRuntimeSpec {
    pub phase_id: String,
    pub raid_id: String,
    pub kind: String,
    pub pressure_tier_milli: i64,
}

/// Full path the sim consults: one raid + its phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitheringRaidPathRuntime {
    pub raid: WitheringRaidRuntimeSpec,
    pub phases: Vec<WitheringPhaseRuntimeSpec>,
}

impl WitheringRaidPathRuntime {
    /// Load exactly one raid row and all phase rows from the frozen table.
    pub fn from_frozen(table: &FrozenContentTable) -> Result<Self, EngineError> {
        let mut raids = Vec::new();
        let mut phases = Vec::new();
        for row in &table.rows {
            match row.payload_str("/surface_kind") {
                Some(WITHERING_RAID_SURFACE_KIND) if row.kind == EngineContentKind::Trigger => {
                    raids.push(WitheringRaidRuntimeSpec {
                        raid_id: read_str(row, "/raid/raid_id")?,
                        minimum_party_size: read_i64(row, "/raid/minimum_party_size")?,
                        minimum_character_level: read_i64(row, "/raid/minimum_character_level")?,
                        pressure_tier_milli: read_i64(row, "/raid/pressure_tier_milli")?,
                        final_guard_agent_id: read_str(row, "/raid/final_guard_agent_id")?,
                        target_item_of_power_id: read_str(row, "/raid/target_item_of_power_id")?,
                        requires_faction_leader_order: read_bool(
                            row,
                            "/requires_faction_leader_order",
                        )?,
                    });
                }
                Some(WITHERING_RAID_PHASE_SURFACE_KIND)
                    if row.kind == EngineContentKind::Trigger =>
                {
                    phases.push(WitheringPhaseRuntimeSpec {
                        phase_id: read_str(row, "/phase/phase_id")?,
                        raid_id: read_str(row, "/raid_id")?,
                        kind: read_str(row, "/phase/kind")?,
                        pressure_tier_milli: read_i64(row, "/phase/pressure_tier_milli")?,
                    });
                }
                _ => {}
            }
        }
        if raids.len() != 1 {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: "withering_raid_path_runtime.raid".to_string(),
                reason: format!(
                    "expected exactly one withering_raid row in frozen table; found {}",
                    raids.len()
                ),
            });
        }
        if phases.is_empty() {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: "withering_raid_path_runtime.phases".to_string(),
                reason: "frozen table holds no withering_raid_phase rows".to_string(),
            });
        }
        Ok(Self {
            raid: raids.remove(0),
            phases,
        })
    }

    pub fn phase_count(&self) -> usize {
        self.phases.len()
    }
}

/// Admission outcome using FROZEN raid thresholds.
#[derive(Debug, Clone, PartialEq, Eq)]
/// Named distinctly from package-summary `WitheringRaidAdmission` in infernal_deeps_runtime.
pub enum WitheringRaidPathAdmission {
    Admitted,
    Denied {
        reason: String,
        minimum_party_size: i64,
        minimum_character_level: i64,
        actual_party_size: i64,
        actual_character_level: i64,
    },
}

/// Admit a party to the withering raid only when size and level meet frozen floors.
pub fn admit_withering_raid_from_path(
    path: &WitheringRaidPathRuntime,
    party_size: i64,
    character_level: i64,
) -> WitheringRaidPathAdmission {
    let raid = &path.raid;
    if party_size < raid.minimum_party_size || character_level < raid.minimum_character_level {
        return WitheringRaidPathAdmission::Denied {
            reason: "party fails frozen withering raid floors".to_string(),
            minimum_party_size: raid.minimum_party_size,
            minimum_character_level: raid.minimum_character_level,
            actual_party_size: party_size,
            actual_character_level: character_level,
        };
    }
    WitheringRaidPathAdmission::Admitted
}

fn read_i64(row: &FrozenContentRow, pointer: &str) -> Result<i64, EngineError> {
    row.payload_i64(pointer)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("withering_raid_path_runtime{pointer}"),
            reason: "authored payload pointer is absent or non-integer".to_string(),
        })
}

fn read_str(row: &FrozenContentRow, pointer: &str) -> Result<String, EngineError> {
    row.payload_str(pointer)
        .map(|value| value.to_string())
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("withering_raid_path_runtime{pointer}"),
            reason: "authored payload pointer is absent or non-string".to_string(),
        })
}

fn read_bool(row: &FrozenContentRow, pointer: &str) -> Result<bool, EngineError> {
    row.readback
        .payload
        .pointer(pointer)
        .and_then(|value| value.as_bool())
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("withering_raid_path_runtime{pointer}"),
            reason: "authored payload pointer is absent or non-bool".to_string(),
        })
}

#[cfg(test)]
#[path = "withering_raid_path_runtime_tests.rs"]
mod tests;
