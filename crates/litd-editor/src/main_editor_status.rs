use std::time::Duration;

use super::*;

const ACTION_FEEDBACK_PULSE_SECONDS: f64 = 0.72;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorStatusSummary {
    pub(crate) health: &'static str,
    pub(crate) message: String,
    pub(crate) error_count: u32,
}

impl EditorStatusSummary {
    pub(crate) fn label(&self, compact: bool) -> String {
        if self.error_count == 0 {
            return self.message.clone();
        }
        if compact {
            return format!(
                "{} Error{}",
                self.error_count,
                if self.error_count == 1 { "" } else { "s" }
            );
        }
        compact_status_text(&self.message, 96)
    }
}

#[derive(Debug, Clone, Copy)]
struct ActionFeedbackPulse {
    invocation_count: u32,
    started_at_seconds: f64,
}

impl EditorShellRuntime {
    pub(crate) fn status_summary(&self) -> EditorStatusSummary {
        let mut errors = Vec::new();
        if !self.model_previewer_error.trim().is_empty() {
            errors.push(format!(
                "Previewer: {}",
                compact_status_text(&self.model_previewer_error, 180)
            ));
        }
        let thumbnail_errors = self.thumbnail_cache.error_count();
        if thumbnail_errors > 0 {
            errors.push(format!(
                "Thumbnails: {thumbnail_errors} asset thumbnail{} failed",
                if thumbnail_errors == 1 { "" } else { "s" }
            ));
        }
        if self.map_build_status.starts_with("error:") {
            errors.push(format!(
                "Map Build: {}",
                compact_status_text(&self.map_build_status, 180)
            ));
        }
        if let Some(error) = self.world_creation_last_error_message() {
            errors.push(format!("World: {}", compact_status_text(error, 180)));
        }
        let validation = self.world_validation_summary();
        if validation.blocking {
            errors.push(format!(
                "Validation: {}",
                compact_status_text(&validation.error_summary(), 180)
            ));
        }
        if let Some(error) = self.ai_coauthor_last_error_message() {
            errors.push(format!("AI Co-Author: {}", compact_status_text(error, 180)));
        }
        let vfx = self.vfx_editor.evidence();
        if !vfx.last_error.trim().is_empty() {
            errors.push(format!(
                "VFX: {}",
                compact_status_text(&vfx.last_error, 180)
            ));
        }
        if let Some(error) = self.last_command_error_message() {
            errors.push(format!("Command: {}", compact_status_text(error, 180)));
        }

        if errors.is_empty() {
            return EditorStatusSummary {
                health: "ok",
                message: self
                    .successful_action_message()
                    .unwrap_or_else(|| "No Errors".to_string()),
                error_count: 0,
            };
        }
        let error_count =
            u32::try_from(errors.len()).expect("editor status error source count must fit u32");
        EditorStatusSummary {
            health: "error",
            message: format!(
                "{} Error{}: {}",
                error_count,
                if error_count == 1 { "" } else { "s" },
                errors.join(" | ")
            ),
            error_count,
        }
    }

    pub(super) fn status_feedback_emphasis(&self, ctx: &egui::Context) -> f32 {
        if self.command_invocation_count == 0 || self.last_command_error_message().is_some() {
            return 0.0;
        }
        let now = ctx.input(|input| input.time);
        let id = egui::Id::new("editor.status.action_feedback_pulse");
        let pulse = ctx.data_mut(|data| {
            let mut pulse =
                data.get_temp::<ActionFeedbackPulse>(id)
                    .unwrap_or(ActionFeedbackPulse {
                        invocation_count: 0,
                        started_at_seconds: now,
                    });
            if pulse.invocation_count != self.command_invocation_count {
                pulse = ActionFeedbackPulse {
                    invocation_count: self.command_invocation_count,
                    started_at_seconds: now,
                };
            }
            data.insert_temp(id, pulse);
            pulse
        });
        let emphasis = action_feedback_emphasis(now - pulse.started_at_seconds);
        if emphasis > 0.0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        emphasis
    }

    fn successful_action_message(&self) -> Option<String> {
        use litd_editor::editor_commands::{
            COMMAND_CREATE_WORLD, COMMAND_TOGGLE_PLAYTEST, COMMAND_VALIDATE_WORLD,
        };

        if self.command_invocation_count == 0 {
            return None;
        }
        let command = self.command_registry.get(&self.last_command_id)?;
        let message = match command.command_id.as_str() {
            COMMAND_CREATE_WORLD => "World created — your story starts here".to_string(),
            COMMAND_VALIDATE_WORLD => "World check complete — ready for the next move".to_string(),
            COMMAND_TOGGLE_PLAYTEST if self.play_running() => {
                "Playtest live — your world is moving".to_string()
            }
            COMMAND_TOGGLE_PLAYTEST => "Playtest complete — back to building".to_string(),
            _ => command
                .title
                .strip_prefix("Switch to ")
                .and_then(|title| title.strip_suffix(" Mode"))
                .map(|mode| format!("{mode} mode ready — keep creating"))
                .unwrap_or_else(|| format!("Done — {}", command.title)),
        };
        Some(message)
    }
}

fn action_feedback_emphasis(elapsed_seconds: f64) -> f32 {
    if !(0.0..ACTION_FEEDBACK_PULSE_SECONDS).contains(&elapsed_seconds) {
        return 0.0;
    }
    let remaining = 1.0 - elapsed_seconds / ACTION_FEEDBACK_PULSE_SECONDS;
    (remaining * remaining) as f32
}

fn compact_status_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut value = text
        .chars()
        .take(limit.saturating_sub(3))
        .collect::<String>();
    value.push_str("...");
    value
}

#[cfg(test)]
mod tests {
    use litd_editor::editor_commands::{COMMAND_CREATE_WORLD, COMMAND_MODE_OBJECTS};

    use super::*;

    #[test]
    fn issue906_success_status_turns_commands_into_concise_momentum_feedback() {
        let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
        assert_eq!(runtime.status_summary().message, "No Errors");

        runtime.command_invocation_count = 1;
        runtime.last_command_id = COMMAND_MODE_OBJECTS.to_string();
        assert_eq!(
            runtime.status_summary().message,
            "Objects mode ready — keep creating"
        );

        runtime.command_invocation_count = 2;
        runtime.last_command_id = COMMAND_CREATE_WORLD.to_string();
        assert_eq!(
            runtime.status_summary().message,
            "World created — your story starts here"
        );
    }

    #[test]
    fn issue906_success_pulse_is_brief_and_eases_to_rest() {
        assert_eq!(action_feedback_emphasis(-0.01), 0.0);
        assert_eq!(action_feedback_emphasis(0.0), 1.0);
        let halfway = action_feedback_emphasis(ACTION_FEEDBACK_PULSE_SECONDS / 2.0);
        assert!((halfway - 0.25).abs() < f32::EPSILON);
        assert_eq!(action_feedback_emphasis(ACTION_FEEDBACK_PULSE_SECONDS), 0.0);
        assert_eq!(action_feedback_emphasis(10.0), 0.0);
    }
}
