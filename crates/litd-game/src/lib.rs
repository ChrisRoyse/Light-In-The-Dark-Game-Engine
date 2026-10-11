//! Reusable deterministic replay-viewer logic for the LitD runtime.
//!
//! The production `litd-game` binary is the runtime source of truth for packaged worlds; this
//! library keeps replay input and presentation policy reusable without introducing a second host.
//! Package validation remains fail-closed at the runtime boundary before a world is presented.

pub mod replay_viewer;
pub mod replay_viewer_input;

#[cfg(test)]
mod replay_viewer_tests;
