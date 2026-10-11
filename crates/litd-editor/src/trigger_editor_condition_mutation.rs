use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use super::{
    TriggerEditorProject, TriggerEditorVariableType,
    support::{trigger_editor_error, trigger_editor_error_value},
};
use crate::mcp_trigger_tools::EcaNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerConditionMove {
    Earlier,
    Later,
}

/// Move one condition by one position in its graph's authored evaluation order.
///
/// Boundary moves are exact no-ops. The complete project is restored if the
/// post-edit validation fails.
pub fn move_trigger_condition(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    direction: TriggerConditionMove,
) -> Result<bool, EngineError> {
    project.validate()?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "move_condition.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let graph = &project.graphs[graph_index];
    let mut ordered_indices = (0..graph.conditions.len()).collect::<Vec<_>>();
    ordered_indices.sort_by_key(|index| graph.conditions[*index].order);
    let position = ordered_indices
        .iter()
        .position(|index| graph.conditions[*index].node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "move_condition.node_id",
                format!("unknown condition node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let adjacent_position = match direction {
        TriggerConditionMove::Earlier if position > 0 => position - 1,
        TriggerConditionMove::Later if position + 1 < ordered_indices.len() => position + 1,
        TriggerConditionMove::Earlier | TriggerConditionMove::Later => return Ok(false),
    };
    let condition_index = ordered_indices[position];
    let adjacent_index = ordered_indices[adjacent_position];
    let before = project.clone();
    let condition_order = project.graphs[graph_index].conditions[condition_index].order;
    let adjacent_order = project.graphs[graph_index].conditions[adjacent_index].order;
    project.graphs[graph_index].conditions[condition_index].order = adjacent_order;
    project.graphs[graph_index].conditions[adjacent_index].order = condition_order;
    validate_or_restore(project, before)?;
    Ok(true)
}

/// Delete one authored condition and return the removed node as mutation evidence.
///
/// Conditions are optional, so deleting the final condition is valid. The
/// complete project is restored if the post-edit validation fails.
pub fn delete_trigger_condition(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "delete_condition.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let condition_index = project.graphs[graph_index]
        .conditions
        .iter()
        .position(|condition| condition.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "delete_condition.node_id",
                format!("unknown condition node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let before = project.clone();
    let removed = project.graphs[graph_index]
        .conditions
        .remove(condition_index);
    validate_or_restore(project, before)?;
    Ok(removed)
}

/// Append a `game_flag_true` condition using the first stable Boolean variable.
pub fn append_default_game_flag_condition(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    let variable_name = project
        .variables
        .values()
        .find(|variable| {
            variable.value_type == TriggerEditorVariableType::Boolean
                && variable.array_values.is_empty()
        })
        .map(|variable| variable.name.clone())
        .ok_or_else(|| {
            trigger_editor_error_value(
                "append_condition.defaults.variable",
                "no Boolean variable is available for Game flag is true",
            )
        })?;
    append_condition(
        project,
        trigger_id,
        "game_flag_true",
        "cond_game_flag_",
        BTreeMap::from([("flag".to_string(), variable_name.clone())]),
        vec![variable_name],
    )
}

/// Append a `unit_in_region` condition using the first stable project references.
pub fn append_default_unit_in_region_condition(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    let unit_id = first_key(
        &project.initial_units,
        "append_condition.defaults.unit_id",
        "no initial unit is available for Unit is in region",
    )?;
    let region_id = first_key(
        &project.regions,
        "append_condition.defaults.region_id",
        "no region is available for Unit is in region",
    )?;
    append_condition(
        project,
        trigger_id,
        "unit_in_region",
        "cond_unit_in_region_",
        BTreeMap::from([
            ("region_id".to_string(), region_id),
            ("unit_id".to_string(), unit_id),
        ]),
        Vec::new(),
    )
}

/// Append an `integer_greater_equal` condition with deterministic literal defaults.
pub fn append_default_integer_greater_equal_condition(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    append_condition(
        project,
        trigger_id,
        "integer_greater_equal",
        "cond_integer_greater_equal_",
        BTreeMap::from([
            ("left".to_string(), "0".to_string()),
            ("right".to_string(), "0".to_string()),
        ]),
        Vec::new(),
    )
}

fn append_condition(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    operation: &str,
    node_prefix: &str,
    args: BTreeMap<String, String>,
    variable_refs: Vec<String>,
) -> Result<EcaNode, EngineError> {
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "append_condition.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let graph = &project.graphs[graph_index];
    let mut seen_orders = BTreeSet::new();
    for condition in &graph.conditions {
        if !seen_orders.insert(condition.order) {
            return trigger_editor_error(
                "append_condition.order",
                format!("duplicate condition order {}", condition.order),
            );
        }
    }
    let order = graph
        .conditions
        .iter()
        .map(|condition| condition.order)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| {
            trigger_editor_error_value("append_condition.order", "condition order exhausted")
        })?;
    let node_id = next_condition_node_id(graph, node_prefix)?;
    let condition = EcaNode {
        node_id,
        operation: operation.to_string(),
        order,
        args,
        variable_refs,
    };
    let before = project.clone();
    project.graphs[graph_index]
        .conditions
        .push(condition.clone());
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(condition)
}

fn next_condition_node_id(
    graph: &crate::mcp_trigger_tools::EcaGraph,
    prefix: &str,
) -> Result<String, EngineError> {
    let next_suffix = graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
        .filter_map(|node| node.node_id.strip_prefix(prefix))
        .filter_map(|suffix| suffix.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "append_condition.node_id",
                "condition node id suffix exhausted",
            )
        })?;
    let node_id = format!("{prefix}{next_suffix}");
    let duplicate = graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
        .any(|node| node.node_id == node_id);
    if duplicate {
        return trigger_editor_error(
            "append_condition.node_id",
            format!("duplicate node id {node_id}"),
        );
    }
    Ok(node_id)
}

fn first_key<T>(
    values: &BTreeMap<String, T>,
    field: &str,
    reason: &str,
) -> Result<String, EngineError> {
    values
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| trigger_editor_error_value(field, reason))
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
