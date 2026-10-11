//! Authored match blueprint + its lowering through the map pipeline.
//!
//! A [`MatchBlueprint`] is the map-authored match structure (player slots,
//! force count, alliance overrides, victory conditions, the observers flag, and
//! the starting units/structures with their lowered capabilities). Authoring
//! goes through the builder here; [`MatchBlueprint::lower`] emits a canonical,
//! fingerprinted [`LoweredMatch`] artifact (stable RON + sha256, no side store),
//! and [`LoweredMatch::verify`] fails loud on any fingerprint mismatch. The
//! runtime `MatchState` is built from a verified `LoweredMatch`, never from raw
//! author state — the engine executes lowered data (#1312/#1783).

use litd_hash::{sha256_hex, to_stable_ron};
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    game_runtime::{
        groups::{UnitCapability, UnitId},
        players::{AllianceCell, ForceId, PlayerId, PlayerSlot},
        victory::VictoryCondition,
    },
};

pub const LITD_MATCH_BLUEPRINT_SCHEMA: &str = "litd.game.match_blueprint.v1";
pub const LITD_LOWERED_MATCH_SCHEMA: &str = "litd.game.lowered_match.v1";

fn blueprint_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::GamePackageInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

/// An authored alliance-matrix override (map data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoredAlliance {
    pub from: PlayerId,
    pub to: PlayerId,
    pub cell: AllianceCell,
}

/// An authored starting unit. Structures count toward melee victory/defeat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoredUnit {
    pub unit: UnitId,
    pub owner: PlayerId,
    pub is_structure: bool,
    pub capabilities: Vec<UnitCapability>,
}

/// The full map-authored match structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchBlueprint {
    pub schema: String,
    pub map_id: String,
    pub slots: Vec<PlayerSlot>,
    pub force_count: u8,
    pub alliance_overrides: Vec<AuthoredAlliance>,
    pub conditions: Vec<VictoryCondition>,
    pub observers_after_defeat: bool,
    pub units: Vec<AuthoredUnit>,
}

impl MatchBlueprint {
    /// Start a blueprint for a map. Slots + forces are validated when the roster
    /// is materialized during [`Self::lower`].
    pub fn new(map_id: impl Into<String>, slots: Vec<PlayerSlot>, force_count: ForceId) -> Self {
        Self {
            schema: LITD_MATCH_BLUEPRINT_SCHEMA.to_string(),
            map_id: map_id.into(),
            slots,
            force_count,
            alliance_overrides: Vec::new(),
            conditions: Vec::new(),
            observers_after_defeat: false,
            units: Vec::new(),
        }
    }

    pub fn author_alliance(&mut self, from: PlayerId, to: PlayerId, cell: AllianceCell) {
        self.alliance_overrides
            .push(AuthoredAlliance { from, to, cell });
    }

    pub fn author_condition(&mut self, condition: VictoryCondition) {
        self.conditions.push(condition);
    }

    pub fn set_observers_after_defeat(&mut self, value: bool) {
        self.observers_after_defeat = value;
    }

    pub fn author_unit(&mut self, unit: AuthoredUnit) {
        self.units.push(unit);
    }

    /// Structural authoring checks that do not need the runtime roster: schema
    /// tag, unique unit ids, and unit owners within the slot range.
    fn validate(&self) -> Result<(), EngineError> {
        if self.schema != LITD_MATCH_BLUEPRINT_SCHEMA {
            return Err(blueprint_error(
                "blueprint.schema",
                format!(
                    "expected {LITD_MATCH_BLUEPRINT_SCHEMA}, got {}",
                    self.schema
                ),
            ));
        }
        if self.map_id.is_empty() {
            return Err(blueprint_error("blueprint.map_id", "must be non-empty"));
        }
        let n = self.slots.len();
        let mut seen: Vec<UnitId> = Vec::with_capacity(self.units.len());
        for unit in &self.units {
            if usize::from(unit.owner) >= n {
                return Err(blueprint_error(
                    "blueprint.units.owner",
                    format!("unit {} owner {} out of range", unit.unit, unit.owner),
                ));
            }
            if seen.contains(&unit.unit) {
                return Err(blueprint_error(
                    "blueprint.units.unit",
                    format!("duplicate authored unit id {}", unit.unit),
                ));
            }
            seen.push(unit.unit);
        }
        Ok(())
    }

    /// Lower the blueprint to a canonical, fingerprinted artifact. The canonical
    /// form is stable RON (deterministic key/element order); the fingerprint is
    /// the sha256 of that RON. No side store is written — the artifact carries
    /// its own bytes + hash for the runtime to verify.
    pub fn lower(&self) -> Result<LoweredMatch, EngineError> {
        self.validate()?;
        let canonical_ron =
            to_stable_ron(self).map_err(|error| EngineError::SerializationFailed {
                format: "ron",
                reason: error.to_string(),
            })?;
        let fingerprint = sha256_hex(canonical_ron.as_bytes());
        Ok(LoweredMatch {
            schema: LITD_LOWERED_MATCH_SCHEMA.to_string(),
            map_id: self.map_id.clone(),
            canonical_ron,
            fingerprint,
        })
    }
}

/// A lowered, fingerprinted match artifact. Carries the canonical RON bytes and
/// their sha256 so the runtime can independently re-verify before executing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredMatch {
    pub schema: String,
    pub map_id: String,
    pub canonical_ron: String,
    pub fingerprint: String,
}

impl LoweredMatch {
    /// Re-parse the canonical RON and confirm it re-serializes to the same bytes
    /// and re-hashes to the recorded fingerprint. Any drift (a sabotaged or
    /// truncated artifact, a mismatched fingerprint) fails loud.
    pub fn verify(&self) -> Result<MatchBlueprint, EngineError> {
        if self.schema != LITD_LOWERED_MATCH_SCHEMA {
            return Err(blueprint_error(
                "lowered.schema",
                format!("expected {LITD_LOWERED_MATCH_SCHEMA}, got {}", self.schema),
            ));
        }
        let actual = sha256_hex(self.canonical_ron.as_bytes());
        if actual != self.fingerprint {
            return Err(blueprint_error(
                "lowered.fingerprint",
                format!(
                    "artifact fingerprint {} does not match its canonical bytes {actual}",
                    self.fingerprint
                ),
            ));
        }
        let blueprint: MatchBlueprint = ron::from_str(&self.canonical_ron).map_err(|error| {
            EngineError::SerializationFailed {
                format: "ron",
                reason: error.to_string(),
            }
        })?;
        // Re-lower to prove the parsed blueprint is byte-identical to the artifact
        // (a canonical round-trip; guards against a fingerprint computed over
        // different bytes than the ones stored).
        let relowered = blueprint.lower()?;
        if relowered.fingerprint != self.fingerprint {
            return Err(blueprint_error(
                "lowered.roundtrip",
                format!(
                    "re-lowered fingerprint {} does not match stored {}",
                    relowered.fingerprint, self.fingerprint
                ),
            ));
        }
        Ok(blueprint)
    }
}
