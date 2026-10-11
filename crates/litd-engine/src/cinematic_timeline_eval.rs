//! #892: the pure `(timeline, t) -> state` cinematic evaluator (child of
//! `cinematic_timeline`). Every rule is documented on the parent module.

use serde::{Deserialize, Serialize};

use super::{
    CinematicCameraPose, CinematicTimeline, CinematicTimelineError, CueKind, FadeKind, invalid,
};

/// The resolved transmission shown at a sample time (already overlap-resolved).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveTransmission {
    pub speaker_unit_type: String,
    pub portrait_ref: String,
    pub text: String,
    pub track: u32,
    pub start_seconds: f32,
    pub duration_seconds: f32,
}

/// The resolved fade state at a sample time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedFade {
    /// 0 = fully visible, 1 = fully covered by `color`.
    pub alpha: f32,
    pub color: [f32; 4],
}

/// The complete evaluated cinematic state at one sample time -- a pure function of
/// the lowered timeline and `t`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedCinematic {
    pub time_seconds: f32,
    pub camera: CinematicCameraPose,
    pub active_transmission: Option<ActiveTransmission>,
    pub fade: EvaluatedFade,
    pub active_subtitle: Option<String>,
    pub active_music_cue: Option<String>,
    pub active_voice_cue: Option<String>,
}

/// Evaluate the timeline at `time_seconds`. Validates first (fails loud on a
/// sabotaged/invalid timeline) and rejects a non-finite sample time.
pub fn evaluate_cinematic(
    timeline: &CinematicTimeline,
    time_seconds: f32,
) -> Result<EvaluatedCinematic, CinematicTimelineError> {
    timeline.validate()?;
    if !time_seconds.is_finite() {
        return Err(invalid("sample.time", 0, "sample time must be finite"));
    }
    Ok(EvaluatedCinematic {
        time_seconds,
        camera: evaluate_camera(timeline, time_seconds),
        active_transmission: evaluate_transmission(timeline, time_seconds),
        fade: evaluate_fade(timeline, time_seconds),
        active_subtitle: evaluate_subtitle(timeline, time_seconds),
        active_music_cue: evaluate_cue(timeline, time_seconds, CueKind::Music),
        active_voice_cue: evaluate_cue(timeline, time_seconds, CueKind::Voice),
    })
}

/// Evaluate at every requested sample time, in order. Fails loud if any sample
/// fails (invalid timeline or non-finite time), so a partial series never returns.
pub fn sample_cinematic_at_times(
    timeline: &CinematicTimeline,
    times: &[f32],
) -> Result<Vec<EvaluatedCinematic>, CinematicTimelineError> {
    times
        .iter()
        .map(|time| evaluate_cinematic(timeline, *time))
        .collect()
}

fn evaluate_camera(timeline: &CinematicTimeline, t: f32) -> CinematicCameraPose {
    let track = &timeline.camera_track;
    // validate() guarantees the track is non-empty and strictly time-ordered.
    let first = &track[0];
    if t <= first.time_seconds {
        return first.pose;
    }
    let last = &track[track.len() - 1];
    if t >= last.time_seconds {
        return last.pose;
    }
    for pair in track.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t >= a.time_seconds && t <= b.time_seconds {
            let span = b.time_seconds - a.time_seconds;
            let alpha = (t - a.time_seconds) / span;
            return a.pose.lerp(&b.pose, alpha);
        }
    }
    // Unreachable given the bracketing above; hold the last pose defensively.
    last.pose
}

fn evaluate_transmission(timeline: &CinematicTimeline, t: f32) -> Option<ActiveTransmission> {
    timeline
        .transmissions
        .iter()
        .filter(|transmission| {
            t >= transmission.start_seconds
                && t < transmission.start_seconds + transmission.duration_seconds
        })
        // Documented total order: highest track, then latest start, then greatest speaker.
        .max_by(|a, b| {
            a.track
                .cmp(&b.track)
                .then(total_cmp(a.start_seconds, b.start_seconds))
                .then(a.speaker_unit_type.cmp(&b.speaker_unit_type))
        })
        .map(|transmission| ActiveTransmission {
            speaker_unit_type: transmission.speaker_unit_type.clone(),
            portrait_ref: transmission.portrait_ref.clone(),
            text: transmission.text.clone(),
            track: transmission.track,
            start_seconds: transmission.start_seconds,
            duration_seconds: transmission.duration_seconds,
        })
}

fn evaluate_fade(timeline: &CinematicTimeline, t: f32) -> EvaluatedFade {
    // Most recent fade that has already started governs alpha.
    let governing = timeline
        .fades
        .iter()
        .filter(|fade| fade.start_seconds <= t)
        .max_by(|a, b| {
            total_cmp(a.start_seconds, b.start_seconds)
                .then(total_cmp(a.duration_seconds, b.duration_seconds))
        });
    match governing {
        None => EvaluatedFade {
            alpha: 0.0,
            color: [0.0, 0.0, 0.0, 1.0],
        },
        Some(fade) => {
            let end = fade.start_seconds + fade.duration_seconds;
            let progress = if t >= end {
                1.0
            } else {
                (t - fade.start_seconds) / fade.duration_seconds
            };
            let alpha = match fade.kind {
                // Fade-in reveals: alpha 1 -> 0 across the window, holds 0 after.
                FadeKind::In => 1.0 - progress,
                // Fade-out covers: alpha 0 -> 1 across the window, holds 1 after.
                FadeKind::Out => progress,
            };
            EvaluatedFade {
                alpha,
                color: fade.color,
            }
        }
    }
}

fn evaluate_subtitle(timeline: &CinematicTimeline, t: f32) -> Option<String> {
    timeline
        .subtitles
        .iter()
        .filter(|subtitle| {
            t >= subtitle.start_seconds && t < subtitle.start_seconds + subtitle.duration_seconds
        })
        .max_by(|a, b| total_cmp(a.start_seconds, b.start_seconds).then(a.text.cmp(&b.text)))
        .map(|subtitle| subtitle.text.clone())
}

fn evaluate_cue(timeline: &CinematicTimeline, t: f32, kind: CueKind) -> Option<String> {
    timeline
        .cues
        .iter()
        .filter(|cue| cue.kind == kind && cue.time_seconds <= t)
        .max_by(|a, b| total_cmp(a.time_seconds, b.time_seconds).then(a.cue_ref.cmp(&b.cue_ref)))
        .map(|cue| cue.cue_ref.clone())
}

fn total_cmp(a: f32, b: f32) -> std::cmp::Ordering {
    a.total_cmp(&b)
}
