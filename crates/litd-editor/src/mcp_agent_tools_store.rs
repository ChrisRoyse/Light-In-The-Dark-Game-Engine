use litd_hash::sha256_hex;
use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

use super::{
    AgentBinding, AgentPromptModel, AgentWardEnvelope,
    mcp_agent_tools_validation::validate_agent_state,
};

pub const MCP_AGENT_STATE_SCHEMA: &str = "litd.mcp.agent_tools.state.v1";
pub const MCP_AGENT_LEDGER_SCHEMA: &str = "litd.mcp.agent_tools.ledger.v1";
const MCP_AGENT_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC5";
const MCP_AGENT_VAULT_PURPOSE: &[u8] = b"litd-mcp-agent-tools-issue1363";
const MCP_AGENT_COLLECTION: &str = "litd_mcp_agent_tools";
const MCP_AGENT_STATE_ROW: &[u8] = b"agent/state/latest";
const MCP_AGENT_LEDGER_ROW: &[u8] = b"agent/ledger/latest";
const MCP_AGENT_STATE_COLUMN: &[u8] = b"state_json";
const MCP_AGENT_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAgentState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub update_count: u64,
    pub bindings: Vec<AgentBinding>,
    pub prompt_models: Vec<AgentPromptModel>,
    pub ward_envelopes: Vec<AgentWardEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAgentStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state_fingerprint: String,
    pub state: McpAgentState,
    pub binding_count: usize,
    pub prompt_model_count: usize,
    pub ward_envelope_count: usize,
    pub enabled_binding_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAgentToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpAgentToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAgentToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub state_path: String,
    pub state_sha256: String,
    pub state_bytes: u64,
    pub action_summary: String,
    pub affected_ids: Vec<String>,
    pub binding_count: usize,
    pub prompt_model_count: usize,
    pub ward_envelope_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAgentToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpAgentToolLedger,
}

impl McpAgentState {
    pub fn new(state_id: impl Into<String>) -> Result<Self, EngineError> {
        let state = Self {
            schema: MCP_AGENT_STATE_SCHEMA.to_string(),
            issue: 924,
            state_id: state_id.into(),
            source_of_truth: "Calyx Aster MCP in-game agent state row".to_string(),
            update_count: 0,
            bindings: Vec::new(),
            prompt_models: Vec::new(),
            ward_envelopes: Vec::new(),
        };
        validate_mcp_agent_state(&state)?;
        Ok(state)
    }
}

pub fn load_or_create_mcp_agent_state(
    path: impl AsRef<Path>,
    state_id: impl Into<String>,
) -> Result<McpAgentState, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let state_id = state_id.into();
    if path.exists() {
        let state = read_mcp_agent_state(path)?.state;
        if state.state_id != state_id {
            return Err(store_error("state_id", "state_id mismatch"));
        }
        Ok(state)
    } else {
        McpAgentState::new(state_id)
    }
}

pub fn write_mcp_agent_state(
    path: impl AsRef<Path>,
    state: &McpAgentState,
) -> Result<McpAgentStateReadback, EngineError> {
    validate_mcp_agent_state(state)?;
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-agent-state-json",
            reason: format!("serialize agent state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&agent_state_spec(path), &bytes)
        .map_err(|error| store_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "state.write",
            "Aster agent state bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_agent_state(path)?;
    if readback.state != *state {
        return Err(store_error("state.readback", "state readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_agent_state(path: impl AsRef<Path>) -> Result<McpAgentStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let readback = read_calyx_json_bytes(&agent_state_spec(path))
        .map_err(|error| store_error("state.read", error))?
        .ok_or_else(|| {
            store_error(
                "state.read",
                format!("missing Calyx Aster agent state row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let state: McpAgentState =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-agent-state-json",
            reason: format!("parse agent state {}: {error}", path.display()),
        })?;
    readback_from_state(path, bytes.len() as u64, sha256_hex(&bytes), state)
}

pub fn write_mcp_agent_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpAgentToolRecord,
) -> Result<McpAgentToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    // Append to the existing ledger instead of overwriting it (#1521, same fix as #1490):
    // the record_count/records Vec schema is an accumulating audit trail; the old
    // record_count:1, records:vec![record] destroyed every prior mutation's audit record.
    // Read the current ledger (if any), reject a ledger_id mismatch, push, persist history.
    let mut ledger = match read_calyx_json_bytes(&agent_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
    {
        Some(readback) => {
            let existing: McpAgentToolLedger =
                serde_json::from_slice(&readback.bytes).map_err(|error| {
                    EngineError::SerializationFailed {
                        format: "mcp-agent-ledger-json",
                        reason: format!("parse agent ledger {}: {error}", path.display()),
                    }
                })?;
            validate_mcp_agent_tool_ledger(&existing)?;
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
        None => McpAgentToolLedger {
            schema: MCP_AGENT_LEDGER_SCHEMA.to_string(),
            issue: 924,
            ledger_id,
            source_of_truth: "Calyx Aster MCP agent tool ledger row + agent state row".to_string(),
            record_count: 0,
            records: Vec::new(),
        },
    };
    ledger.records.push(record);
    ledger.record_count = ledger.records.len();
    validate_mcp_agent_tool_ledger(&ledger)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-agent-ledger-json",
            reason: format!("serialize agent ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&agent_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster agent ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_agent_tool_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error("ledger.readback", "ledger readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_agent_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpAgentToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&agent_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!("missing Calyx Aster agent ledger row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpAgentToolLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-agent-ledger-json",
            reason: format!("parse agent ledger {}: {error}", path.display()),
        })?;
    validate_mcp_agent_tool_ledger(&ledger)?;
    Ok(McpAgentToolLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn validate_mcp_agent_state(state: &McpAgentState) -> Result<(), EngineError> {
    if state.schema != MCP_AGENT_STATE_SCHEMA
        || state.issue != 924
        || state.state_id.trim().is_empty()
        || state.source_of_truth.trim().is_empty()
    {
        return Err(store_error(
            "state",
            "schema, issue, and identity are invalid",
        ));
    }
    validate_agent_state(state)
}

pub fn validate_mcp_agent_tool_ledger(ledger: &McpAgentToolLedger) -> Result<(), EngineError> {
    if ledger.schema != MCP_AGENT_LEDGER_SCHEMA
        || ledger.issue != 924
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
    state: McpAgentState,
) -> Result<McpAgentStateReadback, EngineError> {
    validate_mcp_agent_state(&state)?;
    Ok(McpAgentStateReadback {
        path: path.display().to_string(),
        bytes,
        sha256,
        state_fingerprint: stable_fingerprint(&state)?,
        binding_count: state.bindings.len(),
        prompt_model_count: state.prompt_models.len(),
        ward_envelope_count: state.ward_envelopes.len(),
        enabled_binding_count: state
            .bindings
            .iter()
            .filter(|binding| binding.enabled)
            .count(),
        state,
    })
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(store_error(field, "path must be non-empty"));
    }
    Ok(())
}

fn agent_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_AGENT_VAULT_ID,
        purpose: MCP_AGENT_VAULT_PURPOSE,
        collection: MCP_AGENT_COLLECTION,
        row_key: MCP_AGENT_STATE_ROW,
        column: MCP_AGENT_STATE_COLUMN,
    }
}

fn agent_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_AGENT_VAULT_ID,
        purpose: MCP_AGENT_VAULT_PURPOSE,
        collection: MCP_AGENT_COLLECTION,
        row_key: MCP_AGENT_LEDGER_ROW,
        column: MCP_AGENT_LEDGER_COLUMN,
    }
}

fn store_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.mcp_agent_tools.store.{}", field.into()),
        reason: reason.to_string(),
    }
}
