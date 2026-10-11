use litd_hash::sha256_hex;
use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use serde::{Deserialize, Serialize};

use crate::history::{EditorDocument, stable_fingerprint};

use super::invalid_mcp_world;

pub const MCP_WORLD_LEDGER_SCHEMA: &str = "litd.mcp.world_tools.ledger.v1";
const MCP_WORLD_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC9";
const MCP_WORLD_VAULT_PURPOSE: &[u8] = b"litd-mcp-world-tools-issue1363";
const MCP_WORLD_COLLECTION: &str = "litd_mcp_world_tools";
const MCP_WORLD_DOCUMENT_ROW: &[u8] = b"world/document/latest";
const MCP_WORLD_LEDGER_ROW: &[u8] = b"world/ledger/latest";
const MCP_WORLD_DOCUMENT_COLUMN: &[u8] = b"document_json";
const MCP_WORLD_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpWorldDocumentReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub document_fingerprint: String,
    pub document: EditorDocument,
    pub terrain_cell_count: usize,
    pub terrain_stamp_count: usize,
    pub object_count: usize,
    pub region_count: usize,
    pub named_camera_count: usize,
    pub pathing_override_count: usize,
    pub tile_pathing_rule_count: usize,
    pub start_location_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpWorldToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpWorldToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpWorldToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub document_path: String,
    pub document_sha256: String,
    pub document_bytes: u64,
    pub document_fingerprint: String,
    pub action_summary: String,
    pub terrain_cell_count: usize,
    pub terrain_stamp_count: usize,
    pub object_count: usize,
    pub region_count: usize,
    pub pathing_override_count: usize,
    pub start_location_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpWorldToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpWorldToolLedger,
}

pub fn load_mcp_world_document_for_mutation(
    path: impl AsRef<Path>,
    initial_document: Option<EditorDocument>,
) -> Result<EditorDocument, EngineError> {
    let path = path.as_ref();
    validate_path("document_path", path)?;
    if path.exists() {
        return Ok(read_mcp_world_document(path)?.document);
    }
    let Some(document) = initial_document else {
        return Err(store_error(
            "document.missing",
            format!(
                "{} does not exist and no explicit initial_document was supplied",
                path.display()
            ),
        ));
    };
    document.validate()?;
    Ok(document)
}

pub fn write_mcp_world_document(
    path: impl AsRef<Path>,
    document: &EditorDocument,
) -> Result<McpWorldDocumentReadback, EngineError> {
    document.validate()?;
    let path = path.as_ref();
    validate_path("document_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(document).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-world-document-json",
            reason: format!("serialize world MCP document: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&world_document_spec(path), &bytes)
        .map_err(|error| store_error("document.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "document.write",
            "Aster world document bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_world_document(path)?;
    if readback.document != *document {
        return Err(store_error(
            "document.readback",
            format!("{} readback did not match written document", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_mcp_world_document(
    path: impl AsRef<Path>,
) -> Result<McpWorldDocumentReadback, EngineError> {
    let path = path.as_ref();
    validate_path("document_path", path)?;
    let readback = read_calyx_json_bytes(&world_document_spec(path))
        .map_err(|error| store_error("document.read", error))?
        .ok_or_else(|| {
            store_error(
                "document.read",
                format!(
                    "missing Calyx Aster world document row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let document: EditorDocument =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-world-document-json",
            reason: format!("parse world MCP document {}: {error}", path.display()),
        })?;
    document.validate()?;
    readback_from_document(path, bytes.len() as u64, sha256_hex(&bytes), document)
}

pub fn write_mcp_world_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpWorldToolRecord,
) -> Result<McpWorldToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    // Append to the existing ledger instead of overwriting it (#1521, same fix as #1490):
    // read the current records, reject a ledger_id mismatch, push the new record, rebuild.
    let mut records =
        match read_calyx_json_bytes(&world_ledger_spec(path))
            .map_err(|error| store_error("ledger.read", error))?
        {
            Some(readback) => {
                let existing: McpWorldToolLedger = serde_json::from_slice(&readback.bytes)
                    .map_err(|error| EngineError::SerializationFailed {
                        format: "mcp-world-ledger-json",
                        reason: format!("parse world MCP ledger {}: {error}", path.display()),
                    })?;
                validate_mcp_world_tool_ledger(&existing)?;
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
    let ledger = build_mcp_world_tool_ledger(ledger_id, records)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-world-ledger-json",
            reason: format!("serialize world MCP ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&world_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster world ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_world_tool_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error(
            "ledger.readback",
            format!("{} readback did not match written ledger", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_mcp_world_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpWorldToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&world_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!("missing Calyx Aster world ledger row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpWorldToolLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-world-ledger-json",
            reason: format!("parse world MCP ledger {}: {error}", path.display()),
        })?;
    validate_mcp_world_tool_ledger(&ledger)?;
    Ok(McpWorldToolLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn validate_mcp_world_tool_ledger(ledger: &McpWorldToolLedger) -> Result<(), EngineError> {
    validate_single_line("ledger_id", &ledger.ledger_id)?;
    if ledger.schema != MCP_WORLD_LEDGER_SCHEMA
        || ledger.issue != 918
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

fn readback_from_document(
    path: &Path,
    bytes: u64,
    sha256: String,
    document: EditorDocument,
) -> Result<McpWorldDocumentReadback, EngineError> {
    let start_location_count = document
        .objects
        .values()
        .filter(|object| object.kind == "StartLocation")
        .count();
    Ok(McpWorldDocumentReadback {
        path: path.display().to_string(),
        bytes,
        sha256,
        document_fingerprint: stable_fingerprint(&document)?,
        terrain_cell_count: document.terrain_cells.len(),
        terrain_stamp_count: document.terrain_stamps.len(),
        object_count: document.objects.len(),
        region_count: document.regions.len(),
        named_camera_count: document.named_cameras.len(),
        pathing_override_count: document.terrain_pathing_overrides.len(),
        tile_pathing_rule_count: document.terrain_tile_pathing_rules.len(),
        start_location_count,
        document,
    })
}

fn build_mcp_world_tool_ledger(
    ledger_id: impl Into<String>,
    records: Vec<McpWorldToolRecord>,
) -> Result<McpWorldToolLedger, EngineError> {
    let ledger = McpWorldToolLedger {
        schema: MCP_WORLD_LEDGER_SCHEMA.to_string(),
        issue: 918,
        ledger_id: ledger_id.into(),
        source_of_truth: "Calyx Aster MCP world tool ledger row + EditorDocument row".to_string(),
        record_count: records.len(),
        records,
    };
    validate_mcp_world_tool_ledger(&ledger)?;
    Ok(ledger)
}

fn validate_record(record: &McpWorldToolRecord) -> Result<(), EngineError> {
    validate_single_line("record.request_id", &record.request_id)?;
    validate_single_line("record.requested_by", &record.requested_by)?;
    validate_single_line("record.document_path", &record.document_path)?;
    validate_single_line("record.document_fingerprint", &record.document_fingerprint)?;
    validate_single_line("record.action_summary", &record.action_summary)?;
    if !record.tool_name.starts_with("litd.editor.world.")
        || !record.operation_id.starts_with("editor.world.")
        || record.document_sha256.len() != 64
        || record.document_bytes == 0
        || record.terrain_cell_count == 0
    {
        return Err(store_error(
            "record",
            "tool, document hash, bytes, and terrain counts must be concrete",
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
    Ok(())
}

fn world_document_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_WORLD_VAULT_ID,
        purpose: MCP_WORLD_VAULT_PURPOSE,
        collection: MCP_WORLD_COLLECTION,
        row_key: MCP_WORLD_DOCUMENT_ROW,
        column: MCP_WORLD_DOCUMENT_COLUMN,
    }
}

fn world_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_WORLD_VAULT_ID,
        purpose: MCP_WORLD_VAULT_PURPOSE,
        collection: MCP_WORLD_COLLECTION,
        row_key: MCP_WORLD_LEDGER_ROW,
        column: MCP_WORLD_LEDGER_COLUMN,
    }
}

fn store_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    invalid_mcp_world(format!("store.{}", field.into()), reason)
}
