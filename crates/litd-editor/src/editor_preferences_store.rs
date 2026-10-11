use std::{
    env,
    path::{Path, PathBuf},
};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use crate::history::{autosave_record, stable_fingerprint};

use super::*;
use crate::editor_state_store::EditorStateStore;

const AUTOSAVE_COLLECTION: &str = "litd_editor_autosave";
const AUTOSAVE_COLUMN: &[u8] = b"autosave_snapshot_json";
const AUTOSAVE_ROW_PREFIX: &str = "autosave/";
const PREFS_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
const PREFS_VAULT_PURPOSE: &[u8] = b"litd-editor-preferences-issue1221";

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutosaveAsterReadback {
    bytes: Vec<u8>,
    seq: u64,
    graph_rows: usize,
}

impl EditorPreferencesStore {
    pub fn from_env() -> Result<Self, EngineError> {
        Ok(Self {
            preferences_path: env_path(EDITOR_PREFERENCES_ENV, default_preferences_path())?,
            autosave_aster_dir: env_path(EDITOR_AUTOSAVE_ASTER_ENV, default_autosave_aster_dir())?,
        })
    }

    pub fn new(preferences_path: PathBuf, autosave_aster_dir: PathBuf) -> Self {
        Self {
            preferences_path,
            autosave_aster_dir,
        }
    }

    pub fn save_preferences(
        &self,
        preferences: &EditorPreferences,
    ) -> Result<PreferenceFileReadback, EngineError> {
        preferences.validate()?;
        let bytes = serde_json::to_vec_pretty(preferences).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("serialize preferences: {error}"),
            }
        })?;
        let store = EditorStateStore::new(self.preferences_path.clone());
        let readback = store.put_preferences(&bytes)?;
        let loaded = store.read_preferences()?.ok_or_else(|| {
            preferences_error(
                "preferences.readback",
                "preferences missing after Aster put",
            )
        })?;
        if loaded != bytes {
            return invalid_preferences(
                "preferences.readback",
                "preferences Aster bytes changed between write and readback",
            );
        }
        Ok(PreferenceFileReadback {
            path: readback.vault_dir,
            bytes: readback.bytes,
            fingerprint: stable_fingerprint(preferences)?,
        })
    }

    pub fn load_preferences(&self) -> Result<EditorPreferences, EngineError> {
        let bytes = EditorStateStore::new(self.preferences_path.clone())
            .read_preferences()?
            .ok_or_else(|| {
                preferences_error("preferences.read", "preferences Aster row missing")
            })?;
        let preferences = serde_json::from_slice::<EditorPreferences>(&bytes).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("parse preferences from Aster: {error}"),
            }
        })?;
        preferences.validate()?;
        Ok(preferences)
    }

    pub fn load_or_default_loud(&self) -> PreferencesLoadOutcome {
        match self.load_preferences() {
            Ok(preferences) => PreferencesLoadOutcome {
                preferences,
                loaded_from_disk: true,
                recovered_from_corruption: false,
                error: String::new(),
            },
            Err(error) => PreferencesLoadOutcome {
                preferences: EditorPreferences::default(),
                loaded_from_disk: false,
                recovered_from_corruption: true,
                error: error.to_string(),
            },
        }
    }

    pub fn write_autosave(
        &self,
        preferences: &EditorPreferences,
        document: &EditorDocument,
    ) -> Result<AutosaveWriteReport, EngineError> {
        preferences.validate()?;
        document.validate()?;
        if preferences.autosave.interval_seconds.is_none() {
            return Ok(AutosaveWriteReport {
                status: AutosaveWriteStatus::Disabled,
                document_id: document.calyx_document_id.clone(),
                stable_fingerprint: stable_fingerprint(document)?,
                seq: 0,
                graph_rows: 0,
            });
        }
        let snapshot = EditorAutosaveSnapshot {
            schema_version: EDITOR_PREFERENCES_SCHEMA_VERSION,
            record_kind: EDITOR_AUTOSAVE_RECORD_KIND.to_string(),
            preferences_fingerprint: stable_fingerprint(preferences)?,
            autosave: autosave_record(document, true)?,
            document: document.clone(),
        };
        let readback = write_autosave_snapshot(&self.autosave_aster_dir, &snapshot)?;
        let parsed = parse_autosave_snapshot(&readback.bytes)?;
        Ok(AutosaveWriteReport {
            status: AutosaveWriteStatus::Written,
            document_id: parsed.document.calyx_document_id,
            stable_fingerprint: parsed.autosave.stable_fingerprint,
            seq: readback.seq,
            graph_rows: readback.graph_rows,
        })
    }

    pub fn recover_autosave(
        &self,
        document_id: &str,
        choice: RecoveryChoice,
    ) -> Result<RecoveryReport, EngineError> {
        if document_id.trim().is_empty() {
            return invalid_preferences("recovery.document_id", "document id must be non-empty");
        }
        if choice == RecoveryChoice::Decline {
            return Ok(RecoveryReport {
                accepted: false,
                document_id: document_id.to_string(),
                recovered_fingerprint: "declined".to_string(),
                document: None,
            });
        }
        let readback = read_autosave_snapshot(&self.autosave_aster_dir, document_id)?
            .ok_or_else(|| preferences_error("recovery.autosave", "missing autosave snapshot"))?;
        let snapshot = parse_autosave_snapshot(&readback.bytes)?;
        Ok(RecoveryReport {
            accepted: true,
            document_id: snapshot.document.calyx_document_id.clone(),
            recovered_fingerprint: snapshot.autosave.stable_fingerprint,
            document: Some(snapshot.document),
        })
    }
}

fn write_autosave_snapshot(
    dir: &Path,
    snapshot: &EditorAutosaveSnapshot,
) -> Result<AutosaveAsterReadback, EngineError> {
    validate_snapshot(snapshot)?;
    let bytes =
        serde_json::to_vec_pretty(snapshot).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize autosave snapshot: {error}"),
        })?;
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, AUTOSAVE_COLLECTION)
        .map_err(|error| preferences_error("aster.column", error.to_string()))?;
    let row = autosave_row(&snapshot.document.calyx_document_id)?;
    let commit = column
        .put(&row, AUTOSAVE_COLUMN, &bytes)
        .map_err(|error| preferences_error("aster.put", error.to_string()))?;
    let value = column
        .get(commit.seq, &row, AUTOSAVE_COLUMN)
        .map_err(|error| preferences_error("aster.get", error.to_string()))?
        .ok_or_else(|| preferences_error("aster.get", "autosave snapshot missing after put"))?;
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| preferences_error("aster.graph_scan", error.to_string()))?;
    let cell_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.cell_key && value.as_slice() == bytes);
    let index_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.index_key && value.as_slice() == bytes);
    if !cell_ok || !index_ok {
        return invalid_preferences(
            "aster.graph_cf",
            "Aster Graph CF did not contain both autosave rows",
        );
    }
    Ok(AutosaveAsterReadback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
    })
}

fn read_autosave_snapshot(
    dir: &Path,
    document_id: &str,
) -> Result<Option<AutosaveAsterReadback>, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, AUTOSAVE_COLLECTION)
        .map_err(|error| preferences_error("aster.column", error.to_string()))?;
    let seq = vault.latest_seq();
    let row = autosave_row(document_id)?;
    let Some(bytes) = column
        .get(seq, &row, AUTOSAVE_COLUMN)
        .map_err(|error| preferences_error("aster.get", error.to_string()))?
    else {
        return Ok(None);
    };
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| preferences_error("aster.graph_scan", error.to_string()))?;
    Ok(Some(AutosaveAsterReadback {
        bytes,
        seq,
        graph_rows: raw.len(),
    }))
}

fn parse_autosave_snapshot(bytes: &[u8]) -> Result<EditorAutosaveSnapshot, EngineError> {
    let snapshot = serde_json::from_slice::<EditorAutosaveSnapshot>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse autosave snapshot: {error}"),
        }
    })?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_snapshot(snapshot: &EditorAutosaveSnapshot) -> Result<(), EngineError> {
    if snapshot.schema_version != EDITOR_PREFERENCES_SCHEMA_VERSION {
        return invalid_preferences("autosave.schema_version", "unsupported autosave schema");
    }
    if snapshot.record_kind != EDITOR_AUTOSAVE_RECORD_KIND {
        return invalid_preferences("autosave.record_kind", "unexpected autosave record kind");
    }
    snapshot.document.validate()?;
    let fingerprint = stable_fingerprint(&snapshot.document)?;
    if fingerprint != snapshot.autosave.stable_fingerprint {
        return invalid_preferences(
            "autosave.fingerprint",
            "autosave fingerprint does not match document",
        );
    }
    Ok(())
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = PREFS_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| preferences_error("aster.vault_id", error.to_string()))?;
    AsterVault::open(
        dir,
        vault_id,
        PREFS_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| preferences_error("aster.open", error.to_string()))
}

fn autosave_row(document_id: &str) -> Result<Vec<u8>, EngineError> {
    if document_id.trim().is_empty() {
        return invalid_preferences("autosave.row", "document id must be non-empty");
    }
    Ok(format!("{AUTOSAVE_ROW_PREFIX}{document_id}").into_bytes())
}

fn env_path(name: &str, default: PathBuf) -> Result<PathBuf, EngineError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => {
            invalid_preferences("path.env", format!("{name} is empty"))
        }
        Ok(value) => Ok(absolute_or_workspace_relative(PathBuf::from(value))),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => invalid_preferences("path.env", format!("{name}: {error}")),
    }
}

fn default_preferences_path() -> PathBuf {
    workspace_root()
        .join("tmp")
        .join("litd-editor-preferences-aster")
}

fn default_autosave_aster_dir() -> PathBuf {
    workspace_root()
        .join("tmp")
        .join("litd-editor-autosave-aster")
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
