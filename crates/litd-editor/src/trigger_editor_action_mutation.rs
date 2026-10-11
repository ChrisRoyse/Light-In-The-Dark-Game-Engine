use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use super::{
    TriggerEditorProject,
    support::{trigger_editor_error, trigger_editor_error_value, validate_symbol},
};
use crate::mcp_trigger_tools::{EcaGraph, EcaNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerActionMove {
    Earlier,
    Later,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerActionDraft {
    pub node_id: String,
    pub operation: String,
    pub args: BTreeMap<String, String>,
    pub variable_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerActionPaletteKind {
    Wait,
    CreateUnitAtPoint,
    SetVariable,
}

/// Move one action by one position in its graph's authored execution order.
///
/// The project is validated before and after the edit. Duplicate action orders
/// are rejected because they make "adjacent" ambiguous, and a failed post-edit
/// validation restores the complete pre-edit project.
pub fn move_trigger_action(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    direction: TriggerActionMove,
) -> Result<bool, EngineError> {
    project.validate()?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "move_action.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let graph = &project.graphs[graph_index];
    validate_unique_action_orders(graph, "move_action.order")?;
    let mut ordered_indices = (0..graph.actions.len()).collect::<Vec<_>>();
    ordered_indices.sort_by_key(|index| graph.actions[*index].order);
    let position = ordered_indices
        .iter()
        .position(|index| graph.actions[*index].node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "move_action.node_id",
                format!("unknown action node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let adjacent_position = match direction {
        TriggerActionMove::Earlier if position > 0 => position - 1,
        TriggerActionMove::Later if position + 1 < ordered_indices.len() => position + 1,
        TriggerActionMove::Earlier | TriggerActionMove::Later => return Ok(false),
    };
    let action_index = ordered_indices[position];
    let adjacent_index = ordered_indices[adjacent_position];
    let before = project.clone();
    let action_order = project.graphs[graph_index].actions[action_index].order;
    let adjacent_order = project.graphs[graph_index].actions[adjacent_index].order;
    project.graphs[graph_index].actions[action_index].order = adjacent_order;
    project.graphs[graph_index].actions[adjacent_index].order = action_order;
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(true)
}

/// Delete one authored action while preserving the graph's required-action invariant.
///
/// The removed node is returned as explicit mutation evidence. A failed post-edit
/// validation restores the complete pre-edit project.
pub fn delete_trigger_action(
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
                "delete_action.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let graph = &project.graphs[graph_index];
    let action_index = graph
        .actions
        .iter()
        .position(|action| action.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "delete_action.node_id",
                format!("unknown action node {node_id} in trigger {trigger_id}"),
            )
        })?;
    if graph.actions.len() == 1 {
        return trigger_editor_error(
            "delete_action.required",
            format!("cannot delete final action {node_id} from trigger {trigger_id}"),
        );
    }
    let before = project.clone();
    let removed = project.graphs[graph_index].actions.remove(action_index);
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(removed)
}

/// Return the next deterministic numeric node ID for an action stem.
pub fn next_trigger_action_node_id(
    project: &TriggerEditorProject,
    trigger_id: &str,
    stem: &str,
) -> Result<String, EngineError> {
    project.validate()?;
    validate_symbol("append_action.node_id_stem", stem)?;
    let graph = graph_by_id(project, trigger_id, "append_action.trigger_id")?;
    let prefix = format!("{stem}_");
    let next_suffix = graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
        .filter_map(|node| node.node_id.strip_prefix(&prefix))
        .filter_map(|suffix| suffix.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "append_action.node_id",
                format!("node id suffix exhausted for stem {stem}"),
            )
        })?;
    let node_id = format!("{prefix}{next_suffix}");
    if graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
        .any(|node| node.node_id == node_id)
    {
        return trigger_editor_error(
            "append_action.node_id",
            format!("duplicate node id {node_id}"),
        );
    }
    Ok(node_id)
}

/// Append one validated action using a derived, stable execution order.
pub fn append_trigger_action(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    draft: TriggerActionDraft,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "append_action.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let graph = &project.graphs[graph_index];
    if graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
        .any(|node| node.node_id == draft.node_id)
    {
        return trigger_editor_error(
            "append_action.node_id",
            format!("duplicate node id {}", draft.node_id),
        );
    }
    validate_unique_action_orders(graph, "append_action.order")?;
    let order = graph
        .actions
        .iter()
        .map(|action| action.order)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| {
            trigger_editor_error_value("append_action.order", "action order exhausted")
        })?;
    let action = EcaNode {
        node_id: draft.node_id,
        operation: draft.operation,
        order,
        args: draft.args,
        variable_refs: draft.variable_refs,
    };
    let before = project.clone();
    project.graphs[graph_index].actions.push(action.clone());
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(action)
}

/// Build and append one action from the live editor's typed operation palette.
pub fn append_default_trigger_action(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    kind: TriggerActionPaletteKind,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    let draft = match kind {
        TriggerActionPaletteKind::Wait => TriggerActionDraft {
            node_id: next_trigger_action_node_id(project, trigger_id, "act_wait")?,
            operation: "sleep".to_string(),
            args: BTreeMap::from([
                ("duration_ms".to_string(), "250".to_string()),
                ("reason".to_string(), "authored wait".to_string()),
            ]),
            variable_refs: Vec::new(),
        },
        TriggerActionPaletteKind::CreateUnitAtPoint => {
            let unit_type_id = project.unit_types.keys().next().cloned().ok_or_else(|| {
                trigger_editor_error_value(
                    "append_action.defaults.unit_type_id",
                    "no unit type is available for Create unit at point",
                )
            })?;
            let region = project.regions.values().next().ok_or_else(|| {
                trigger_editor_error_value(
                    "append_action.defaults.region_id",
                    "no region is available for Create unit at point",
                )
            })?;
            let owner_player_id = project
                .initial_units
                .values()
                .next()
                .map(|unit| unit.owner_player_id.clone())
                .ok_or_else(|| {
                    trigger_editor_error_value(
                        "append_action.defaults.owner_player_id",
                        "no initial unit owner is available for Create unit at point",
                    )
                })?;
            TriggerActionDraft {
                node_id: next_trigger_action_node_id(project, trigger_id, "act_create_unit")?,
                operation: "create_unit_at_point".to_string(),
                args: BTreeMap::from([
                    (
                        "unit_id".to_string(),
                        next_authored_unit_id(project, "unit_authored")?,
                    ),
                    ("unit_type_id".to_string(), unit_type_id),
                    ("owner_player_id".to_string(), owner_player_id),
                    ("region_id".to_string(), region.region_id.clone()),
                    ("x".to_string(), region.center.x.to_string()),
                    ("y".to_string(), region.center.y.to_string()),
                    ("z".to_string(), region.center.z.to_string()),
                ]),
                variable_refs: Vec::new(),
            }
        }
        TriggerActionPaletteKind::SetVariable => {
            let variable = project.variables.values().next().ok_or_else(|| {
                trigger_editor_error_value(
                    "append_action.defaults.variable",
                    "no variable is available for Set variable",
                )
            })?;
            let mut args = BTreeMap::from([("variable".to_string(), variable.name.clone())]);
            if let Some(value) = variable.array_values.first() {
                args.insert("index".to_string(), "0".to_string());
                args.insert("value".to_string(), value.clone());
            } else {
                args.insert("value".to_string(), variable.initial_value.clone());
            }
            TriggerActionDraft {
                node_id: next_trigger_action_node_id(project, trigger_id, "act_set_variable")?,
                operation: "set_variable".to_string(),
                args,
                variable_refs: vec![variable.name.clone()],
            }
        }
    };
    append_trigger_action(project, trigger_id, draft)
}

/// Change the duration payload of one existing `sleep` action.
pub fn set_trigger_wait_duration(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    duration_ms: u32,
) -> Result<bool, EngineError> {
    project.validate()?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_wait.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let action_index = project.graphs[graph_index]
        .actions
        .iter()
        .position(|action| action.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_wait.node_id",
                format!("unknown action node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let action = &project.graphs[graph_index].actions[action_index];
    if action.operation != "sleep" {
        return trigger_editor_error(
            "edit_wait.operation",
            format!(
                "action {node_id} has operation {}; expected sleep",
                action.operation
            ),
        );
    }
    if !(1..=60_000).contains(&duration_ms) {
        return trigger_editor_error(
            "edit_wait.duration_ms",
            format!("wait duration must be 1..=60000 ms, got {duration_ms}"),
        );
    }
    let duration = duration_ms.to_string();
    if action.args.get("duration_ms") == Some(&duration) {
        return Ok(false);
    }
    let before = project.clone();
    project.graphs[graph_index].actions[action_index]
        .args
        .insert("duration_ms".to_string(), duration);
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(true)
}

fn graph_by_id<'a>(
    project: &'a TriggerEditorProject,
    trigger_id: &str,
    field: &'static str,
) -> Result<&'a EcaGraph, EngineError> {
    project
        .graphs
        .iter()
        .find(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(field, format!("unknown trigger graph {trigger_id}"))
        })
}

fn next_authored_unit_id(
    project: &TriggerEditorProject,
    stem: &str,
) -> Result<String, EngineError> {
    validate_symbol("append_action.unit_id_stem", stem)?;
    let prefix = format!("{stem}_");
    let next_suffix = project
        .initial_units
        .keys()
        .map(String::as_str)
        .chain(
            project
                .graphs
                .iter()
                .flat_map(|graph| graph.actions.iter())
                .filter_map(|action| action.args.get("unit_id").map(String::as_str)),
        )
        .filter_map(|unit_id| unit_id.strip_prefix(&prefix))
        .filter_map(|suffix| suffix.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "append_action.unit_id",
                format!("unit id suffix exhausted for stem {stem}"),
            )
        })?;
    let unit_id = format!("{prefix}{next_suffix}");
    let duplicate = project.initial_units.contains_key(&unit_id)
        || project.graphs.iter().any(|graph| {
            graph
                .actions
                .iter()
                .any(|action| action.args.get("unit_id") == Some(&unit_id))
        });
    if duplicate {
        return trigger_editor_error(
            "append_action.unit_id",
            format!("duplicate authored unit id {unit_id}"),
        );
    }
    Ok(unit_id)
}

fn validate_unique_action_orders(graph: &EcaGraph, field: &'static str) -> Result<(), EngineError> {
    let mut seen_orders = BTreeSet::new();
    for action in &graph.actions {
        if !seen_orders.insert(action.order) {
            return trigger_editor_error(field, format!("duplicate action order {}", action.order));
        }
    }
    Ok(())
}
