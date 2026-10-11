//! Trigger ECA-graph validation (#1466 split of `trigger_editor.rs`).
//!
//! Structural validation of a lowered trigger graph against its project: node identity,
//! event/condition/action operations, ref integrity and the wasm-escape guard. The project
//! types, sample builders and store I/O stay in `trigger_editor.rs`; `validate_graph` is
//! `pub(super)` so it is called unchanged.

use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::TriggerEditorProject;
use super::support::{
    parse_u32, parse_vec3, required_arg, trigger_editor_error, trigger_editor_error_value,
    validate_single_line, validate_symbol,
};
use super::trigger_editor_graph_metadata::validate_graph_metadata;
use super::trigger_editor_variables::{
    validate_trigger_editor_variable_value, validate_trigger_variable_action_target,
};
use crate::mcp_trigger_tools::{EcaGraph, EcaNode};
use crate::trigger_condition_library::validate_condition_args;
use crate::trigger_event_catalog::validate_event_args;

pub(super) fn validate_graph(
    project: &TriggerEditorProject,
    graph: &EcaGraph,
) -> Result<(), EngineError> {
    validate_symbol("graph.trigger_id", &graph.trigger_id)?;
    validate_graph_metadata(graph)?;
    if graph.events.is_empty() || graph.actions.is_empty() {
        return trigger_editor_error("graph.nodes", "event and action are required");
    }
    validate_unique_event_orders(graph)?;
    validate_unique_condition_orders(graph)?;
    validate_unique_action_orders(graph)?;
    let vars = project
        .variables
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut node_ids = BTreeSet::new();
    for node in graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
    {
        validate_node(node, &vars, &mut node_ids)?;
    }
    for event in &graph.events {
        validate_event_args(&event.operation, &event.args)?;
        validate_event_refs(project, event)?;
    }
    for condition in &graph.conditions {
        validate_condition_args(&condition.operation, &condition.args)?;
        validate_condition_refs(project, condition)?;
    }
    for action in &graph.actions {
        validate_action(project, action)?;
    }
    crate::trigger_wasm_authoring::validate_graph_wasm_escape(graph)?;
    Ok(())
}

fn validate_unique_event_orders(graph: &EcaGraph) -> Result<(), EngineError> {
    let mut orders = BTreeSet::new();
    for event in &graph.events {
        if !orders.insert(event.order) {
            return trigger_editor_error(
                "event.order",
                format!("duplicate event order {}", event.order),
            );
        }
    }
    Ok(())
}

fn validate_unique_condition_orders(graph: &EcaGraph) -> Result<(), EngineError> {
    let mut orders = BTreeSet::new();
    for condition in &graph.conditions {
        if !orders.insert(condition.order) {
            return trigger_editor_error(
                "condition.order",
                format!("duplicate condition order {}", condition.order),
            );
        }
    }
    Ok(())
}

fn validate_unique_action_orders(graph: &EcaGraph) -> Result<(), EngineError> {
    let mut orders = BTreeSet::new();
    for action in &graph.actions {
        if !orders.insert(action.order) {
            return trigger_editor_error(
                "action.order",
                format!("duplicate action order {}", action.order),
            );
        }
    }
    Ok(())
}

fn validate_condition_refs(
    project: &TriggerEditorProject,
    condition: &EcaNode,
) -> Result<(), EngineError> {
    match condition.operation.as_str() {
        "game_flag_true" => {
            let flag = required_arg(condition, "flag")?;
            let variable = project.variables.get(flag).ok_or_else(|| {
                trigger_editor_error_value("condition.flag", format!("unknown variable {flag}"))
            })?;
            if variable.value_type != super::TriggerEditorVariableType::Boolean
                || !variable.array_values.is_empty()
            {
                return trigger_editor_error(
                    "condition.flag",
                    format!("game flag {flag} must be a scalar Boolean variable"),
                );
            }
            if condition.variable_refs.as_slice() != [flag] {
                return trigger_editor_error(
                    "condition.variable_refs",
                    format!("game flag condition must reference exactly {flag}"),
                );
            }
        }
        "unit_in_region" => {
            validate_unit_region_refs(project, condition, "condition")?;
            if !condition.variable_refs.is_empty() {
                return trigger_editor_error(
                    "condition.variable_refs",
                    "Unit is in region does not accept variable references",
                );
            }
        }
        "integer_greater_equal" if !condition.variable_refs.is_empty() => {
            return trigger_editor_error(
                "condition.variable_refs",
                "Integer comparison literals do not accept variable references",
            );
        }
        _ => {}
    }
    Ok(())
}

fn validate_event_refs(project: &TriggerEditorProject, event: &EcaNode) -> Result<(), EngineError> {
    match event.operation.as_str() {
        "unit_dies" => {
            let unit = required_arg(event, "unit_id")?;
            if !project.initial_units.contains_key(unit) {
                return trigger_editor_error("event.unit_id", format!("unknown unit {unit}"));
            }
        }
        "unit_enters_region" | "unit_leaves_region" => {
            validate_unit_region_refs(project, event, "event")?;
        }
        _ => {}
    }
    Ok(())
}

fn validate_unit_region_refs(
    project: &TriggerEditorProject,
    node: &EcaNode,
    field: &str,
) -> Result<(), EngineError> {
    let unit_id = required_arg(node, "unit_id")?;
    if !project.initial_units.contains_key(unit_id) {
        return trigger_editor_error(
            format!("{field}.unit_id"),
            format!("unknown unit {unit_id}"),
        );
    }
    let region_id = required_arg(node, "region_id")?;
    if !project.regions.contains_key(region_id) {
        return trigger_editor_error(
            format!("{field}.region_id"),
            format!("unknown region {region_id}"),
        );
    }
    Ok(())
}

fn validate_action(project: &TriggerEditorProject, action: &EcaNode) -> Result<(), EngineError> {
    match action.operation.as_str() {
        "create_unit_at_point" => {
            for key in [
                "unit_id",
                "unit_type_id",
                "owner_player_id",
                "region_id",
                "x",
                "y",
                "z",
            ] {
                required_arg(action, key)?;
            }
            let ty = required_arg(action, "unit_type_id")?;
            let region = required_arg(action, "region_id")?;
            if !project.unit_types.contains_key(ty) {
                return trigger_editor_error(
                    "action.unit_type_id",
                    format!("unknown unit type {ty}"),
                );
            }
            if !project.regions.contains_key(region) {
                return trigger_editor_error(
                    "action.region_id",
                    format!("unknown region {region}"),
                );
            }
            let _ = parse_vec3(action)?;
        }
        "sleep" => {
            let ms = parse_u32(action, "duration_ms")?;
            if ms == 0 || ms > 60_000 {
                return trigger_editor_error("action.sleep", "sleep must be 1..=60000 ms");
            }
        }
        "set_variable" => {
            let name = required_arg(action, "variable")?;
            let value = required_arg(action, "value")?;
            let variable = project.variables.get(name).ok_or_else(|| {
                trigger_editor_error_value("action.variable", format!("unknown variable {name}"))
            })?;
            validate_trigger_variable_action_target(
                variable,
                action.args.get("index").map(String::as_str),
            )?;
            validate_trigger_editor_variable_value(variable, value)?;
        }
        other => {
            return trigger_editor_error("action.operation", format!("unknown action {other}"));
        }
    }
    Ok(())
}

fn validate_node(
    node: &EcaNode,
    vars: &BTreeSet<&str>,
    node_ids: &mut BTreeSet<String>,
) -> Result<(), EngineError> {
    validate_symbol("node.node_id", &node.node_id)?;
    validate_symbol("node.operation", &node.operation)?;
    if !node_ids.insert(node.node_id.clone()) {
        return trigger_editor_error("node.node_id", format!("duplicate {}", node.node_id));
    }
    for (key, value) in &node.args {
        validate_symbol("node.args.key", key)?;
        validate_single_line("node.args.value", value)?;
    }
    for variable in &node.variable_refs {
        validate_symbol("node.variable_refs", variable)?;
        if !vars.contains(variable.as_str()) {
            return trigger_editor_error(
                "node.variable_refs",
                format!("unknown variable {variable}"),
            );
        }
    }
    Ok(())
}
