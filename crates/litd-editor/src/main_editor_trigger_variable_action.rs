use std::collections::BTreeMap;

use egui::RichText;
use litd_editor::{
    mcp_trigger_tools::EcaNode,
    trigger_editor::{TriggerEditorVariable, set_trigger_variable_action_indexed_value},
};

use super::{ERROR, EditorShellRuntime, TEXT_DIM};

pub(super) struct VariableActionEdit {
    variable_name: String,
    index: Option<String>,
    value: String,
}

pub(super) fn variable_action_control(
    ui: &mut egui::Ui,
    node: &EcaNode,
    variables: &BTreeMap<String, TriggerEditorVariable>,
    requested: &mut Option<VariableActionEdit>,
) {
    let Some(current_name) = node.args.get("variable") else {
        ui.label(RichText::new("Missing variable target").color(ERROR));
        return;
    };
    let Some(current_value) = node.args.get("value") else {
        ui.label(RichText::new("Missing variable value").color(ERROR));
        return;
    };
    let variable_id = ui
        .id()
        .with(("trigger_action_variable", node.node_id.as_str()));
    let index_id = ui
        .id()
        .with(("trigger_action_variable_index", node.node_id.as_str()));
    let value_id = ui
        .id()
        .with(("trigger_action_variable_value", node.node_id.as_str()));
    let mut variable_name = temp_or(ui, variable_id, current_name.clone());
    let mut index = temp_or(
        ui,
        index_id,
        node.args.get("index").cloned().unwrap_or_default(),
    );
    let mut value = temp_or(ui, value_id, current_value.clone());
    ui.horizontal(|ui| {
        ui.label(RichText::new("Target").size(10.0).color(TEXT_DIM));
        egui::ComboBox::from_id_salt(variable_id)
            .selected_text(&variable_name)
            .show_ui(ui, |ui| {
                for name in variables.keys() {
                    ui.selectable_value(&mut variable_name, name.clone(), name);
                }
            });
    });
    let target = variables.get(&variable_name);
    ui.horizontal(|ui| {
        if target.is_some_and(|variable| !variable.array_values.is_empty()) {
            ui.label(RichText::new("Index").size(10.0).color(TEXT_DIM));
            ui.text_edit_singleline(&mut index);
        } else {
            index.clear();
        }
        ui.label(RichText::new("Value").size(10.0).color(TEXT_DIM));
        ui.text_edit_singleline(&mut value);
        if ui.small_button("Apply").clicked() {
            *requested = Some(VariableActionEdit {
                variable_name: variable_name.clone(),
                index: (!index.is_empty()).then(|| index.clone()),
                value: value.clone(),
            });
        }
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(variable_id, variable_name);
        data.insert_temp(index_id, index);
        data.insert_temp(value_id, value);
    });
}

pub(super) fn apply_variable_action_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    edit: VariableActionEdit,
) {
    match set_trigger_variable_action_indexed_value(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        &edit.variable_name,
        edit.index.as_deref(),
        &edit.value,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = match edit.index {
                Some(index) => format!(
                    "set {}[{index}]={} in {node_id}",
                    edit.variable_name, edit.value
                ),
                None => format!("set {}={} in {node_id}", edit.variable_name, edit.value),
            };
        }
        Ok(false) => {
            runtime.trigger_editor_last_action = format!("{node_id} variable edit unchanged");
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                variable = edit.variable_name,
                index = edit.index,
                value = edit.value,
                error = %error,
                "trigger editor indexed variable action edit failed"
            );
        }
    }
}

fn temp_or<T: Clone + 'static>(ui: &egui::Ui, id: egui::Id, fallback: T) -> T {
    ui.ctx()
        .data_mut(|data| data.get_temp::<T>(id))
        .unwrap_or(fallback)
}
