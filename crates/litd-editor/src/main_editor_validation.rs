use litd_editor::{
    history::EditorDocument,
    world_validation::{
        WorldValidationReport, WorldValidationUseCase, require_editor_world_valid,
        validate_editor_world,
    },
};
use litd_engine::EngineError;

use super::*;

impl EditorShellRuntime {
    pub(crate) fn validate_active_world_command(&mut self) -> Result<(), EngineError> {
        let document = self.play_document_from_vault()?;
        let report = validate_editor_world(&document, WorldValidationUseCase::Play);
        self.record_world_validation_report(report);
        Ok(())
    }

    pub(super) fn require_world_validation(
        &mut self,
        document: &EditorDocument,
        use_case: WorldValidationUseCase,
    ) -> Result<WorldValidationReport, EngineError> {
        let report = validate_editor_world(document, use_case);
        self.record_world_validation_report(report.clone());
        if report.blocking {
            return Err(EngineError::UiRenderInvalidConfig {
                field: "editor.validation.blocking".to_string(),
                reason: report.error_summary(),
            });
        }
        require_editor_world_valid(document, use_case)
    }

    fn record_world_validation_report(&mut self, report: WorldValidationReport) {
        tracing::info!(
            use_case = ?report.use_case,
            blocking = report.blocking,
            issue_count = report.issue_count,
            error_count = report.error_count,
            summary = %report.summary,
            "editor world validation completed"
        );
        self.world_validation_report = report;
        self.world_validation_run_count = self.world_validation_run_count.saturating_add(1);
    }

    pub(crate) fn world_validation_summary(&self) -> &WorldValidationReport {
        &self.world_validation_report
    }
}

#[cfg(test)]
#[path = "main_editor_validation_tests.rs"]
mod tests;
