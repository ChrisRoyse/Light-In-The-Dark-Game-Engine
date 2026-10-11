//! #1851: lower authored ECA trigger graphs to the RUNTIME-EXECUTABLE
//! [`LoweredTriggerGraph`] the packaged `.litdgame` carries.
//!
//! This closes the root-cause gap the issue names: the pre-#1851
//! `lower_trigger_state` (organization lowering, #833/#835) dropped every
//! event/condition/action, so no authored trigger could ever fire in a packaged
//! game. This valve maps the authored `EcaGraph` nodes to the TYPED engine enums.
//! Doctrine: any authored construct outside the runtime scope is rejected
//! FAIL-LOUD with a typed error naming the construct -- NEVER silently dropped.
//!
//! Scope executed by the session evaluator:
//! - events: map_initialization, elapsed_game_time, countdown_timer_expires,
//!   unit_dies, unit_enters_region / unit_leaves_region (#1856)
//! - conditions: unit_alive, integer_equal/greater_than/greater_equal,
//!   boolean_equal, unit_in_region (#1856)
//! - actions: unit-affecting + #1855 runtime-effect set (see action lowering)
//! - variables: global Integer/Boolean/String scalars
//!
//! Unit/region references are AUTHORED OBJECT IDS, resolved here against the
//! handle maps `lower_runtime_datasets` assigns, so a dangling name fails loud
//! at lowering, never at runtime.

use std::collections::BTreeMap;

use litd_engine::EngineError;
use litd_engine::game_runtime::{
    CompareOp, FlagOperand, IntOperand, LoweredTrigger, LoweredTriggerGraph, TriggerCondition,
    TriggerEvent, TriggerValue, TriggerVarInit, lower_trigger_graph,
};

use crate::mcp_trigger_tools::{
    EcaGraph, EcaNode, McpTriggerRuntimeState, TriggerValueType, TriggerVariable,
};

fn scope_error<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("litd_editor.trigger_runtime_lowering.{field}"),
        reason: reason.into(),
    })
}

/// Lower the persisted trigger state into the runtime-executable graph. `tick_rate`
/// converts authored elapsed SECONDS to deterministic ticks; `unit_handles` maps
/// authored placed-unit object ids to runtime unit handles; `region_handles` maps
/// authored region name/id to runtime region ids (#1856).
pub fn lower_trigger_graphs_for_runtime(
    state: &McpTriggerRuntimeState,
    tick_rate: u32,
    unit_handles: &BTreeMap<String, u32>,
    region_handles: &BTreeMap<String, u32>,
) -> Result<LoweredTriggerGraph, EngineError> {
    if tick_rate == 0 {
        return scope_error("tick_rate", "tick_rate must be non-zero");
    }
    let mut variables = Vec::with_capacity(state.variables.len());
    for variable in &state.variables {
        variables.push(lower_variable(variable)?);
    }
    let mut triggers = Vec::with_capacity(state.graphs.len());
    for graph in &state.graphs {
        triggers.push(lower_graph(
            graph,
            state,
            tick_rate,
            unit_handles,
            region_handles,
        )?);
    }
    lower_trigger_graph(variables, triggers)
}

fn lower_variable(variable: &TriggerVariable) -> Result<TriggerVarInit, EngineError> {
    let name = variable.name.as_str();
    if variable.is_array {
        return scope_error(
            name,
            format!(
                "array variable {name} is outside the #1851 first-cut runtime scope \
                 (scalar integer/boolean/string only); arrays stay authoring-only for now"
            ),
        );
    }
    let initial = variable.initial_value.as_deref();
    let value = match variable.value_type {
        TriggerValueType::Integer => TriggerValue::Integer(match initial {
            Some(text) => text.trim().parse::<i64>().map_err(|error| {
                EngineError::GamePackageInvalidConfig {
                    field: format!("litd_editor.trigger_runtime_lowering.{name}"),
                    reason: format!("initial value {text:?} is not an integer: {error}"),
                }
            })?,
            None => 0,
        }),
        TriggerValueType::Boolean => TriggerValue::Flag(match initial {
            Some("true") => true,
            Some("false") | None => false,
            Some(other) => {
                return scope_error(name, format!("initial value {other:?} is not a boolean"));
            }
        }),
        TriggerValueType::String => TriggerValue::Text(initial.unwrap_or_default().to_string()),
        other => {
            return scope_error(
                name,
                format!(
                    "variable {name} has type {other:?}, outside the #1851 first-cut runtime \
                     scope (integer/boolean/string); region-typed data is #1856"
                ),
            );
        }
    };
    Ok(TriggerVarInit {
        name: name.to_string(),
        value,
    })
}

fn lower_graph(
    graph: &EcaGraph,
    state: &McpTriggerRuntimeState,
    tick_rate: u32,
    unit_handles: &BTreeMap<String, u32>,
    region_handles: &BTreeMap<String, u32>,
) -> Result<LoweredTrigger, EngineError> {
    let id = graph.trigger_id.as_str();
    if graph.wasm_escape.is_some() {
        return scope_error(
            id,
            format!(
                "trigger {id} carries a wasm escape, outside the #1851 first-cut runtime \
                 scope (typed ECA nodes only)"
            ),
        );
    }
    let mut events = Vec::new();
    for node in ordered(&graph.events) {
        events.push(lower_event(
            id,
            node,
            tick_rate,
            unit_handles,
            region_handles,
        )?);
    }
    let mut conditions = Vec::new();
    for node in ordered(&graph.conditions) {
        conditions.push(lower_condition(
            id,
            node,
            state,
            unit_handles,
            region_handles,
        )?);
    }
    let mut actions = Vec::new();
    for node in ordered(&graph.actions) {
        actions.push(lower_action(id, node, state, unit_handles)?);
    }
    Ok(LoweredTrigger {
        trigger_id: id.to_string(),
        enabled: graph.enabled,
        initially_on: graph.initially_on,
        events,
        conditions,
        actions,
    })
}

/// Authored nodes in ascending `order` -- the deterministic evaluation order.
fn ordered(nodes: &[EcaNode]) -> Vec<&EcaNode> {
    let mut sorted: Vec<&EcaNode> = nodes.iter().collect();
    sorted.sort_by_key(|node| node.order);
    sorted
}

fn arg<'a>(trigger: &str, node: &'a EcaNode, key: &str) -> Result<&'a str, EngineError> {
    node.args
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: format!(
                "litd_editor.trigger_runtime_lowering.{trigger}.{}",
                node.node_id
            ),
            reason: format!("{} node needs a {key:?} arg", node.operation),
        })
}

fn parse_u32(trigger: &str, node: &EcaNode, key: &str) -> Result<u32, EngineError> {
    let text = arg(trigger, node, key)?;
    text.trim()
        .parse::<u32>()
        .map_err(|error| EngineError::GamePackageInvalidConfig {
            field: format!(
                "litd_editor.trigger_runtime_lowering.{trigger}.{}",
                node.node_id
            ),
            reason: format!("{key} {text:?} is not a u32: {error}"),
        })
}

fn parse_i32(trigger: &str, node: &EcaNode, key: &str) -> Result<i32, EngineError> {
    let text = arg(trigger, node, key)?;
    text.trim()
        .parse::<i32>()
        .map_err(|error| EngineError::GamePackageInvalidConfig {
            field: format!(
                "litd_editor.trigger_runtime_lowering.{trigger}.{}",
                node.node_id
            ),
            reason: format!("{key} {text:?} is not an i32: {error}"),
        })
}

fn resolve_unit(
    trigger: &str,
    node: &EcaNode,
    unit_handles: &BTreeMap<String, u32>,
) -> Result<u32, EngineError> {
    let name = arg(trigger, node, "unit_id")?;
    unit_handles
        .get(name)
        .copied()
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: format!(
                "litd_editor.trigger_runtime_lowering.{trigger}.{}",
                node.node_id
            ),
            reason: format!(
                "unit_id {name:?} does not name a placed unit object (no runtime handle)"
            ),
        })
}

fn resolve_region(
    trigger: &str,
    node: &EcaNode,
    region_handles: &BTreeMap<String, u32>,
) -> Result<u32, EngineError> {
    let name = arg(trigger, node, "region")?;
    region_handles
        .get(name)
        .copied()
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: format!(
                "litd_editor.trigger_runtime_lowering.{trigger}.{}",
                node.node_id
            ),
            reason: format!(
                "region {name:?} does not name an authored map region (no runtime handle)"
            ),
        })
}

fn lower_event(
    trigger: &str,
    node: &EcaNode,
    tick_rate: u32,
    unit_handles: &BTreeMap<String, u32>,
    region_handles: &BTreeMap<String, u32>,
) -> Result<TriggerEvent, EngineError> {
    match node.operation.as_str() {
        "map_initialization" => Ok(TriggerEvent::MapInitialization),
        "elapsed_game_time" => {
            let text = arg(trigger, node, "elapsed_seconds")?;
            let seconds =
                text.trim()
                    .parse::<u64>()
                    .map_err(|_| EngineError::GamePackageInvalidConfig {
                        field: format!(
                            "litd_editor.trigger_runtime_lowering.{trigger}.{}",
                            node.node_id
                        ),
                        reason: format!(
                            "elapsed_seconds {text:?} must be a whole non-negative integer \
                         (fractional seconds are outside the #1851 first-cut scope)"
                        ),
                    })?;
            let elapsed_ticks = seconds
                .checked_mul(u64::from(tick_rate))
                .ok_or_else(|| EngineError::GamePackageInvalidConfig {
                    field: format!("litd_editor.trigger_runtime_lowering.{trigger}"),
                    reason: format!("elapsed_seconds {seconds} overflows the tick clock"),
                })?
                .max(1);
            Ok(TriggerEvent::ElapsedGameTime { elapsed_ticks })
        }
        "countdown_timer_expires" => Ok(TriggerEvent::CountdownTimerExpires {
            timer: parse_u32(trigger, node, "timer_id")?,
        }),
        "unit_dies" => Ok(TriggerEvent::UnitDies {
            unit: resolve_unit(trigger, node, unit_handles)?,
        }),
        "unit_enters_region" => Ok(TriggerEvent::UnitEntersRegion {
            region: resolve_region(trigger, node, region_handles)?,
        }),
        "unit_leaves_region" => Ok(TriggerEvent::UnitLeavesRegion {
            region: resolve_region(trigger, node, region_handles)?,
        }),
        "region_becomes_active" => scope_error(
            trigger,
            format!(
                "event {:?} is not in the #1856 runtime scope (use unit_enters_region / \
                 unit_leaves_region)",
                node.operation
            ),
        ),
        other => scope_error(
            trigger,
            format!(
                "event {other:?} is outside the runtime scope \
                 (map_initialization, elapsed_game_time, countdown_timer_expires, unit_dies, \
                  unit_enters_region, unit_leaves_region)"
            ),
        ),
    }
}

/// Resolve an authored operand string: a declared variable name wins; otherwise it
/// must parse as a literal of the required type.
fn int_operand(
    trigger: &str,
    node: &EcaNode,
    state: &McpTriggerRuntimeState,
    key: &str,
) -> Result<IntOperand, EngineError> {
    let text = arg(trigger, node, key)?;
    if let Some(variable) = state.variables.iter().find(|v| v.name == text) {
        if variable.value_type != TriggerValueType::Integer || variable.is_array {
            return scope_error(
                trigger,
                format!("operand {text:?} must be a scalar Integer variable"),
            );
        }
        return Ok(IntOperand::Variable(text.to_string()));
    }
    match text.trim().parse::<i64>() {
        Ok(value) => Ok(IntOperand::Literal(value)),
        Err(_) => scope_error(
            trigger,
            format!("operand {text:?} is neither a declared variable nor an integer literal"),
        ),
    }
}

fn flag_operand(
    trigger: &str,
    node: &EcaNode,
    state: &McpTriggerRuntimeState,
    key: &str,
) -> Result<FlagOperand, EngineError> {
    let text = arg(trigger, node, key)?;
    if let Some(variable) = state.variables.iter().find(|v| v.name == text) {
        if variable.value_type != TriggerValueType::Boolean || variable.is_array {
            return scope_error(
                trigger,
                format!("operand {text:?} must be a scalar Boolean variable"),
            );
        }
        return Ok(FlagOperand::Variable(text.to_string()));
    }
    match text.trim() {
        "true" => Ok(FlagOperand::Literal(true)),
        "false" => Ok(FlagOperand::Literal(false)),
        _ => scope_error(
            trigger,
            format!("operand {text:?} is neither a declared variable nor true/false"),
        ),
    }
}

fn lower_condition(
    trigger: &str,
    node: &EcaNode,
    state: &McpTriggerRuntimeState,
    unit_handles: &BTreeMap<String, u32>,
    region_handles: &BTreeMap<String, u32>,
) -> Result<TriggerCondition, EngineError> {
    let compare = |op: CompareOp| -> Result<TriggerCondition, EngineError> {
        Ok(TriggerCondition::IntegerCompare {
            op,
            left: int_operand(trigger, node, state, "left")?,
            right: int_operand(trigger, node, state, "right")?,
        })
    };
    match node.operation.as_str() {
        "unit_alive" => Ok(TriggerCondition::UnitAlive {
            unit: resolve_unit(trigger, node, unit_handles)?,
        }),
        "integer_equal" => compare(CompareOp::Equal),
        "integer_greater_than" => compare(CompareOp::GreaterThan),
        "integer_greater_equal" => compare(CompareOp::GreaterEqual),
        "boolean_equal" => Ok(TriggerCondition::FlagEquals {
            left: flag_operand(trigger, node, state, "left")?,
            right: flag_operand(trigger, node, state, "right")?,
        }),
        "unit_in_region" => Ok(TriggerCondition::UnitInRegion {
            unit: resolve_unit(trigger, node, unit_handles)?,
            region: resolve_region(trigger, node, region_handles)?,
        }),
        other => scope_error(
            trigger,
            format!(
                "condition {other:?} is outside the runtime scope \
                 (unit_alive, integer_equal/greater_than/greater_equal, boolean_equal, \
                  unit_in_region)"
            ),
        ),
    }
}

// #1855: the action-lowering half (unit-affecting + runtime-effect actions) lives in
// its own file so this module stays modular as the effect vocabulary grows.
#[path = "trigger_runtime_action_lowering.rs"]
mod action_lowering;
use action_lowering::lower_action;

#[cfg(test)]
#[path = "trigger_runtime_lowering_tests.rs"]
mod tests;
