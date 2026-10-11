//! #1843: mid-match save/load for [`super::MatchRuntimeSession`] as a deterministic
//! replay log.
//!
//! Because every session mutation flows through a [`super::SessionOrder`] and the runtime
//! is fully deterministic (#85), a save is the applied order stream plus the tick it was
//! captured at, fingerprinted with sha256. Loading rebuilds the session from the same
//! package datasets and REPLAYS the log, so a reload continues to the identical
//! `combined_state_hash` as an uninterrupted run -- the save/load acid test. A tampered
//! save (a mutated order, a truncated log, a swapped datasets fingerprint) fails loud with
//! a typed [`GameRuntimeError`], never a silent divergence.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use super::super::datasets::RuntimeDatasetsManifest;
use super::GameRuntimeError;
use super::{MatchRuntimeSession, SessionOrder};

pub const SESSION_SAVE_SCHEMA: &str = "litd.game.session_save.v1";

/// A fingerprinted deterministic replay save of a running session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSave {
    pub schema: String,
    /// Binds the save to the exact package datasets it was produced against.
    pub datasets_fingerprint: String,
    pub applied_tick: u64,
    pub order_log: Vec<SessionOrder>,
    pub save_fingerprint: String,
}

impl SessionSave {
    /// Capture the current session as a replayable save.
    pub fn capture(session: &MatchRuntimeSession) -> Result<Self, GameRuntimeError> {
        let mut save = Self {
            schema: SESSION_SAVE_SCHEMA.to_string(),
            datasets_fingerprint: session.datasets_fingerprint().to_string(),
            applied_tick: session.applied_tick(),
            order_log: session.order_log().to_vec(),
            save_fingerprint: String::new(),
        };
        save.save_fingerprint = save.compute_fingerprint()?;
        Ok(save)
    }

    fn compute_fingerprint(&self) -> Result<String, GameRuntimeError> {
        let mut preimage = self.clone();
        preimage.save_fingerprint.clear();
        let ron =
            litd_hash::to_stable_ron(&preimage).map_err(|error| GameRuntimeError::Decode {
                reason: format!("encode session save RON failed: {error}"),
            })?;
        Ok(litd_hash::sha256_hex(ron.as_bytes()))
    }

    /// Fail-loud structural + fingerprint validation. Rejects a wrong schema or a
    /// tampered/corrupt save (fingerprint recompute mismatch).
    pub fn validate(&self) -> Result<(), GameRuntimeError> {
        if self.schema != SESSION_SAVE_SCHEMA {
            return Err(GameRuntimeError::SchemaMismatch {
                expected: SESSION_SAVE_SCHEMA.to_string(),
                actual: self.schema.clone(),
            });
        }
        let expected = self.compute_fingerprint()?;
        if self.save_fingerprint != expected {
            return Err(GameRuntimeError::FingerprintMismatch {
                expected,
                actual: self.save_fingerprint.clone(),
            });
        }
        Ok(())
    }

    pub fn to_ron_string(&self) -> Result<String, GameRuntimeError> {
        litd_hash::to_stable_ron(self).map_err(|error| GameRuntimeError::Decode {
            reason: format!("encode session save RON failed: {error}"),
        })
    }

    pub fn from_ron_str(text: &str) -> Result<Self, GameRuntimeError> {
        let save: Self = ron::from_str(text).map_err(|error| GameRuntimeError::Decode {
            reason: format!("decode session save RON failed: {error}"),
        })?;
        save.validate()?;
        Ok(save)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), GameRuntimeError> {
        let text = self.to_ron_string()?;
        fs::write(path, text.as_bytes()).map_err(|error| GameRuntimeError::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        })
    }

    pub fn load_from_path(path: &Path) -> Result<Self, GameRuntimeError> {
        let text = fs::read_to_string(path).map_err(|error| GameRuntimeError::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        Self::from_ron_str(&text)
    }
}

impl MatchRuntimeSession {
    /// Capture the session as a deterministic replay save.
    pub fn capture_save(&self) -> Result<SessionSave, GameRuntimeError> {
        SessionSave::capture(self)
    }

    /// Rebuild a session from the same package datasets and replay a validated save.
    /// The datasets fingerprint in the save must match the manifest -- loading a save
    /// against a different package fails loud (a save is not portable across rule changes).
    pub fn restore_from_save(
        manifest: &RuntimeDatasetsManifest,
        save: &SessionSave,
    ) -> Result<Self, GameRuntimeError> {
        save.validate()?;
        if save.datasets_fingerprint != manifest.datasets_fingerprint {
            return Err(GameRuntimeError::FingerprintMismatch {
                expected: manifest.datasets_fingerprint.clone(),
                actual: save.datasets_fingerprint.clone(),
            });
        }
        let mut session = Self::from_manifest(manifest)?;
        for order in &save.order_log {
            session.apply(order.clone())?;
        }
        if session.applied_tick() != save.applied_tick {
            return Err(GameRuntimeError::Invalid {
                field: "session.restore.applied_tick".to_string(),
                reason: format!(
                    "replayed tick {} does not match saved tick {}",
                    session.applied_tick(),
                    save.applied_tick
                ),
            });
        }
        Ok(session)
    }
}
