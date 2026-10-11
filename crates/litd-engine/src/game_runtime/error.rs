//! Typed, fail-loud errors for the game-runtime persistence + day/night systems
//! (#850 / #848). Kept LOCAL to `game_runtime` (a `thiserror` enum with stable
//! `LITD_GAME_RUNTIME_*` diagnostic codes) so the frozen crate-wide `error.rs`
//! baseline is untouched -- the same pattern `world_environment` uses.

use crate::EngineError;

/// Every failure mode of the campaign KV store, save/load spine, day/night sim
/// state, and hero carry-over. `code()` yields a stable, path-free diagnostic
/// code for the FSV report and logs (never a silent fallback).
#[derive(Debug, thiserror::Error)]
pub enum GameRuntimeError {
    #[error("game-runtime field {field} is invalid: {reason} [LITD_GAME_RUNTIME_INVALID]")]
    Invalid { field: String, reason: String },
    #[error("campaign kv key {category}/{key} is absent [LITD_GAME_RUNTIME_KV_MISSING]")]
    KvKeyMissing { category: String, key: String },
    #[error(
        "campaign kv key {category}/{key} is a {actual}, not a {expected} [LITD_GAME_RUNTIME_KV_TYPE]"
    )]
    KvTypeMismatch {
        category: String,
        key: String,
        expected: &'static str,
        actual: &'static str,
    },
    #[error(
        "game-runtime save schema mismatch: expected {expected}, found {actual} [LITD_GAME_RUNTIME_SCHEMA]"
    )]
    SchemaMismatch { expected: String, actual: String },
    #[error(
        "game-runtime save fingerprint mismatch: expected {expected}, found {actual} (tampered/corrupt save) [LITD_GAME_RUNTIME_FINGERPRINT]"
    )]
    FingerprintMismatch { expected: String, actual: String },
    #[error("game-runtime save could not be decoded: {reason} [LITD_GAME_RUNTIME_DECODE]")]
    Decode { reason: String },
    #[error("game-runtime save i/o failed for {path}: {reason} [LITD_GAME_RUNTIME_IO]")]
    Io { path: String, reason: String },
    #[error("game-runtime sim error: {0}")]
    Sim(#[from] EngineError),
}

impl GameRuntimeError {
    /// Stable, run-invariant diagnostic code (no absolute paths, no line info).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid { .. } => "LITD_GAME_RUNTIME_INVALID",
            Self::KvKeyMissing { .. } => "LITD_GAME_RUNTIME_KV_MISSING",
            Self::KvTypeMismatch { .. } => "LITD_GAME_RUNTIME_KV_TYPE",
            Self::SchemaMismatch { .. } => "LITD_GAME_RUNTIME_SCHEMA",
            Self::FingerprintMismatch { .. } => "LITD_GAME_RUNTIME_FINGERPRINT",
            Self::Decode { .. } => "LITD_GAME_RUNTIME_DECODE",
            Self::Io { .. } => "LITD_GAME_RUNTIME_IO",
            Self::Sim(_) => "LITD_GAME_RUNTIME_SIM",
        }
    }
}

pub(crate) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, GameRuntimeError> {
    Err(GameRuntimeError::Invalid {
        field: field.into(),
        reason: reason.into(),
    })
}
