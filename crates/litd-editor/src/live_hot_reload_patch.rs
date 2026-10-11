use litd_engine::EngineError;

use super::{LiveHotReloadChangeClass, LiveHotReloadPatch, validate_single_line};

impl LiveHotReloadPatch {
    pub(super) fn patch_id(&self) -> &str {
        match self {
            Self::UnitStat { patch_id, .. }
            | Self::TriggerFlag { patch_id, .. }
            | Self::TextureMaterial { patch_id, .. }
            | Self::DialogueLine { patch_id, .. }
            | Self::CinematicSubtitle { patch_id, .. }
            | Self::StructuralAddUnit { patch_id, .. } => patch_id,
        }
    }

    pub(super) fn class(&self) -> LiveHotReloadChangeClass {
        match self {
            Self::UnitStat { .. }
            | Self::TriggerFlag { .. }
            | Self::TextureMaterial { .. }
            | Self::DialogueLine { .. }
            | Self::CinematicSubtitle { .. } => LiveHotReloadChangeClass::SafeLivePatch,
            Self::StructuralAddUnit { .. } => LiveHotReloadChangeClass::StructuralRestartRequired,
        }
    }

    pub(super) fn validate(&self) -> Result<(), EngineError> {
        validate_single_line("patch.patch_id", self.patch_id())?;
        match self {
            Self::UnitStat {
                object_id,
                stat_key,
                value,
                ..
            } => {
                validate_single_line("patch.object_id", object_id)?;
                validate_single_line("patch.stat_key", stat_key)?;
                validate_single_line("patch.value", value)?;
            }
            Self::TriggerFlag {
                object_id,
                trigger_id,
                ..
            } => {
                validate_single_line("patch.object_id", object_id)?;
                validate_single_line("patch.trigger_id", trigger_id)?;
            }
            Self::TextureMaterial {
                object_id,
                material_id,
                ..
            } => {
                validate_single_line("patch.object_id", object_id)?;
                validate_single_line("patch.material_id", material_id)?;
            }
            Self::DialogueLine {
                object_id, line, ..
            } => {
                validate_single_line("patch.object_id", object_id)?;
                validate_single_line("patch.line", line)?;
            }
            Self::CinematicSubtitle {
                timeline_id, text, ..
            } => {
                validate_single_line("patch.timeline_id", timeline_id)?;
                validate_single_line("patch.text", text)?;
            }
            Self::StructuralAddUnit { object_id, .. } => {
                validate_single_line("patch.object_id", object_id)?;
            }
        }
        Ok(())
    }
}
