//! #1851: the lowered EVENT/CONDITION/ACTION trigger graph a packaged `.litdgame`
//! carries so authored triggers EXECUTE at runtime (previously `lower_trigger_state`
//! dropped every ECA node, so no authored trigger could ever fire in a packaged game).
//!
//! A [`LoweredTriggerGraph`] is pure DATA (editor-authored, lowered through the
//! editor trigger-lowering valve, never hardcoded in Rust per #1312/#1783): a typed
//! variable table plus per-trigger typed events, conditions, and actions. Every node
//! is a closed enum -- the #1851 first-cut scope -- and the EDITOR lowering rejects
//! any authored construct outside it FAIL-LOUD (it never silently drops a node the
//! way the pre-#1851 lowering did). #1856 adds `UnitEntersRegion` / `UnitLeavesRegion`
//! events and `UnitInRegion` conditions over the fingerprinted region set.
//!
//! Self-fingerprinted with the same idiom as [`super::ai_profile::LoweredAiProfile`]:
//! fnv1a64 over the canonical RON with the fingerprint cleared. A mutated node
//! changes this fingerprint, changes the binding manifest fingerprint, and is
//! rejected typed at package load.

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::blueprint::MatchBlueprint;

#[path = "trigger_graph_economy.rs"]
mod trigger_graph_economy;
pub use trigger_graph_economy::validate_trigger_graph_against_economy;

pub const LITD_TRIGGER_GRAPH_SCHEMA: &str = "litd.game.trigger_graph.v1";

pub(super) fn graph_error<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("litd_game.package.trigger_graph.{field}"),
        reason: reason.into(),
    })
}

/// A runtime trigger value: the #1851 first-cut variable types (integer, flag,
/// string). Deterministic (no floats) and hashable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerValue {
    Integer(i64),
    Flag(bool),
    Text(String),
}

impl TriggerValue {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Integer(_) => "integer",
            Self::Flag(_) => "flag",
            Self::Text(_) => "text",
        }
    }

    pub(crate) fn fold_hash(&self, hasher: &mut Fnv1a64) {
        match self {
            Self::Integer(value) => {
                hasher.update_u64_le(1);
                hasher.update_u64_le(*value as u64);
            }
            Self::Flag(value) => {
                hasher.update_u64_le(2);
                hasher.update_u64_le(u64::from(*value));
            }
            Self::Text(value) => {
                hasher.update_u64_le(3);
                hasher.update_u64_le(value.len() as u64);
                hasher.update(value.as_bytes());
            }
        }
    }
}

/// One typed variable declaration + its initial value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerVarInit {
    pub name: String,
    pub value: TriggerValue,
}

/// A runtime trigger event (the WHEN).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerEvent {
    /// Fires exactly once, on the first session tick.
    MapInitialization,
    /// Fires exactly once, when the session tick reaches `elapsed_ticks` (>=1).
    ElapsedGameTime { elapsed_ticks: u64 },
    /// Fires on every expiry of the HUD countdown timer with this id (#841).
    CountdownTimerExpires { timer: u32 },
    /// Fires when the blueprint unit transitions alive -> dead.
    UnitDies { unit: u32 },
    /// #1856: any field unit transitions outside -> inside this region this tick.
    UnitEntersRegion { region: u32 },
    /// #1856: any field unit transitions inside -> outside this region this tick.
    UnitLeavesRegion { region: u32 },
}

/// Integer comparison operator for [`TriggerCondition::IntegerCompare`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Equal,
    GreaterThan,
    GreaterEqual,
}

/// An integer operand: a literal or a declared integer variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntOperand {
    Literal(i64),
    Variable(String),
}

/// A flag (boolean) operand: a literal or a declared flag variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlagOperand {
    Literal(bool),
    Variable(String),
}

/// A runtime trigger condition (the IF). All conditions of a trigger must hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerCondition {
    UnitAlive {
        unit: u32,
    },
    IntegerCompare {
        op: CompareOp,
        left: IntOperand,
        right: IntOperand,
    },
    FlagEquals {
        left: FlagOperand,
        right: FlagOperand,
    },
    /// #1856: the named field unit's cell is inside the region right now.
    UnitInRegion {
        unit: u32,
        region: u32,
    },
}

// #1855: the trigger ACTION vocabulary grew past the #1851 unit-affecting set to the
// WC3-parity runtime-effect set, so it lives in its own file to keep this one modular.
#[path = "trigger_actions.rs"]
mod trigger_actions;
pub use trigger_actions::TriggerAction;
use trigger_actions::validate_action_against_blueprint;

/// One lowered trigger: organization flags + typed ECA nodes, evaluated in the
/// authored node order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTrigger {
    pub trigger_id: String,
    pub enabled: bool,
    pub initially_on: bool,
    pub events: Vec<TriggerEvent>,
    pub conditions: Vec<TriggerCondition>,
    pub actions: Vec<TriggerAction>,
}

/// The full lowered trigger dataset carried in the runtime datasets manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTriggerGraph {
    pub schema: String,
    /// Sorted by name, unique. Initial values seed the runtime variable store.
    pub variables: Vec<TriggerVarInit>,
    /// Evaluated in this (authored) order every tick -- the determinism spine.
    pub triggers: Vec<LoweredTrigger>,
    pub graph_fingerprint: String,
}

impl LoweredTriggerGraph {
    /// fnv1a64 hex over the canonical RON with the fingerprint field cleared -- the
    /// same idiom the economy ruleset / runtime constants / AI profiles use.
    pub fn compute_fingerprint(&self) -> Result<String, EngineError> {
        let mut preimage = self.clone();
        preimage.graph_fingerprint.clear();
        let ron = litd_hash::to_stable_ron(&preimage).map_err(|error| {
            EngineError::SerializationFailed {
                format: "ron",
                reason: format!("encode trigger graph RON failed: {error}"),
            }
        })?;
        Ok(litd_hash::fnv1a64_hex(ron.as_bytes()))
    }

    pub fn variable(&self, name: &str) -> Option<&TriggerVarInit> {
        self.variables
            .binary_search_by(|v| v.name.as_str().cmp(name))
            .ok()
            .map(|index| &self.variables[index])
    }

    /// Structural + fingerprint validation. Cross-dataset unit-reference checks live
    /// in [`validate_trigger_graph_against_blueprint`], which has the blueprint.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema != LITD_TRIGGER_GRAPH_SCHEMA {
            return graph_error(
                "schema",
                format!("expected {LITD_TRIGGER_GRAPH_SCHEMA}, got {}", self.schema),
            );
        }
        for pair in self.variables.windows(2) {
            if pair[0].name >= pair[1].name {
                return graph_error(
                    "variables",
                    format!(
                        "variable table must be strictly sorted by name; {} >= {}",
                        pair[0].name, pair[1].name
                    ),
                );
            }
        }
        for variable in &self.variables {
            if variable.name.trim().is_empty() {
                return graph_error("variables.name", "variable name must be non-empty");
            }
        }
        let mut seen_ids: Vec<&str> = Vec::new();
        for trigger in &self.triggers {
            if trigger.trigger_id.trim().is_empty() {
                return graph_error("trigger_id", "trigger id must be non-empty");
            }
            if seen_ids.contains(&trigger.trigger_id.as_str()) {
                return graph_error(
                    "trigger_id",
                    format!("duplicate trigger id {}", trigger.trigger_id),
                );
            }
            seen_ids.push(&trigger.trigger_id);
            self.validate_trigger(trigger)?;
        }
        let expected = self.compute_fingerprint()?;
        if self.graph_fingerprint != expected {
            return Err(EngineError::SerializedFingerprintMismatch {
                expected,
                actual: self.graph_fingerprint.clone(),
            });
        }
        Ok(())
    }

    fn validate_trigger(&self, trigger: &LoweredTrigger) -> Result<(), EngineError> {
        let id = &trigger.trigger_id;
        if trigger.events.is_empty() || trigger.actions.is_empty() {
            return graph_error(
                id,
                "a trigger needs at least one event and one action (it could never run)",
            );
        }
        for event in &trigger.events {
            if let TriggerEvent::ElapsedGameTime { elapsed_ticks } = event
                && *elapsed_ticks == 0
            {
                return graph_error(id, "elapsed_game_time needs elapsed_ticks >= 1");
            }
        }
        for condition in &trigger.conditions {
            match condition {
                TriggerCondition::UnitAlive { .. } => {}
                TriggerCondition::IntegerCompare { left, right, .. } => {
                    self.check_int_operand(id, left)?;
                    self.check_int_operand(id, right)?;
                }
                TriggerCondition::FlagEquals { left, right } => {
                    self.check_flag_operand(id, left)?;
                    self.check_flag_operand(id, right)?;
                }
                TriggerCondition::UnitInRegion { .. } => {}
            }
        }
        for action in &trigger.actions {
            if let TriggerAction::SetVariable { name, value } = action {
                let declared =
                    self.variable(name)
                        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
                            field: format!("litd_game.package.trigger_graph.{id}"),
                            reason: format!("set_variable targets undeclared variable {name}"),
                        })?;
                if declared.value.kind() != value.kind() {
                    return graph_error(
                        id,
                        format!(
                            "set_variable assigns {} to variable {name} declared {}",
                            value.kind(),
                            declared.value.kind()
                        ),
                    );
                }
            }
        }
        Ok(())
    }

    fn check_int_operand(&self, id: &str, operand: &IntOperand) -> Result<(), EngineError> {
        if let IntOperand::Variable(name) = operand {
            match self.variable(name) {
                Some(init) if matches!(init.value, TriggerValue::Integer(_)) => Ok(()),
                Some(init) => graph_error(
                    id,
                    format!(
                        "integer operand references variable {name} of type {}",
                        init.value.kind()
                    ),
                ),
                None => graph_error(
                    id,
                    format!("integer operand references undeclared variable {name}"),
                ),
            }
        } else {
            Ok(())
        }
    }

    fn check_flag_operand(&self, id: &str, operand: &FlagOperand) -> Result<(), EngineError> {
        if let FlagOperand::Variable(name) = operand {
            match self.variable(name) {
                Some(init) if matches!(init.value, TriggerValue::Flag(_)) => Ok(()),
                Some(init) => graph_error(
                    id,
                    format!(
                        "flag operand references variable {name} of type {}",
                        init.value.kind()
                    ),
                ),
                None => graph_error(
                    id,
                    format!("flag operand references undeclared variable {name}"),
                ),
            }
        } else {
            Ok(())
        }
    }
}

/// Assemble + fingerprint + validate a lowered trigger graph. Sorts the variable
/// table by name (rejecting duplicates loud) so the canonical form is unique.
/// Called by the editor lowering valve; the in-Rust call sites in engine tests are
/// not the production source (authored data is).
pub fn lower_trigger_graph(
    mut variables: Vec<TriggerVarInit>,
    triggers: Vec<LoweredTrigger>,
) -> Result<LoweredTriggerGraph, EngineError> {
    variables.sort_by(|a, b| a.name.cmp(&b.name));
    for pair in variables.windows(2) {
        if pair[0].name == pair[1].name {
            return graph_error(
                "variables",
                format!("duplicate variable declaration {}", pair[0].name),
            );
        }
    }
    let mut graph = LoweredTriggerGraph {
        schema: LITD_TRIGGER_GRAPH_SCHEMA.to_string(),
        variables,
        triggers,
        graph_fingerprint: String::new(),
    };
    graph.graph_fingerprint = graph.compute_fingerprint()?;
    graph.validate()?;
    Ok(graph)
}

/// Cross-dataset reference checks, run by `validate_runtime_datasets` with the
/// verified match blueprint in hand: every unit a trigger names must exist in the
/// blueprint; move/order targets must be mobile (non-structure) units; an ordered
/// unit must carry a point-target ability. A dangling or impossible reference is
/// rejected typed at package load, never a runtime surprise.
pub fn validate_trigger_graph_against_blueprint(
    graph: &LoweredTriggerGraph,
    blueprint: &MatchBlueprint,
) -> Result<(), EngineError> {
    let find = |unit: u32| blueprint.units.iter().find(|u| u.unit == unit);
    let require = |id: &str, unit: u32, role: &str| match find(unit) {
        Some(authored) => Ok(authored),
        None => graph_error(
            id,
            format!("{role} references unit {unit} absent from the match blueprint"),
        ),
    };
    for trigger in &graph.triggers {
        let id = trigger.trigger_id.as_str();
        for event in &trigger.events {
            if let TriggerEvent::UnitDies { unit } = event {
                require(id, *unit, "unit_dies event")?;
            }
        }
        for condition in &trigger.conditions {
            match condition {
                TriggerCondition::UnitAlive { unit } => {
                    require(id, *unit, "unit_alive condition")?;
                }
                TriggerCondition::UnitInRegion { unit, .. } => {
                    require(id, *unit, "unit_in_region condition")?;
                }
                _ => {}
            }
        }
        for action in &trigger.actions {
            validate_action_against_blueprint(id, action, blueprint)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "trigger_graph_tests.rs"]
mod trigger_graph_tests;
