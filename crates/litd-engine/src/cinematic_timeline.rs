//! #892: deterministic cinematic/cutscene timeline data model + pure evaluator.
//!
//! The editor authors a no-code cutscene as a set of tracks (camera moves between
//! named cameras, character transmissions, fades, subtitles, and music/voice cue
//! refs) and LOWERS it into this frozen, runtime-facing [`CinematicTimeline`]. The
//! runtime never re-queries the editor: it calls [`evaluate_cinematic`] which is a
//! PURE function of `(timeline, t)`. Given the same lowered timeline and the same
//! sample time the evaluated state is byte-identical every run, so what plays back
//! is exactly what the editor authored.
//!
//! Documented resolution rules (encoded once here, exercised by FSV):
//! - Camera: at `t <= first.time` hold the first keyframe pose; at `t >= last.time`
//!   hold the last; otherwise linearly interpolate every pose scalar between the two
//!   bracketing keyframes by `(t - t0) / (t1 - t0)`.
//! - Overlapping transmissions: the ACTIVE one is the highest `track` index; ties
//!   broken by the latest `start_seconds`, then by the lexicographically greatest
//!   `speaker_unit_type` -- a total order, so resolution is deterministic.
//! - Overlapping subtitles: latest `start_seconds` wins (then greatest `text`).
//! - Fades: the most recent fade whose `start_seconds <= t` governs alpha (greatest
//!   start, ties by greatest duration). Inside its window alpha interpolates; after
//!   its window it HOLDS the terminal value (fade-in ends at 0 = revealed, fade-out
//!   ends at 1 = covered). Before any fade, alpha is 0 (fully visible).
//! - Cues: the active music/voice cue is the latest cue of that kind with `time <= t`.

use serde::{Deserialize, Serialize};

use crate::math::Vec3;

#[path = "cinematic_timeline_eval.rs"]
mod cinematic_timeline_eval;

pub use cinematic_timeline_eval::{
    ActiveTransmission, EvaluatedCinematic, EvaluatedFade, evaluate_cinematic,
    sample_cinematic_at_times,
};

/// A resolved camera pose the runtime can consume directly (named cameras have
/// already been resolved to concrete world poses by the editor lowering step).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CinematicCameraPose {
    pub target: Vec3,
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub fov_degrees: f32,
}

impl CinematicCameraPose {
    /// Component-wise linear interpolation at `alpha` in `[0, 1]`.
    pub fn lerp(&self, other: &Self, alpha: f32) -> Self {
        Self {
            target: self.target.lerp(other.target, alpha),
            distance: lerp_scalar(self.distance, other.distance, alpha),
            yaw_degrees: lerp_scalar(self.yaw_degrees, other.yaw_degrees, alpha),
            pitch_degrees: lerp_scalar(self.pitch_degrees, other.pitch_degrees, alpha),
            fov_degrees: lerp_scalar(self.fov_degrees, other.fov_degrees, alpha),
        }
    }

    fn validate(&self, index: usize) -> Result<(), CinematicTimelineError> {
        if !self.target.is_finite() {
            return Err(invalid("camera.target", index, "target must be finite"));
        }
        if !(self.distance.is_finite() && self.distance > 0.0) {
            return Err(invalid("camera.distance", index, "distance must be > 0"));
        }
        if !self.yaw_degrees.is_finite() || !self.pitch_degrees.is_finite() {
            return Err(invalid("camera.angles", index, "angles must be finite"));
        }
        if !(self.fov_degrees.is_finite() && self.fov_degrees > 0.0 && self.fov_degrees < 180.0) {
            return Err(invalid("camera.fov", index, "fov must be in (0, 180)"));
        }
        Ok(())
    }
}

/// One camera keyframe: hold/interp target pose at `time_seconds`. `camera_ref` is
/// the named-camera id it was lowered from (kept for provenance/readback).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraKeyframe {
    pub time_seconds: f32,
    pub camera_ref: String,
    pub pose: CinematicCameraPose,
}

/// A character transmission: who speaks, their portrait, the line, and its window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CinematicTransmission {
    pub start_seconds: f32,
    pub duration_seconds: f32,
    /// Unit-type id of the speaker (references the object/unit catalog).
    pub speaker_unit_type: String,
    pub portrait_ref: String,
    pub text: String,
    /// Track index; higher tracks win when transmissions overlap.
    pub track: u32,
}

/// Direction of a fade. `In` reveals (alpha 1 -> 0); `Out` covers (alpha 0 -> 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FadeKind {
    In,
    Out,
}

/// A screen fade over `[start, start + duration]` in `color` (RGBA, each 0..=1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CinematicFade {
    pub start_seconds: f32,
    pub duration_seconds: f32,
    pub kind: FadeKind,
    pub color: [f32; 4],
}

/// A subtitle line shown over `[start, start + duration]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CinematicSubtitle {
    pub start_seconds: f32,
    pub duration_seconds: f32,
    pub text: String,
}

/// Kind of a one-shot audio cue reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CueKind {
    Music,
    Voice,
}

/// A music/voice cue triggered at `time_seconds`. `cue_ref` names the audio asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CinematicCue {
    pub time_seconds: f32,
    pub kind: CueKind,
    pub cue_ref: String,
}

/// The frozen, runtime-facing lowered timeline the evaluator consumes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CinematicTimeline {
    pub timeline_id: String,
    pub duration_seconds: f32,
    pub camera_track: Vec<CameraKeyframe>,
    pub transmissions: Vec<CinematicTransmission>,
    pub fades: Vec<CinematicFade>,
    pub subtitles: Vec<CinematicSubtitle>,
    pub cues: Vec<CinematicCue>,
}

/// Stable diagnostic-coded validation error for a lowered timeline. The runtime
/// refuses to play an invalid timeline (fail loud, never a degraded silent play).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cinematic timeline field {field}[{index}] is invalid [{code}]: {reason}")]
pub struct CinematicTimelineError {
    pub code: &'static str,
    pub field: String,
    pub index: usize,
    pub reason: String,
}

impl CinematicTimelineError {
    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl CinematicTimeline {
    /// Validate every invariant the pure evaluator relies on. Fails loud with a
    /// typed, diagnostic-coded error -- the negative control in FSV sabotages a
    /// lowered timeline and proves this rejects it.
    pub fn validate(&self) -> Result<(), CinematicTimelineError> {
        if self.timeline_id.trim().is_empty() {
            return Err(invalid("timeline_id", 0, "timeline id must be non-empty"));
        }
        if !(self.duration_seconds.is_finite() && self.duration_seconds > 0.0) {
            return Err(invalid("duration_seconds", 0, "duration must be > 0"));
        }
        self.validate_camera_track()?;
        self.validate_transmissions()?;
        self.validate_fades()?;
        self.validate_subtitles()?;
        self.validate_cues()?;
        Ok(())
    }

    fn validate_camera_track(&self) -> Result<(), CinematicTimelineError> {
        if self.camera_track.is_empty() {
            return Err(invalid(
                "camera_track",
                0,
                "at least one camera keyframe is required",
            ));
        }
        let mut previous: Option<f32> = None;
        for (index, keyframe) in self.camera_track.iter().enumerate() {
            if keyframe.camera_ref.trim().is_empty() {
                return Err(invalid("camera_track.ref", index, "camera_ref non-empty"));
            }
            validate_window(
                "camera_track.time",
                index,
                keyframe.time_seconds,
                0.0,
                self.duration_seconds,
            )?;
            if let Some(previous) = previous
                && keyframe.time_seconds <= previous
            {
                return Err(invalid(
                    "camera_track.order",
                    index,
                    "keyframe times must strictly increase",
                ));
            }
            previous = Some(keyframe.time_seconds);
            keyframe.pose.validate(index)?;
        }
        Ok(())
    }

    fn validate_transmissions(&self) -> Result<(), CinematicTimelineError> {
        for (index, transmission) in self.transmissions.iter().enumerate() {
            if transmission.speaker_unit_type.trim().is_empty() {
                return Err(invalid("transmission.speaker", index, "speaker non-empty"));
            }
            if transmission.text.trim().is_empty() {
                return Err(invalid("transmission.text", index, "text non-empty"));
            }
            validate_span(
                "transmission",
                index,
                transmission.start_seconds,
                transmission.duration_seconds,
                self.duration_seconds,
            )?;
        }
        Ok(())
    }

    fn validate_fades(&self) -> Result<(), CinematicTimelineError> {
        for (index, fade) in self.fades.iter().enumerate() {
            // Zero-length fade is a hard, typed rejection (#892 edge case): a fade
            // with no duration would divide by zero in the evaluator.
            if !(fade.duration_seconds.is_finite() && fade.duration_seconds > 0.0) {
                return Err(CinematicTimelineError {
                    code: "CINEMATIC_FADE_ZERO_LENGTH",
                    field: "fade.duration".to_string(),
                    index,
                    reason: "fade duration must be > 0".to_string(),
                });
            }
            validate_span(
                "fade",
                index,
                fade.start_seconds,
                fade.duration_seconds,
                self.duration_seconds,
            )?;
            validate_color("fade.color", index, &fade.color)?;
        }
        Ok(())
    }

    fn validate_subtitles(&self) -> Result<(), CinematicTimelineError> {
        for (index, subtitle) in self.subtitles.iter().enumerate() {
            if subtitle.text.trim().is_empty() {
                return Err(invalid("subtitle.text", index, "text non-empty"));
            }
            validate_span(
                "subtitle",
                index,
                subtitle.start_seconds,
                subtitle.duration_seconds,
                self.duration_seconds,
            )?;
        }
        Ok(())
    }

    fn validate_cues(&self) -> Result<(), CinematicTimelineError> {
        for (index, cue) in self.cues.iter().enumerate() {
            if cue.cue_ref.trim().is_empty() {
                return Err(invalid("cue.ref", index, "cue_ref non-empty"));
            }
            validate_window(
                "cue.time",
                index,
                cue.time_seconds,
                0.0,
                self.duration_seconds,
            )?;
        }
        Ok(())
    }
}

fn validate_span(
    field: &str,
    index: usize,
    start: f32,
    duration: f32,
    timeline_duration: f32,
) -> Result<(), CinematicTimelineError> {
    if !(duration.is_finite() && duration > 0.0) {
        return Err(invalid(
            &format!("{field}.duration"),
            index,
            "duration must be > 0",
        ));
    }
    validate_window(
        &format!("{field}.start"),
        index,
        start,
        0.0,
        timeline_duration,
    )?;
    if start + duration > timeline_duration + f32::EPSILON {
        return Err(invalid(
            &format!("{field}.end"),
            index,
            "start + duration must not exceed timeline duration",
        ));
    }
    Ok(())
}

fn validate_window(
    field: &str,
    index: usize,
    value: f32,
    min: f32,
    max: f32,
) -> Result<(), CinematicTimelineError> {
    if !value.is_finite() || value < min || value > max + f32::EPSILON {
        return Err(invalid(
            field,
            index,
            "time must be finite and within [0, duration]",
        ));
    }
    Ok(())
}

fn validate_color(
    field: &str,
    index: usize,
    color: &[f32; 4],
) -> Result<(), CinematicTimelineError> {
    for component in color {
        if !component.is_finite() || *component < 0.0 || *component > 1.0 {
            return Err(invalid(field, index, "color components must be in [0, 1]"));
        }
    }
    Ok(())
}

pub(crate) fn invalid(field: &str, index: usize, reason: &str) -> CinematicTimelineError {
    CinematicTimelineError {
        code: "CINEMATIC_TIMELINE_INVALID",
        field: field.to_string(),
        index,
        reason: reason.to_string(),
    }
}

fn lerp_scalar(a: f32, b: f32, alpha: f32) -> f32 {
    a + (b - a) * alpha
}

#[cfg(test)]
#[path = "cinematic_timeline_tests.rs"]
mod tests;
