use egui::RichText;
use litd_editor::trigger_editor::{
    TriggerEditorVariableType, TriggerVariablePaletteKind, add_default_trigger_variable,
    add_default_trigger_variable_array, delete_trigger_variable,
    update_trigger_variable_array_definition, update_trigger_variable_definition,
};

use super::{EditorShellRuntime, GOLD, TEXT, TEXT_DIM};

pub(super) fn variable_controls(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime) {
    let selection_id = ui.make_persistent_id("trigger_editor_selected_variable");
    let mut selected = ui
        .ctx()
        .data_mut(|data| data.get_temp::<String>(selection_id));
    let mut add_kind = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Variables").size(12.0).strong().color(GOLD));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("Add variable", |ui| {
                if ui.button("Boolean").clicked() {
                    add_kind = Some(TriggerVariablePaletteKind::Boolean);
                }
                if ui.button("Integer").clicked() {
                    add_kind = Some(TriggerVariablePaletteKind::Integer);
                }
                if ui.button("String").clicked() {
                    add_kind = Some(TriggerVariablePaletteKind::String);
                }
                ui.separator();
                if ui.button("Boolean array [4]").clicked() {
                    add_array(runtime, TriggerVariablePaletteKind::Boolean);
                }
                if ui.button("Integer array [4]").clicked() {
                    add_array(runtime, TriggerVariablePaletteKind::Integer);
                }
                if ui.button("String array [4]").clicked() {
                    add_array(runtime, TriggerVariablePaletteKind::String);
                }
            });
        });
    });
    if let Some(kind) = add_kind {
        apply_add(runtime, kind);
    }
    let variables = runtime
        .trigger_editor_project
        .variables
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for variable in &variables {
        let is_selected = selected.as_deref() == Some(variable.name.as_str());
        if ui
            .selectable_label(is_selected, variable_summary(variable))
            .clicked()
        {
            selected = Some(variable.name.clone());
            ui.ctx()
                .data_mut(|data| data.insert_temp(selection_id, variable.name.clone()));
        }
    }
    let Some(name) = selected else {
        return;
    };
    let Some(variable) = variables.iter().find(|variable| variable.name == name) else {
        return;
    };
    if variable.array_values.is_empty() {
        scalar_editor(ui, runtime, variable);
    } else {
        array_editor(ui, runtime, variable);
    }
}

fn variable_summary(variable: &litd_editor::trigger_editor::TriggerEditorVariable) -> String {
    if variable.array_values.is_empty() {
        format!(
            "{}  {:?} = {}",
            variable.name, variable.value_type, variable.initial_value
        )
    } else {
        format!(
            "{}  {:?}[{}]",
            variable.name,
            variable.value_type,
            variable.array_values.len()
        )
    }
}

fn scalar_editor(
    ui: &mut egui::Ui,
    runtime: &mut EditorShellRuntime,
    variable: &litd_editor::trigger_editor::TriggerEditorVariable,
) {
    let name_id = ui
        .id()
        .with(("trigger_variable_name", variable.name.as_str()));
    let value_id = ui
        .id()
        .with(("trigger_variable_value", variable.name.as_str()));
    let type_id = ui
        .id()
        .with(("trigger_variable_type", variable.name.as_str()));
    let mut name = ui
        .ctx()
        .data_mut(|data| data.get_temp::<String>(name_id))
        .unwrap_or_else(|| variable.name.clone());
    let mut value = ui
        .ctx()
        .data_mut(|data| data.get_temp::<String>(value_id))
        .unwrap_or_else(|| variable.initial_value.clone());
    let mut value_type = ui
        .ctx()
        .data_mut(|data| data.get_temp::<TriggerEditorVariableType>(type_id))
        .unwrap_or(variable.value_type);
    ui.group(|ui| {
        ui.label(RichText::new("Selected variable").size(11.0).color(TEXT));
        ui.text_edit_singleline(&mut name);
        ui.text_edit_singleline(&mut value);
        egui::ComboBox::from_id_salt(type_id)
            .selected_text(format!("{value_type:?}"))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut value_type,
                    TriggerEditorVariableType::Boolean,
                    "Boolean",
                );
                ui.selectable_value(
                    &mut value_type,
                    TriggerEditorVariableType::Integer,
                    "Integer",
                );
                ui.selectable_value(&mut value_type, TriggerEditorVariableType::String, "String");
            });
        ui.horizontal(|ui| {
            if ui.small_button("Apply").clicked() {
                apply_update(runtime, variable, &name, value_type, &value);
            }
            if ui.small_button("Delete").clicked() {
                apply_delete(runtime, &variable.name);
            }
            ui.label(
                RichText::new("Referenced variables cannot change type or delete")
                    .size(9.0)
                    .color(TEXT_DIM),
            );
        });
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(name_id, name);
        data.insert_temp(value_id, value);
        data.insert_temp(type_id, value_type);
    });
}

fn apply_add(runtime: &mut EditorShellRuntime, kind: TriggerVariablePaletteKind) {
    match add_default_trigger_variable(&mut runtime.trigger_editor_project, kind) {
        Ok(variable) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("added variable {}", variable.name);
        }
        Err(error) => report_error(runtime, error, "add"),
    }
}

fn add_array(runtime: &mut EditorShellRuntime, kind: TriggerVariablePaletteKind) {
    match add_default_trigger_variable_array(&mut runtime.trigger_editor_project, kind, 4) {
        Ok(variable) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("added variable array {}[4]", variable.name);
        }
        Err(error) => report_error(runtime, error, "add_array"),
    }
}

fn apply_update(
    runtime: &mut EditorShellRuntime,
    variable: &litd_editor::trigger_editor::TriggerEditorVariable,
    name: &str,
    value_type: TriggerEditorVariableType,
    value: &str,
) {
    match update_trigger_variable_definition(
        &mut runtime.trigger_editor_project,
        &variable.name,
        name,
        value_type,
        value,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("updated variable {}", variable.name);
        }
        Ok(false) => {
            runtime.trigger_editor_last_action = format!("variable {} unchanged", variable.name);
        }
        Err(error) => report_error(runtime, error, "update"),
    }
}

fn array_editor(
    ui: &mut egui::Ui,
    runtime: &mut EditorShellRuntime,
    variable: &litd_editor::trigger_editor::TriggerEditorVariable,
) {
    let name_id = ui
        .id()
        .with(("trigger_variable_array_name", variable.name.as_str()));
    let type_id = ui
        .id()
        .with(("trigger_variable_array_type", variable.name.as_str()));
    let size_id = ui
        .id()
        .with(("trigger_variable_array_size", variable.name.as_str()));
    let index_id = ui
        .id()
        .with(("trigger_variable_array_index", variable.name.as_str()));
    let mut name = temp_or(ui, name_id, variable.name.clone());
    let mut value_type = ui
        .ctx()
        .data_mut(|data| data.get_temp::<TriggerEditorVariableType>(type_id))
        .unwrap_or(variable.value_type);
    let mut size = temp_or(ui, size_id, variable.array_values.len()).clamp(2, 256);
    let mut index = temp_or(ui, index_id, 0_usize).min(size - 1);
    let value_id = ui.id().with((
        "trigger_variable_array_value",
        variable.name.as_str(),
        index,
    ));
    let source = variable
        .array_values
        .get(index)
        .cloned()
        .unwrap_or_else(|| default_value(value_type).to_string());
    let mut value = temp_or(ui, value_id, source);
    ui.group(|ui| {
        ui.label(RichText::new("Selected array").size(11.0).color(TEXT));
        ui.horizontal(|ui| {
            ui.label(RichText::new("Name").size(10.0).color(TEXT_DIM));
            ui.text_edit_singleline(&mut name);
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Type").size(10.0).color(TEXT_DIM));
            variable_type_combo(ui, type_id, &mut value_type);
            ui.label(RichText::new("Size").size(10.0).color(TEXT_DIM));
            ui.add(egui::DragValue::new(&mut size).range(2..=256));
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Index").size(10.0).color(TEXT_DIM));
            ui.add(egui::DragValue::new(&mut index).range(0..=size - 1));
            ui.label(RichText::new("Initial").size(10.0).color(TEXT_DIM));
            ui.text_edit_singleline(&mut value);
        });
        ui.horizontal(|ui| {
            if ui.small_button("Apply").clicked() {
                apply_array_update(runtime, variable, &name, value_type, size, index, &value);
            }
            if ui.small_button("Delete").clicked() {
                apply_delete(runtime, &variable.name);
            }
            ui.label(
                RichText::new("Shrink cannot discard referenced indices")
                    .size(9.0)
                    .color(TEXT_DIM),
            );
        });
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(name_id, name);
        data.insert_temp(type_id, value_type);
        data.insert_temp(size_id, size);
        data.insert_temp(index_id, index);
        data.insert_temp(value_id, value);
    });
}

fn variable_type_combo(
    ui: &mut egui::Ui,
    id: egui::Id,
    value_type: &mut TriggerEditorVariableType,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("{value_type:?}"))
        .show_ui(ui, |ui| {
            ui.selectable_value(value_type, TriggerEditorVariableType::Boolean, "Boolean");
            ui.selectable_value(value_type, TriggerEditorVariableType::Integer, "Integer");
            ui.selectable_value(value_type, TriggerEditorVariableType::String, "String");
        });
}

fn apply_array_update(
    runtime: &mut EditorShellRuntime,
    variable: &litd_editor::trigger_editor::TriggerEditorVariable,
    name: &str,
    value_type: TriggerEditorVariableType,
    size: usize,
    index: usize,
    value: &str,
) {
    let mut values = if value_type == variable.value_type {
        variable.array_values.clone()
    } else {
        vec![default_value(value_type).to_string(); variable.array_values.len()]
    };
    values.resize(size, default_value(value_type).to_string());
    values[index] = value.to_string();
    match update_trigger_variable_array_definition(
        &mut runtime.trigger_editor_project,
        &variable.name,
        name,
        value_type,
        values,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("updated variable array {}", variable.name);
        }
        Ok(false) => {
            runtime.trigger_editor_last_action =
                format!("variable array {} unchanged", variable.name);
        }
        Err(error) => report_error(runtime, error, "update_array"),
    }
}

fn default_value(value_type: TriggerEditorVariableType) -> &'static str {
    match value_type {
        TriggerEditorVariableType::Boolean => "false",
        TriggerEditorVariableType::Integer => "0",
        TriggerEditorVariableType::String => "value",
        TriggerEditorVariableType::Real => "0",
    }
}

fn temp_or<T: Clone + 'static>(ui: &egui::Ui, id: egui::Id, fallback: T) -> T {
    ui.ctx()
        .data_mut(|data| data.get_temp::<T>(id))
        .unwrap_or(fallback)
}

fn apply_delete(runtime: &mut EditorShellRuntime, name: &str) {
    match delete_trigger_variable(&mut runtime.trigger_editor_project, name) {
        Ok(variable) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("deleted variable {}", variable.name);
        }
        Err(error) => report_error(runtime, error, "delete"),
    }
}

fn report_error(
    runtime: &mut EditorShellRuntime,
    error: litd_engine::EngineError,
    operation: &str,
) {
    runtime.trigger_editor_last_action = error.to_string();
    tracing::error!(
        operation,
        error = %error,
        "trigger editor variable definition mutation failed"
    );
}
