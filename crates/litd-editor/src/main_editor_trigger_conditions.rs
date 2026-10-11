use egui::RichText;
use litd_editor::{
    mcp_trigger_tools::{EcaGraph, EcaNode},
    trigger_editor::{
        TriggerConditionMove, TriggerEditorVariableType, append_default_game_flag_condition,
        append_default_integer_greater_equal_condition, append_default_unit_in_region_condition,
        set_game_flag_condition_variable, set_integer_greater_equal_payload,
        set_unit_in_region_condition_payload,
    },
};

use super::{EditorShellRuntime, GOOD, TEXT, TEXT_DIM, condition_lifecycle, node_details, ordered};

pub(super) fn condition_nodes(
    ui: &mut egui::Ui,
    runtime: &mut EditorShellRuntime,
    graph: &EcaGraph,
) {
    let mut append_requested = None;
    let mut flag_edit = None;
    let mut integer_edit = None;
    let mut region_edit = None;
    let mut delete_requested = None;
    let mut move_requested = None;
    let flag_names = runtime
        .trigger_editor_project
        .variables
        .values()
        .filter(|variable| {
            variable.value_type == TriggerEditorVariableType::Boolean
                && variable.array_values.is_empty()
        })
        .map(|variable| variable.name.clone())
        .collect::<Vec<_>>();
    let unit_ids = runtime
        .trigger_editor_project
        .initial_units
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let region_ids = runtime
        .trigger_editor_project
        .regions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Condition").size(12.0).strong().color(GOOD));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("Add condition", |ui| {
                if ui.button("Game flag is true").clicked() {
                    append_requested = Some(ConditionPaletteKind::GameFlagTrue);
                }
                if ui.button("Integer greater or equal").clicked() {
                    append_requested = Some(ConditionPaletteKind::IntegerGreaterEqual);
                }
                if ui.button("Unit is in region").clicked() {
                    append_requested = Some(ConditionPaletteKind::UnitInRegion);
                }
            });
        });
    });
    for node in ordered(&graph.conditions) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}  /  {}", node.operation, node.node_id))
                        .size(11.0)
                        .strong()
                        .color(TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Delete").clicked() {
                        delete_requested = Some(node.node_id.clone());
                    }
                    if ui.small_button("Move down").clicked() {
                        move_requested = Some((node.node_id.clone(), TriggerConditionMove::Later));
                    }
                    if ui.small_button("Move up").clicked() {
                        move_requested =
                            Some((node.node_id.clone(), TriggerConditionMove::Earlier));
                    }
                });
            });
            node_details(ui, node);
            game_flag_control(ui, node, &flag_names, &mut flag_edit);
            integer_comparison_control(ui, node, &mut integer_edit);
            unit_in_region_control(ui, node, &unit_ids, &region_ids, &mut region_edit);
        });
    }
    if let Some(node_id) = delete_requested {
        condition_lifecycle::apply_condition_delete(runtime, &graph.trigger_id, &node_id);
        return;
    }
    if let Some((node_id, direction)) = move_requested {
        condition_lifecycle::apply_condition_move(runtime, &graph.trigger_id, &node_id, direction);
        return;
    }
    if let Some(kind) = append_requested {
        apply_condition_append(runtime, &graph.trigger_id, kind);
    }
    if let Some((node_id, variable_name)) = flag_edit {
        apply_game_flag_edit(runtime, &graph.trigger_id, &node_id, &variable_name);
    }
    if let Some((node_id, left, right)) = integer_edit {
        apply_integer_comparison_edit(runtime, &graph.trigger_id, &node_id, left, right);
    }
    if let Some((node_id, unit_id, region_id)) = region_edit {
        apply_unit_in_region_edit(runtime, &graph.trigger_id, &node_id, &unit_id, &region_id);
    }
}

#[derive(Clone, Copy)]
enum ConditionPaletteKind {
    GameFlagTrue,
    IntegerGreaterEqual,
    UnitInRegion,
}

fn game_flag_control(
    ui: &mut egui::Ui,
    node: &EcaNode,
    flag_names: &[String],
    requested: &mut Option<(String, String)>,
) {
    if node.operation != "game_flag_true" {
        return;
    }
    let Some(current) = node.args.get("flag") else {
        return;
    };
    let mut selected = current.clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Boolean flag").size(10.0).color(TEXT_DIM));
        egui::ComboBox::from_id_salt(("trigger_condition_flag", node.node_id.as_str()))
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for flag_name in flag_names {
                    ui.selectable_value(&mut selected, flag_name.clone(), flag_name);
                }
            });
    });
    if selected != *current {
        *requested = Some((node.node_id.clone(), selected));
    }
}

fn integer_comparison_control(
    ui: &mut egui::Ui,
    node: &EcaNode,
    requested: &mut Option<(String, i64, i64)>,
) {
    if node.operation != "integer_greater_equal" {
        return;
    }
    let (Some(current_left), Some(current_right)) = (
        node.args
            .get("left")
            .and_then(|value| value.parse::<i64>().ok()),
        node.args
            .get("right")
            .and_then(|value| value.parse::<i64>().ok()),
    ) else {
        return;
    };
    let left_id = ui
        .id()
        .with(("trigger_integer_condition_left", node.node_id.as_str()));
    let right_id = ui
        .id()
        .with(("trigger_integer_condition_right", node.node_id.as_str()));
    let mut left = temp_or(ui, left_id, current_left);
    let mut right = temp_or(ui, right_id, current_right);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Left").size(10.0).color(TEXT_DIM));
        ui.add(egui::DragValue::new(&mut left));
        ui.label(RichText::new(">=").size(10.0).color(TEXT_DIM));
        ui.label(RichText::new("Right").size(10.0).color(TEXT_DIM));
        ui.add(egui::DragValue::new(&mut right));
        if ui.small_button("Apply").clicked() {
            *requested = Some((node.node_id.clone(), left, right));
        }
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(left_id, left);
        data.insert_temp(right_id, right);
    });
}

fn unit_in_region_control(
    ui: &mut egui::Ui,
    node: &EcaNode,
    unit_ids: &[String],
    region_ids: &[String],
    requested: &mut Option<(String, String, String)>,
) {
    if node.operation != "unit_in_region" {
        return;
    }
    let (Some(current_unit), Some(current_region)) =
        (node.args.get("unit_id"), node.args.get("region_id"))
    else {
        return;
    };
    let mut unit_id = current_unit.clone();
    let mut region_id = current_region.clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Unit").size(10.0).color(TEXT_DIM));
        reference_combo(
            ui,
            ("trigger_region_condition_unit", node.node_id.as_str()),
            &mut unit_id,
            unit_ids,
        );
        ui.label(RichText::new("Region").size(10.0).color(TEXT_DIM));
        reference_combo(
            ui,
            ("trigger_region_condition_region", node.node_id.as_str()),
            &mut region_id,
            region_ids,
        );
    });
    if unit_id != *current_unit || region_id != *current_region {
        *requested = Some((node.node_id.clone(), unit_id, region_id));
    }
}

fn reference_combo(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash,
    selected: &mut String,
    choices: &[String],
) {
    egui::ComboBox::from_id_salt(id_source)
        .selected_text(selected.as_str())
        .show_ui(ui, |ui| {
            for choice in choices {
                ui.selectable_value(selected, choice.clone(), choice);
            }
        });
}

fn apply_game_flag_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    variable_name: &str,
) {
    match set_game_flag_condition_variable(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        variable_name,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("set {node_id} Boolean flag to {variable_name}");
        }
        Ok(false) => runtime.trigger_editor_last_action = format!("{node_id} flag unchanged"),
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                variable_name,
                error = %error,
                "trigger editor game flag payload edit failed"
            );
        }
    }
}

fn apply_integer_comparison_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    left: i64,
    right: i64,
) {
    match set_integer_greater_equal_payload(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        left,
        right,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("set {node_id} comparison to {left} >= {right}");
        }
        Ok(false) => runtime.trigger_editor_last_action = format!("{node_id} payload unchanged"),
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                left,
                right,
                error = %error,
                "trigger editor integer-comparison payload edit failed"
            );
        }
    }
}

fn apply_unit_in_region_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    unit_id: &str,
    region_id: &str,
) {
    match set_unit_in_region_condition_payload(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        unit_id,
        region_id,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("set {node_id} to {unit_id} in {region_id}");
        }
        Ok(false) => runtime.trigger_editor_last_action = format!("{node_id} payload unchanged"),
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                unit_id,
                region_id,
                error = %error,
                "trigger editor Unit is in region payload edit failed"
            );
        }
    }
}

fn apply_condition_append(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    kind: ConditionPaletteKind,
) {
    let result = match kind {
        ConditionPaletteKind::GameFlagTrue => {
            append_default_game_flag_condition(&mut runtime.trigger_editor_project, trigger_id)
        }
        ConditionPaletteKind::IntegerGreaterEqual => {
            append_default_integer_greater_equal_condition(
                &mut runtime.trigger_editor_project,
                trigger_id,
            )
        }
        ConditionPaletteKind::UnitInRegion => {
            append_default_unit_in_region_condition(&mut runtime.trigger_editor_project, trigger_id)
        }
    };
    match result {
        Ok(condition) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("added condition {}", condition.node_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                error = %error,
                "trigger editor condition append failed"
            );
        }
    }
}

fn temp_or<T: Clone + 'static>(ui: &egui::Ui, id: egui::Id, fallback: T) -> T {
    ui.ctx()
        .data_mut(|data| data.get_temp::<T>(id))
        .unwrap_or(fallback)
}
