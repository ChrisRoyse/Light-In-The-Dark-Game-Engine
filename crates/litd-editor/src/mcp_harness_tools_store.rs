use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

use super::{
    HarnessAssertionVerdict, HarnessBotDriveRecord, HarnessCalyxSignalsRecord, HarnessEventRecord,
    HarnessOracleRunRecord, HarnessPevRunRecord, HarnessPevTranscriptRecord,
    HarnessPevTranscriptTurn, HarnessRunRecord, HarnessScreenshotDiffRecord,
    HarnessSnapshotBranchRecord,
    mcp_harness_tools_bot_validation::validate_bot_drive_record,
    mcp_harness_tools_calyx_signals::validate_calyx_signals_record,
    mcp_harness_tools_feedback::validate_feedback_stream,
    mcp_harness_tools_oracles::validate_oracle_run_record,
    mcp_harness_tools_run_support::validate_run_record,
    mcp_harness_tools_sandbox::{HarnessSandboxBatchRecord, validate_sandbox_batch_record},
    mcp_harness_tools_visual::validate_screenshot_diff_record,
};

#[path = "mcp_harness_tools_store_aster.rs"]
mod aster;
#[path = "mcp_harness_tools_store_validation.rs"]
mod validation;

pub use aster::read_mcp_harness_aster_ledger;
use aster::write_aster_ledger;
pub use validation::{validate_mcp_harness_ledger, validate_mcp_harness_state};

pub const MCP_HARNESS_STATE_SCHEMA: &str = "litd.mcp.agent_harness.state.v9";
pub const MCP_HARNESS_LEDGER_SCHEMA: &str = "litd.mcp.agent_harness.ledger.v9";

const HARNESS_ASTER_COLLECTION: &str = "litd_agent_harness";
const HARNESS_LEDGER_COLUMN: &[u8] = b"agent_harness_ledger_json";
const HARNESS_STATE_COLUMN: &[u8] = b"agent_harness_state_json";
const HARNESS_STATE_ROW: &[u8] = b"agent_harness/state/latest";
const HARNESS_LATEST_LEDGER_ROW: &[u8] = b"agent_harness/ledger/latest";
const HARNESS_ROW_PREFIX: &str = "agent_harness/ledger/";
const HARNESS_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC1";
const HARNESS_VAULT_PURPOSE: &[u8] = b"litd-agent-harness-issue1178";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpHarnessState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub update_count: u64,
    pub runs: Vec<HarnessRunRecord>,
    #[serde(default)]
    pub sandbox_batches: Vec<HarnessSandboxBatchRecord>,
    pub snapshots: Vec<HarnessSnapshotBranchRecord>,
    pub assertions: Vec<HarnessAssertionVerdict>,
    pub oracle_runs: Vec<HarnessOracleRunRecord>,
    pub bot_runs: Vec<HarnessBotDriveRecord>,
    pub screenshot_diffs: Vec<HarnessScreenshotDiffRecord>,
    pub calyx_signals: Vec<HarnessCalyxSignalsRecord>,
    pub pev_runs: Vec<HarnessPevRunRecord>,
    pub pev_transcripts: Vec<HarnessPevTranscriptRecord>,
    pub events: Vec<HarnessEventRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpHarnessStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state_fingerprint: String,
    pub state: McpHarnessState,
    pub run_count: usize,
    pub assertion_count: usize,
    pub oracle_run_count: usize,
    pub pev_transcript_count: usize,
    pub event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpHarnessLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpHarnessLedgerRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpHarnessLedgerRecord {
    pub event: HarnessEventRecord,
    pub state_path: String,
    pub state_sha256: String,
    pub state_bytes: u64,
    pub state_fingerprint: String,
    #[serde(default)]
    pub transcript_turns: Vec<HarnessPevTranscriptTurn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpHarnessLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpHarnessLedger,
    pub aster: HarnessAsterReadback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessAsterReadback {
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub bytes: u64,
}

impl McpHarnessState {
    pub fn new(state_id: impl Into<String>) -> Result<Self, EngineError> {
        let state = Self {
            schema: MCP_HARNESS_STATE_SCHEMA.to_string(),
            issue: 1178,
            state_id: state_id.into(),
            source_of_truth: "Calyx Aster MCP agent harness state row + ledger rows".to_string(),
            update_count: 0,
            runs: Vec::new(),
            sandbox_batches: Vec::new(),
            snapshots: Vec::new(),
            assertions: Vec::new(),
            oracle_runs: Vec::new(),
            bot_runs: Vec::new(),
            screenshot_diffs: Vec::new(),
            calyx_signals: Vec::new(),
            pev_runs: Vec::new(),
            pev_transcripts: Vec::new(),
            events: Vec::new(),
        };
        validate_mcp_harness_state(&state)?;
        Ok(state)
    }
}

pub fn load_or_create_mcp_harness_state(
    path: impl AsRef<Path>,
    state_id: impl Into<String>,
) -> Result<McpHarnessState, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let state_id = state_id.into();
    if path.exists() {
        let state = read_mcp_harness_state(path)?.state;
        if state.state_id != state_id {
            return Err(store_error("state_id", "state_id mismatch"));
        }
        Ok(state)
    } else {
        McpHarnessState::new(state_id)
    }
}

pub fn write_mcp_harness_state(
    path: impl AsRef<Path>,
    state: &McpHarnessState,
) -> Result<McpHarnessStateReadback, EngineError> {
    validate_mcp_harness_state(state)?;
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let bytes = serialize(state, "mcp-harness-state-json")?;
    let stored = write_calyx_json_bytes(&harness_state_spec(path), &bytes)
        .map_err(|error| store_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "state.write",
            "Aster harness state bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_harness_state(path)?;
    if readback.state != *state {
        return Err(store_error("state.readback", "state readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_harness_state(
    path: impl AsRef<Path>,
) -> Result<McpHarnessStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let readback = read_calyx_json_bytes(&harness_state_spec(path))
        .map_err(|error| store_error("state.read", error))?
        .ok_or_else(|| {
            store_error(
                "state.read",
                format!(
                    "missing Calyx Aster harness state row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let state: McpHarnessState =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-harness-state-json",
            reason: format!("parse harness state {}: {error}", path.display()),
        })?;
    readback_from_state(path, bytes.len() as u64, sha256_hex(&bytes), state)
}

pub fn write_mcp_harness_ledger(
    path: impl AsRef<Path>,
    aster_dir: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    event: HarnessEventRecord,
    state_readback: &McpHarnessStateReadback,
    transcript_turns: Vec<HarnessPevTranscriptTurn>,
) -> Result<McpHarnessLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    let new_record = McpHarnessLedgerRecord {
        event,
        state_path: state_readback.path.clone(),
        state_sha256: state_readback.sha256.clone(),
        state_bytes: state_readback.bytes,
        state_fingerprint: state_readback.state_fingerprint.clone(),
        transcript_turns,
    };
    let mut records = match read_calyx_json_bytes(&harness_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
    {
        Some(readback) => {
            let existing: McpHarnessLedger =
                serde_json::from_slice(&readback.bytes).map_err(|error| {
                    EngineError::SerializationFailed {
                        format: "mcp-harness-ledger-json",
                        reason: format!("parse harness ledger {}: {error}", path.display()),
                    }
                })?;
            validate_mcp_harness_ledger(&existing)?;
            if existing.ledger_id != ledger_id {
                return Err(store_error(
                    "ledger.ledger_id",
                    format!(
                        "ledger_id mismatch: existing {}, requested {ledger_id}",
                        existing.ledger_id
                    ),
                ));
            }
            existing.records
        }
        None => Vec::new(),
    };
    records.push(new_record);
    let ledger = McpHarnessLedger {
        schema: MCP_HARNESS_LEDGER_SCHEMA.to_string(),
        issue: 1178,
        ledger_id,
        source_of_truth: "Calyx Aster agent harness ledger row + latest ledger row".to_string(),
        record_count: records.len(),
        records,
    };
    validate_mcp_harness_ledger(&ledger)?;
    let bytes = serialize(&ledger, "mcp-harness-ledger-json")?;
    let aster = write_aster_ledger(aster_dir.as_ref(), &ledger.ledger_id, &bytes)?;
    let stored = write_calyx_json_bytes(&harness_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster harness latest ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_harness_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error("ledger.readback", "ledger readback mismatch"));
    }
    Ok(McpHarnessLedgerReadback { aster, ..readback })
}

pub fn read_mcp_harness_ledger(
    path: impl AsRef<Path>,
) -> Result<McpHarnessLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&harness_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!(
                    "missing Calyx Aster harness latest ledger row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpHarnessLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-harness-ledger-json",
            reason: format!("parse harness ledger {}: {error}", path.display()),
        })?;
    validate_mcp_harness_ledger(&ledger)?;
    Ok(McpHarnessLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
        aster: HarnessAsterReadback {
            seq: readback.seq,
            graph_rows: readback.graph_rows,
            cell_key: readback.cell_key,
            index_key: readback.index_key,
            sha256: sha256_hex(&bytes),
            bytes: bytes.len() as u64,
        },
    })
}

fn readback_from_state(
    path: &Path,
    bytes: u64,
    sha256: String,
    state: McpHarnessState,
) -> Result<McpHarnessStateReadback, EngineError> {
    validate_mcp_harness_state(&state)?;
    Ok(McpHarnessStateReadback {
        path: path.display().to_string(),
        bytes,
        sha256,
        state_fingerprint: stable_fingerprint(&state)?,
        run_count: state.runs.len(),
        assertion_count: state.assertions.len(),
        oracle_run_count: state.oracle_runs.len(),
        pev_transcript_count: state.pev_transcripts.len(),
        event_count: state.events.len(),
        state,
    })
}

fn serialize<T: Serialize>(value: &T, format: &'static str) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(value).map_err(|error| EngineError::SerializationFailed {
        format,
        reason: format!("serialize harness artifact: {error}"),
    })
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(store_error(field, "path must be non-empty"));
    }
    Ok(())
}

fn harness_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: HARNESS_VAULT_ID,
        purpose: HARNESS_VAULT_PURPOSE,
        collection: HARNESS_ASTER_COLLECTION,
        row_key: HARNESS_STATE_ROW,
        column: HARNESS_STATE_COLUMN,
    }
}

fn harness_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: HARNESS_VAULT_ID,
        purpose: HARNESS_VAULT_PURPOSE,
        collection: HARNESS_ASTER_COLLECTION,
        row_key: HARNESS_LATEST_LEDGER_ROW,
        column: HARNESS_LEDGER_COLUMN,
    }
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(store_error(
            field,
            "value must be non-empty and single-line",
        ));
    }
    Ok(())
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.mcp_harness_tools.store.{}", field.into()),
        reason: reason.to_string(),
    }
}
