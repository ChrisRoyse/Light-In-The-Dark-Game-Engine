//! Startup-time loader that freezes editor-authored Calyx content for the sim (#1455).
//!
//! This is the runtime half of the content pipeline: the editor authoring path writes full
//! Calyx-shaped rows into the CANONICAL engine content-panel vault, and this module reads those
//! rows back ONCE at startup and freezes their lowered artifacts into an in-memory table.
//!
//! The deterministic hot path must never query Calyx live
//! (`calyx_live_query_allowed_in_hot_path=false`): a per-tick Calyx query would break lockstep
//! determinism. So every read here happens at load time, outside the hot path, and the frozen
//! table is the only thing sim systems consult afterwards.
//!
//! Every failure path errors loudly with a structured code - a missing row, a vault that cannot
//! be resolved, or a lowered fingerprint that disagrees with the row it was frozen from is an
//! error, never a silent empty-ok.

use crate::calyx_content::{
    EngineCalyxContentError, EngineCalyxContentStore, EngineContentConstellationReadback,
    EngineContentKind, EngineLoweredContentArtifact,
};
use crate::calyx_paths::resolve_engine_content_panel_vault;
use crate::calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig};

pub const ENGINE_FROZEN_CONTENT_TABLE_SCHEMA: &str = "litd.engine.frozen_content_table.v1";

/// Structured code for a canonical content-panel vault that cannot be resolved. Surfaced in the
/// `EngineCalyxContentError::InvalidContent` reason so callers can distinguish "the vault is
/// unreachable" from "the row is missing".
pub const CONTENT_RUNTIME_VAULT_UNRESOLVED: &str = "LITD_CONTENT_RUNTIME_VAULT_UNRESOLVED";
/// Structured code for an empty freeze request - freezing zero ids is a programming error.
pub const CONTENT_RUNTIME_EMPTY_REQUEST: &str = "LITD_CONTENT_RUNTIME_EMPTY_REQUEST";

/// Open an [`EngineCalyxContentStore`] on the CANONICAL engine content-panel vault
/// (`gameeditor-engine-content-panel`, resolved under the canonical Calyx root). This is the one
/// store both the editor authoring tools and the runtime loader share, so a row authored by the
/// editor is the exact same byte-for-byte row the sim loads.
pub fn open_canonical_engine_content_store()
-> Result<EngineCalyxContentStore, EngineCalyxContentError> {
    let vault_dir = resolve_engine_content_panel_vault().map_err(|error| {
        EngineCalyxContentError::InvalidContent {
            field: "canonical_content_panel_vault".to_string(),
            reason: format!("{CONTENT_RUNTIME_VAULT_UNRESOLVED}: {error}"),
        }
    })?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(vault_dir))?;
    Ok(EngineCalyxContentStore::new(database))
}

/// One frozen content row: the verified editor readback plus its lowered artifact. Held in memory
/// for the whole session so the sim never re-queries Calyx.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenContentRow {
    pub kind: EngineContentKind,
    pub id: String,
    pub artifact: EngineLoweredContentArtifact,
    pub readback: EngineContentConstellationReadback,
}

impl FrozenContentRow {
    /// The lowered fingerprint (== the row's `input_sha256`) that changes whenever any authored
    /// field changes. This is what a sim system pins to detect a no-recompile content edit.
    pub fn fingerprint(&self) -> &str {
        &self.artifact.fingerprint
    }

    /// Read an integer value out of the frozen authored payload by JSON pointer
    /// (e.g. `/stat_caps/intelligence`). Returns `None` if the pointer is absent or non-integer;
    /// value inspection is a read of already-frozen data, never a live Calyx query.
    pub fn payload_i64(&self, pointer: &str) -> Option<i64> {
        self.readback
            .payload
            .pointer(pointer)
            .and_then(|value| value.as_i64())
    }

    /// Read a string value out of the frozen authored payload by JSON pointer (e.g. `/role`).
    /// Returns `None` if the pointer is absent or non-string; value inspection is a read of
    /// already-frozen data, never a live Calyx query.
    pub fn payload_str(&self, pointer: &str) -> Option<&str> {
        self.readback
            .payload
            .pointer(pointer)
            .and_then(|value| value.as_str())
    }
}

/// The frozen content table built once at startup. After construction it is pure in-memory data;
/// nothing here touches Calyx again.
#[derive(Debug, Clone, PartialEq)]
pub struct FrozenContentTable {
    pub schema: String,
    pub rows: Vec<FrozenContentRow>,
}

impl FrozenContentTable {
    /// Freeze the requested `(kind, id)` rows from the store. Each id is read back through the same
    /// read-after-write path the editor uses, then its lowered artifact is pinned. A row whose
    /// lowered fingerprint disagrees with the readback it was frozen from is a hard error - the
    /// two must be self-consistent or the freeze is meaningless.
    pub fn freeze(
        store: &EngineCalyxContentStore,
        ids: &[(EngineContentKind, String)],
    ) -> Result<Self, EngineCalyxContentError> {
        if ids.is_empty() {
            return Err(EngineCalyxContentError::InvalidContent {
                field: "ids".to_string(),
                reason: format!(
                    "{CONTENT_RUNTIME_EMPTY_REQUEST}: at least one (kind, id) is required to freeze content"
                ),
            });
        }
        let mut rows = Vec::with_capacity(ids.len());
        for (kind, id) in ids {
            let readback = store.get_content_for_editor(*kind, id)?;
            let artifact = store.lowered_content_for_engine(*kind, id)?;
            if artifact.fingerprint != readback.input_sha256 {
                return Err(EngineCalyxContentError::ReadbackMismatch {
                    field: "frozen.fingerprint".to_string(),
                    expected: readback.input_sha256.clone(),
                    actual: artifact.fingerprint.clone(),
                });
            }
            rows.push(FrozenContentRow {
                kind: *kind,
                id: id.clone(),
                artifact,
                readback,
            });
        }
        Ok(Self {
            schema: ENGINE_FROZEN_CONTENT_TABLE_SCHEMA.to_string(),
            rows,
        })
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row(&self, kind: EngineContentKind, id: &str) -> Option<&FrozenContentRow> {
        self.rows
            .iter()
            .find(|row| row.kind == kind && row.id == id)
    }

    pub fn fingerprint(&self, kind: EngineContentKind, id: &str) -> Option<&str> {
        self.row(kind, id).map(FrozenContentRow::fingerprint)
    }

    pub fn artifacts(&self) -> Vec<EngineLoweredContentArtifact> {
        self.rows.iter().map(|row| row.artifact.clone()).collect()
    }
}

/// Load the frozen lowered artifacts for `ids` from the store at startup. The returned vector is
/// the frozen table sim systems consult; the read happens once, here, never per tick.
pub fn load_frozen_content_for_sim(
    store: &EngineCalyxContentStore,
    ids: &[(EngineContentKind, String)],
) -> Result<Vec<EngineLoweredContentArtifact>, EngineCalyxContentError> {
    Ok(FrozenContentTable::freeze(store, ids)?.artifacts())
}

#[cfg(test)]
#[path = "calyx_content_runtime_tests.rs"]
mod tests;
