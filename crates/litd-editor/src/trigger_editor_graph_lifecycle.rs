//! Transactional Trigger Editor graph creation and deletion.
//!
//! The current strict editor schema has no inter-graph reference operations, so
//! no valid event/condition/action can retain a deleted trigger ID. Without a
//! persisted monotonic counter, creation therefore uses the smallest
//! collision-free `trg_authored_N`; deterministic reuse after deletion is safe.

use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    TriggerEditorProject,
    support::{node, trigger_editor_error, trigger_editor_error_value},
};
use crate::mcp_trigger_tools::EcaGraph;

pub fn append_default_trigger_graph(
    project: &mut TriggerEditorProject,
) -> Result<EcaGraph, EngineError> {
    project.validate()?;
    let (trigger_id, suffix) = next_authored_trigger_id(project)?;
    let graph = EcaGraph {
        trigger_id,
        display_name: format!("Authored Trigger {suffix}"),
        enabled: true,
        folder_path: None,
        initially_on: true,
        comment: None,
        events: vec![node(
            "evt_map_initialization",
            "map_initialization",
            1,
            [],
            [],
        )],
        conditions: Vec::new(),
        actions: vec![node(
            "act_wait_1",
            "sleep",
            1,
            [("duration_ms", "250"), ("reason", "authored wait")],
            [],
        )],
        wasm_escape: None,
    };
    let before = project.clone();
    project.graphs.push(graph.clone());
    validate_or_restore(project, before)?;
    Ok(graph)
}

pub fn delete_trigger_graph(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
) -> Result<EcaGraph, EngineError> {
    project.validate()?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "delete_graph.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    if project.graphs.len() == 1 {
        return trigger_editor_error(
            "delete_graph.required",
            format!("cannot delete final trigger graph {trigger_id}"),
        );
    }
    let before = project.clone();
    let removed = project.graphs.remove(graph_index);
    validate_or_restore(project, before)?;
    Ok(removed)
}

pub(super) fn validate_unique_trigger_ids(graphs: &[EcaGraph]) -> Result<(), EngineError> {
    let mut ids = BTreeSet::new();
    for graph in graphs {
        if !ids.insert(graph.trigger_id.as_str()) {
            return trigger_editor_error(
                "graph.trigger_id",
                format!("duplicate trigger graph {}", graph.trigger_id),
            );
        }
    }
    Ok(())
}

fn next_authored_trigger_id(project: &TriggerEditorProject) -> Result<(String, u32), EngineError> {
    for suffix in 1..=u32::MAX {
        let candidate = format!("trg_authored_{suffix}");
        if project
            .graphs
            .iter()
            .all(|graph| graph.trigger_id != candidate)
        {
            return Ok((candidate, suffix));
        }
    }
    trigger_editor_error(
        "append_graph.trigger_id",
        "authored trigger ID space exhausted",
    )
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
