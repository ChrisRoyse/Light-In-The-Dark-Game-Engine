//! #892: no-code cinematic authoring in the editor.
//!
//! An [`EditorCinematicTimeline`] is the authored form: its camera track references
//! the named cameras (#813) placed on the same [`EditorDocument`] by id, so the
//! author picks shots by name and never types a camera pose. The other tracks
//! (transmissions, fades, subtitles, cues) reuse the engine's frozen data types
//! directly -- no duplicate model.
//!
//! [`lower_cinematic_timeline`] is the persist -> lower valve: it resolves each
//! `camera_ref` against the document's named cameras into a concrete
//! [`CinematicCameraPose`] (the same `anchor + target_offset` resolution the
//! play-in-editor camera uses) and produces a frozen, fingerprinted
//! [`CinematicTimeline`] that the runtime evaluator consumes without ever touching
//! the editor. Lowering validates the result, so a dangling camera ref or an
//! otherwise-invalid timeline fails loud here.

use std::collections::BTreeMap;

use litd_engine::EngineError;
use litd_engine::cinematic_timeline::{
    CameraKeyframe, CinematicCameraPose, CinematicCue, CinematicFade, CinematicSubtitle,
    CinematicTimeline, CinematicTransmission,
};
use serde::{Deserialize, Serialize};

use crate::history::{EditorDocument, EditorNamedCamera};

/// One authored camera cut: hold/interpolate the named camera `camera_ref` at
/// `time_seconds`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorCameraKeyframe {
    pub time_seconds: f32,
    pub camera_ref: String,
}

/// An authored cutscene timeline living on the editor document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorCinematicTimeline {
    pub id: String,
    pub name: String,
    pub duration_seconds: f32,
    pub camera_keys: Vec<EditorCameraKeyframe>,
    #[serde(default)]
    pub transmissions: Vec<CinematicTransmission>,
    #[serde(default)]
    pub fades: Vec<CinematicFade>,
    #[serde(default)]
    pub subtitles: Vec<CinematicSubtitle>,
    #[serde(default)]
    pub cues: Vec<CinematicCue>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

impl EditorCinematicTimeline {
    pub fn new(id: impl Into<String>, name: impl Into<String>, duration_seconds: f32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            duration_seconds,
            camera_keys: Vec::new(),
            transmissions: Vec::new(),
            fades: Vec::new(),
            subtitles: Vec::new(),
            cues: Vec::new(),
            properties: BTreeMap::new(),
        }
    }

    pub fn with_camera_key(mut self, time_seconds: f32, camera_ref: impl Into<String>) -> Self {
        self.camera_keys.push(EditorCameraKeyframe {
            time_seconds,
            camera_ref: camera_ref.into(),
        });
        self
    }

    pub fn with_transmission(mut self, transmission: CinematicTransmission) -> Self {
        self.transmissions.push(transmission);
        self
    }

    pub fn with_fade(mut self, fade: CinematicFade) -> Self {
        self.fades.push(fade);
        self
    }

    pub fn with_subtitle(mut self, subtitle: CinematicSubtitle) -> Self {
        self.subtitles.push(subtitle);
        self
    }

    pub fn with_cue(mut self, cue: CinematicCue) -> Self {
        self.cues.push(cue);
        self
    }

    /// Structural validation independent of the document (ids present, at least one
    /// camera cut, duration positive). Cross-references to named cameras are checked
    /// by the owning document and at lowering.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err(invalid_cinematic("ids", "id and name must be non-empty"));
        }
        if !(self.duration_seconds.is_finite() && self.duration_seconds > 0.0) {
            return Err(invalid_cinematic("duration", "duration must be > 0"));
        }
        if self.camera_keys.is_empty() {
            return Err(invalid_cinematic(
                "camera_keys",
                "at least one camera keyframe is required",
            ));
        }
        for key in &self.camera_keys {
            if key.camera_ref.trim().is_empty() {
                return Err(invalid_cinematic(
                    "camera_ref",
                    "camera_ref must be non-empty",
                ));
            }
        }
        Ok(())
    }
}

/// Resolve a named camera into a concrete cinematic pose. The framed world point is
/// `look_at + target_offset` -- identical to the play-in-editor camera resolution --
/// and the rig scalars come straight from the camera config.
pub fn resolve_named_camera_pose(camera: &EditorNamedCamera) -> CinematicCameraPose {
    CinematicCameraPose {
        target: camera.look_at + camera.config.target_offset,
        distance: camera.config.distance,
        yaw_degrees: camera.config.yaw_degrees,
        pitch_degrees: camera.config.pitch_degrees,
        fov_degrees: camera.config.fov_degrees,
    }
}

/// Persist -> lower valve: resolve every camera ref against `document` and freeze a
/// runtime-facing [`CinematicTimeline`]. Fails loud on a dangling camera ref or any
/// invariant the engine evaluator relies on (via the engine `validate`).
pub fn lower_cinematic_timeline(
    document: &EditorDocument,
    timeline: &EditorCinematicTimeline,
) -> Result<CinematicTimeline, EngineError> {
    timeline.validate()?;
    let mut camera_track = Vec::with_capacity(timeline.camera_keys.len());
    for key in &timeline.camera_keys {
        let camera = document.named_cameras.get(&key.camera_ref).ok_or_else(|| {
            invalid_cinematic(
                "camera_ref",
                format!(
                    "timeline {} references missing named camera {}",
                    timeline.id, key.camera_ref
                ),
            )
        })?;
        camera_track.push(CameraKeyframe {
            time_seconds: key.time_seconds,
            camera_ref: key.camera_ref.clone(),
            pose: resolve_named_camera_pose(camera),
        });
    }
    let lowered = CinematicTimeline {
        timeline_id: timeline.id.clone(),
        duration_seconds: timeline.duration_seconds,
        camera_track,
        transmissions: timeline.transmissions.clone(),
        fades: timeline.fades.clone(),
        subtitles: timeline.subtitles.clone(),
        cues: timeline.cues.clone(),
    };
    lowered
        .validate()
        .map_err(|error| invalid_cinematic("lowered", error.to_string()))?;
    Ok(lowered)
}

pub(crate) fn invalid_cinematic(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.cinematic_authoring.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "cinematic_authoring_tests.rs"]
mod tests;
