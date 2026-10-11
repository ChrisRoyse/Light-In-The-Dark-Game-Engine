use std::collections::BTreeMap;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::{
    TriggerEditorProject, TriggerEditorVariable,
    support::{
        required_arg, trigger_editor_error, trigger_editor_error_value, validate_single_line,
    },
};
use crate::mcp_trigger_tools::EcaNode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerEditorVariableType {
    #[default]
    Boolean,
    Integer,
    Real,
    String,
}

pub(super) fn validate_trigger_editor_variable_value(
    variable: &TriggerEditorVariable,
    value: &str,
) -> Result<(), EngineError> {
    validate_single_line("variable.value", value)?;
    let valid = match variable.value_type {
        TriggerEditorVariableType::Boolean => matches!(value, "true" | "false"),
        TriggerEditorVariableType::Integer => value.parse::<i64>().is_ok(),
        TriggerEditorVariableType::Real => value
            .parse::<f64>()
            .map(|number| number.is_finite())
            .unwrap_or(false),
        TriggerEditorVariableType::String => true,
    };
    if !valid {
        return trigger_editor_error(
            "variable.value_type",
            format!(
                "value {value:?} does not match {:?} variable {}",
                variable.value_type, variable.name
            ),
        );
    }
    Ok(())
}

pub(super) fn validate_trigger_editor_variable_definition(
    variable: &TriggerEditorVariable,
) -> Result<(), EngineError> {
    if variable.array_values.is_empty() {
        validate_single_line("variable.initial_value", &variable.initial_value)?;
        return validate_trigger_editor_variable_value(variable, &variable.initial_value);
    }
    if !variable.initial_value.is_empty() {
        return trigger_editor_error(
            "variable.initial_value",
            "array variables must store values only in array_values",
        );
    }
    if variable.array_values.len() < 2 || variable.array_values.len() > 256 {
        return trigger_editor_error("variable.array_values", "array size must be 2..=256");
    }
    for value in &variable.array_values {
        validate_trigger_editor_variable_value(variable, value)?;
    }
    Ok(())
}

pub(super) fn trigger_variable_readback_maps(
    variables: &BTreeMap<String, TriggerEditorVariable>,
) -> (BTreeMap<String, String>, BTreeMap<String, Vec<String>>) {
    let scalars = variables
        .iter()
        .filter(|(_, variable)| variable.array_values.is_empty())
        .map(|(name, variable)| (name.clone(), variable.initial_value.clone()))
        .collect();
    let arrays = variables
        .iter()
        .filter(|(_, variable)| !variable.array_values.is_empty())
        .map(|(name, variable)| (name.clone(), variable.array_values.clone()))
        .collect();
    (scalars, arrays)
}

pub(super) fn apply_set_variable_action(
    variables: &mut BTreeMap<String, TriggerEditorVariable>,
    action: &EcaNode,
) -> Result<(), EngineError> {
    let name = required_arg(action, "variable")?;
    let value = required_arg(action, "value")?;
    let variable = variables.get(name).ok_or_else(|| {
        trigger_editor_error_value("action.variable", format!("unknown variable {name}"))
    })?;
    let index = validate_trigger_variable_action_target(
        variable,
        action.args.get("index").map(String::as_str),
    )?;
    validate_trigger_editor_variable_value(variable, value)?;
    let variable = variables.get_mut(name).expect("variable existence checked");
    if let Some(index) = index {
        variable.array_values[index] = value.to_string();
    } else {
        variable.initial_value = value.to_string();
    }
    Ok(())
}

pub fn set_trigger_variable_action_value(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    variable_name: &str,
    value: &str,
) -> Result<bool, EngineError> {
    set_trigger_variable_action_indexed_value(
        project,
        trigger_id,
        node_id,
        variable_name,
        None,
        value,
    )
}

pub fn set_trigger_variable_action_indexed_value(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    variable_name: &str,
    index: Option<&str>,
    value: &str,
) -> Result<bool, EngineError> {
    project.validate()?;
    let variable = project.variables.get(variable_name).ok_or_else(|| {
        trigger_editor_error_value(
            "edit_variable.variable",
            format!("unknown variable {variable_name}"),
        )
    })?;
    let parsed_index = validate_trigger_variable_action_target(variable, index)?;
    validate_trigger_editor_variable_value(variable, value)?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_variable.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let action_index = project.graphs[graph_index]
        .actions
        .iter()
        .position(|action| action.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_variable.node_id",
                format!("unknown action node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let action = &project.graphs[graph_index].actions[action_index];
    if action.operation != "set_variable" {
        return trigger_editor_error(
            "edit_variable.operation",
            format!(
                "action {node_id} has operation {}; expected set_variable",
                action.operation
            ),
        );
    }
    let same_variable = action.args.get("variable").map(String::as_str) == Some(variable_name);
    let same_value = action.args.get("value").map(String::as_str) == Some(value);
    let canonical_index = parsed_index.map(|index| index.to_string());
    let same_index = action.args.get("index") == canonical_index.as_ref();
    if same_variable && same_value && same_index {
        return Ok(false);
    }
    let before = project.clone();
    let action = &mut project.graphs[graph_index].actions[action_index];
    action
        .args
        .insert("variable".to_string(), variable_name.to_string());
    action.args.insert("value".to_string(), value.to_string());
    if let Some(index) = canonical_index {
        action.args.insert("index".to_string(), index);
    } else {
        action.args.remove("index");
    }
    action.variable_refs = vec![variable_name.to_string()];
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(true)
}

pub(super) fn validate_trigger_variable_action_target(
    variable: &TriggerEditorVariable,
    index: Option<&str>,
) -> Result<Option<usize>, EngineError> {
    if variable.array_values.is_empty() {
        if index.is_some() {
            return trigger_editor_error(
                "action.index",
                format!("scalar variable {} forbids an array index", variable.name),
            );
        }
        return Ok(None);
    }
    let raw = index.ok_or_else(|| {
        trigger_editor_error_value(
            "action.index",
            format!("array variable {} requires an index", variable.name),
        )
    })?;
    let parsed = raw.parse::<usize>().map_err(|_| {
        trigger_editor_error_value(
            "action.index",
            format!("invalid array index {raw:?}; expected unsigned integer"),
        )
    })?;
    if parsed >= variable.array_values.len() {
        return trigger_editor_error(
            "action.index",
            format!(
                "array index {parsed} is out of bounds for {} with size {}",
                variable.name,
                variable.array_values.len()
            ),
        );
    }
    Ok(Some(parsed))
}
