//! Transactional Trigger Editor graph name/folder authoring.

use litd_engine::EngineError;

use super::{
    TriggerEditorProject,
    support::{trigger_editor_error, trigger_editor_error_value, validate_single_line},
};
use crate::mcp_trigger_tools::EcaGraph;

pub fn set_trigger_graph_metadata(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    display_name: &str,
    folder_path: Option<&str>,
) -> Result<bool, EngineError> {
    project.validate()?;
    validate_display_name(display_name)?;
    validate_folder_path(folder_path)?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "graph_metadata.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let folder_path = folder_path.map(str::to_string);
    let graph = &project.graphs[graph_index];
    if graph.display_name == display_name && graph.folder_path == folder_path {
        return Ok(false);
    }
    let before = project.clone();
    let graph = &mut project.graphs[graph_index];
    graph.display_name = display_name.to_string();
    graph.folder_path = folder_path;
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(true)
}

pub(super) fn validate_graph_metadata(graph: &EcaGraph) -> Result<(), EngineError> {
    validate_display_name(&graph.display_name)?;
    validate_folder_path(graph.folder_path.as_deref())?;
    if let Some(comment) = &graph.comment {
        validate_single_line("graph.comment", comment)?;
    }
    Ok(())
}

fn validate_display_name(display_name: &str) -> Result<(), EngineError> {
    validate_single_line("graph.display_name", display_name)
}

fn validate_folder_path(folder_path: Option<&str>) -> Result<(), EngineError> {
    let Some(folder_path) = folder_path else {
        return Ok(());
    };
    validate_single_line("graph.folder_path", folder_path)?;
    if folder_path.contains('\\')
        || folder_path
            .split('/')
            .any(|segment| segment.is_empty() || segment.trim() != segment)
    {
        return trigger_editor_error(
            "graph.folder_path",
            "folder path must use non-empty slash-delimited segments without surrounding spaces",
        );
    }
    Ok(())
}
