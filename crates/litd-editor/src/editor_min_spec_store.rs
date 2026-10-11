//! Persist the editor min-spec degrade knobs (#1793 deliverable c) through the REAL
//! Calyx Aster path -- NO side store (#1348). Mirrors the existing `*_store.rs`
//! AsterVault + PlainColumn pattern: explicit-encode the settings as JSON bytes,
//! commit to the Graph column family, and read the bytes back to prove the write.

use std::{
    env,
    path::{Path, PathBuf},
};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::{EngineError, editor_min_spec::EditorDegradeSettings};
use litd_hash::sha256_hex;

const MIN_SPEC_COLLECTION: &str = "litd_editor_min_spec";
const DEGRADE_COLUMN: &[u8] = b"degrade_settings_json";
const DEGRADE_ROW: &[u8] = b"degrade/current";
const MIN_SPEC_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FBX";
const MIN_SPEC_VAULT_PURPOSE: &[u8] = b"litd-editor-min-spec-issue1793";
const MIN_SPEC_ASTER_ENV: &str = "LITD_EDITOR_MIN_SPEC_ASTER";

/// Aster-backed store for the editor degrade settings.
#[derive(Debug, Clone)]
pub struct EditorMinSpecStore {
    vault_dir: PathBuf,
}

/// Independent read-back proof of a persisted settings row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DegradeSettingsReadback {
    pub vault_dir: PathBuf,
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub seq: u64,
    pub graph_rows: usize,
}

impl EditorMinSpecStore {
    pub fn new(vault_dir: PathBuf) -> Self {
        Self { vault_dir }
    }

    pub fn from_env() -> Result<Self, EngineError> {
        Ok(Self::new(env_path(
            MIN_SPEC_ASTER_ENV,
            default_vault_dir(),
        )?))
    }

    pub fn vault_dir(&self) -> &Path {
        &self.vault_dir
    }

    /// Persist the degrade settings. Validates first (fail loud on an invalid knob),
    /// commits the JSON bytes to Aster, then reads them back and confirms both the
    /// cell and index rows landed in the Graph CF byte-identically.
    pub fn save_degrade_settings(
        &self,
        settings: &EditorDegradeSettings,
    ) -> Result<DegradeSettingsReadback, EngineError> {
        settings.validate()?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("serialize degrade settings: {error}"),
            }
        })?;
        let vault = open_aster(&self.vault_dir)?;
        let column = PlainColumn::new(&vault, MIN_SPEC_COLLECTION)
            .map_err(|error| store_error("aster.column", error.to_string()))?;
        let commit = column
            .put(DEGRADE_ROW, DEGRADE_COLUMN, &bytes)
            .map_err(|error| store_error("aster.put", error.to_string()))?;
        let value = column
            .get(commit.seq, DEGRADE_ROW, DEGRADE_COLUMN)
            .map_err(|error| store_error("aster.get", error.to_string()))?
            .ok_or_else(|| store_error("aster.get", "degrade settings missing after put"))?;
        if value != bytes {
            return Err(store_error(
                "aster.readback",
                "degrade settings bytes changed between write and readback",
            ));
        }
        let raw = vault
            .scan_cf_at(commit.seq, ColumnFamily::Graph)
            .map_err(|error| store_error("aster.graph_scan", error.to_string()))?;
        let cell_ok = raw
            .iter()
            .any(|(key, value)| key == &commit.cell_key && value.as_slice() == bytes);
        let index_ok = raw
            .iter()
            .any(|(key, value)| key == &commit.index_key && value.as_slice() == bytes);
        if !cell_ok || !index_ok {
            return Err(store_error(
                "aster.graph_cf",
                "Aster Graph CF did not contain both degrade-settings rows",
            ));
        }
        Ok(DegradeSettingsReadback {
            vault_dir: self.vault_dir.clone(),
            sha256: sha256_hex(&value),
            bytes: value,
            seq: commit.seq,
            graph_rows: raw.len(),
        })
    }

    /// Load the persisted degrade settings, or `None` if none were ever written.
    /// Re-validates on read so a corrupted knob fails loud rather than propagating.
    pub fn load_degrade_settings(&self) -> Result<Option<EditorDegradeSettings>, EngineError> {
        let vault = open_aster(&self.vault_dir)?;
        let column = PlainColumn::new(&vault, MIN_SPEC_COLLECTION)
            .map_err(|error| store_error("aster.column", error.to_string()))?;
        let seq = vault.latest_seq();
        let Some(bytes) = column
            .get(seq, DEGRADE_ROW, DEGRADE_COLUMN)
            .map_err(|error| store_error("aster.get", error.to_string()))?
        else {
            return Ok(None);
        };
        let settings =
            serde_json::from_slice::<EditorDegradeSettings>(&bytes).map_err(|error| {
                EngineError::SerializationFailed {
                    format: "json",
                    reason: format!("parse degrade settings from Aster: {error}"),
                }
            })?;
        settings.validate()?;
        Ok(Some(settings))
    }
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = MIN_SPEC_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error.to_string()))?;
    AsterVault::open(
        dir,
        vault_id,
        MIN_SPEC_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error.to_string()))
}

fn env_path(name: &str, default: PathBuf) -> Result<PathBuf, EngineError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => {
            Err(store_error("path.env", format!("{name} is empty")))
        }
        Ok(value) => Ok(absolute_or_workspace_relative(PathBuf::from(value))),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(store_error("path.env", format!("{name}: {error}"))),
    }
}

fn default_vault_dir() -> PathBuf {
    workspace_root()
        .join("tmp")
        .join("litd-editor-min-spec-aster")
}

fn absolute_or_workspace_relative(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        workspace_root().join(path)
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor must live under workspace crates directory")
        .to_path_buf()
}

fn store_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor_min_spec_store.{field}"),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "editor_min_spec_store_tests.rs"]
mod editor_min_spec_store_tests;
