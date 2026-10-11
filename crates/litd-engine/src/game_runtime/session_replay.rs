//! #83 versioned replay files for [`super::MatchRuntimeSession`].
//!
//! A replay is not a save snapshot. It identifies the authored map + simulation
//! seed, carries the exact applied command stream, and records the expected final
//! deterministic hash. The file envelope is validated before RON decoding:
//! `magic | encoding byte | version byte | fingerprinted payload`.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use super::super::datasets::{RuntimeDatasetsManifest, validate_runtime_datasets};
use super::{GameRuntimeError, MatchRuntimeSession, SessionOrder};

pub const SESSION_REPLAY_MAGIC: &[u8; 8] = b"LITDRPLY";
pub const SESSION_REPLAY_ENCODING_RON: u8 = 1;
pub const SESSION_REPLAY_FORMAT_VERSION: u8 = 1;
pub const SESSION_REPLAY_ENCODING_BYTE_INDEX: usize = SESSION_REPLAY_MAGIC.len();
pub const SESSION_REPLAY_VERSION_BYTE_INDEX: usize = SESSION_REPLAY_ENCODING_BYTE_INDEX + 1;
const SESSION_REPLAY_HEADER_LEN: usize = SESSION_REPLAY_VERSION_BYTE_INDEX + 1;

/// A fingerprinted, deterministic match command stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionReplay {
    pub map_id: String,
    pub sim_seed: u64,
    pub datasets_fingerprint: String,
    pub applied_tick: u64,
    pub order_log: Vec<SessionOrder>,
    pub expected_final_hash: u64,
    pub replay_fingerprint: String,
}

impl SessionReplay {
    /// Capture only replay inputs + proof, never live game state.
    pub fn capture(
        manifest: &RuntimeDatasetsManifest,
        session: &MatchRuntimeSession,
    ) -> Result<Self, GameRuntimeError> {
        validate_runtime_datasets(manifest)?;
        if session.datasets_fingerprint() != manifest.datasets_fingerprint {
            return Err(GameRuntimeError::FingerprintMismatch {
                expected: manifest.datasets_fingerprint.clone(),
                actual: session.datasets_fingerprint().to_string(),
            });
        }
        let mut replay = Self {
            map_id: manifest.lowered_match.map_id.clone(),
            sim_seed: manifest.constants.match_seed,
            datasets_fingerprint: manifest.datasets_fingerprint.clone(),
            applied_tick: session.applied_tick(),
            order_log: session.order_log().to_vec(),
            expected_final_hash: session.combined_state_hash(),
            replay_fingerprint: String::new(),
        };
        replay.replay_fingerprint = replay.compute_fingerprint()?;
        replay.validate()?;
        Ok(replay)
    }

    fn compute_fingerprint(&self) -> Result<String, GameRuntimeError> {
        let mut preimage = self.clone();
        preimage.replay_fingerprint.clear();
        let ron =
            litd_hash::to_stable_ron(&preimage).map_err(|error| GameRuntimeError::Decode {
                reason: format!("encode session replay RON failed: {error}"),
            })?;
        Ok(litd_hash::sha256_hex(ron.as_bytes()))
    }

    pub fn validate(&self) -> Result<(), GameRuntimeError> {
        if self.map_id.is_empty() {
            return Err(GameRuntimeError::Invalid {
                field: "session_replay.map_id".to_string(),
                reason: "must be non-empty".to_string(),
            });
        }
        let expected = self.compute_fingerprint()?;
        if self.replay_fingerprint != expected {
            return Err(GameRuntimeError::FingerprintMismatch {
                expected,
                actual: self.replay_fingerprint.clone(),
            });
        }
        Ok(())
    }

    /// Validate authored identity before constructing the fresh playback sim.
    pub fn validate_for_manifest(
        &self,
        manifest: &RuntimeDatasetsManifest,
    ) -> Result<(), GameRuntimeError> {
        self.validate()?;
        validate_runtime_datasets(manifest)?;
        if self.datasets_fingerprint != manifest.datasets_fingerprint {
            return Err(GameRuntimeError::FingerprintMismatch {
                expected: manifest.datasets_fingerprint.clone(),
                actual: self.datasets_fingerprint.clone(),
            });
        }
        if self.map_id != manifest.lowered_match.map_id {
            return Err(GameRuntimeError::Invalid {
                field: "session_replay.map_id".to_string(),
                reason: format!(
                    "replay map {:?} does not match package map {:?}",
                    self.map_id, manifest.lowered_match.map_id
                ),
            });
        }
        if self.sim_seed != manifest.constants.match_seed {
            return Err(GameRuntimeError::Invalid {
                field: "session_replay.sim_seed".to_string(),
                reason: format!(
                    "replay seed {} does not match package seed {}",
                    self.sim_seed, manifest.constants.match_seed
                ),
            });
        }
        Ok(())
    }

    /// Replay the command stream into a fresh deterministic session and verify its proof.
    pub fn restore(
        &self,
        manifest: &RuntimeDatasetsManifest,
    ) -> Result<MatchRuntimeSession, GameRuntimeError> {
        self.validate_for_manifest(manifest)?;
        let mut session = MatchRuntimeSession::from_manifest(manifest)?;
        for order in &self.order_log {
            session.apply(order.clone())?;
        }
        if session.applied_tick() != self.applied_tick {
            return Err(GameRuntimeError::Invalid {
                field: "session_replay.applied_tick".to_string(),
                reason: format!(
                    "replayed tick {} does not match recorded tick {}",
                    session.applied_tick(),
                    self.applied_tick
                ),
            });
        }
        let actual = session.combined_state_hash();
        if actual != self.expected_final_hash {
            return Err(GameRuntimeError::FingerprintMismatch {
                expected: format!("{:016x}", self.expected_final_hash),
                actual: format!("{actual:016x}"),
            });
        }
        Ok(session)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GameRuntimeError> {
        self.validate()?;
        let payload = litd_hash::to_stable_ron(self).map_err(|error| GameRuntimeError::Decode {
            reason: format!("encode session replay RON failed: {error}"),
        })?;
        let mut bytes = Vec::with_capacity(SESSION_REPLAY_HEADER_LEN + payload.len());
        bytes.extend_from_slice(SESSION_REPLAY_MAGIC);
        bytes.push(SESSION_REPLAY_ENCODING_RON);
        bytes.push(SESSION_REPLAY_FORMAT_VERSION);
        bytes.extend_from_slice(payload.as_bytes());
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GameRuntimeError> {
        validate_header(bytes)?;
        let payload =
            std::str::from_utf8(&bytes[SESSION_REPLAY_HEADER_LEN..]).map_err(|error| {
                GameRuntimeError::Decode {
                    reason: format!("decode session replay UTF-8 payload failed: {error}"),
                }
            })?;
        let replay: Self = ron::from_str(payload).map_err(|error| GameRuntimeError::Decode {
            reason: format!("decode session replay RON failed: {error}"),
        })?;
        replay.validate()?;
        Ok(replay)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), GameRuntimeError> {
        let bytes = self.to_bytes()?;
        fs::write(path, bytes).map_err(|error| GameRuntimeError::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        })
    }

    pub fn load_from_path(path: &Path) -> Result<Self, GameRuntimeError> {
        let bytes = fs::read(path).map_err(|error| GameRuntimeError::Io {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        Self::from_bytes(&bytes)
    }
}

fn validate_header(bytes: &[u8]) -> Result<(), GameRuntimeError> {
    if bytes.len() < SESSION_REPLAY_HEADER_LEN {
        return replay_header_error(format!(
            "truncated replay header: got {} bytes, need {SESSION_REPLAY_HEADER_LEN}",
            bytes.len()
        ));
    }
    if &bytes[..SESSION_REPLAY_MAGIC.len()] != SESSION_REPLAY_MAGIC {
        return replay_header_error("replay magic mismatch (not a LITD replay file)");
    }
    let encoding = bytes[SESSION_REPLAY_ENCODING_BYTE_INDEX];
    if encoding != SESSION_REPLAY_ENCODING_RON {
        return replay_header_error(format!(
            "unsupported replay encoding byte {encoding}; expected {SESSION_REPLAY_ENCODING_RON}"
        ));
    }
    let version = bytes[SESSION_REPLAY_VERSION_BYTE_INDEX];
    if version != SESSION_REPLAY_FORMAT_VERSION {
        return replay_header_error(format!(
            "unsupported replay version byte {version}; expected {SESSION_REPLAY_FORMAT_VERSION}"
        ));
    }
    Ok(())
}

fn replay_header_error<T>(reason: impl Into<String>) -> Result<T, GameRuntimeError> {
    Err(GameRuntimeError::Invalid {
        field: "session_replay.header".to_string(),
        reason: reason.into(),
    })
}
