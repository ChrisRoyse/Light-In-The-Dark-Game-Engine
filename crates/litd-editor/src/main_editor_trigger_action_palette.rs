use litd_editor::trigger_editor::{TriggerActionPaletteKind, append_default_trigger_action};

use super::EditorShellRuntime;

pub(super) fn show_action_palette(
    ui: &mut egui::Ui,
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
) {
    let mut requested = None;
    ui.menu_button("Add action", |ui| {
        if ui.button("Wait 250 ms").clicked() {
            requested = Some(TriggerActionPaletteKind::Wait);
        }
        if ui.button("Create unit at point").clicked() {
            requested = Some(TriggerActionPaletteKind::CreateUnitAtPoint);
        }
        if ui.button("Set variable").clicked() {
            requested = Some(TriggerActionPaletteKind::SetVariable);
        }
    });
    if let Some(kind) = requested {
        apply_palette_action(runtime, trigger_id, kind);
    }
}

fn apply_palette_action(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    kind: TriggerActionPaletteKind,
) {
    match append_default_trigger_action(&mut runtime.trigger_editor_project, trigger_id, kind) {
        Ok(action) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("added {} action {}", action.operation, action.node_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                ?kind,
                error = %error,
                "trigger editor palette action append failed"
            );
        }
    }
}
