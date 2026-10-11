//! Runtime consumer of editor-authored Infernal boss-agent tuning (#1455 / child #1874).
//!
//! Doctrine #1348/#1455: content-shaped data must live in editor-authored, Calyx-lowered rows, and a
//! mechanic that legitimately stays in Rust must read its tuning from those FROZEN lowered rows,
//! NEVER from Rust literals. This module is that proof for the Infernal boss hierarchy (#1068). Each
//! boss's engagement-strength floor, lethality tier, and command role are read back out of the
//! frozen Agent rows that [`crate::rpg::infernal_boss_agent_content_records`] authors, and the
//! boss-engagement gate consults only those frozen numbers -- so re-authoring a boss's minimum
//! strength in the editor moves the in-game engagement boundary with no recompile.
//!
//! There are no content literals here: the roster and every per-boss field are read from the frozen
//! table. A table with no boss rows, or a boss row missing a required field, is a loud structured
//! error, never a silent empty roster or default.

use crate::EngineError;
use crate::calyx_content::EngineContentKind;
use crate::calyx_content_runtime::{FrozenContentRow, FrozenContentTable};

/// The `surface_kind` marker every authored infernal boss Agent row carries. Used to pick the boss
/// rows out of the frozen table without hardcoding which ids exist -- the roster is data-driven.
pub const INFERNAL_BOSS_AGENT_SURFACE_KIND: &str = "infernal_boss_agent";
/// The authored `role` value for the single apex tyrant at the bottom of the hierarchy.
pub const INFERNAL_BOSS_ROLE_APEX_TYRANT: &str = "apex_tyrant";

/// One infernal boss's runtime tuning, read entirely from its frozen Agent row. Constructed once at
/// load time (see [`InfernalBossRuntimeRoster::from_frozen`]); never re-queries Calyx.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfernalBossRuntimeSpec {
    pub agent_id: String,
    pub boss_id: String,
    pub circle_id: String,
    pub role: String,
    pub lethality_tier: i64,
    /// The authored minimum party strength (milli) at which this boss issues combat orders.
    pub engagement_strength_milli: i64,
}

/// The infernal garrison the sim consults at runtime, read entirely from frozen Agent rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfernalBossRuntimeRoster {
    pub bosses: Vec<InfernalBossRuntimeSpec>,
}

impl InfernalBossRuntimeRoster {
    /// Read every boss's tuning out of the frozen table. Every Agent row whose payload
    /// `surface_kind` marks it an infernal boss is loaded; a row missing `role`, `lethality_tier`,
    /// or `min_strength_milli` is a hard error, and a table with zero boss rows is a hard error --
    /// the roster must come from the lowered Calyx data, never a compiled-in literal.
    pub fn from_frozen(table: &FrozenContentTable) -> Result<Self, EngineError> {
        let mut bosses = Vec::new();
        for row in &table.rows {
            if row.kind != EngineContentKind::Agent {
                continue;
            }
            if row.payload_str("/surface_kind") != Some(INFERNAL_BOSS_AGENT_SURFACE_KIND) {
                continue;
            }
            bosses.push(InfernalBossRuntimeSpec {
                agent_id: read_str(row, "/agent_id")?,
                boss_id: read_str(row, "/boss_id")?,
                circle_id: read_str(row, "/circle_id")?,
                role: read_str(row, "/role")?,
                lethality_tier: read_i64(row, "/lethality_tier")?,
                engagement_strength_milli: read_i64(row, "/min_strength_milli")?,
            });
        }
        if bosses.is_empty() {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: "infernal_boss_agents_runtime.roster".to_string(),
                reason:
                    "frozen content table holds no infernal boss Agent rows; the boss roster must come from lowered Calyx data, never a literal"
                        .to_string(),
            });
        }
        Ok(Self { bosses })
    }

    pub fn len(&self) -> usize {
        self.bosses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bosses.is_empty()
    }

    /// The boss with a specific Calyx agent id, or `None` if the frozen roster has no such row.
    pub fn boss(&self, agent_id: &str) -> Option<&InfernalBossRuntimeSpec> {
        self.bosses.iter().find(|boss| boss.agent_id == agent_id)
    }

    /// The single apex tyrant (authored `role == apex_tyrant`). `None` if the frozen roster has no
    /// apex row -- the caller decides whether that is fatal for its context.
    pub fn apex(&self) -> Option<&InfernalBossRuntimeSpec> {
        self.bosses
            .iter()
            .find(|boss| boss.role == INFERNAL_BOSS_ROLE_APEX_TYRANT)
    }
}

/// The outcome of the boss-engagement gate. The withheld variant carries the frozen threshold that
/// produced the decision so the boundary is auditable (and demonstrably no-recompile mutable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BossEngagement {
    Engages,
    Withholds {
        required_strength_milli: i64,
        actual_strength_milli: i64,
    },
}

/// A sim mechanic that stays in Rust but reads its threshold from lowered data. An infernal boss
/// issues combat orders only when the incoming party's effective strength (milli) meets the boss's
/// FROZEN `engagement_strength_milli`; below it, the boss withholds. The threshold is the
/// editor-authored one, so re-authoring it in the editor moves this boundary with no recompile.
pub fn boss_engages(boss: &InfernalBossRuntimeSpec, party_strength_milli: i64) -> BossEngagement {
    if party_strength_milli < boss.engagement_strength_milli {
        BossEngagement::Withholds {
            required_strength_milli: boss.engagement_strength_milli,
            actual_strength_milli: party_strength_milli,
        }
    } else {
        BossEngagement::Engages
    }
}

fn read_i64(row: &FrozenContentRow, pointer: &str) -> Result<i64, EngineError> {
    row.payload_i64(pointer)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("infernal_boss_agents_runtime{pointer}"),
            reason: "authored payload pointer is absent or non-integer".to_string(),
        })
}

fn read_str(row: &FrozenContentRow, pointer: &str) -> Result<String, EngineError> {
    row.payload_str(pointer)
        .map(|value| value.to_string())
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("infernal_boss_agents_runtime{pointer}"),
            reason: "authored payload pointer is absent or non-string".to_string(),
        })
}

#[cfg(test)]
#[path = "infernal_boss_agents_runtime_tests.rs"]
mod tests;
