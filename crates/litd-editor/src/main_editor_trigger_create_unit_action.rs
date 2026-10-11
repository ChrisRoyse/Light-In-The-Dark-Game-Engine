use std::collections::BTreeSet;

use egui::RichText;
use litd_editor::{
    mcp_trigger_tools::EcaNode,
    trigger_editor::{
        TriggerCreateUnitPayload, TriggerEditorProject, set_create_unit_action_payload,
    },
};

use super::{ERROR, EditorShellRuntime, TEXT_DIM};

pub(super) struct CreateUnitActionEdit {
    payload: TriggerCreateUnitPayload,
}

pub(super) fn create_unit_action_control(
    ui: &mut egui::Ui,
    node: &EcaNode,
    project: &TriggerEditorProject,
    requested: &mut Option<CreateUnitActionEdit>,
) {
    let Some(defaults) = payload_from_node(node) else {
        ui.label(RichText::new("Invalid create-unit payload").color(ERROR));
        return;
    };
    let base_id = ui
        .id()
        .with(("trigger_create_unit_payload", node.node_id.as_str()));
    let mut payload = TriggerCreateUnitPayload {
        unit_id: temp_or(ui, base_id.with("unit_id"), defaults.unit_id),
        unit_type_id: temp_or(ui, base_id.with("unit_type_id"), defaults.unit_type_id),
        owner_player_id: temp_or(
            ui,
            base_id.with("owner_player_id"),
            defaults.owner_player_id,
        ),
        region_id: temp_or(ui, base_id.with("region_id"), defaults.region_id),
        x: temp_or(ui, base_id.with("x"), defaults.x),
        y: temp_or(ui, base_id.with("y"), defaults.y),
        z: temp_or(ui, base_id.with("z"), defaults.z),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("Unit ID").size(10.0).color(TEXT_DIM));
        ui.text_edit_singleline(&mut payload.unit_id);
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Type").size(10.0).color(TEXT_DIM));
        reference_combo(
            ui,
            base_id.with("type_combo"),
            &mut payload.unit_type_id,
            project.unit_types.keys(),
        );
        ui.label(RichText::new("Owner").size(10.0).color(TEXT_DIM));
        let owners = project
            .initial_units
            .values()
            .map(|unit| unit.owner_player_id.as_str())
            .collect::<BTreeSet<_>>();
        reference_combo(
            ui,
            base_id.with("owner_combo"),
            &mut payload.owner_player_id,
            owners,
        );
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Region").size(10.0).color(TEXT_DIM));
        let previous_region = payload.region_id.clone();
        reference_combo(
            ui,
            base_id.with("region_combo"),
            &mut payload.region_id,
            project.regions.keys(),
        );
        if payload.region_id != previous_region {
            let center = project.regions[&payload.region_id].center;
            payload.x = center.x;
            payload.y = center.y;
            payload.z = center.z;
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Position").size(10.0).color(TEXT_DIM));
        coordinate(ui, "X", &mut payload.x);
        coordinate(ui, "Y", &mut payload.y);
        coordinate(ui, "Z", &mut payload.z);
        if ui.small_button("Apply").clicked() {
            *requested = Some(CreateUnitActionEdit {
                payload: payload.clone(),
            });
        }
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(base_id.with("unit_id"), payload.unit_id);
        data.insert_temp(base_id.with("unit_type_id"), payload.unit_type_id);
        data.insert_temp(base_id.with("owner_player_id"), payload.owner_player_id);
        data.insert_temp(base_id.with("region_id"), payload.region_id);
        data.insert_temp(base_id.with("x"), payload.x);
        data.insert_temp(base_id.with("y"), payload.y);
        data.insert_temp(base_id.with("z"), payload.z);
    });
}

pub(super) fn apply_create_unit_action_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    edit: CreateUnitActionEdit,
) {
    match set_create_unit_action_payload(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        &edit.payload,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("set {node_id} to create {}", edit.payload.unit_id);
        }
        Ok(false) => runtime.trigger_editor_last_action = format!("{node_id} payload unchanged"),
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                unit_id = edit.payload.unit_id,
                unit_type_id = edit.payload.unit_type_id,
                owner_player_id = edit.payload.owner_player_id,
                region_id = edit.payload.region_id,
                x = edit.payload.x,
                y = edit.payload.y,
                z = edit.payload.z,
                error = %error,
                "trigger editor create-unit payload edit failed"
            );
        }
    }
}

fn payload_from_node(node: &EcaNode) -> Option<TriggerCreateUnitPayload> {
    Some(TriggerCreateUnitPayload {
        unit_id: node.args.get("unit_id")?.clone(),
        unit_type_id: node.args.get("unit_type_id")?.clone(),
        owner_player_id: node.args.get("owner_player_id")?.clone(),
        region_id: node.args.get("region_id")?.clone(),
        x: node.args.get("x")?.parse().ok()?,
        y: node.args.get("y")?.parse().ok()?,
        z: node.args.get("z")?.parse().ok()?,
    })
}

fn reference_combo<I, S>(ui: &mut egui::Ui, id: egui::Id, selected: &mut String, choices: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected.as_str())
        .show_ui(ui, |ui| {
            for choice in choices {
                let choice = choice.as_ref();
                ui.selectable_value(selected, choice.to_string(), choice);
            }
        });
}

fn coordinate(ui: &mut egui::Ui, label: &str, value: &mut f32) {
    ui.label(RichText::new(label).size(9.0).color(TEXT_DIM));
    ui.add(egui::DragValue::new(value).speed(0.25));
}

fn temp_or<T: Clone + 'static>(ui: &egui::Ui, id: egui::Id, fallback: T) -> T {
    ui.ctx()
        .data_mut(|data| data.get_temp::<T>(id))
        .unwrap_or(fallback)
}
