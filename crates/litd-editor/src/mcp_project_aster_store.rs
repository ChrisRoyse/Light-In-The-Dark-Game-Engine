use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde::{Serialize, de::DeserializeOwned};

const MCP_PROJECT_ASTER_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC0";
const MCP_PROJECT_ASTER_VAULT_PURPOSE: &[u8] = b"litd-mcp-project-issue1362";

pub struct AsterJsonSpec {
    pub label: &'static str,
    pub collection: &'static str,
    pub row: &'static [u8],
    pub column: &'static [u8],
    pub format: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsterJsonReadback<T> {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub value: T,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
}

pub fn write_json_record<T>(
    dir: impl AsRef<Path>,
    spec: &AsterJsonSpec,
    value: &T,
    validate: fn(&T) -> Result<(), EngineError>,
) -> Result<AsterJsonReadback<T>, EngineError>
where
    T: Serialize + DeserializeOwned + PartialEq,
{
    validate(value)?;
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| EngineError::SerializationFailed {
            format: spec.format,
            reason: format!("serialize {}: {error}", spec.label),
        })?;
    let vault = open_aster(dir.as_ref(), spec)?;
    let column = PlainColumn::new(&vault, spec.collection)
        .map_err(|error| store_error(spec, "aster.column", error))?;
    let commit = column
        .put(spec.row, spec.column, &bytes)
        .map_err(|error| store_error(spec, "aster.put", error))?;
    let stored = column
        .get(commit.seq, spec.row, spec.column)
        .map_err(|error| store_error(spec, "aster.get", error))?
        .ok_or_else(|| store_error(spec, "aster.get", "record missing immediately after put"))?;
    if stored != bytes {
        return invalid_store(
            spec,
            "aster.readback",
            "Aster bytes differed immediately after put",
        );
    }
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| store_error(spec, "aster.graph_scan", error))?;
    let cell_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.cell_key && value.as_slice() == bytes);
    let index_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.index_key && value.as_slice() == bytes);
    if !cell_ok || !index_ok {
        return invalid_store(
            spec,
            "aster.graph_cf",
            "Aster Graph CF did not contain both committed rows",
        );
    }
    let parsed = parse_json(spec, &stored, validate)?;
    if &parsed != value {
        return invalid_store(
            spec,
            "aster.parsed_readback",
            "Aster parsed record differed immediately after put",
        );
    }
    Ok(AsterJsonReadback {
        path: dir.as_ref().display().to_string(),
        bytes: stored.len() as u64,
        sha256: sha256_hex(&stored),
        value: parsed,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
    })
}

pub fn read_json_record<T>(
    dir: impl AsRef<Path>,
    spec: &AsterJsonSpec,
    validate: fn(&T) -> Result<(), EngineError>,
) -> Result<AsterJsonReadback<T>, EngineError>
where
    T: DeserializeOwned,
{
    read_json_record_optional(dir.as_ref(), spec, validate)?
        .ok_or_else(|| store_error(spec, "aster.get", "record missing from Aster vault"))
}

pub fn read_json_record_optional<T>(
    dir: impl AsRef<Path>,
    spec: &AsterJsonSpec,
    validate: fn(&T) -> Result<(), EngineError>,
) -> Result<Option<AsterJsonReadback<T>>, EngineError>
where
    T: DeserializeOwned,
{
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(None);
    }
    let vault = open_aster(dir, spec)?;
    let column = PlainColumn::new(&vault, spec.collection)
        .map_err(|error| store_error(spec, "aster.column", error))?;
    let seq = vault.latest_seq();
    let Some(bytes) = column
        .get(seq, spec.row, spec.column)
        .map_err(|error| store_error(spec, "aster.get", error))?
    else {
        return Ok(None);
    };
    let parsed = parse_json(spec, &bytes, validate)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error(spec, "aster.graph_scan", error))?;
    Ok(Some(AsterJsonReadback {
        path: dir.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        value: parsed,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
    }))
}

fn parse_json<T>(
    spec: &AsterJsonSpec,
    bytes: &[u8],
    validate: fn(&T) -> Result<(), EngineError>,
) -> Result<T, EngineError>
where
    T: DeserializeOwned,
{
    let parsed =
        serde_json::from_slice::<T>(bytes).map_err(|error| EngineError::SerializationFailed {
            format: spec.format,
            reason: format!("parse {} from Aster: {error}", spec.label),
        })?;
    validate(&parsed)?;
    Ok(parsed)
}

fn open_aster(dir: &Path, spec: &AsterJsonSpec) -> Result<AsterVault<SystemClock>, EngineError> {
    if dir.exists() && !dir.is_dir() {
        return Err(store_error(
            spec,
            "aster.path",
            format!("{} is not an Aster vault directory", dir.display()),
        ));
    }
    let vault_id = MCP_PROJECT_ASTER_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error(spec, "aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        MCP_PROJECT_ASTER_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error(spec, "aster.open", error))
}

fn invalid_store<T>(
    spec: &AsterJsonSpec,
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(store_error(spec, field, reason.into()))
}

fn store_error(
    spec: &AsterJsonSpec,
    field: impl Into<String>,
    reason: impl ToString,
) -> EngineError {
    let field = format!(
        "editor.mcp_project_tools.aster.{}.{}",
        spec.label,
        field.into()
    );
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::aster",
        field = %field,
        reason = %reason,
        code = "LITD-MCP-PROJECT-ASTER-STORE",
        "project-domain Aster store operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(18)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
