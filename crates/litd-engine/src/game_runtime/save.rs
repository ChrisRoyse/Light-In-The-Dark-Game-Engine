//! #850(b): full mid-match save/load of the deterministic runtime.
//!
//! A [`GameSaveFile`] bundles the sim snapshot (world + RNG seed state via
//! `SimSnapshot`), BOTH order queues' pending positions, the day/night state, and
//! the campaign KV store, then stamps a SHA-256 fingerprint over all of it. Save
//! files live under the caller-chosen project/user directory. On load the schema
//! and fingerprint are both re-verified: a corrupt file fails with a typed
//! [`GameRuntimeError::Decode`], a tampered one with
//! [`GameRuntimeError::FingerprintMismatch`] -- never a silent fresh start.

use std::path::Path;

use litd_hash::{sha256_hex, to_stable_ron};
use serde::{Deserialize, Serialize};

use crate::{sim_order::SimOrder, sim_replay::SimSnapshot};

use super::{
    error::GameRuntimeError,
    kv_store::CampaignKvStore,
    time_of_day::{RuntimeEnvOrder, TimeOfDayState},
};

pub const GAME_SAVE_SCHEMA: &str = "litd.game_runtime.save.v1";

/// The complete, fingerprinted snapshot of a running match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSaveFile {
    pub schema: String,
    pub sim_snapshot: SimSnapshot,
    /// Sim orders enqueued but not yet applied at save time (queue position).
    pub pending_sim_orders: Vec<SimOrder>,
    /// Environment orders enqueued but not yet applied at save time.
    pub pending_env_orders: Vec<RuntimeEnvOrder>,
    pub time_of_day: TimeOfDayState,
    pub kv: CampaignKvStore,
    /// SHA-256 over the stable RON of every field above. The load-time integrity
    /// gate: recomputed and compared, so any byte tampering is rejected typed.
    pub save_fingerprint: String,
}

impl GameSaveFile {
    /// Assemble a save from its parts and stamp the fingerprint.
    pub fn capture(
        sim_snapshot: SimSnapshot,
        pending_sim_orders: Vec<SimOrder>,
        pending_env_orders: Vec<RuntimeEnvOrder>,
        time_of_day: TimeOfDayState,
        kv: CampaignKvStore,
    ) -> Result<Self, GameRuntimeError> {
        let mut save = Self {
            schema: GAME_SAVE_SCHEMA.to_string(),
            sim_snapshot,
            pending_sim_orders,
            pending_env_orders,
            time_of_day,
            kv,
            save_fingerprint: String::new(),
        };
        save.save_fingerprint = save.compute_fingerprint()?;
        Ok(save)
    }

    /// SHA-256 over the stable RON of every field except `save_fingerprint`.
    pub fn compute_fingerprint(&self) -> Result<String, GameRuntimeError> {
        let canonical = to_stable_ron(&(
            &self.schema,
            &self.sim_snapshot,
            &self.pending_sim_orders,
            &self.pending_env_orders,
            &self.time_of_day,
            &self.kv,
        ))
        .map_err(|error| GameRuntimeError::Decode {
            reason: error.to_string(),
        })?;
        Ok(sha256_hex(canonical.as_bytes()))
    }

    /// Fail-loud integrity check: schema tag, then fingerprint recomputation.
    pub fn validate(&self) -> Result<(), GameRuntimeError> {
        if self.schema != GAME_SAVE_SCHEMA {
            return Err(GameRuntimeError::SchemaMismatch {
                expected: GAME_SAVE_SCHEMA.to_string(),
                actual: self.schema.clone(),
            });
        }
        let expected = self.compute_fingerprint()?;
        if expected != self.save_fingerprint {
            return Err(GameRuntimeError::FingerprintMismatch {
                expected,
                actual: self.save_fingerprint.clone(),
            });
        }
        Ok(())
    }

    pub fn to_ron_string(&self) -> Result<String, GameRuntimeError> {
        to_stable_ron(self).map_err(|error| GameRuntimeError::Decode {
            reason: error.to_string(),
        })
    }

    pub fn from_ron_str(text: &str) -> Result<Self, GameRuntimeError> {
        ron::from_str(text).map_err(|error| GameRuntimeError::Decode {
            reason: error.to_string(),
        })
    }

    /// Write the save to `path` (project/user dir). Creates parent dirs.
    pub fn save_to_path(&self, path: &Path) -> Result<(), GameRuntimeError> {
        let text = self.to_ron_string()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| io_error(path, &error))?;
        }
        std::fs::write(path, text.as_bytes()).map_err(|error| io_error(path, &error))
    }

    /// Read + fully validate a save from disk. Corrupt bytes -> `Decode`; tampered
    /// content -> `FingerprintMismatch`; wrong schema -> `SchemaMismatch`.
    pub fn load_from_path(path: &Path) -> Result<Self, GameRuntimeError> {
        let text = std::fs::read_to_string(path).map_err(|error| io_error(path, &error))?;
        let save = Self::from_ron_str(&text)?;
        save.validate()?;
        Ok(save)
    }
}

fn io_error(path: &Path, error: &std::io::Error) -> GameRuntimeError {
    GameRuntimeError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    }
}
