use egui::RichText;
use litd_editor::{
    mcp_trigger_tools::{EcaGraph, EcaNode},
    trigger_editor::{
        TriggerActionMove, delete_trigger_action, fire_unit_dies, move_trigger_action,
        set_trigger_enabled, set_trigger_wait_duration, trigger_graph_summary,
    },
};

use super::main_editor_frame::panel_width;
use super::*;

#[path = "main_editor_trigger_action_palette.rs"]
mod action_palette;
#[path = "main_editor_trigger_conditions.rs"]
mod condition_editor;
#[path = "main_editor_trigger_condition_lifecycle.rs"]
mod condition_lifecycle;
#[path = "main_editor_trigger_create_unit_action.rs"]
mod create_unit_action_editor;
#[path = "main_editor_trigger_events.rs"]
mod event_editor;
#[path = "main_editor_trigger_graph_lifecycle.rs"]
mod graph_lifecycle;
#[path = "main_editor_trigger_graph_metadata.rs"]
mod graph_metadata_editor;
#[path = "main_editor_trigger_project_file.rs"]
mod project_file;
#[path = "main_editor_trigger_variable_action.rs"]
mod variable_action_editor;
#[path = "main_editor_trigger_variables.rs"]
mod variable_editor;

pub(super) use project_file::TriggerProjectFileState;

impl EditorShellRuntime {
    /// Invalidate the memoized trigger-graph RON byte count. Call after ANY mutation of
    /// `trigger_editor_project` so the next draw re-serializes (#1461). O(1).
    pub(super) fn mark_trigger_editor_dirty(&mut self) {
        self.trigger_editor_generation = self.trigger_editor_generation.wrapping_add(1);
        self.trigger_editor_file.dirty = true;
    }

    pub(super) fn trigger_controls(&mut self, ui: &mut egui::Ui) {
        let compact = panel_width(ui) < 260.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Trigger Editor").strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Fire").clicked() {
                    match fire_unit_dies(&self.trigger_editor_project, "unit_scout") {
                        Ok(report) => {
                            self.trigger_editor_last_action = format!(
                                "fired {} actions; created {}",
                                report.actions_executed.len(),
                                report.created_unit_ids.join(",")
                            );
                        }
                        Err(error) => {
                            self.trigger_editor_last_action = error.to_string();
                            tracing::error!(error = %error, "trigger editor fire failed");
                        }
                    }
                }
            });
        });
        project_file::project_file_controls(ui, self);
        graph_lifecycle::graph_lifecycle_controls(ui, self);
        let graph_count = self.trigger_editor_project.graphs.len();
        if graph_count == 0 {
            ui.label(RichText::new("No trigger graphs").color(ERROR));
            return;
        }
        self.trigger_editor_selected_graph = self
            .trigger_editor_selected_graph
            .min(graph_count.saturating_sub(1));
        graph_picker(ui, self);
        variable_editor::variable_controls(ui, self);
        graph_enabled_control(ui, self);
        let graph = self.trigger_editor_project.graphs[self.trigger_editor_selected_graph].clone();
        graph_metadata_editor::graph_metadata_control(ui, self, &graph);
        ui.separator();
        // #1461: memoized byte count — RON-serialize the graph only when it (or the selection)
        // changes, not every frame.
        let ron_bytes = self.trigger_editor_ron_cache.bytes(
            self.trigger_editor_generation,
            self.trigger_editor_selected_graph,
            &graph,
        );
        graph_summary(ui, &graph, compact, &ron_bytes);
        ui.separator();
        event_editor::event_nodes(ui, self, &graph);
        condition_editor::condition_nodes(ui, self, &graph);
        action_nodes(ui, self, &graph);
        ui.separator();
        ui.label(
            RichText::new(format!("Action: {}", self.trigger_editor_last_action))
                .size(11.0)
                .color(TEXT_DIM),
        );
    }
}

fn graph_enabled_control(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime) {
    let selected = runtime.trigger_editor_selected_graph;
    let trigger_id = runtime.trigger_editor_project.graphs[selected]
        .trigger_id
        .clone();
    let mut enabled = runtime.trigger_editor_project.graphs[selected].enabled;
    if !ui.checkbox(&mut enabled, "Enabled").changed() {
        return;
    }
    match set_trigger_enabled(&mut runtime.trigger_editor_project, &trigger_id, enabled) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!(
                "{} {trigger_id}",
                if enabled { "enabled" } else { "disabled" }
            );
        }
        Ok(false) => {
            runtime.trigger_editor_last_action = format!("{trigger_id} already enabled={enabled}");
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                enabled,
                error = %error,
                "trigger editor enable mutation failed"
            );
        }
    }
}

fn graph_picker(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime) {
    egui::ScrollArea::vertical()
        .id_salt("trigger_editor_graph_list")
        .max_height(70.0)
        .show(ui, |ui| {
            for (index, graph) in runtime.trigger_editor_project.graphs.iter().enumerate() {
                let selected = runtime.trigger_editor_selected_graph == index;
                if ui
                    .selectable_label(selected, graph.display_name.clone())
                    .clicked()
                {
                    runtime.trigger_editor_selected_graph = index;
                    runtime.trigger_editor_last_action = format!("selected {}", graph.trigger_id);
                }
            }
        });
}

fn graph_summary(
    ui: &mut egui::Ui,
    graph: &EcaGraph,
    compact: bool,
    ron_bytes: &Result<usize, String>,
) {
    ui.label(
        RichText::new(if compact {
            graph.display_name.clone()
        } else {
            trigger_graph_summary(graph)
        })
        .size(12.0)
        .strong()
        .color(GOLD),
    );
    ui.label(
        RichText::new(format!(
            "folder={} enabled={}",
            graph.folder_path.as_deref().unwrap_or("root"),
            graph.enabled
        ))
        .size(11.0)
        .color(TEXT_DIM),
    );
    match ron_bytes {
        Ok(bytes) => ui.label(
            RichText::new(format!("stable graph bytes={bytes}"))
                .size(11.0)
                .color(TEXT_DIM),
        ),
        Err(error) => ui.label(
            RichText::new(format!("graph hash error: {error}"))
                .size(11.0)
                .color(ERROR),
        ),
    };
}

fn action_nodes(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime, graph: &EcaGraph) {
    let selection_id = ui.make_persistent_id("trigger_editor_selected_action");
    let mut selected_action = ui
        .ctx()
        .data_mut(|data| data.get_temp::<String>(selection_id));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Action").size(12.0).strong().color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            action_palette::show_action_palette(ui, runtime, &graph.trigger_id);
        });
    });
    for node in ordered(&graph.actions) {
        let mut requested_move = None;
        let mut requested_delete = false;
        let mut requested_duration = None;
        let mut requested_create_unit_edit = None;
        let mut requested_variable_edit = None;
        ui.group(|ui| {
            ui.set_min_width(panel_width(ui).max(120.0));
            ui.horizontal(|ui| {
                let selected = selected_action.as_deref() == Some(node.node_id.as_str());
                if ui
                    .selectable_label(
                        selected,
                        RichText::new(format!("{}  /  {}", node.operation, node.node_id))
                            .size(11.0)
                            .strong()
                            .color(TEXT),
                    )
                    .clicked()
                {
                    selected_action = Some(node.node_id.clone());
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(selection_id, node.node_id.clone()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Delete").clicked() {
                        requested_delete = true;
                    }
                    if ui.small_button("Move down").clicked() {
                        requested_move = Some(TriggerActionMove::Later);
                    }
                    if ui.small_button("Move up").clicked() {
                        requested_move = Some(TriggerActionMove::Earlier);
                    }
                });
            });
            node_details(ui, node);
            if selected_action.as_deref() == Some(node.node_id.as_str())
                && node.operation == "sleep"
            {
                wait_duration_control(ui, node, &mut requested_duration);
            }
            if selected_action.as_deref() == Some(node.node_id.as_str())
                && node.operation == "set_variable"
            {
                variable_action_editor::variable_action_control(
                    ui,
                    node,
                    &runtime.trigger_editor_project.variables,
                    &mut requested_variable_edit,
                );
            }
            if selected_action.as_deref() == Some(node.node_id.as_str())
                && node.operation == "create_unit_at_point"
            {
                create_unit_action_editor::create_unit_action_control(
                    ui,
                    node,
                    &runtime.trigger_editor_project,
                    &mut requested_create_unit_edit,
                );
            }
        });
        if requested_delete {
            apply_action_delete(runtime, &graph.trigger_id, &node.node_id);
        } else if let Some(duration_ms) = requested_duration {
            apply_wait_duration_edit(runtime, &graph.trigger_id, &node.node_id, duration_ms);
        } else if let Some(edit) = requested_variable_edit {
            variable_action_editor::apply_variable_action_edit(
                runtime,
                &graph.trigger_id,
                &node.node_id,
                edit,
            );
        } else if let Some(edit) = requested_create_unit_edit {
            create_unit_action_editor::apply_create_unit_action_edit(
                runtime,
                &graph.trigger_id,
                &node.node_id,
                edit,
            );
        } else if let Some(direction) = requested_move {
            apply_action_move(runtime, &graph.trigger_id, &node.node_id, direction);
        }
    }
}

fn wait_duration_control(ui: &mut egui::Ui, node: &EcaNode, requested: &mut Option<u32>) {
    let Some(duration_ms) = node
        .args
        .get("duration_ms")
        .and_then(|value| value.parse::<u32>().ok())
    else {
        ui.label(RichText::new("Invalid wait duration").color(ERROR));
        return;
    };
    let mut edited = duration_ms;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Duration").size(10.0).color(TEXT_DIM));
        if ui
            .add(
                egui::DragValue::new(&mut edited)
                    .range(1..=60_000)
                    .suffix(" ms"),
            )
            .changed()
        {
            *requested = Some(edited);
        }
    });
}

fn apply_wait_duration_edit(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    duration_ms: u32,
) {
    match set_trigger_wait_duration(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        duration_ms,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("set {node_id} duration to {duration_ms} ms");
        }
        Ok(false) => {
            runtime.trigger_editor_last_action =
                format!("{node_id} duration already {duration_ms} ms");
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                duration_ms,
                error = %error,
                "trigger editor wait duration edit failed"
            );
        }
    }
}

fn apply_action_delete(runtime: &mut EditorShellRuntime, trigger_id: &str, node_id: &str) {
    match delete_trigger_action(&mut runtime.trigger_editor_project, trigger_id, node_id) {
        Ok(removed) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("deleted action {}", removed.node_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                error = %error,
                "trigger editor action delete failed"
            );
        }
    }
}

fn apply_action_move(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    direction: TriggerActionMove,
) {
    match move_trigger_action(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        direction,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("moved {node_id} {}", move_label(direction));
        }
        Ok(false) => {
            runtime.trigger_editor_last_action =
                format!("{node_id} already {}", boundary_label(direction));
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                ?direction,
                error = %error,
                "trigger editor action reorder failed"
            );
        }
    }
}

fn move_label(direction: TriggerActionMove) -> &'static str {
    match direction {
        TriggerActionMove::Earlier => "up",
        TriggerActionMove::Later => "down",
    }
}

fn boundary_label(direction: TriggerActionMove) -> &'static str {
    match direction {
        TriggerActionMove::Earlier => "first",
        TriggerActionMove::Later => "last",
    }
}

fn node_details(ui: &mut egui::Ui, node: &EcaNode) {
    let args = node
        .args
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    ui.label(RichText::new(args).size(10.0).color(TEXT_DIM));
    if !node.variable_refs.is_empty() {
        ui.label(
            RichText::new(format!("refs={}", node.variable_refs.join(",")))
                .size(10.0)
                .color(GOLD),
        );
    }
}

fn ordered(nodes: &[EcaNode]) -> Vec<&EcaNode> {
    let mut nodes = nodes.iter().collect::<Vec<_>>();
    nodes.sort_by_key(|node| node.order);
    nodes
}
