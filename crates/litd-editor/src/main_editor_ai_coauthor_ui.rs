use egui::RichText;
use tracing::error;

use super::main_editor_frame::panel_width;
use super::*;

impl EditorShellRuntime {
    pub(in crate::main_editor_ui) fn ai_controls(&mut self, ui: &mut egui::Ui) {
        if let Err(err) = self.ai_coauthor.refresh_calyx_feature() {
            self.ai_coauthor.last_error = err.to_string();
        }
        ui.label(RichText::new("Grounded prompt").color(TEXT_DIM));
        ui.add(
            egui::TextEdit::multiline(self.ai_coauthor.prompt_mut())
                .desired_rows(5)
                .desired_width(panel_width(ui)),
        );
        ui.add(
            egui::ProgressBar::new(self.ai_coauthor.progress_fraction())
                .text(self.ai_coauthor.progress_label()),
        );
        if !self.ai_coauthor.calyx_feature_enabled() {
            ui.label(
                RichText::new(self.ai_coauthor.calyx_disabled_reason())
                    .size(11.0)
                    .color(ERROR),
            );
        }
        self.ai_action_button(ui, "Generate Proposal", AiAction::Generate);
        self.ai_action_button(ui, "Apply To World", AiAction::Apply);
        self.ai_action_button(ui, "Reject Proposal", AiAction::Reject);
        self.ai_action_button(ui, "Hand Edit Label", AiAction::HandEdit);
        self.ai_action_button(ui, "Undo Apply", AiAction::Undo);
        let evidence = self.ai_coauthor.evidence();
        ui.separator();
        ui.label(
            RichText::new(evidence.state_summary)
                .size(11.0)
                .color(TEXT_DIM),
        );
        ui.label(RichText::new(evidence.aster_summary).size(11.0).color(GOLD));
        if !evidence.last_error.trim().is_empty() {
            ui.label(RichText::new(evidence.last_error).size(11.0).color(ERROR));
        }
    }

    pub(in crate::main_editor_ui) fn ai_coauthor_compact_prompt(
        &mut self,
        ui: &mut egui::Ui,
        compact: bool,
    ) {
        let height = if compact { 46.0 } else { 70.0 };
        let rows = if compact { 2 } else { 3 };
        if let Err(err) = self.ai_coauthor.refresh_calyx_feature() {
            self.ai_coauthor.last_error = err.to_string();
        }
        ui.add_sized(
            [panel_width(ui), height],
            egui::TextEdit::multiline(self.ai_coauthor.prompt_mut())
                .desired_rows(rows)
                .desired_width(panel_width(ui)),
        );
        let progress_height = if compact { 12.0 } else { 18.0 };
        ui.add_sized(
            [panel_width(ui), progress_height],
            egui::ProgressBar::new(self.ai_coauthor.progress_fraction())
                .desired_width(panel_width(ui)),
        );
        let generate_size = egui::vec2(panel_width(ui), if compact { 22.0 } else { 28.0 });
        if ui
            .add_enabled(
                self.ai_coauthor.calyx_feature_enabled(),
                egui::Button::new("Generate").min_size(generate_size),
            )
            .on_hover_text(self.ai_coauthor.calyx_disabled_reason())
            .clicked()
        {
            self.run_ai_action(AiAction::Generate);
        }
        if compact && !self.ai_coauthor.calyx_feature_enabled() {
            ui.label(
                RichText::new(self.ai_coauthor.calyx_disabled_reason())
                    .size(10.0)
                    .color(ERROR),
            );
        } else if !compact && !self.ai_coauthor.calyx_feature_enabled() {
            ui.add_sized(
                [panel_width(ui), if compact { 22.0 } else { 28.0 }],
                egui::Label::new(
                    RichText::new(self.ai_coauthor.calyx_disabled_reason())
                        .size(10.0)
                        .color(ERROR),
                ),
            );
        }
    }

    pub(in crate::main_editor_ui) fn ai_coauthor_evidence(&self) -> AiCoAuthorEvidence {
        self.ai_coauthor.evidence()
    }

    pub(in crate::main_editor_ui) fn ai_coauthor_last_error_message(&self) -> Option<&str> {
        self.ai_coauthor.last_error_message()
    }

    fn ai_action_button(&mut self, ui: &mut egui::Ui, label: &str, action: AiAction) {
        let enabled = !matches!(action, AiAction::Generate | AiAction::Apply)
            || self.ai_coauthor.calyx_feature_enabled();
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(label).min_size(egui::vec2(panel_width(ui), 28.0)),
            )
            .on_hover_text(self.ai_coauthor.calyx_disabled_reason())
            .clicked()
        {
            self.run_ai_action(action);
        }
    }

    fn run_ai_action(&mut self, action: AiAction) {
        let result = match action {
            AiAction::Generate => self.ai_coauthor.generate_proposal(),
            AiAction::Apply => self.ai_coauthor.apply_to_world(),
            AiAction::Reject => self.ai_coauthor.reject_proposal(),
            AiAction::HandEdit => self
                .ai_coauthor
                .hand_edit_last_object("Cinder Bloom Ward (hand edited)"),
            AiAction::Undo => self.ai_coauthor.undo_last_apply(),
        };
        if let Err(err) = result {
            let message = err.to_string();
            self.ai_coauthor.last_error = message.clone();
            error!(error = %message, action = ?action, "ai co-author action failed");
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AiAction {
    Generate,
    Apply,
    Reject,
    HandEdit,
    Undo,
}
