use std::collections::BTreeMap;

use litd_engine::EngineError;

use super::{
    TriggerEditorProject,
    support::{trigger_editor_error, trigger_editor_error_value},
};
use crate::mcp_trigger_tools::EcaNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEventPaletteKind {
    ElapsedGameTime,
    PeriodicTimer,
    UnitDies,
    UnitEntersRegion,
    MapInitialization,
}

impl TriggerEventPaletteKind {
    pub fn operation(self) -> &'static str {
        match self {
            Self::ElapsedGameTime => "elapsed_game_time",
            Self::PeriodicTimer => "periodic_timer",
            Self::UnitDies => "unit_dies",
            Self::UnitEntersRegion => "unit_enters_region",
            Self::MapInitialization => "map_initialization",
        }
    }
}

pub fn replace_default_trigger_event(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    kind: TriggerEventPaletteKind,
) -> Result<bool, EngineError> {
    replace_trigger_event_operation(project, trigger_id, kind.operation())
}

pub fn append_default_trigger_event(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    kind: TriggerEventPaletteKind,
) -> Result<EcaNode, EngineError> {
    append_trigger_event_operation(project, trigger_id, kind.operation())
}

pub fn append_trigger_event_operation(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    operation: &str,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    let graph_index = graph_index(project, trigger_id, "append_event")?;
    let order = project.graphs[graph_index]
        .events
        .iter()
        .map(|event| event.order)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| trigger_editor_error_value("append_event.order", "event order exhausted"))?;
    let node_id = next_event_node_id(&project.graphs[graph_index], operation)?;
    let event = default_event(project, operation, node_id, order, "append_event")?;
    let before = project.clone();
    project.graphs[graph_index].events.push(event.clone());
    validate_or_restore(project, before)?;
    Ok(event)
}

pub fn delete_trigger_event(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
) -> Result<EcaNode, EngineError> {
    project.validate()?;
    let graph_index = graph_index(project, trigger_id, "delete_event")?;
    let event_index = project.graphs[graph_index]
        .events
        .iter()
        .position(|event| event.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "delete_event.node_id",
                format!("unknown event node {node_id} in trigger {trigger_id}"),
            )
        })?;
    if project.graphs[graph_index].events.len() == 1 {
        return trigger_editor_error(
            "delete_event.required",
            format!("cannot delete final event {node_id} from trigger {trigger_id}"),
        );
    }
    let before = project.clone();
    let removed = project.graphs[graph_index].events.remove(event_index);
    validate_or_restore(project, before)?;
    Ok(removed)
}

/// Replace a graph's event list with one deterministic, validated event.
pub fn replace_trigger_event_operation(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    operation: &str,
) -> Result<bool, EngineError> {
    project.validate()?;
    let graph_index = graph_index(project, trigger_id, "replace_event")?;
    let event = default_event(
        project,
        operation,
        fixed_event_node_id(operation)?.to_string(),
        1,
        "replace_event",
    )?;
    if project.graphs[graph_index].events.as_slice() == [event.clone()] {
        return Ok(false);
    }
    let before = project.clone();
    project.graphs[graph_index].events = vec![event];
    validate_or_restore(project, before)?;
    Ok(true)
}

fn default_event(
    project: &TriggerEditorProject,
    operation: &str,
    node_id: String,
    order: u32,
    field: &str,
) -> Result<EcaNode, EngineError> {
    let event = match operation {
        "elapsed_game_time" => scalar_event(node_id, operation, order, "elapsed_seconds", "30"),
        "periodic_timer" => scalar_event(node_id, operation, order, "period_seconds", "1"),
        "unit_dies" => {
            let unit_id = project
                .initial_units
                .keys()
                .next()
                .cloned()
                .ok_or_else(|| {
                    trigger_editor_error_value(
                        format!("{field}.defaults.unit_id"),
                        "no initial unit is available for Unit dies",
                    )
                })?;
            EcaNode {
                node_id,
                operation: operation.to_string(),
                order,
                args: BTreeMap::from([("unit_id".to_string(), unit_id)]),
                variable_refs: Vec::new(),
            }
        }
        "unit_enters_region" => {
            let unit_id = first_key(
                &project.initial_units,
                &format!("{field}.defaults.unit_id"),
                "no initial unit is available for Unit enters region",
            )?;
            let region_id = first_key(
                &project.regions,
                &format!("{field}.defaults.region_id"),
                "no region is available for Unit enters region",
            )?;
            EcaNode {
                node_id,
                operation: operation.to_string(),
                order,
                args: BTreeMap::from([
                    ("region_id".to_string(), region_id),
                    ("unit_id".to_string(), unit_id),
                ]),
                variable_refs: Vec::new(),
            }
        }
        "map_initialization" => EcaNode {
            node_id,
            operation: operation.to_string(),
            order,
            args: BTreeMap::new(),
            variable_refs: Vec::new(),
        },
        other => {
            return trigger_editor_error(
                format!("{field}.operation"),
                format!("unsupported live event operation {other}"),
            );
        }
    };
    Ok(event)
}

fn scalar_event(
    node_id: String,
    operation: &str,
    order: u32,
    arg_name: &str,
    value: &str,
) -> EcaNode {
    EcaNode {
        node_id,
        operation: operation.to_string(),
        order,
        args: BTreeMap::from([(arg_name.to_string(), value.to_string())]),
        variable_refs: Vec::new(),
    }
}

fn fixed_event_node_id(operation: &str) -> Result<&'static str, EngineError> {
    match operation {
        "elapsed_game_time" => Ok("evt_elapsed_game_time"),
        "periodic_timer" => Ok("evt_periodic_timer"),
        "unit_dies" => Ok("evt_unit_dies"),
        "unit_enters_region" => Ok("evt_unit_enters_region"),
        "map_initialization" => Ok("evt_map_initialization"),
        other => trigger_editor_error(
            "replace_event.operation",
            format!("unsupported live event operation {other}"),
        ),
    }
}

fn next_event_node_id(
    graph: &crate::mcp_trigger_tools::EcaGraph,
    operation: &str,
) -> Result<String, EngineError> {
    let prefix = format!("evt_{operation}_");
    let suffix = graph
        .events
        .iter()
        .chain(graph.conditions.iter())
        .chain(graph.actions.iter())
        .filter_map(|node| node.node_id.strip_prefix(&prefix))
        .filter_map(|suffix| suffix.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| trigger_editor_error_value("append_event.node_id", "event ID exhausted"))?;
    Ok(format!("{prefix}{suffix}"))
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
