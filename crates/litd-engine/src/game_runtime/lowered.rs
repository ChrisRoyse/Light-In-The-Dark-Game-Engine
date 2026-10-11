//! Lowered, fingerprinted data model for the base-building & economy runtime (#1785/#837).
//!
//! The runtime host is generic (#1312/#1783): every quantity here is editor-authored and
//! lowered into a single fnv1a64-fingerprinted TOML artifact (the same idiom as the #827
//! gameplay-constants lowering). Nothing in the runtime hardcodes a cost, rate, or threshold
//! -- it all arrives through [`LoweredEconomyRuleset`]. A sabotaged cost field changes the
//! canonical TOML and therefore the fingerprint, so [`verify_lowered_economy_toml`] rejects
//! it loud (the negative control) rather than minting free units.

use serde::{Deserialize, Serialize};

use crate::EngineError;

pub const ECONOMY_LOWERING_SCHEMA_VERSION: u32 = 1;
pub const ECONOMY_LOWERING_VERSION: &str = "litd-engine.game_runtime.economy.v1";

/// A player-facing resource type. `gold` and `lumber` are the classic pair, but the model is
/// data-driven so a project can author custom resource keys (#837).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredResourceKind {
    pub key: String,
    pub display_name: String,
    pub starting_balance: u64,
}

/// A food-based upkeep tier. Income multiplier is an exact integer ratio (numerator over
/// denominator) so income math stays deterministic with no float in sim state (#85). Tier
/// bounds come from the #827 constants record (no/low/high upkeep food caps).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredUpkeepTier {
    pub name: String,
    pub food_min: u32,
    pub food_max: u32,
    pub income_numerator: u32,
    pub income_denominator: u32,
}

/// The economy fields of a lowered unit record (#818 `UnitEconomy`). The runtime consumes
/// exactly these fields to price training; it never invents a cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredUnitEconomy {
    pub unit_id: String,
    pub gold_cost: u32,
    pub lumber_cost: u32,
    pub food_cost: u32,
    pub build_time_secs: u32,
    /// Food/supply this unit provides when alive (structures use this too via a unit record).
    pub food_provided: u32,
}

/// An editor-authored resource node placed on the map grid. Workers harvest `yield_per_trip`
/// from `resource_key` until `amount_total` is exhausted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredResourceNode {
    pub node_id: String,
    pub resource_key: String,
    pub cell_x: i32,
    pub cell_z: i32,
    pub amount_total: u64,
    pub yield_per_trip: u64,
}

/// An editor-authored structure type. Footprint is a rectangle of grid cells that must all be
/// buildable + reachable. `trains` lists unit ids this structure can produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredStructureType {
    pub struct_id: String,
    pub gold_cost: u32,
    pub lumber_cost: u32,
    pub build_time_secs: u32,
    pub footprint_w: u32,
    pub footprint_h: u32,
    pub food_provided: u32,
    pub trains: Vec<String>,
}

/// The whole lowered economy ruleset for a project. Deterministic ordering is enforced on
/// lowering (all lists sorted by id) so the canonical TOML -- and thus the fingerprint -- is
/// stable regardless of authoring order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredEconomyRuleset {
    pub schema_version: u32,
    pub lowering_version: String,
    pub tick_rate: u32,
    /// Grid cells a worker traverses per second (integer, pathing speed).
    pub worker_cells_per_sec: u32,
    /// Ticks a worker spends depositing at the depot before returning to harvest.
    pub deposit_ticks: u32,
    pub resources: Vec<LoweredResourceKind>,
    pub upkeep_tiers: Vec<LoweredUpkeepTier>,
    pub units: Vec<LoweredUnitEconomy>,
    pub nodes: Vec<LoweredResourceNode>,
    pub structures: Vec<LoweredStructureType>,
    pub lowering_fingerprint: String,
}

/// Author a ruleset then lower it: sort every list deterministically, validate, and compute
/// the canonical fnv1a64 fingerprint over the fingerprint-cleared TOML preimage.
#[derive(Debug, Clone, Default)]
pub struct EconomyRulesetAuthor {
    tick_rate: u32,
    worker_cells_per_sec: u32,
    deposit_ticks: u32,
    resources: Vec<LoweredResourceKind>,
    upkeep_tiers: Vec<LoweredUpkeepTier>,
    units: Vec<LoweredUnitEconomy>,
    nodes: Vec<LoweredResourceNode>,
    structures: Vec<LoweredStructureType>,
}

impl EconomyRulesetAuthor {
    pub fn new(tick_rate: u32, worker_cells_per_sec: u32, deposit_ticks: u32) -> Self {
        Self {
            tick_rate,
            worker_cells_per_sec,
            deposit_ticks,
            ..Self::default()
        }
    }

    pub fn resource(mut self, kind: LoweredResourceKind) -> Self {
        self.resources.push(kind);
        self
    }

    pub fn upkeep_tier(mut self, tier: LoweredUpkeepTier) -> Self {
        self.upkeep_tiers.push(tier);
        self
    }

    pub fn unit(mut self, unit: LoweredUnitEconomy) -> Self {
        self.units.push(unit);
        self
    }

    pub fn node(mut self, node: LoweredResourceNode) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn structure(mut self, structure: LoweredStructureType) -> Self {
        self.structures.push(structure);
        self
    }

    /// Finalize: deterministic sort, validate, fingerprint.
    pub fn lower(mut self) -> Result<LoweredEconomyRuleset, EngineError> {
        self.resources.sort_by(|a, b| a.key.cmp(&b.key));
        self.upkeep_tiers.sort_by_key(|t| t.food_min);
        self.units.sort_by(|a, b| a.unit_id.cmp(&b.unit_id));
        self.nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        self.structures
            .sort_by(|a, b| a.struct_id.cmp(&b.struct_id));
        let mut ruleset = LoweredEconomyRuleset {
            schema_version: ECONOMY_LOWERING_SCHEMA_VERSION,
            lowering_version: ECONOMY_LOWERING_VERSION.to_string(),
            tick_rate: self.tick_rate,
            worker_cells_per_sec: self.worker_cells_per_sec,
            deposit_ticks: self.deposit_ticks,
            resources: self.resources,
            upkeep_tiers: self.upkeep_tiers,
            units: self.units,
            nodes: self.nodes,
            structures: self.structures,
            lowering_fingerprint: String::new(),
        };
        validate_ruleset(&ruleset)?;
        ruleset.lowering_fingerprint = ruleset_fingerprint(&ruleset)?;
        Ok(ruleset)
    }
}

impl LoweredEconomyRuleset {
    /// Canonical, stable RON encoding (the engine's deterministic serialization idiom). The
    /// fingerprint is fnv1a64 over this byte stream, so any authored quantity that changes the
    /// encoding changes the fingerprint.
    pub fn to_canonical_ron(&self) -> Result<String, EngineError> {
        litd_hash::to_stable_ron(self).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: format!("encode lowered economy RON failed: {error}"),
        })
    }

    pub fn resource_index(&self, key: &str) -> Option<usize> {
        self.resources.iter().position(|r| r.key == key)
    }

    pub fn unit(&self, unit_id: &str) -> Option<&LoweredUnitEconomy> {
        self.units.iter().find(|u| u.unit_id == unit_id)
    }

    pub fn structure(&self, struct_id: &str) -> Option<&LoweredStructureType> {
        self.structures.iter().find(|s| s.struct_id == struct_id)
    }
}

fn econ_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::GamePackageInvalidConfig {
        field: format!("economy.{field}"),
        reason: reason.into(),
    }
}

/// Structural validation. Every referenced resource key / unit id must resolve, tiers must be
/// contiguous and non-overlapping, and ratios must have a non-zero denominator.
pub fn validate_ruleset(ruleset: &LoweredEconomyRuleset) -> Result<(), EngineError> {
    if ruleset.schema_version != ECONOMY_LOWERING_SCHEMA_VERSION {
        return Err(econ_error("schema_version", "unexpected schema version"));
    }
    if ruleset.lowering_version != ECONOMY_LOWERING_VERSION {
        return Err(econ_error(
            "lowering_version",
            "unexpected lowering version",
        ));
    }
    if ruleset.tick_rate == 0 || ruleset.worker_cells_per_sec == 0 {
        return Err(econ_error(
            "tick_rate",
            "tick_rate and worker speed must be non-zero",
        ));
    }
    if ruleset.resources.is_empty() {
        return Err(econ_error(
            "resources",
            "at least one resource type is required",
        ));
    }
    for tier in &ruleset.upkeep_tiers {
        if tier.income_denominator == 0 {
            return Err(econ_error(
                "upkeep.denominator",
                "income denominator must be non-zero",
            ));
        }
        if tier.food_max < tier.food_min {
            return Err(econ_error("upkeep.range", "food_max must be >= food_min"));
        }
    }
    for unit in &ruleset.units {
        if unit
            .build_time_secs
            .checked_mul(ruleset.tick_rate)
            .is_none()
        {
            return Err(econ_error(
                "unit.build_time_secs",
                format!(
                    "unit {} build time {} overflows at tick rate {}",
                    unit.unit_id, unit.build_time_secs, ruleset.tick_rate
                ),
            ));
        }
    }
    for node in &ruleset.nodes {
        if ruleset.resource_index(&node.resource_key).is_none() {
            return Err(econ_error(
                "node.resource_key",
                format!("unknown resource {}", node.resource_key),
            ));
        }
        if node.yield_per_trip == 0 {
            return Err(econ_error("node.yield", "yield_per_trip must be non-zero"));
        }
    }
    for structure in &ruleset.structures {
        if structure
            .build_time_secs
            .checked_mul(ruleset.tick_rate)
            .is_none()
        {
            return Err(econ_error(
                "structure.build_time_secs",
                format!(
                    "structure {} build time {} overflows at tick rate {}",
                    structure.struct_id, structure.build_time_secs, ruleset.tick_rate
                ),
            ));
        }
        if structure.footprint_w == 0 || structure.footprint_h == 0 {
            return Err(econ_error(
                "structure.footprint",
                "footprint must be non-zero",
            ));
        }
        for trained in &structure.trains {
            if ruleset.unit(trained).is_none() {
                return Err(econ_error(
                    "structure.trains",
                    format!("unknown unit {trained}"),
                ));
            }
        }
    }
    Ok(())
}

/// Reject a lowered artifact whose recomputed fingerprint does not match the expected one.
/// This is the negative control: mutate any authored quantity and the canonical RON changes,
/// so the recomputed fingerprint diverges and load fails loud instead of silently.
pub fn verify_lowered_economy_ron(
    canonical_ron: &str,
    expected_fingerprint: &str,
) -> Result<LoweredEconomyRuleset, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return Err(econ_error(
            "fingerprint",
            "expected fingerprint must be non-empty",
        ));
    }
    let ruleset: LoweredEconomyRuleset =
        ron::from_str(canonical_ron).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: format!("parse lowered economy RON failed: {error}"),
        })?;
    validate_ruleset(&ruleset)?;
    if ruleset.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: ruleset.lowering_fingerprint,
        });
    }
    let actual = ruleset_fingerprint(&ruleset)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(ruleset)
}

fn ruleset_fingerprint(ruleset: &LoweredEconomyRuleset) -> Result<String, EngineError> {
    let mut preimage = ruleset.clone();
    preimage.lowering_fingerprint.clear();
    let text = preimage.to_canonical_ron()?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}
