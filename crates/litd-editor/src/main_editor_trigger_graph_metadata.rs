//! Live selected-trigger display-name and folder authoring.

use egui::RichText;
use litd_editor::{mcp_trigger_tools::EcaGraph, trigger_editor::set_trigger_graph_metadata};

use super::{EditorShellRuntime, TEXT_DIM};

pub(super) fn graph_metadata_control(
    ui: &mut egui::Ui,
    runtime: &mut EditorShellRuntime,
    graph: &EcaGraph,
) {
    let generation = runtime.trigger_editor_generation;
    let name_id = ui.make_persistent_id((
        "trigger_graph_display_name",
        graph.trigger_id.as_str(),
        generation,
    ));
    let folder_id = ui.make_persistent_id((
        "trigger_graph_folder",
        graph.trigger_id.as_str(),
        generation,
    ));
    let mut display_name = temp_or(ui, name_id, graph.display_name.clone());
    let mut folder_path = temp_or(ui, folder_id, graph.folder_path.clone().unwrap_or_default());
    let mut apply_requested = false;
    let mut clear_requested = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Name").size(10.0).color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(&mut display_name).desired_width(150.0));
        ui.label(RichText::new("Folder").size(10.0).color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(&mut folder_path).desired_width(130.0));
        if ui.small_button("Apply metadata").clicked() {
            apply_requested = true;
        }
        if ui.small_button("Clear folder").clicked() {
            clear_requested = true;
        }
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(name_id, display_name.clone());
        data.insert_temp(folder_id, folder_path.clone());
    });
    if clear_requested {
        apply_metadata(runtime, &graph.trigger_id, &display_name, None);
    } else if apply_requested {
        apply_metadata(
            runtime,
            &graph.trigger_id,
            &display_name,
            Some(folder_path.as_str()),
        );
    }
}

fn apply_metadata(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    display_name: &str,
    folder_path: Option<&str>,
) {
    let folder_path = folder_path.filter(|path| !path.is_empty());
    match set_trigger_graph_metadata(
        &mut runtime.trigger_editor_project,
        trigger_id,
        display_name,
        folder_path,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!(
                "updated {trigger_id} metadata: name={display_name:?} folder={}",
                folder_path.unwrap_or("root")
            );
        }
        Ok(false) => {
            runtime.trigger_editor_last_action = format!("{trigger_id} metadata unchanged");
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                display_name,
                folder_path = ?folder_path,
                error = %error,
                "trigger graph metadata edit failed"
            );
        }
    }
}

fn temp_or<T: Clone + 'static>(ui: &egui::Ui, id: egui::Id, fallback: T) -> T {
    ui.ctx()
        .data_mut(|data| data.get_temp::<T>(id))
        .unwrap_or(fallback)
}
