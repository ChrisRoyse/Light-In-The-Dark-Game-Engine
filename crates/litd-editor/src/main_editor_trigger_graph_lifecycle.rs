//! Live Trigger Editor graph creation and deletion controls.

use litd_editor::trigger_editor::{append_default_trigger_graph, delete_trigger_graph};

use super::EditorShellRuntime;

pub(super) fn graph_lifecycle_controls(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime) {
    let mut add_requested = false;
    let mut delete_requested = false;
    ui.horizontal(|ui| {
        if ui.small_button("Add trigger").clicked() {
            add_requested = true;
        }
        if ui.small_button("Delete selected trigger").clicked() {
            delete_requested = true;
        }
    });
    if add_requested {
        apply_graph_append(runtime);
    } else if delete_requested {
        apply_graph_delete(runtime);
    }
}

fn apply_graph_append(runtime: &mut EditorShellRuntime) {
    match append_default_trigger_graph(&mut runtime.trigger_editor_project) {
        Ok(graph) => {
            runtime.trigger_editor_selected_graph = runtime
                .trigger_editor_project
                .graphs
                .len()
                .saturating_sub(1);
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("added trigger {}", graph.trigger_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(error = %error, "trigger graph creation failed");
        }
    }
}

fn apply_graph_delete(runtime: &mut EditorShellRuntime) {
    let selected = runtime.trigger_editor_selected_graph.min(
        runtime
            .trigger_editor_project
            .graphs
            .len()
            .saturating_sub(1),
    );
    let Some(trigger_id) = runtime
        .trigger_editor_project
        .graphs
        .get(selected)
        .map(|graph| graph.trigger_id.clone())
    else {
        runtime.trigger_editor_last_action =
            "trigger graph delete failed: no selected trigger".to_string();
        tracing::error!("trigger graph delete failed: no selected trigger");
        return;
    };
    match delete_trigger_graph(&mut runtime.trigger_editor_project, &trigger_id) {
        Ok(removed) => {
            runtime.trigger_editor_selected_graph = selected.min(
                runtime
                    .trigger_editor_project
                    .graphs
                    .len()
                    .saturating_sub(1),
            );
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("deleted trigger {}", removed.trigger_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                error = %error,
                "trigger graph deletion failed"
            );
        }
    }
}
