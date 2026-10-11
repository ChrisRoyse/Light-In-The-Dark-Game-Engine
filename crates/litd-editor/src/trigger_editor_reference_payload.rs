//! Transactional editors for reference-backed event and condition payloads.

use litd_engine::EngineError;

use super::{
    TriggerEditorProject, TriggerEditorVariableType,
    support::{trigger_editor_error, trigger_editor_error_value},
};

pub fn set_unit_dies_event_unit(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    unit_id: &str,
) -> Result<bool, EngineError> {
    project.validate()?;
    if !project.initial_units.contains_key(unit_id) {
        return trigger_editor_error(
            "edit_event.unit_id",
            format!("unknown initial unit {unit_id}"),
        );
    }
    let graph_index = graph_index(project, trigger_id, "edit_event")?;
    let event_index = project.graphs[graph_index]
        .events
        .iter()
        .position(|event| event.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_event.node_id",
                format!("unknown event node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let event = &project.graphs[graph_index].events[event_index];
    require_operation("edit_event", node_id, &event.operation, "unit_dies")?;
    if event.args.get("unit_id").map(String::as_str) == Some(unit_id) {
        return Ok(false);
    }

    let before = project.clone();
    project.graphs[graph_index].events[event_index]
        .args
        .insert("unit_id".to_string(), unit_id.to_string());
    validate_or_restore(project, before)?;
    Ok(true)
}

pub fn set_unit_enters_region_payload(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    unit_id: &str,
    region_id: &str,
) -> Result<bool, EngineError> {
    project.validate()?;
    validate_unit_region_refs(project, "edit_event", unit_id, region_id)?;
    let graph_index = graph_index(project, trigger_id, "edit_event")?;
    let event_index = project.graphs[graph_index]
        .events
        .iter()
        .position(|event| event.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_event.node_id",
                format!("unknown event node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let event = &project.graphs[graph_index].events[event_index];
    require_operation(
        "edit_event",
        node_id,
        &event.operation,
        "unit_enters_region",
    )?;
    if has_unit_region_args(event, unit_id, region_id) {
        return Ok(false);
    }

    let before = project.clone();
    set_unit_region_args(
        &mut project.graphs[graph_index].events[event_index],
        unit_id,
        region_id,
    );
    validate_or_restore(project, before)?;
    Ok(true)
}

pub fn set_game_flag_condition_variable(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    variable_name: &str,
) -> Result<bool, EngineError> {
    project.validate()?;
    let variable = project.variables.get(variable_name).ok_or_else(|| {
        trigger_editor_error_value(
            "edit_condition.variable",
            format!("unknown variable {variable_name}"),
        )
    })?;
    if variable.value_type != TriggerEditorVariableType::Boolean
        || !variable.array_values.is_empty()
    {
        return trigger_editor_error(
            "edit_condition.variable",
            format!("game flag {variable_name} must be a scalar Boolean variable"),
        );
    }
    let graph_index = graph_index(project, trigger_id, "edit_condition")?;
    let condition_index = project.graphs[graph_index]
        .conditions
        .iter()
        .position(|condition| condition.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_condition.node_id",
                format!("unknown condition node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let condition = &project.graphs[graph_index].conditions[condition_index];
    require_operation(
        "edit_condition",
        node_id,
        &condition.operation,
        "game_flag_true",
    )?;
    let expected_refs = [variable_name.to_string()];
    if condition.args.get("flag").map(String::as_str) == Some(variable_name)
        && condition.variable_refs == expected_refs
    {
        return Ok(false);
    }

    let before = project.clone();
    let condition = &mut project.graphs[graph_index].conditions[condition_index];
    condition
        .args
        .insert("flag".to_string(), variable_name.to_string());
    condition.variable_refs = expected_refs.into();
    validate_or_restore(project, before)?;
    Ok(true)
}

pub fn set_unit_in_region_condition_payload(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    unit_id: &str,
    region_id: &str,
) -> Result<bool, EngineError> {
    project.validate()?;
    validate_unit_region_refs(project, "edit_condition", unit_id, region_id)?;
    let graph_index = graph_index(project, trigger_id, "edit_condition")?;
    let condition_index = project.graphs[graph_index]
        .conditions
        .iter()
        .position(|condition| condition.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_condition.node_id",
                format!("unknown condition node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let condition = &project.graphs[graph_index].conditions[condition_index];
    require_operation(
        "edit_condition",
        node_id,
        &condition.operation,
        "unit_in_region",
    )?;
    if has_unit_region_args(condition, unit_id, region_id) {
        return Ok(false);
    }

    let before = project.clone();
    set_unit_region_args(
        &mut project.graphs[graph_index].conditions[condition_index],
        unit_id,
        region_id,
    );
    validate_or_restore(project, before)?;
    Ok(true)
}

fn validate_unit_region_refs(
    project: &TriggerEditorProject,
    field: &str,
    unit_id: &str,
    region_id: &str,
) -> Result<(), EngineError> {
    if !project.initial_units.contains_key(unit_id) {
        return trigger_editor_error(
            format!("{field}.unit_id"),
            format!("unknown initial unit {unit_id}"),
        );
    }
    if !project.regions.contains_key(region_id) {
        return trigger_editor_error(
            format!("{field}.region_id"),
            format!("unknown region {region_id}"),
        );
    }
    Ok(())
}

fn has_unit_region_args(
    node: &crate::mcp_trigger_tools::EcaNode,
    unit_id: &str,
    region_id: &str,
) -> bool {
    node.args.get("unit_id").map(String::as_str) == Some(unit_id)
        && node.args.get("region_id").map(String::as_str) == Some(region_id)
}

fn set_unit_region_args(
    node: &mut crate::mcp_trigger_tools::EcaNode,
    unit_id: &str,
    region_id: &str,
) {
    node.args.insert("unit_id".to_string(), unit_id.to_string());
    node.args
        .insert("region_id".to_string(), region_id.to_string());
}

fn graph_index(
    project: &TriggerEditorProject,
    trigger_id: &str,
    field: &str,
) -> Result<usize, EngineError> {
    project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                format!("{field}.trigger_id"),
                format!("unknown trigger graph {trigger_id}"),
            )
        })
}

fn require_operation(
    field: &str,
    node_id: &str,
    actual: &str,
    expected: &str,
) -> Result<(), EngineError> {
    if actual != expected {
        return trigger_editor_error(
            format!("{field}.operation"),
            format!("node {node_id} has operation {actual}; expected {expected}"),
        );
    }
    Ok(())
}

fn validate_or_restore(
    project: &mut TriggerEditorProject,
    before: TriggerEditorProject,
) -> Result<(), EngineError> {
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(())
}
