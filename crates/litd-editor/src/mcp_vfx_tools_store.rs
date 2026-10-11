use litd_hash::sha256_hex;
use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
    render::vfx::{VfxEffectDescriptor, VfxRenderCue, validate_effect_descriptor},
};
use serde::{Deserialize, Serialize};

pub const VFX_MCP_LEDGER_SCHEMA: &str = "litd.mcp.vfx_tools.ledger.v1";
const VFX_MCP_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC3";
const VFX_MCP_VAULT_PURPOSE: &[u8] = b"litd-mcp-vfx-tools-issue1360";
const VFX_MCP_COLLECTION: &str = "litd_mcp_vfx_tools";
const VFX_MCP_LEDGER_ROW: &[u8] = b"vfx/mcp/ledger/latest";
const VFX_MCP_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpVfxToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpVfxToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpVfxToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub effect_id: String,
    pub prompt: Option<String>,
    pub descriptor: VfxEffectDescriptor,
    pub content_object_path: String,
    pub content_object_sha256: String,
    pub content_object_bytes: u64,
    pub cue: Option<VfxRenderCue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpVfxToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpVfxToolLedger,
}

pub fn write_mcp_vfx_tool_ledger(
    path: impl AsRef<Path>,
    ledger: &McpVfxToolLedger,
) -> Result<McpVfxToolLedgerReadback, EngineError> {
    validate_mcp_vfx_tool_ledger(ledger)?;
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-vfx-ledger-json",
            reason: format!("serialize VFX MCP ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&vfx_mcp_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster VFX MCP ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_vfx_tool_ledger(path)?;
    if readback.ledger != *ledger {
        return Err(store_error(
            "ledger.readback",
            format!("{} readback did not match written ledger", path.display()),
        ));
    }
    Ok(readback)
}

/// Append a record to the VFX tool ledger at `path` (#1521). Reads the current ledger
/// (if any), rejects a `ledger_id` mismatch, pushes the record, and persists the full
/// history. Callers must use this instead of `build_mcp_vfx_tool_ledger(id, vec![record])`
/// + `write_mcp_vfx_tool_ledger`, which overwrote the row and kept only the latest record.
pub fn append_mcp_vfx_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpVfxToolRecord,
) -> Result<McpVfxToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    let mut records = match read_calyx_json_bytes(&vfx_mcp_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
    {
        Some(readback) => {
            let existing: McpVfxToolLedger =
                serde_json::from_slice(&readback.bytes).map_err(|error| {
                    EngineError::SerializationFailed {
                        format: "mcp-vfx-ledger-json",
                        reason: format!("parse VFX MCP ledger {}: {error}", path.display()),
                    }
                })?;
            validate_mcp_vfx_tool_ledger(&existing)?;
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
    let ledger = build_mcp_vfx_tool_ledger(ledger_id, records)?;
    write_mcp_vfx_tool_ledger(path, &ledger)
}

pub fn read_mcp_vfx_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpVfxToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&vfx_mcp_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!(
                    "missing Calyx Aster VFX MCP ledger row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpVfxToolLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-vfx-ledger-json",
            reason: format!("parse VFX MCP ledger {}: {error}", path.display()),
        })?;
    validate_mcp_vfx_tool_ledger(&ledger)?;
    Ok(McpVfxToolLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn build_mcp_vfx_tool_ledger(
    ledger_id: impl Into<String>,
    records: Vec<McpVfxToolRecord>,
) -> Result<McpVfxToolLedger, EngineError> {
    let ledger = McpVfxToolLedger {
        schema: VFX_MCP_LEDGER_SCHEMA.to_string(),
        issue: 922,
        ledger_id: ledger_id.into(),
        source_of_truth: "Calyx Aster MCP VFX tool ledger row + content-addressed VFX object bytes"
            .to_string(),
        record_count: records.len(),
        records,
    };
    validate_mcp_vfx_tool_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_mcp_vfx_tool_ledger(ledger: &McpVfxToolLedger) -> Result<(), EngineError> {
    validate_single_line("ledger_id", &ledger.ledger_id)?;
    if ledger.schema != VFX_MCP_LEDGER_SCHEMA
        || ledger.issue != 922
        || ledger.record_count == 0
        || ledger.record_count != ledger.records.len()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and record count are invalid",
        ));
    }
    for record in &ledger.records {
        validate_record(record)?;
    }
    Ok(())
}

fn validate_record(record: &McpVfxToolRecord) -> Result<(), EngineError> {
    validate_single_line("record.request_id", &record.request_id)?;
    validate_single_line("record.requested_by", &record.requested_by)?;
    if !record.tool_name.starts_with("litd.engine.vfx.")
        || !record.operation_id.starts_with("engine.vfx.")
        || record.effect_id != record.descriptor.effect_id
        || record.content_object_sha256 != validate_effect_descriptor(&record.descriptor)?
        || record.content_object_bytes != record.descriptor.size
        || record.content_object_path.trim().is_empty()
    {
        return Err(store_error(
            "record",
            "tool, descriptor, and content-object readback must agree",
        ));
    }
    if let Some(cue) = &record.cue
        && cue.effect != record.descriptor
    {
        return Err(store_error(
            "record.cue",
            "attached cue descriptor must match ledger descriptor",
        ));
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(store_error(
            field,
            "value must be non-empty single-line text",
        ));
    }
    Ok(())
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

fn vfx_mcp_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: VFX_MCP_VAULT_ID,
        purpose: VFX_MCP_VAULT_PURPOSE,
        collection: VFX_MCP_COLLECTION,
        row_key: VFX_MCP_LEDGER_ROW,
        column: VFX_MCP_LEDGER_COLUMN,
    }
}

fn store_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.mcp_vfx_tools.store.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_vfx_tools::store",
        field = %field,
        reason = %reason,
        "invalid VFX MCP tool ledger"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
