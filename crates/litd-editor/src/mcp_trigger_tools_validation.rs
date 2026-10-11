use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use super::{
    EcaGraph, EcaNode, RuntimeCue, TriggerValueType, TriggerVariable, invalid_mcp_trigger,
    mcp_trigger_tools_store::McpTriggerRuntimeState,
};

const MAX_SYMBOL_LEN: usize = 96;
const MAX_TEXT_LEN: usize = 2048;
const MAX_NODES_PER_GROUP: usize = 128;

const MAX_ARRAY_SIZE: u32 = 8192;

pub(crate) fn validate_trigger_variable(variable: &TriggerVariable) -> Result<(), EngineError> {
    validate_symbol("variable.name", &variable.name)?;
    if variable.is_array {
        let size = variable.array_size.ok_or_else(|| {
            invalid_mcp_trigger(
                "variable.array_size",
                "an array variable requires a fixed array_size",
            )
        })?;
        if size == 0 || size > MAX_ARRAY_SIZE {
            return Err(invalid_mcp_trigger(
                "variable.array_size",
                format!("array_size must be between 1 and {MAX_ARRAY_SIZE}"),
            ));
        }
        if variable.initial_value.is_some() {
            return Err(invalid_mcp_trigger(
                "variable.initial_value",
                "array variables use array_initial_values, not the scalar initial_value",
            ));
        }
        if !variable.array_initial_values.is_empty()
            && variable.array_initial_values.len() as u32 != size
        {
            return Err(invalid_mcp_trigger(
                "variable.array_initial_values",
                format!(
                    "array_initial_values must be empty (default-initialized) or exactly {size} entries"
                ),
            ));
        }
        for (index, value) in variable.array_initial_values.iter().enumerate() {
            validate_value(
                &format!("variable.array_initial_values[{index}]"),
                variable.value_type,
                value,
            )?;
        }
    } else {
        if variable.array_size.is_some() {
            return Err(invalid_mcp_trigger(
                "variable.array_size",
                "array_size is only valid when is_array is true",
            ));
        }
        if !variable.array_initial_values.is_empty() {
            return Err(invalid_mcp_trigger(
                "variable.array_initial_values",
                "array_initial_values is only valid when is_array is true",
            ));
        }
        if let Some(value) = &variable.initial_value {
            validate_value("variable.initial_value", variable.value_type, value)?;
        }
    }
    Ok(())
}

/// Type-check an assignment of `value` to a declared variable at authoring time
/// (#833). A `set_variable` action whose literal does not parse as the variable's
/// declared type is rejected with a typed error here -- never a runtime surprise.
pub(crate) fn validate_variable_assignment(
    state: &McpTriggerRuntimeState,
    variable_name: &str,
    value: &str,
) -> Result<(), EngineError> {
    let variable = state
        .variables
        .iter()
        .find(|variable| variable.name == variable_name)
        .ok_or_else(|| {
            invalid_mcp_trigger(
                "assignment.variable",
                format!("set_variable targets undeclared variable {variable_name}"),
            )
        })?;
    validate_value("assignment.value", variable.value_type, value)
}

pub(super) fn validate_eca_graph(
    state: &McpTriggerRuntimeState,
    graph: &EcaGraph,
) -> Result<(), EngineError> {
    validate_symbol("graph.trigger_id", &graph.trigger_id)?;
    validate_single_line("graph.display_name", &graph.display_name)?;
    if let Some(folder) = &graph.folder_path {
        validate_single_line("graph.folder_path", folder)?;
    }
    if let Some(comment) = &graph.comment {
        validate_single_line("graph.comment", comment)?;
    }
    if graph.events.is_empty() || graph.actions.is_empty() {
        return Err(invalid_mcp_trigger(
            "graph.nodes",
            "at least one event and one action are required",
        ));
    }
    let variable_names = state
        .variables
        .iter()
        .map(|variable| variable.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut node_ids = BTreeSet::new();
    validate_nodes("events", &graph.events, &variable_names, &mut node_ids)?;
    validate_nodes(
        "conditions",
        &graph.conditions,
        &variable_names,
        &mut node_ids,
    )?;
    validate_nodes("actions", &graph.actions, &variable_names, &mut node_ids)?;
    // #833: type-check every `set_variable` assignment at authoring time.
    for action in &graph.actions {
        if action.operation == "set_variable" {
            let variable = action.args.get("variable").ok_or_else(|| {
                invalid_mcp_trigger(
                    "actions.set_variable",
                    format!(
                        "set_variable action {} needs a 'variable' arg",
                        action.node_id
                    ),
                )
            })?;
            let value = action.args.get("value").ok_or_else(|| {
                invalid_mcp_trigger(
                    "actions.set_variable",
                    format!("set_variable action {} needs a 'value' arg", action.node_id),
                )
            })?;
            validate_variable_assignment(state, variable, value)?;
        }
    }
    if let Some(wasm) = &graph.wasm_escape {
        crate::trigger_wasm_authoring::validate_materialized_wasm_escape(wasm)?;
    }
    Ok(())
}

pub(super) fn validate_runtime_cue(
    state: &McpTriggerRuntimeState,
    cue: &RuntimeCue,
) -> Result<(), EngineError> {
    validate_symbol("runtime_cue.cue_id", &cue.cue_id)?;
    validate_symbol("runtime_cue.trigger_id", &cue.trigger_id)?;
    if !state
        .graphs
        .iter()
        .any(|graph| graph.trigger_id == cue.trigger_id)
    {
        return Err(invalid_mcp_trigger(
            "runtime_cue.trigger_id",
            format!("missing trigger graph {}", cue.trigger_id),
        ));
    }
    validate_property_map("runtime_cue.properties", &cue.properties)?;
    if cue.properties.is_empty() {
        return Err(invalid_mcp_trigger(
            "runtime_cue.properties",
            "runtime cues require at least one concrete property",
        ));
    }
    Ok(())
}

pub(super) fn validate_request_identity(
    request_id: &str,
    requested_by: &str,
) -> Result<(), EngineError> {
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_trigger(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

pub(super) fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() > MAX_SYMBOL_LEN {
        return Err(invalid_mcp_trigger(
            field,
            format!("symbol must be at most {MAX_SYMBOL_LEN} bytes"),
        ));
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_mcp_trigger(field, "symbol cannot be empty"));
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(invalid_mcp_trigger(
            field,
            "symbol must be an ASCII identifier",
        ));
    }
    Ok(())
}

fn validate_nodes(
    field: &str,
    nodes: &[EcaNode],
    variable_names: &BTreeSet<&str>,
    node_ids: &mut BTreeSet<String>,
) -> Result<(), EngineError> {
    if nodes.len() > MAX_NODES_PER_GROUP {
        return Err(invalid_mcp_trigger(
            field,
            format!("node count must be at most {MAX_NODES_PER_GROUP}"),
        ));
    }
    let mut orders = BTreeSet::new();
    for node in nodes {
        validate_symbol(&format!("{field}.node_id"), &node.node_id)?;
        validate_symbol(&format!("{field}.operation"), &node.operation)?;
        validate_property_map(&format!("{field}.args"), &node.args)?;
        if !node_ids.insert(node.node_id.clone()) {
            return Err(invalid_mcp_trigger(
                field,
                format!("duplicate node id {}", node.node_id),
            ));
        }
        if !orders.insert(node.order) {
            return Err(invalid_mcp_trigger(
                field,
                format!("duplicate node order {}", node.order),
            ));
        }
        for variable in &node.variable_refs {
            validate_symbol(&format!("{field}.variable_refs"), variable)?;
            if !variable_names.contains(variable.as_str()) {
                return Err(invalid_mcp_trigger(
                    field,
                    format!(
                        "node {} references missing variable {variable}",
                        node.node_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_property_map(
    field: &str,
    properties: &BTreeMap<String, String>,
) -> Result<(), EngineError> {
    for (key, value) in properties {
        validate_symbol(field, key)?;
        validate_single_line(field, value)?;
    }
    Ok(())
}

fn validate_value(
    field: &str,
    value_type: TriggerValueType,
    value: &str,
) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    match value_type {
        TriggerValueType::Integer => value
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| invalid_mcp_trigger(field, format!("{value:?} is not a valid integer"))),
        TriggerValueType::Real => value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|_| ())
            .ok_or_else(|| invalid_mcp_trigger(field, format!("{value:?} is not a finite real"))),
        TriggerValueType::Boolean => match value {
            "true" | "false" => Ok(()),
            _ => Err(invalid_mcp_trigger(
                field,
                format!("{value:?} is not true or false"),
            )),
        },
        TriggerValueType::String => Ok(()),
        TriggerValueType::UnitId
        | TriggerValueType::RegionId
        | TriggerValueType::SoundVariable
        | TriggerValueType::UnitGroup
        | TriggerValueType::Timer => validate_symbol(field, value),
        TriggerValueType::Player => {
            let index = value.parse::<u32>().map_err(|_| {
                invalid_mcp_trigger(field, format!("{value:?} is not a player index"))
            })?;
            if index > 31 {
                return Err(invalid_mcp_trigger(
                    field,
                    format!("player index {index} is out of range 0..=31"),
                ));
            }
            Ok(())
        }
        TriggerValueType::Point => {
            let coords = value.split(',').collect::<Vec<_>>();
            if coords.len() != 3 {
                return Err(invalid_mcp_trigger(
                    field,
                    format!("{value:?} is not a 'x,y,z' point literal"),
                ));
            }
            for coord in coords {
                let parsed = coord.trim().parse::<f32>().ok().filter(|v| v.is_finite());
                if parsed.is_none() {
                    return Err(invalid_mcp_trigger(
                        field,
                        format!("point coordinate {coord:?} is not a finite real"),
                    ));
                }
            }
            Ok(())
        }
    }
}
