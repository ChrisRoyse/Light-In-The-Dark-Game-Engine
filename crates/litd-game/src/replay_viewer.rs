//! #83 replay-viewer deterministic playback shell.
//!
//! The viewer owns a fresh [`MatchRuntimeSession`] and feeds it the versioned
//! [`SessionReplay::order_log`] incrementally. Speed changes alter only how many
//! recorded simulation ticks are consumed per host frame. Pause, camera motion,
//! and player perspective are presentation state and never enter the order log.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use litd_engine::game_runtime::{
    MatchRuntimeSession, RuntimeDatasetsManifest, SessionOrder, SessionReplay,
    error::GameRuntimeError, fog::VisibilityGrid,
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaySpeed {
    Half,
    Normal,
    Double,
    Quad,
    Octuple,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayViewerConfig {
    pub replay_path: PathBuf,
    pub speed: ReplaySpeed,
    pub perspective: u32,
    pub paused: bool,
    pub start_tick: Option<u64>,
}

impl ReplaySpeed {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Half => "0.5x",
            Self::Normal => "1x",
            Self::Double => "2x",
            Self::Quad => "4x",
            Self::Octuple => "8x",
        }
    }

    const fn tick_budget(self, half_phase: bool) -> usize {
        match self {
            Self::Half => {
                if half_phase {
                    1
                } else {
                    0
                }
            }
            Self::Normal => 1,
            Self::Double => 2,
            Self::Quad => 4,
            Self::Octuple => 8,
        }
    }
}

impl FromStr for ReplaySpeed {
    type Err = ReplayViewerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "0.5" | "0.5x" | "half" => Ok(Self::Half),
            "1" | "1x" | "normal" => Ok(Self::Normal),
            "2" | "2x" | "double" => Ok(Self::Double),
            "4" | "4x" | "quad" => Ok(Self::Quad),
            "8" | "8x" | "octuple" => Ok(Self::Octuple),
            other => Err(ReplayViewerError::InvalidSpeed(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayViewerSnapshot {
    pub paused: bool,
    pub speed: ReplaySpeed,
    pub perspective: u32,
    pub perspective_fog: (usize, usize, usize),
    pub order_cursor: usize,
    pub order_count: usize,
    pub applied_tick: u64,
    pub state_hash: u64,
    pub expected_final_hash: u64,
    pub complete: bool,
    pub final_hash_matches: bool,
}

pub struct ReplayViewer {
    session: MatchRuntimeSession,
    replay: SessionReplay,
    expected_final_hash: u64,
    order_cursor: usize,
    paused: bool,
    speed: ReplaySpeed,
    perspective: u32,
    player_count: u32,
    half_phase: bool,
}

impl ReplayViewer {
    pub fn load(
        manifest: &RuntimeDatasetsManifest,
        replay_path: &Path,
    ) -> Result<Self, ReplayViewerError> {
        let replay = SessionReplay::load_from_path(replay_path)?;
        Self::new(manifest, replay)
    }

    pub fn new(
        manifest: &RuntimeDatasetsManifest,
        replay: SessionReplay,
    ) -> Result<Self, ReplayViewerError> {
        replay.validate_for_manifest(manifest)?;
        if replay.datasets_fingerprint != manifest.datasets_fingerprint {
            return Err(ReplayViewerError::DatasetMismatch {
                expected: manifest.datasets_fingerprint.clone(),
                actual: replay.datasets_fingerprint.clone(),
            });
        }
        let expected_final_hash = replay.expected_final_hash;
        let session = MatchRuntimeSession::from_manifest(manifest)?;
        let player_count = session.match_state().roster().player_count() as u32;
        Ok(Self {
            session,
            replay,
            expected_final_hash,
            order_cursor: 0,
            paused: false,
            speed: ReplaySpeed::Normal,
            perspective: 0,
            player_count,
            half_phase: false,
        })
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn toggle_paused(&mut self) {
        self.paused = !self.paused;
    }

    pub fn set_speed(&mut self, speed: ReplaySpeed) {
        self.speed = speed;
        self.half_phase = false;
    }

    pub fn set_perspective(&mut self, player: u32) -> Result<(), ReplayViewerError> {
        if player >= self.player_count {
            return Err(ReplayViewerError::InvalidPerspective {
                requested: player,
                player_count: self.player_count,
            });
        }
        self.perspective = player;
        Ok(())
    }

    /// Replay real recorded orders until the requested deterministic tick.
    /// The caller may then pause and inspect that exact state before playback.
    pub fn seek_to_tick(&mut self, target_tick: u64) -> Result<(), ReplayViewerError> {
        if target_tick < self.session.applied_tick() {
            return Err(ReplayViewerError::ReplaySeekBackwards {
                current: self.session.applied_tick(),
                requested: target_tick,
            });
        }
        while self.session.applied_tick() < target_tick {
            if !self.advance_one_recorded_tick()? {
                return Err(ReplayViewerError::ReplaySeekOutOfRange {
                    requested: target_tick,
                    available: self.session.applied_tick(),
                });
            }
        }
        Ok(())
    }

    pub const fn perspective(&self) -> u32 {
        self.perspective
    }

    pub const fn player_count(&self) -> u32 {
        self.player_count
    }

    /// Borrow the selected player's deterministic fog grid for presentation.
    pub fn perspective_grid(&self) -> Result<&VisibilityGrid, ReplayViewerError> {
        self.session
            .fog()
            .grid(self.perspective)
            .ok_or(ReplayViewerError::InvalidPerspective {
                requested: self.perspective,
                player_count: self.player_count,
            })
    }

    /// Advance one host frame. Camera state is intentionally absent: the existing
    /// `GameRuntimeRenderer::rts_camera` remains free and presentation-only.
    pub fn advance_host_frame(&mut self) -> Result<ReplayViewerSnapshot, ReplayViewerError> {
        if self.paused || self.is_complete() {
            return self.snapshot();
        }
        self.half_phase = !self.half_phase;
        let tick_budget = self.speed.tick_budget(self.half_phase);
        for _ in 0..tick_budget {
            if !self.advance_one_recorded_tick()? {
                break;
            }
        }
        self.snapshot()
    }

    pub fn run_to_completion(&mut self) -> Result<ReplayViewerSnapshot, ReplayViewerError> {
        if self.paused {
            return Err(ReplayViewerError::PausedRunToCompletion);
        }
        while !self.is_complete() {
            self.advance_host_frame()?;
        }
        let snapshot = self.snapshot()?;
        if !snapshot.final_hash_matches {
            return Err(ReplayViewerError::FinalHashMismatch {
                expected: snapshot.expected_final_hash,
                actual: snapshot.state_hash,
            });
        }
        Ok(snapshot)
    }

    pub fn snapshot(&self) -> Result<ReplayViewerSnapshot, ReplayViewerError> {
        let perspective_fog = self.session.fog().counts(self.perspective).ok_or(
            ReplayViewerError::InvalidPerspective {
                requested: self.perspective,
                player_count: self.player_count,
            },
        )?;
        let complete = self.is_complete();
        let state_hash = self.session.combined_state_hash();
        Ok(ReplayViewerSnapshot {
            paused: self.paused,
            speed: self.speed,
            perspective: self.perspective,
            perspective_fog,
            order_cursor: self.order_cursor,
            order_count: self.replay.order_log.len(),
            applied_tick: self.session.applied_tick(),
            state_hash,
            expected_final_hash: self.expected_final_hash,
            complete,
            final_hash_matches: complete && state_hash == self.expected_final_hash,
        })
    }

    fn is_complete(&self) -> bool {
        self.order_cursor == self.replay.order_log.len()
    }

    /// Consume through one recorded `Tick`. Non-tick orders before that boundary
    /// remain in their exact recorded order. A final non-tick tail is consumed too.
    fn advance_one_recorded_tick(&mut self) -> Result<bool, ReplayViewerError> {
        if self.is_complete() {
            return Ok(false);
        }
        while let Some(order) = self.replay.order_log.get(self.order_cursor).cloned() {
            self.order_cursor += 1;
            let is_tick = matches!(order, SessionOrder::Tick);
            self.session.apply(order)?;
            if is_tick {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[derive(Debug, Error)]
pub enum ReplayViewerError {
    #[error("replay runtime failed: {0}")]
    Runtime(#[from] GameRuntimeError),
    #[error("replay datasets fingerprint mismatch: expected {expected}, got {actual}")]
    DatasetMismatch { expected: String, actual: String },
    #[error("unsupported replay speed {0}; expected 0.5, 1, 2, 4, or 8")]
    InvalidSpeed(String),
    #[error("replay perspective {requested} is invalid for {player_count} players")]
    InvalidPerspective { requested: u32, player_count: u32 },
    #[error("replay seek cannot move backwards from tick {current} to {requested}")]
    ReplaySeekBackwards { current: u64, requested: u64 },
    #[error(
        "replay seek tick {requested} is beyond the recorded replay (available through tick {available})"
    )]
    ReplaySeekOutOfRange { requested: u64, available: u64 },
    #[error("cannot run replay to completion while paused")]
    PausedRunToCompletion,
    #[error("replay final hash mismatch: expected {expected:016x}, got {actual:016x}")]
    FinalHashMismatch { expected: u64, actual: u64 },
}
