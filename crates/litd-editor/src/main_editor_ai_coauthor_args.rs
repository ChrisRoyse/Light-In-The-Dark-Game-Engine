use super::*;

pub(super) struct FromRecordArgs {
    pub(super) state_path: PathBuf,
    pub(super) aster_dir: PathBuf,
    pub(super) record: AiCoAuthorStateFile,
    pub(super) loaded_from_aster: bool,
    pub(super) save_count: u32,
    pub(super) readback: AsterReadback,
    pub(super) calyx_probe: CalyxAvailabilityProbe,
    pub(super) calyx_probe_cache: CalyxProbeCache,
}

pub(super) fn ai_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.ai_coauthor.{}", field.into()),
        reason: reason.to_string(),
    }
}
