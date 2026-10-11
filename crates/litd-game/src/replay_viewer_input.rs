//! #83 replay-viewer-only keymap context.
//!
//! This state is separate from RTS play input. Replay controls cannot issue
//! simulation orders; the normal RTS input remains available only to the
//! presentation camera.

use litd_engine::input::{InputBinding, InputBindings, InputState};
use winit::{event::WindowEvent, keyboard::KeyCode};

use crate::replay_viewer::{ReplaySpeed, ReplayViewer, ReplayViewerError};

pub const ACTION_REPLAY_PAUSE: &str = "replay.viewer.pause";
pub const ACTION_REPLAY_HALF: &str = "replay.viewer.speed.half";
pub const ACTION_REPLAY_NORMAL: &str = "replay.viewer.speed.normal";
pub const ACTION_REPLAY_DOUBLE: &str = "replay.viewer.speed.double";
pub const ACTION_REPLAY_QUAD: &str = "replay.viewer.speed.quad";
pub const ACTION_REPLAY_OCTUPLE: &str = "replay.viewer.speed.octuple";
pub const ACTION_REPLAY_PREVIOUS_PLAYER: &str = "replay.viewer.perspective.previous";
pub const ACTION_REPLAY_NEXT_PLAYER: &str = "replay.viewer.perspective.next";

pub struct ReplayViewerInput {
    state: InputState,
}

impl ReplayViewerInput {
    pub fn new() -> Self {
        Self {
            state: InputState::new(replay_bindings()),
        }
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        self.state.handle_window_event(event);
    }

    pub fn apply(&self, viewer: &mut ReplayViewer) -> Result<bool, ReplayViewerError> {
        let mut changed = false;
        if self.state.action_state(ACTION_REPLAY_PAUSE).just_pressed {
            viewer.toggle_paused();
            changed = true;
        }
        for (action, speed) in [
            (ACTION_REPLAY_HALF, ReplaySpeed::Half),
            (ACTION_REPLAY_NORMAL, ReplaySpeed::Normal),
            (ACTION_REPLAY_DOUBLE, ReplaySpeed::Double),
            (ACTION_REPLAY_QUAD, ReplaySpeed::Quad),
            (ACTION_REPLAY_OCTUPLE, ReplaySpeed::Octuple),
        ] {
            if self.state.action_state(action).just_pressed {
                viewer.set_speed(speed);
                changed = true;
            }
        }
        let count = viewer.player_count();
        if self
            .state
            .action_state(ACTION_REPLAY_PREVIOUS_PLAYER)
            .just_pressed
        {
            viewer.set_perspective((viewer.perspective() + count - 1) % count)?;
            changed = true;
        }
        if self
            .state
            .action_state(ACTION_REPLAY_NEXT_PLAYER)
            .just_pressed
        {
            viewer.set_perspective((viewer.perspective() + 1) % count)?;
            changed = true;
        }
        Ok(changed)
    }

    pub fn finish_frame(&mut self) {
        self.state.finish_frame();
    }

    #[cfg(test)]
    pub fn set_key(&mut self, key: KeyCode, pressed: bool) {
        self.state.set_key(key, pressed);
    }

    #[cfg(test)]
    pub fn bindings(&self) -> &InputBindings {
        self.state.bindings()
    }
}

impl Default for ReplayViewerInput {
    fn default() -> Self {
        Self::new()
    }
}

fn replay_bindings() -> InputBindings {
    let mut bindings = InputBindings::default();
    for (action, key) in [
        (ACTION_REPLAY_PAUSE, KeyCode::Space),
        (ACTION_REPLAY_HALF, KeyCode::Digit1),
        (ACTION_REPLAY_NORMAL, KeyCode::Digit2),
        (ACTION_REPLAY_DOUBLE, KeyCode::Digit3),
        (ACTION_REPLAY_QUAD, KeyCode::Digit4),
        (ACTION_REPLAY_OCTUPLE, KeyCode::Digit5),
        (ACTION_REPLAY_PREVIOUS_PLAYER, KeyCode::Comma),
        (ACTION_REPLAY_NEXT_PLAYER, KeyCode::Period),
    ] {
        bindings.bind(action, InputBinding::Key(key));
    }
    bindings
}
