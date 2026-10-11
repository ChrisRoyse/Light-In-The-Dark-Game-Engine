//! Transactional editors for scalar time-event and integer-condition payloads.

use litd_engine::EngineError;

use super::{
    TriggerEditorProject,
    support::{trigger_editor_error, trigger_editor_error_value},
};

pub fn set_time_event_seconds(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    seconds: f32,
) -> Result<bool, EngineError> {
    project.validate()?;
    let graph_index = graph_index(project, trigger_id, "edit_time_event")?;
    let event_index = project.graphs[graph_index]
        .events
        .iter()
        .position(|event| event.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_time_event.node_id",
                format!("unknown event node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let event = &project.graphs[graph_index].events[event_index];
    let (arg_name, minimum, description) = match event.operation.as_str() {
        "elapsed_game_time" => ("elapsed_seconds", 0.0, "elapsed time"),
        "periodic_timer" => ("period_seconds", f32::MIN_POSITIVE, "timer period"),
        operation => {
            return trigger_editor_error(
                "edit_time_event.operation",
                format!(
                    "event {node_id} has operation {operation}; expected elapsed_game_time or periodic_timer"
                ),
            );
        }
    };
    if !seconds.is_finite() || seconds < minimum {
        return trigger_editor_error(
            "edit_time_event.seconds",
            format!("{description} must be finite and at least {minimum}, got {seconds}"),
        );
    }
    let canonical = seconds.to_string();
    if event.args.get(arg_name) == Some(&canonical) {
        return Ok(false);
    }

    let before = project.clone();
    project.graphs[graph_index].events[event_index]
        .args
        .insert(arg_name.to_string(), canonical);
    validate_or_restore(project, before)?;
    Ok(true)
}

pub fn set_integer_greater_equal_payload(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    left: i64,
    right: i64,
) -> Result<bool, EngineError> {
    project.validate()?;
    let graph_index = graph_index(project, trigger_id, "edit_integer_condition")?;
    let condition_index = project.graphs[graph_index]
        .conditions
        .iter()
        .position(|condition| condition.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_integer_condition.node_id",
                format!("unknown condition node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let condition = &project.graphs[graph_index].conditions[condition_index];
    if condition.operation != "integer_greater_equal" {
        return trigger_editor_error(
            "edit_integer_condition.operation",
            format!(
                "condition {node_id} has operation {}; expected integer_greater_equal",
                condition.operation
            ),
        );
    }
    let left = left.to_string();
    let right = right.to_string();
    if condition.args.get("left") == Some(&left) && condition.args.get("right") == Some(&right) {
        return Ok(false);
    }

    let before = project.clone();
    let condition = &mut project.graphs[graph_index].conditions[condition_index];
    condition.args.insert("left".to_string(), left);
    condition.args.insert("right".to_string(), right);
    condition.variable_refs.clear();
    validate_or_restore(project, before)?;
    Ok(true)
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
