use litd_hash::sha256_hex;
use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

use crate::{
    history::stable_fingerprint,
    sound_editor::{
        SoundEditorRecord, SoundEditorRuntime, read_sound_editor_record_source,
        save_sound_editor_record,
    },
};

use super::invalid_mcp_trigger;

#[path = "mcp_trigger_tools_store_types.rs"]
mod mcp_trigger_tools_store_types;
pub use mcp_trigger_tools_store_types::{
    McpSoundRecordReadback, McpSoundRuntimeReadback, McpTriggerRuntimeState,
    McpTriggerStateReadback, McpTriggerToolLedger, McpTriggerToolLedgerReadback,
    McpTriggerToolRecord,
};

pub const MCP_TRIGGER_STATE_SCHEMA: &str = "litd.mcp.trigger_tools.state.v2";
pub const MCP_TRIGGER_LEDGER_SCHEMA: &str = "litd.mcp.trigger_tools.ledger.v1";
const MCP_TRIGGER_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC2";
const MCP_TRIGGER_VAULT_PURPOSE: &[u8] = b"litd-mcp-trigger-tools-issue1360";
const MCP_TRIGGER_COLLECTION: &str = "litd_mcp_trigger_tools";
const MCP_TRIGGER_STATE_ROW: &[u8] = b"trigger/state/latest";
const MCP_TRIGGER_RUNTIME_ROW: &[u8] = b"trigger/sound-runtime/latest";
const MCP_TRIGGER_LEDGER_ROW: &[u8] = b"trigger/ledger/latest";
const MCP_TRIGGER_STATE_COLUMN: &[u8] = b"state_json";
const MCP_TRIGGER_RUNTIME_COLUMN: &[u8] = b"runtime_json";
const MCP_TRIGGER_LEDGER_COLUMN: &[u8] = b"ledger_json";

impl McpTriggerRuntimeState {
    pub fn new(state_id: impl Into<String>) -> Result<Self, EngineError> {
        let state = Self {
            schema: MCP_TRIGGER_STATE_SCHEMA.to_string(),
            issue: 920,
            state_id: state_id.into(),
            source_of_truth: "Calyx Aster MCP trigger/runtime state row".to_string(),
            update_count: 0,
            variables: Vec::new(),
            graphs: Vec::new(),
            runtime_cues: Vec::new(),
        };
        validate_mcp_trigger_state(&state)?;
        Ok(state)
    }
}

pub fn load_or_create_mcp_trigger_state(
    path: impl AsRef<Path>,
    state_id: impl Into<String>,
) -> Result<McpTriggerRuntimeState, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let state_id = state_id.into();
    if path.exists() {
        let state = read_mcp_trigger_state(path)?.state;
        if state.state_id != state_id {
            return Err(store_error(
                "state_id",
                format!("{} contains a different state_id", path.display()),
            ));
        }
        Ok(state)
    } else {
        McpTriggerRuntimeState::new(state_id)
    }
}

pub fn write_mcp_trigger_state(
    path: impl AsRef<Path>,
    state: &McpTriggerRuntimeState,
) -> Result<McpTriggerStateReadback, EngineError> {
    validate_mcp_trigger_state(state)?;
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-trigger-state-json",
            reason: format!("serialize trigger MCP state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&trigger_state_spec(path), &bytes)
        .map_err(|error| store_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "state.write",
            "Aster trigger state bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_trigger_state(path)?;
    if readback.state != *state {
        return Err(store_error("state.readback", "state readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_trigger_state(
    path: impl AsRef<Path>,
) -> Result<McpTriggerStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let readback = read_calyx_json_bytes(&trigger_state_spec(path))
        .map_err(|error| store_error("state.read", error))?
        .ok_or_else(|| {
            store_error(
                "state.read",
                format!("missing Aster trigger state row: {}", path.display()),
            )
        })?;
    let (seq, bytes) = (readback.seq, readback.bytes);
    let state: McpTriggerRuntimeState =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-trigger-state-json",
            reason: format!("parse trigger MCP state {}: {error}", path.display()),
        })?;
    readback_from_state(path, bytes.len() as u64, seq, sha256_hex(&bytes), state)
}

pub fn load_mcp_sound_record_for_mutation(
    path: impl AsRef<Path>,
    initial_record: Option<SoundEditorRecord>,
) -> Result<SoundEditorRecord, EngineError> {
    let path = path.as_ref();
    validate_path("record_path", path)?;
    if path.exists() {
        return Ok(read_sound_editor_record_source(path)?.record);
    }
    let Some(record) = initial_record else {
        return Err(store_error(
            "sound_record.missing",
            format!(
                "{} does not exist and no explicit initial_record was supplied",
                path.display()
            ),
        ));
    };
    record.validate()?;
    Ok(record)
}

pub fn write_mcp_sound_record(
    path: impl AsRef<Path>,
    record: &SoundEditorRecord,
) -> Result<McpSoundRecordReadback, EngineError> {
    let path = path.as_ref();
    save_sound_editor_record(path, record)?;
    let readback = read_mcp_sound_record(path)?;
    if readback.record != *record {
        return Err(store_error(
            "sound_record.readback",
            "record readback mismatch",
        ));
    }
    Ok(readback)
}

pub fn read_mcp_sound_record(
    path: impl AsRef<Path>,
) -> Result<McpSoundRecordReadback, EngineError> {
    let path = path.as_ref();
    validate_path("record_path", path)?;
    let source = read_sound_editor_record_source(path)?;
    let bytes = source.bytes;
    let record = source.record;
    Ok(McpSoundRecordReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: source.sha256,
        record_fingerprint: record.stable_fingerprint()?,
        library_count: record.library.len(),
        named_sound_count: record.named_sounds.len(),
        trigger_binding_count: record.trigger_bindings.len(),
        record,
    })
}

pub fn write_mcp_sound_runtime(
    path: impl AsRef<Path>,
    runtime: &SoundEditorRuntime,
) -> Result<McpSoundRuntimeReadback, EngineError> {
    let path = path.as_ref();
    validate_path("runtime_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(runtime).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-sound-runtime-json",
            reason: format!("serialize sound runtime: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&trigger_runtime_spec(path), &bytes)
        .map_err(|error| store_error("runtime.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "runtime.write",
            "Aster sound runtime bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_sound_runtime(path)?;
    if readback.runtime != *runtime {
        return Err(store_error("runtime.readback", "runtime readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_sound_runtime(
    path: impl AsRef<Path>,
) -> Result<McpSoundRuntimeReadback, EngineError> {
    let path = path.as_ref();
    validate_path("runtime_path", path)?;
    let readback = read_calyx_json_bytes(&trigger_runtime_spec(path))
        .map_err(|error| store_error("runtime.read", error))?
        .ok_or_else(|| {
            store_error(
                "runtime.read",
                format!(
                    "missing Calyx Aster sound runtime row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let runtime: SoundEditorRuntime =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-sound-runtime-json",
            reason: format!("parse sound runtime {}: {error}", path.display()),
        })?;
    Ok(McpSoundRuntimeReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        library_row_count: runtime.library_rows.len(),
        resolved_trigger_binding_count: runtime.resolved_trigger_bindings.len(),
        preview_active: runtime.preview.is_some(),
        runtime,
    })
}

pub fn write_mcp_trigger_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpTriggerToolRecord,
) -> Result<McpTriggerToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    // Append existing records, reject a ledger_id mismatch, then rebuild (#1521/#1490).
    let mut records =
        match read_calyx_json_bytes(&trigger_ledger_spec(path))
            .map_err(|error| store_error("ledger.read", error))?
        {
            Some(readback) => {
                let existing: McpTriggerToolLedger = serde_json::from_slice(&readback.bytes)
                    .map_err(|error| EngineError::SerializationFailed {
                        format: "mcp-trigger-ledger-json",
                        reason: format!("parse trigger MCP ledger {}: {error}", path.display()),
                    })?;
                validate_mcp_trigger_tool_ledger(&existing)?;
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
    records.push(record);
    let ledger = build_ledger(ledger_id, records)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-trigger-ledger-json",
            reason: format!("serialize trigger MCP ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&trigger_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster trigger ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_trigger_tool_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error("ledger.readback", "ledger readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_trigger_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpTriggerToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&trigger_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!(
                    "missing Calyx Aster trigger ledger row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpTriggerToolLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-trigger-ledger-json",
            reason: format!("parse trigger MCP ledger {}: {error}", path.display()),
        })?;
    validate_mcp_trigger_tool_ledger(&ledger)?;
    Ok(McpTriggerToolLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn validate_mcp_trigger_state(state: &McpTriggerRuntimeState) -> Result<(), EngineError> {
    if state.schema != MCP_TRIGGER_STATE_SCHEMA
        || state.issue != 920
        || state.state_id.trim().is_empty()
        || state.source_of_truth.trim().is_empty()
    {
        return Err(store_error(
            "state",
            "schema, issue, and identity are invalid",
        ));
    }
    crate::trigger_wasm_authoring::validate_state_wasm_escapes(&state.graphs)?;
    Ok(())
}

pub fn validate_mcp_trigger_tool_ledger(ledger: &McpTriggerToolLedger) -> Result<(), EngineError> {
    if ledger.schema != MCP_TRIGGER_LEDGER_SCHEMA
        || ledger.issue != 920
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.record_count == 0
        || ledger.record_count != ledger.records.len()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and count are invalid",
        ));
    }
    for record in &ledger.records {
        if record.request_id.trim().is_empty()
            || record.requested_by.trim().is_empty()
            || record.tool_name.trim().is_empty()
            || record.operation_id.trim().is_empty()
            || record.primary_sha256.len() != 64
            || record.primary_bytes == 0
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
    seq: u64,
    sha256: String,
    state: McpTriggerRuntimeState,
) -> Result<McpTriggerStateReadback, EngineError> {
    validate_mcp_trigger_state(&state)?;
    Ok(McpTriggerStateReadback {
        path: path.display().to_string(),
        bytes,
        seq,
        sha256,
        state_fingerprint: stable_fingerprint(&state)?,
        variable_count: state.variables.len(),
        graph_count: state.graphs.len(),
        enabled_graph_count: state.graphs.iter().filter(|graph| graph.enabled).count(),
        runtime_cue_count: state.runtime_cues.len(),
        wasm_escape_count: state
            .graphs
            .iter()
            .filter(|graph| graph.wasm_escape.is_some())
            .count(),
        state,
    })
}

fn build_ledger(
    ledger_id: impl Into<String>,
    records: Vec<McpTriggerToolRecord>,
) -> Result<McpTriggerToolLedger, EngineError> {
    let ledger = McpTriggerToolLedger {
        schema: MCP_TRIGGER_LEDGER_SCHEMA.to_string(),
        issue: 920,
        ledger_id: ledger_id.into(),
        source_of_truth: "Calyx Aster MCP trigger tool ledger row + trigger/sound rows".to_string(),
        record_count: records.len(),
        records,
    };
    validate_mcp_trigger_tool_ledger(&ledger)?;
    Ok(ledger)
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(store_error(field, "path must be non-empty"));
    }
    if path.is_file() {
        return Err(store_error(
            field,
            format!(
                "{} is a file; expected a Calyx Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn trigger_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    trigger_spec(path, MCP_TRIGGER_STATE_ROW, MCP_TRIGGER_STATE_COLUMN)
}

fn trigger_runtime_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    trigger_spec(path, MCP_TRIGGER_RUNTIME_ROW, MCP_TRIGGER_RUNTIME_COLUMN)
}

fn trigger_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    trigger_spec(path, MCP_TRIGGER_LEDGER_ROW, MCP_TRIGGER_LEDGER_COLUMN)
}

fn trigger_spec<'a>(path: &'a Path, row_key: &'a [u8], column: &'a [u8]) -> CalyxJsonStoreSpec<'a> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_TRIGGER_VAULT_ID,
        purpose: MCP_TRIGGER_VAULT_PURPOSE,
        collection: MCP_TRIGGER_COLLECTION,
        row_key,
        column,
    }
}

fn store_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    invalid_mcp_trigger(format!("store.{}", field.into()), reason)
}
