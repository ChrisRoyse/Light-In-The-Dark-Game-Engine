use std::{
    env, fs,
    path::{Path, PathBuf},
};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_editor::{authoring::AuthoringOutcome, history::EditorDocument};
use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::{
    AI_ASTER_COLLECTION, AI_ASTER_COLUMN, AI_ASTER_ENV, AI_ASTER_ROW, AI_STATE_ENV,
    AI_STATE_RECORD_KIND, AI_STATE_SCHEMA_VERSION, AI_VAULT_ID, AiCoAuthorTransaction, ai_error,
    invalid_ai,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct AiCoAuthorStateFile {
    pub(super) schema_version: u32,
    pub(super) record_kind: String,
    pub(super) capability_operation_id: String,
    pub(super) readback_tool_name: String,
    pub(super) prompt: String,
    pub(super) document: EditorDocument,
    pub(super) outcome: Option<AuthoringOutcome>,
    pub(super) transactions: Vec<AiCoAuthorTransaction>,
    pub(super) last_status: String,
    pub(super) last_error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AsterReadback {
    pub(super) bytes: Vec<u8>,
    pub(super) seq: u64,
    pub(super) graph_rows: usize,
    pub(super) cell_key: String,
    pub(super) index_key: String,
}

pub(super) fn state_path_from_env() -> Result<PathBuf, EngineError> {
    env_path(AI_STATE_ENV, default_state_path())
}

pub(super) fn aster_dir_from_env() -> Result<PathBuf, EngineError> {
    env_path(AI_ASTER_ENV, default_aster_dir())
}

pub(super) fn write_aster_state(dir: &Path, bytes: &[u8]) -> Result<AsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, AI_ASTER_COLLECTION)
        .map_err(|error| ai_error("aster.column", error))?;
    let commit = column
        .put(AI_ASTER_ROW, AI_ASTER_COLUMN, bytes)
        .map_err(|error| ai_error("aster.put", error))?;
    let value = column
        .get(commit.seq, AI_ASTER_ROW, AI_ASTER_COLUMN)
        .map_err(|error| ai_error("aster.get", error))?
        .ok_or_else(|| ai_error("aster.get", "state_json missing immediately after put"))?;
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| ai_error("aster.graph_scan", error))?;
    let cell_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.cell_key && value.as_slice() == bytes);
    let index_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.index_key && value.as_slice() == bytes);
    if !cell_ok || !index_ok {
        return invalid_ai(
            "aster.graph_cf",
            "Aster Graph CF did not contain both row-major and column-major AI state rows",
        );
    }
    Ok(AsterReadback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
    })
}

pub(super) fn read_aster_state(dir: &Path) -> Result<Option<AsterReadback>, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, AI_ASTER_COLLECTION)
        .map_err(|error| ai_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let value = column
        .get(seq, AI_ASTER_ROW, AI_ASTER_COLUMN)
        .map_err(|error| ai_error("aster.get", error))?;
    let Some(bytes) = value else {
        return Ok(None);
    };
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| ai_error("aster.graph_scan", error))?;
    Ok(Some(AsterReadback {
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
    }))
}

pub(super) fn parse_state(bytes: &[u8]) -> Result<AiCoAuthorStateFile, EngineError> {
    serde_json::from_slice::<AiCoAuthorStateFile>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse ai co-author state: {error}"),
        }
    })
}

pub(super) fn validate_state_record(
    record: &AiCoAuthorStateFile,
    operation_id: &str,
    readback_tool: &str,
) -> Result<(), EngineError> {
    if record.schema_version != AI_STATE_SCHEMA_VERSION {
        return invalid_ai(
            "state.schema_version",
            format!(
                "expected schema {}, got {}",
                AI_STATE_SCHEMA_VERSION, record.schema_version
            ),
        );
    }
    if record.record_kind != AI_STATE_RECORD_KIND {
        return invalid_ai(
            "state.record_kind",
            format!(
                "expected {AI_STATE_RECORD_KIND}, got {}",
                record.record_kind
            ),
        );
    }
    if record.capability_operation_id != operation_id || record.readback_tool_name != readback_tool
    {
        return invalid_ai(
            "state.capability",
            "capability route changed under saved state",
        );
    }
    record.document.validate()
}

pub(super) fn verify_json_mirror_if_present(
    path: &Path,
    expected: &[u8],
) -> Result<(), EngineError> {
    if !path.exists() {
        return Ok(());
    }
    let bytes = fs::read(path).map_err(|error| {
        ai_error(
            "state.read_mirror",
            format!("read {}: {error}", path.display()),
        )
    })?;
    if bytes != expected {
        return invalid_ai(
            "state.mirror",
            format!(
                "{} does not match Aster state_json; refusing split-brain state",
                path.display()
            ),
        );
    }
    Ok(())
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = AI_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| ai_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        b"litd-editor-ai-coauthor-issue1230".to_vec(),
        Default::default(),
    )
    .map_err(|error| ai_error("aster.open", error))
}

fn env_path(name: &str, default: PathBuf) -> Result<PathBuf, EngineError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => invalid_ai("path.env", format!("{name} is empty")),
        Ok(value) => Ok(absolute_or_workspace_relative(PathBuf::from(value))),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => invalid_ai("path.env", format!("{name}: {error}")),
    }
}

fn default_state_path() -> PathBuf {
    workspace_root()
        .join("tmp")
        .join("litd-editor-ai-coauthor-state.json")
}

fn default_aster_dir() -> PathBuf {
    workspace_root()
        .join("tmp")
        .join("litd-editor-ai-coauthor-aster")
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
        .expect("litd-editor must live under the workspace crates directory")
        .to_path_buf()
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(18)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
