use litd_hash::sha256_hex;
use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

use super::{
    AssetBudgetConformance, AssetPipelineRecord, AssetPreparation,
    mcp_asset_tools_validation::validate_asset_state,
};

pub const MCP_ASSET_STATE_SCHEMA: &str = "litd.mcp.asset_tools.state.v1";
pub const MCP_ASSET_LEDGER_SCHEMA: &str = "litd.mcp.asset_tools.ledger.v1";
const MCP_ASSET_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC6";
const MCP_ASSET_VAULT_PURPOSE: &[u8] = b"litd-mcp-asset-tools-issue1363";
const MCP_ASSET_COLLECTION: &str = "litd_mcp_asset_tools";
const MCP_ASSET_STATE_ROW: &[u8] = b"asset/state/latest";
const MCP_ASSET_LEDGER_ROW: &[u8] = b"asset/ledger/latest";
const MCP_ASSET_STATE_COLUMN: &[u8] = b"state_json";
const MCP_ASSET_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAssetState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub update_count: u64,
    pub assets: Vec<AssetPipelineRecord>,
    pub preparations: Vec<AssetPreparation>,
    pub budgets: Vec<AssetBudgetConformance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAssetStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state_fingerprint: String,
    pub state: McpAssetState,
    pub asset_count: usize,
    pub preparation_count: usize,
    pub budget_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAssetToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpAssetToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAssetToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub state_path: String,
    pub state_sha256: String,
    pub state_bytes: u64,
    pub action_summary: String,
    pub affected_ids: Vec<String>,
    pub asset_count: usize,
    pub preparation_count: usize,
    pub budget_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAssetToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpAssetToolLedger,
}

impl McpAssetState {
    pub fn new(state_id: impl Into<String>) -> Result<Self, EngineError> {
        let state = Self {
            schema: MCP_ASSET_STATE_SCHEMA.to_string(),
            issue: 925,
            state_id: state_id.into(),
            source_of_truth: "Calyx Aster MCP asset-pipeline state row".to_string(),
            update_count: 0,
            assets: Vec::new(),
            preparations: Vec::new(),
            budgets: Vec::new(),
        };
        validate_mcp_asset_state(&state)?;
        Ok(state)
    }
}

pub fn load_or_create_mcp_asset_state(
    path: impl AsRef<Path>,
    state_id: impl Into<String>,
) -> Result<McpAssetState, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let state_id = state_id.into();
    if path.exists() {
        let state = read_mcp_asset_state(path)?.state;
        if state.state_id != state_id {
            return Err(store_error("state_id", "state_id mismatch"));
        }
        Ok(state)
    } else {
        McpAssetState::new(state_id)
    }
}

pub fn write_mcp_asset_state(
    path: impl AsRef<Path>,
    state: &McpAssetState,
) -> Result<McpAssetStateReadback, EngineError> {
    validate_mcp_asset_state(state)?;
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-asset-state-json",
            reason: format!("serialize asset state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&asset_state_spec(path), &bytes)
        .map_err(|error| store_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "state.write",
            "Aster asset state bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_asset_state(path)?;
    if readback.state != *state {
        return Err(store_error("state.readback", "state readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_asset_state(path: impl AsRef<Path>) -> Result<McpAssetStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let readback = read_calyx_json_bytes(&asset_state_spec(path))
        .map_err(|error| store_error("state.read", error))?
        .ok_or_else(|| {
            store_error(
                "state.read",
                format!("missing Calyx Aster asset state row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let state: McpAssetState =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-asset-state-json",
            reason: format!("parse asset state {}: {error}", path.display()),
        })?;
    readback_from_state(path, bytes.len() as u64, sha256_hex(&bytes), state)
}

pub fn write_mcp_asset_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpAssetToolRecord,
) -> Result<McpAssetToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    // Append to the existing ledger instead of overwriting it (#1521, same fix as #1490):
    // record_count:1, records:vec![record] destroyed every prior mutation's audit record.
    let mut ledger =
        match read_calyx_json_bytes(&asset_ledger_spec(path))
            .map_err(|error| store_error("ledger.read", error))?
        {
            Some(readback) => {
                let existing: McpAssetToolLedger = serde_json::from_slice(&readback.bytes)
                    .map_err(|error| EngineError::SerializationFailed {
                        format: "mcp-asset-ledger-json",
                        reason: format!("parse asset ledger {}: {error}", path.display()),
                    })?;
                validate_mcp_asset_tool_ledger(&existing)?;
                if existing.ledger_id != ledger_id {
                    return Err(store_error(
                        "ledger.ledger_id",
                        format!(
                            "ledger_id mismatch: existing {}, requested {ledger_id}",
                            existing.ledger_id
                        ),
                    ));
                }
                existing
            }
            None => McpAssetToolLedger {
                schema: MCP_ASSET_LEDGER_SCHEMA.to_string(),
                issue: 925,
                ledger_id,
                source_of_truth: "Calyx Aster MCP asset tool ledger row + asset-pipeline state row"
                    .to_string(),
                record_count: 0,
                records: Vec::new(),
            },
        };
    ledger.records.push(record);
    ledger.record_count = ledger.records.len();
    validate_mcp_asset_tool_ledger(&ledger)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-asset-ledger-json",
            reason: format!("serialize asset ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&asset_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster asset ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_asset_tool_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error("ledger.readback", "ledger readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_asset_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpAssetToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&asset_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!("missing Calyx Aster asset ledger row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpAssetToolLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-asset-ledger-json",
            reason: format!("parse asset ledger {}: {error}", path.display()),
        })?;
    validate_mcp_asset_tool_ledger(&ledger)?;
    Ok(McpAssetToolLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn validate_mcp_asset_state(state: &McpAssetState) -> Result<(), EngineError> {
    if state.schema != MCP_ASSET_STATE_SCHEMA
        || state.issue != 925
        || state.state_id.trim().is_empty()
        || state.source_of_truth.trim().is_empty()
    {
        return Err(store_error(
            "state",
            "schema, issue, and identity are invalid",
        ));
    }
    validate_asset_state(state)
}

pub fn validate_mcp_asset_tool_ledger(ledger: &McpAssetToolLedger) -> Result<(), EngineError> {
    if ledger.schema != MCP_ASSET_LEDGER_SCHEMA
        || ledger.issue != 925
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.record_count != ledger.records.len()
        || ledger.records.is_empty()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and count are invalid",
        ));
    }
    for record in &ledger.records {
        if record.request_id.trim().is_empty()
            || record.requested_by.trim().is_empty()
            || record.state_sha256.len() != 64
            || record.state_bytes == 0
            || record.affected_ids.is_empty()
        {
            return Err(store_error("record", "required record fields are invalid"));
        }
    }
    Ok(())
}

fn readback_from_state(
    path: &Path,
    bytes: u64,
    sha256: String,
    state: McpAssetState,
) -> Result<McpAssetStateReadback, EngineError> {
    validate_mcp_asset_state(&state)?;
    Ok(McpAssetStateReadback {
        path: path.display().to_string(),
        bytes,
        sha256,
        state_fingerprint: stable_fingerprint(&state)?,
        asset_count: state.assets.len(),
        preparation_count: state.preparations.len(),
        budget_count: state.budgets.len(),
        state,
    })
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(store_error(field, "path must be non-empty"));
    }
    Ok(())
}

fn asset_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_ASSET_VAULT_ID,
        purpose: MCP_ASSET_VAULT_PURPOSE,
        collection: MCP_ASSET_COLLECTION,
        row_key: MCP_ASSET_STATE_ROW,
        column: MCP_ASSET_STATE_COLUMN,
    }
}

fn asset_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_ASSET_VAULT_ID,
        purpose: MCP_ASSET_VAULT_PURPOSE,
        collection: MCP_ASSET_COLLECTION,
        row_key: MCP_ASSET_LEDGER_ROW,
        column: MCP_ASSET_LEDGER_COLUMN,
    }
}

fn store_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.mcp_asset_tools.store.{}", field.into()),
        reason: reason.to_string(),
    }
}
