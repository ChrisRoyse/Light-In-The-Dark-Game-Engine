use litd_hash::sha256_hex;
use std::path::Path;

use litd_engine::EngineError;

use crate::history::EditorDocument;

use super::invalid_mcp_object;

#[path = "mcp_object_tools_store_aster.rs"]
mod mcp_object_tools_store_aster;
#[path = "mcp_object_tools_store_ingest.rs"]
mod mcp_object_tools_store_ingest;
#[path = "mcp_object_tools_store_records.rs"]
mod mcp_object_tools_store_records;
#[path = "mcp_object_tools_store_types.rs"]
mod mcp_object_tools_store_types;
#[path = "mcp_object_tools_store_validation.rs"]
mod mcp_object_tools_store_validation;

use mcp_object_tools_store_aster::{
    McpObjectAsterStore, object_document_exists, readback_from_document,
};
pub use mcp_object_tools_store_types::{
    McpObjectDocumentReadback, McpObjectReactiveNotification, McpObjectToolLedger,
    McpObjectToolLedgerReadback, McpObjectToolRecord,
};
pub use mcp_object_tools_store_validation::validate_mcp_object_tool_ledger;
use mcp_object_tools_store_validation::validate_path;

pub const MCP_OBJECT_LEDGER_SCHEMA: &str = "litd.mcp.object_tools.ledger.v1";

const OBJECT_STORE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAY";
const OBJECT_STORE_VAULT_PURPOSE: &[u8] = b"litd-editor-object-tools-issue1358";
const DOCUMENT_KEY: &[u8] = b"mcp/object/document/latest";
const OBJECT_ROW_PREFIX: &[u8] = b"mcp/object/entity/";
const LEDGER_LATEST_KEY: &[u8] = b"mcp/object/ledger/latest";
const LEDGER_ROW_PREFIX: &[u8] = b"mcp/object/ledger/by-id/";

pub fn load_mcp_object_document_for_mutation(
    path: impl AsRef<Path>,
    initial_document: Option<EditorDocument>,
) -> Result<EditorDocument, EngineError> {
    let path = path.as_ref();
    validate_path("document_path", path)?;
    if object_document_exists(path)? {
        return Ok(read_mcp_object_document(path)?.document);
    }
    let Some(document) = initial_document else {
        return Err(store_error(
            "document.missing",
            format!(
                "{} does not exist in Calyx Aster and no explicit initial_document was supplied",
                path.display()
            ),
        ));
    };
    document.validate()?;
    Ok(document)
}

pub fn write_mcp_object_document(
    path: impl AsRef<Path>,
    document: &EditorDocument,
) -> Result<McpObjectDocumentReadback, EngineError> {
    document.validate()?;
    let path = path.as_ref();
    validate_path("document_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(document).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-object-document-aster",
            reason: format!("serialize object MCP document: {error}"),
        })?;
    let store = McpObjectAsterStore::new(path);
    let structured_ingest = store.ingest_object_constellations(document)?;
    store.put_document_bytes(document, &bytes, structured_ingest)?;
    let readback = read_mcp_object_document(path)?;
    if readback.document != *document {
        return Err(store_error(
            "document.readback",
            format!("{} readback did not match written document", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_mcp_object_document(
    path: impl AsRef<Path>,
) -> Result<McpObjectDocumentReadback, EngineError> {
    let path = path.as_ref();
    validate_path("document_path", path)?;
    let read = McpObjectAsterStore::new(path).read_document_bytes()?;
    let document: EditorDocument =
        serde_json::from_slice(&read.bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-object-document-aster",
            reason: format!(
                "parse object MCP document from Aster {}: {error}",
                path.display()
            ),
        })?;
    document.validate()?;
    readback_from_document(path, &read, document)
}

pub fn write_mcp_object_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpObjectToolRecord,
) -> Result<McpObjectToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    // Append to the existing ledger instead of overwriting it (#1521, same fix as #1490):
    // read the current records, reject a ledger_id mismatch, push the new record, rebuild.
    let store = McpObjectAsterStore::new(path);
    let mut records = match store.read_latest_ledger_bytes_optional()? {
        Some(read) => {
            let existing: McpObjectToolLedger =
                serde_json::from_slice(&read.bytes).map_err(|error| {
                    EngineError::SerializationFailed {
                        format: "mcp-object-ledger-aster",
                        reason: format!(
                            "parse object MCP ledger from Aster {}: {error}",
                            path.display()
                        ),
                    }
                })?;
            validate_mcp_object_tool_ledger(&existing)?;
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
    let ledger = build_mcp_object_tool_ledger(ledger_id, records)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-object-ledger-aster",
            reason: format!("serialize object MCP ledger: {error}"),
        })?;
    store.put_ledger_bytes(&ledger.ledger_id, &bytes)?;
    let readback = read_mcp_object_tool_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error(
            "ledger.readback",
            format!("{} readback did not match written ledger", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_mcp_object_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpObjectToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let read = McpObjectAsterStore::new(path).read_latest_ledger_bytes()?;
    let ledger: McpObjectToolLedger =
        serde_json::from_slice(&read.bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-object-ledger-aster",
            reason: format!(
                "parse object MCP ledger from Aster {}: {error}",
                path.display()
            ),
        })?;
    validate_mcp_object_tool_ledger(&ledger)?;
    Ok(McpObjectToolLedgerReadback {
        path: path.display().to_string(),
        vault_dir: read.vault_dir,
        cf: read.cf,
        key_hex: read.key_hex,
        seq: read.seq,
        graph_rows: read.graph_rows,
        bytes: read.bytes.len() as u64,
        sha256: sha256_hex(&read.bytes),
        ledger,
    })
}

fn build_mcp_object_tool_ledger(
    ledger_id: impl Into<String>,
    records: Vec<McpObjectToolRecord>,
) -> Result<McpObjectToolLedger, EngineError> {
    let ledger = McpObjectToolLedger {
        schema: MCP_OBJECT_LEDGER_SCHEMA.to_string(),
        issue: 919,
        ledger_id: ledger_id.into(),
        source_of_truth: "Calyx Aster Graph CF object tool ledger rows".to_string(),
        record_count: records.len(),
        records,
    };
    validate_mcp_object_tool_ledger(&ledger)?;
    Ok(ledger)
}

fn store_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    invalid_mcp_object(format!("store.{}", field.into()), reason)
}
