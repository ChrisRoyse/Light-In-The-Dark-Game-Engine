//! #845: the RUNTIME PLAYER for #892 cinematic timeline records.
//!
//! #892 authored the pure `(timeline, t) -> state` evaluator
//! (`crate::cinematic_timeline`). This is the host-loop driver around it: it
//! owns the play clock, toggles cinematic (letterbox) mode, feeds the fade /
//! transmission / subtitle / camera state out to presentation, wires SKIP, and
//! GATES sim interaction -- per the authored `pauses_sim` flag the host either
//! freezes the deterministic sim while the cutscene runs or keeps ticking it.
//!
//! BOUNDARY: the player clock and letterbox are PRESENTATION (per-client,
//! floats). The only sim-affecting output is the boolean [`CinematicPlayer::
//! should_advance_sim`], which the host reads to decide whether to step the sim
//! this frame -- and because it derives from the authored flag + play state, all
//! clients driving the same timeline gate identically.

use crate::cinematic_timeline::{
    CinematicTimeline, CinematicTimelineError, EvaluatedCinematic, evaluate_cinematic,
};

/// How the sim behaves while a cinematic plays (authored per timeline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimGating {
    /// Freeze the deterministic sim for the cutscene's duration.
    Pause,
    /// Keep ticking the sim underneath the cutscene.
    RunUnderneath,
}

/// Runtime playback state of the cinematic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    /// Reached the end (naturally or via skip); no longer active.
    Finished,
}

/// Host-loop driver over a validated cinematic timeline.
#[derive(Debug, Clone)]
pub struct CinematicPlayer {
    timeline: CinematicTimeline,
    gating: SimGating,
    elapsed_seconds: f32,
    state: PlayState,
    skipped: bool,
}

impl CinematicPlayer {
    /// Start playing a lowered timeline. Validates first (fails loud on a
    /// sabotaged/invalid record -- the FSV negative control), so the host never
    /// begins a degraded silent playback.
    pub fn start(
        timeline: CinematicTimeline,
        gating: SimGating,
    ) -> Result<Self, CinematicTimelineError> {
        timeline.validate()?;
        Ok(Self {
            timeline,
            gating,
            elapsed_seconds: 0.0,
            state: PlayState::Playing,
            skipped: false,
        })
    }

    pub fn timeline_id(&self) -> &str {
        &self.timeline.timeline_id
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn is_active(&self) -> bool {
        self.state == PlayState::Playing
    }

    pub fn was_skipped(&self) -> bool {
        self.skipped
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    /// Cinematic (letterbox) mode is ON exactly while the cutscene is active.
    pub fn letterbox_active(&self) -> bool {
        self.is_active()
    }

    /// Whether the host should step the sim this frame. Only a paused, still
    /// active cutscene freezes it; once finished (or if authored to run
    /// underneath) the sim advances normally.
    pub fn should_advance_sim(&self) -> bool {
        if !self.is_active() {
            return true;
        }
        self.gating == SimGating::RunUnderneath
    }

    /// Advance the play clock by a frame delta (non-negative seconds). Clamps to
    /// the timeline duration and marks the cutscene `Finished` at the end.
    pub fn advance(&mut self, delta_seconds: f32) {
        if !(self.is_active() && delta_seconds.is_finite() && delta_seconds >= 0.0) {
            return;
        }
        self.elapsed_seconds += delta_seconds;
        if self.elapsed_seconds >= self.timeline.duration_seconds {
            self.elapsed_seconds = self.timeline.duration_seconds;
            self.state = PlayState::Finished;
        }
    }

    /// Player-invoked skip: jump to the end and finish immediately. Idempotent.
    pub fn skip(&mut self) {
        self.elapsed_seconds = self.timeline.duration_seconds;
        self.state = PlayState::Finished;
        self.skipped = true;
    }

    /// Evaluate the cutscene at the current play clock (the pure #892 evaluator).
    pub fn sample(&self) -> Result<EvaluatedCinematic, CinematicTimelineError> {
        evaluate_cinematic(&self.timeline, self.elapsed_seconds)
    }

    /// Evaluate at an explicit time without moving the play clock -- for the FSV
    /// sampling-at-timestamps check.
    pub fn sample_at(
        &self,
        time_seconds: f32,
    ) -> Result<EvaluatedCinematic, CinematicTimelineError> {
        evaluate_cinematic(&self.timeline, time_seconds)
    }
}
