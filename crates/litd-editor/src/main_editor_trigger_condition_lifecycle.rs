//! Structural condition controls kept separate from payload authoring UI.

use litd_editor::trigger_editor::{
    TriggerConditionMove, delete_trigger_condition, move_trigger_condition,
};

use super::EditorShellRuntime;

pub(super) fn apply_condition_delete(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
) {
    match delete_trigger_condition(&mut runtime.trigger_editor_project, trigger_id, node_id) {
        Ok(removed) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action = format!("deleted condition {}", removed.node_id);
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                error = %error,
                "trigger editor condition delete failed"
            );
        }
    }
}

pub(super) fn apply_condition_move(
    runtime: &mut EditorShellRuntime,
    trigger_id: &str,
    node_id: &str,
    direction: TriggerConditionMove,
) {
    match move_trigger_condition(
        &mut runtime.trigger_editor_project,
        trigger_id,
        node_id,
        direction,
    ) {
        Ok(true) => {
            runtime.mark_trigger_editor_dirty();
            runtime.trigger_editor_last_action =
                format!("moved condition {node_id} {}", move_label(direction));
        }
        Ok(false) => {
            runtime.trigger_editor_last_action =
                format!("condition {node_id} already {}", boundary_label(direction));
        }
        Err(error) => {
            runtime.trigger_editor_last_action = error.to_string();
            tracing::error!(
                trigger_id,
                node_id,
                ?direction,
                error = %error,
                "trigger editor condition reorder failed"
            );
        }
    }
}

fn move_label(direction: TriggerConditionMove) -> &'static str {
    match direction {
        TriggerConditionMove::Earlier => "up",
        TriggerConditionMove::Later => "down",
    }
}

fn boundary_label(direction: TriggerConditionMove) -> &'static str {
    match direction {
        TriggerConditionMove::Earlier => "first",
        TriggerConditionMove::Later => "last",
    }
}
