//! #892: cinematic-timeline document methods (child module of `history`).
//!
//! Authored [`EditorCinematicTimeline`]s reference the document's named cameras by
//! id. Upsert and whole-document validation both enforce that every referenced
//! camera exists, so a dangling shot fails loud at author time rather than at play.

use litd_engine::EngineError;

use super::{EditorDocument, invalid_history};
use crate::cinematic_authoring::EditorCinematicTimeline;

impl EditorDocument {
    /// #892: upsert an authored cinematic timeline. Validates the timeline
    /// structurally and that every camera keyframe references a named camera that
    /// already exists on the document (a dangling ref fails loud here, not at play).
    pub fn upsert_cinematic_timeline(
        &mut self,
        timeline: EditorCinematicTimeline,
    ) -> Result<(), EngineError> {
        timeline.validate()?;
        validate_timeline_camera_refs(self, &timeline)?;
        self.cinematic_timelines
            .insert(timeline.id.clone(), timeline);
        Ok(())
    }
}

/// Validate every stored timeline: its map key matches its id, it is structurally
/// valid, and each camera keyframe references a named camera on the document.
pub(super) fn validate_document_timelines(document: &EditorDocument) -> Result<(), EngineError> {
    for (timeline_id, timeline) in &document.cinematic_timelines {
        if timeline_id != &timeline.id {
            return invalid_history(
                "document.cinematic_timelines",
                format!(
                    "map key {timeline_id} does not match timeline id {}",
                    timeline.id
                ),
            );
        }
        timeline.validate()?;
        validate_timeline_camera_refs(document, timeline)?;
    }
    Ok(())
}

fn validate_timeline_camera_refs(
    document: &EditorDocument,
    timeline: &EditorCinematicTimeline,
) -> Result<(), EngineError> {
    for key in &timeline.camera_keys {
        if !document.named_cameras.contains_key(&key.camera_ref) {
            return invalid_history(
                "document.cinematic_timelines",
                format!(
                    "timeline {} references missing named camera {}",
                    timeline.id, key.camera_ref
                ),
            );
        }
    }
    Ok(())
}
