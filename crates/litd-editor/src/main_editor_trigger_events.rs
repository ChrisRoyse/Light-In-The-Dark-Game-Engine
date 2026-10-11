use egui::RichText;
use litd_editor::{
    mcp_trigger_tools::{EcaGraph, EcaNode},
    trigger_editor::{
        TriggerEventPaletteKind, append_default_trigger_event, delete_trigger_event,
        set_time_event_seconds, set_unit_dies_event_unit, set_unit_enters_region_payload,
    },
};

use super::{EditorShellRuntime, GOLD, TEXT, TEXT_DIM, node_details, ordered};

pub(super) fn event_nodes(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime, graph: &EcaGraph) {
    let mut requested = None;
    let mut time_edit = None;
    let mut unit_edit = None;
    let mut region_edit = None;
    let mut delete_requested = None;
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
        ui.label(RichText::new("Event").size(12.0).strong().color(GOLD));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("Add event", |ui| {
                if ui.button("Elapsed game time").clicked() {
                    requested = Some(TriggerEventPaletteKind::ElapsedGameTime);
                }
                if ui.button("Periodic timer").clicked() {
                    requested = Some(TriggerEventPaletteKind::PeriodicTimer);
                }
                if ui.button("Unit dies").clicked() {
                    requested = Some(TriggerEventPaletteKind::UnitDies);
                }
                if ui.button("Unit enters region").clicked() {
                    requested = Some(TriggerEventPaletteKind::UnitEntersRegion);
                }
                if ui.button("Map initialization").clicked() {
                    requested = Some(TriggerEventPaletteKind::MapInitialization);
                }
            });
        });
    });
    for node in ordered(&graph.events) {
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
                });
            });
            node_details(ui, node);
            time_event_control(ui, node, &mut time_edit);
            unit_dies_control(ui, node, &unit_ids, &mut unit_edit);
            unit_enters_region_control(ui, node, &unit_ids, &region_ids, &mut region_edit);
        });
    }
    if let Some(node_id) = delete_requested {
        apply_event_delete(runtime, &graph.trigger_id, &node_id);
        return;
    }
    if let Some(kind) = requested {
        apply_event_append(runtime, &graph.trigger_id, kind);
    }
    if let Some((node_id, seconds)) = time_edit {
        apply_time_event_edit(runtime, &graph.trigger_id, &node_id, seconds);
    }
    if let Some((node_id, unit_id)) = unit_edit {
        apply_unit_dies_edit(runtime, &graph.trigger_id, &node_id, &unit_id);
    }
    if let Some((node_id, unit_id, region_id)) = region_edit {
        apply_unit_enters_region_edit(runtime, &graph.trigger_id, &node_id, &unit_id, &region_id);
    }
}

fn time_event_control(ui: &mut egui::Ui, node: &EcaNode, requested: &mut Option<(String, f32)>) {
    let (arg_name, label, minimum) = match node.operation.as_str() {
        "elapsed_game_time" => ("elapsed_seconds", "Elapsed", 0.0),
        "periodic_timer" => ("period_seconds", "Period", f32::MIN_POSITIVE),
        _ => return,
    };
    let Some(current) = node
        .args
        .get(arg_name)
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
    else {
        return;
    };
    let id = ui
        .id()
        .with(("trigger_time_event_seconds", node.node_id.as_str()));
    let mut seconds = ui
        .ctx()
        .data_mut(|data| data.get_temp::<f32>(id))
        .unwrap_or(current);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.0).color(TEXT_DIM));
        ui.add(
            egui::DragValue::new(&mut seconds)
                .range(minimum..=f32::MAX)
                .speed(0.25)
                .suffix(" s"),
        );
        if ui.small_button("Apply").clicked() {
            *requested = Some((node.node_id.clone(), seconds));
        }
    });
    ui.ctx().data_mut(|data| data.insert_temp(id, seconds));
}

fn apply_time_event_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    seconds: f32,
) {
    match set_time_event_seconds(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        seconds,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("set {node_id} to {seconds} seconds");
        }
        Ok(false) => runtime.trigger_editor_last_action = format!("{node_id} payload unchanged"),
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                seconds,
                error = %error,
                "trigger editor time-event payload edit failed"
            );
        }
    }
}

fn unit_dies_control(
    ui: &mut egui::Ui,
    node: &EcaNode,
    unit_ids: &[String],
    requested: &mut Option<(String, String)>,
) {
    if node.operation != "unit_dies" {
        return;
    }
    let Some(current) = node.args.get("unit_id") else {
        return;
    };
    let mut selected = current.clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Target unit").size(10.0).color(TEXT_DIM));
        egui::ComboBox::from_id_salt(("trigger_event_unit", node.node_id.as_str()))
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for unit_id in unit_ids {
                    ui.selectable_value(&mut selected, unit_id.clone(), unit_id);
                }
            });
    });
    if selected != *current {
        *requested = Some((node.node_id.clone(), selected));
    }
}

fn unit_enters_region_control(
    ui: &mut egui::Ui,
    node: &EcaNode,
    unit_ids: &[String],
    region_ids: &[String],
    requested: &mut Option<(String, String, String)>,
) {
    if node.operation != "unit_enters_region" {
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
            ("trigger_region_event_unit", node.node_id.as_str()),
            &mut unit_id,
            unit_ids,
        );
        ui.label(RichText::new("Region").size(10.0).color(TEXT_DIM));
        reference_combo(
            ui,
            ("trigger_region_event_region", node.node_id.as_str()),
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

fn apply_unit_dies_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    unit_id: &str,
) {
    match set_unit_dies_event_unit(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        unit_id,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("set {node_id} target unit to {unit_id}");
        }
        Ok(false) => runtime.trigger_editor_last_action = format!("{node_id} target unchanged"),
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                unit_id,
                error = %error,
                "trigger editor Unit dies payload edit failed"
            );
        }
    }
}

fn apply_unit_enters_region_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    unit_id: &str,
    region_id: &str,
) {
    match set_unit_enters_region_payload(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        unit_id,
        region_id,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("set {node_id} to {unit_id} entering {region_id}");
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
                "trigger editor Unit enters region payload edit failed"
            );
        }
    }
}

fn apply_event_append(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    kind: TriggerEventPaletteKind,
) {
    match append_default_trigger_event(&mut runtime.trigger_editor_project, trigger_id, kind) {
        Ok(event) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("added event {}", event.node_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                ?kind,
                error = %error,
                "trigger editor event append failed"
            );
        }
    }
}

fn apply_event_delete(runtime: &mut EditorShellRuntime, trigger_id: &str, node_id: &str) {
    match delete_trigger_event(&mut runtime.trigger_editor_project, trigger_id, node_id) {
        Ok(event) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("deleted event {}", event.node_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                error = %error,
                "trigger editor event delete failed"
            );
        }
    }
}
