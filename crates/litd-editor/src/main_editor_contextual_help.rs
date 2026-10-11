use egui::{RichText, Stroke};
use litd_editor::contextual_help::{ContextualHelpRequest, resolve_contextual_help};
use litd_engine::EngineError;

use super::*;

impl EditorShellRuntime {
    pub(super) fn open_contextual_help(&mut self) {
        self.contextual_help_open = true;
    }

    pub(super) fn show_contextual_help(&mut self, ctx: &egui::Context) {
        if !self.contextual_help_open {
            return;
        }
        let mut open = self.contextual_help_open;
        let mut submit = false;
        let mut execute = false;
        egui::Window::new("How do I…?")
            .id(egui::Id::new("editor.contextual_help"))
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL_BG)
                    .stroke(Stroke::new(1.0, GOLD_DEEP))
                    .corner_radius(4.0)
                    .inner_margin(12.0),
            )
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Ask one question. Get one project-grounded next step.")
                        .color(TEXT_DIM),
                );
                ui.add_space(6.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.contextual_help_query)
                        .hint_text("How do I see my change live?")
                        .desired_width(f32::INFINITY),
                );
                let enter =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                submit = ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new("Show my next step"),
                    )
                    .clicked()
                    || enter;
                if !self.contextual_help_error.is_empty() {
                    ui.label(
                        RichText::new(&self.contextual_help_error)
                            .color(ERROR)
                            .small(),
                    );
                }
                if let Some(answer) = self.contextual_help_answer.as_ref() {
                    ui.separator();
                    ui.label(
                        RichText::new(&answer.recommendation.headline)
                            .color(GOLD)
                            .strong(),
                    );
                    ui.label(RichText::new(&answer.recommendation.why_this_step).color(TEXT_DIM));
                    ui.label(format!("Action: {}", answer.recommendation.command_title));
                    ui.label(format!(
                        "Selection: {} ({})",
                        answer.recommendation.target_id, answer.recommendation.target_kind
                    ));
                    ui.label(
                        RichText::new(format!(
                            "Project proof: {}",
                            short_hash(&answer.context.document_fingerprint)
                        ))
                        .color(TEXT_DIM)
                        .small(),
                    );
                    execute = ui
                        .add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new("Do this next"),
                        )
                        .clicked();
                }
                if self.contextual_help_action_count > 0 {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "Applied {} time{} · {}",
                            self.contextual_help_action_count,
                            if self.contextual_help_action_count == 1 {
                                ""
                            } else {
                                "s"
                            },
                            self.contextual_help_last_action
                        ))
                        .color(GOOD)
                        .small(),
                    );
                }
            });
        self.contextual_help_open = open;
        if submit {
            self.submit_contextual_help();
        }
        if execute && let Err(error) = self.apply_contextual_help_recommendation() {
            self.record_contextual_help_error("apply", &error);
        }
    }

    fn submit_contextual_help(&mut self) {
        let selected_id = if self.plan.active_mode == EditorMode::Objects {
            Some(self.object_editor_selected_object.clone())
        } else {
            None
        };
        match resolve_contextual_help(
            &self.object_editor_document,
            ContextualHelpRequest {
                query: self.contextual_help_query.clone(),
                active_mode: self.plan.active_mode,
                selected_id,
            },
        ) {
            Ok(answer) => {
                self.contextual_help_answer = Some(answer);
                self.contextual_help_error.clear();
                self.clear_command_error();
            }
            Err(error) => {
                self.contextual_help_answer = None;
                self.record_contextual_help_error("resolve", &error);
            }
        }
    }

    fn apply_contextual_help_recommendation(&mut self) -> Result<(), EngineError> {
        let Some(answer) = self.contextual_help_answer.clone() else {
            return invalid_shell_runtime(
                "contextual_help.answer",
                "one grounded answer is required before applying help",
            );
        };
        if answer.recommendation.target_kind == "object"
            && !self
                .object_editor_document
                .objects
                .contains_key(&answer.recommendation.target_id)
        {
            return invalid_shell_runtime(
                "contextual_help.selection",
                format!(
                    "recommended object {} is absent from the active document",
                    answer.recommendation.target_id
                ),
            );
        }
        let outcome =
            self.invoke_editor_command_by_id(&answer.recommendation.command_id, "contextual_help")?;
        if !outcome.known {
            return invalid_shell_runtime(
                "contextual_help.command",
                format!(
                    "recommended command {} is not registered",
                    answer.recommendation.command_id
                ),
            );
        }
        if answer.recommendation.target_kind == "object" {
            self.object_editor_selected_object = answer.recommendation.target_id.clone();
        }
        self.contextual_help_action_count = self.contextual_help_action_count.saturating_add(1);
        self.contextual_help_last_action = format!(
            "command={} target={} selected={}",
            answer.recommendation.command_id,
            answer.recommendation.target_id,
            self.object_editor_selected_object
        );
        self.contextual_help_error.clear();
        Ok(())
    }

    fn record_contextual_help_error(&mut self, stage: &str, error: &EngineError) {
        self.contextual_help_error = error.to_string();
        self.record_command_error("editor.help.contextual", stage, format_args!("{error}"));
    }
}

fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue900_help_surface_keeps_one_answer_and_real_selection_readback() {
        let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
        runtime.contextual_help_query = "How do I improve this object?".to_string();
        runtime.submit_contextual_help();

        let answer = runtime.contextual_help_answer.as_ref().unwrap();
        assert_eq!(answer.recommendation.target_kind, "object");
        assert_eq!(
            answer.recommendation.target_id,
            runtime.object_editor_selected_object
        );
        assert!(runtime.contextual_help_error.is_empty());
    }

    #[test]
    fn issue900_help_surface_exposes_fail_loud_query_errors() {
        let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
        runtime.contextual_help_query = " ".to_string();
        runtime.submit_contextual_help();

        assert!(runtime.contextual_help_answer.is_none());
        assert!(
            runtime
                .contextual_help_error
                .contains("editor.contextual_help.query")
        );
        assert!(runtime.last_command_error_message().is_some());
    }

    #[test]
    fn issue900_help_action_records_command_and_selection_source_of_truth() {
        let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
        runtime.contextual_help_query = "How do I improve this object?".to_string();
        runtime.submit_contextual_help();
        runtime.apply_contextual_help_recommendation().unwrap();

        assert_eq!(runtime.contextual_help_action_count, 1);
        assert!(
            runtime
                .contextual_help_last_action
                .contains("command=editor.mode.objects")
        );
        assert!(
            runtime
                .contextual_help_last_action
                .contains("selected=base.unit.footman")
        );
    }
}
