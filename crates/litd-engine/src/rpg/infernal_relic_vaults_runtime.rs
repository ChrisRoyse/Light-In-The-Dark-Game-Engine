//! Runtime consumer of editor-authored Infernal relic-item tuning (#1455 / child #1875).
//!
//! Same doctrine as boss-agents (#1874): mechanics that stay in Rust read thresholds from
//! FROZEN lowered Item rows (`surface_kind = infernal_relic_item`), never from compiled
//! literals. Re-authoring `power_rating_milli` in the editor moves the qualification gate
//! with no recompile.

use crate::EngineError;
use crate::calyx_content::EngineContentKind;
use crate::calyx_content_runtime::{FrozenContentRow, FrozenContentTable};

/// Payload marker every authored hell-forged relic Item row carries.
pub const INFERNAL_RELIC_ITEM_SURFACE_KIND: &str = "infernal_relic_item";

/// One relic's runtime tuning, read entirely from its frozen Item row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfernalRelicRuntimeSpec {
    pub item_id: String,
    pub slot: String,
    pub rarity: String,
    pub source_vault_id: String,
    pub guarded_by_ruler_ref: String,
    pub world_finite_count: i64,
    pub power_rating_milli: i64,
}

/// Catalog of hell-forged relics the sim consults at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfernalRelicRuntimeCatalog {
    pub items: Vec<InfernalRelicRuntimeSpec>,
}

impl InfernalRelicRuntimeCatalog {
    /// Load every relic Item row from the frozen table. Empty catalog or missing required
    /// fields are hard errors (no silent empty default).
    pub fn from_frozen(table: &FrozenContentTable) -> Result<Self, EngineError> {
        let mut items = Vec::new();
        for row in &table.rows {
            if row.kind != EngineContentKind::Item {
                continue;
            }
            if row.payload_str("/surface_kind") != Some(INFERNAL_RELIC_ITEM_SURFACE_KIND) {
                continue;
            }
            items.push(InfernalRelicRuntimeSpec {
                item_id: read_str(row, "/item_id")?,
                slot: read_str(row, "/slot")?,
                rarity: read_str(row, "/rarity")?,
                source_vault_id: read_str(row, "/source_vault_id")?,
                guarded_by_ruler_ref: read_str(row, "/guarded_by_ruler_ref")?,
                world_finite_count: read_i64(row, "/world_finite_count")?,
                power_rating_milli: read_i64(row, "/power_rating_milli")?,
            });
        }
        if items.is_empty() {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: "infernal_relic_vaults_runtime.catalog".to_string(),
                reason: "frozen content table holds no infernal relic Item rows; the catalog must come from lowered Calyx data, never a literal"
                    .to_string(),
            });
        }
        Ok(Self { items })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn item(&self, item_id: &str) -> Option<&InfernalRelicRuntimeSpec> {
        self.items.iter().find(|item| item.item_id == item_id)
    }
}

/// Outcome of the relic power-qualification gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelicQualification {
    Qualifies,
    Withholds {
        required_power_milli: i64,
        actual_power_milli: i64,
    },
}

/// A party may claim a relic only when effective party power (milli) meets the relic's
/// FROZEN `power_rating_milli`.
pub fn relic_power_qualifies(
    relic: &InfernalRelicRuntimeSpec,
    party_power_milli: i64,
) -> RelicQualification {
    if party_power_milli < relic.power_rating_milli {
        RelicQualification::Withholds {
            required_power_milli: relic.power_rating_milli,
            actual_power_milli: party_power_milli,
        }
    } else {
        RelicQualification::Qualifies
    }
}

/// World-finite issue gate: only `world_finite_count` copies may exist in the world.
pub fn relic_world_finite_issue(
    relic: &InfernalRelicRuntimeSpec,
    already_issued: i64,
) -> Result<(), EngineError> {
    if already_issued < 0 {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "infernal_relic_vaults_runtime.already_issued".to_string(),
            reason: "already_issued must be non-negative".to_string(),
        });
    }
    if already_issued >= relic.world_finite_count {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: format!("infernal_relic_vaults_runtime.{}", relic.item_id),
            reason: format!(
                "world-finite cap reached: issued {already_issued} of {}",
                relic.world_finite_count
            ),
        });
    }
    Ok(())
}

fn read_i64(row: &FrozenContentRow, pointer: &str) -> Result<i64, EngineError> {
    row.payload_i64(pointer)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("infernal_relic_vaults_runtime{pointer}"),
            reason: "authored payload pointer is absent or non-integer".to_string(),
        })
}

fn read_str(row: &FrozenContentRow, pointer: &str) -> Result<String, EngineError> {
    row.payload_str(pointer)
        .map(|value| value.to_string())
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("infernal_relic_vaults_runtime{pointer}"),
            reason: "authored payload pointer is absent or non-string".to_string(),
        })
}

#[cfg(test)]
#[path = "infernal_relic_vaults_runtime_tests.rs"]
mod tests;
