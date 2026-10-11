//! #901: MCP tools for the template gallery -- browse and one-click clone.
//!
//! `list_templates` returns the browsable gallery cards. `clone_template` deep-copies
//! a starter into a FRESH editable project through Calyx persistence: it materializes
//! the template to its own Aster vault (idempotent), reads it back through an
//! independent handle (source of truth), assigns the clone a brand-new Calyx identity,
//! and persists the clone to a SEPARATE destination vault. The clone shares no mutable
//! state with the template (distinct vault dirs, distinct identities), and cloning into
//! a destination that already exists is a typed name-collision error.

use std::path::{Path, PathBuf};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::history::EditorDocument;
use crate::mcp_world_tools::{
    McpWorldDocumentReadback, McpWorldToolLedgerReadback, McpWorldToolRecord,
    read_mcp_world_document, write_mcp_world_document, write_mcp_world_tool_ledger,
};
use crate::template_gallery::{
    TemplateSummary, build_template_document, clone_template_document, list_templates,
};

pub const LIST_TEMPLATES_TOOL: &str = "litd.editor.world.list_templates";
pub const CLONE_TEMPLATE_TOOL: &str = "litd.editor.world.clone_template";

const CLONE_TEMPLATE_OP: &str = "editor.world.clone_template";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListTemplatesArgs {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloneTemplateArgs {
    pub template_id: String,
    /// Vault dir the template source-of-truth is materialized to (idempotent).
    pub template_path: PathBuf,
    /// Destination project vault dir for the clone. Must NOT already exist.
    pub clone_path: PathBuf,
    pub clone_calyx_document_id: String,
    pub clone_revision_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTemplateToolOutput {
    pub tool_name: String,
    pub source_of_truth: String,
    pub result: Value,
    pub template_readback: Option<McpWorldDocumentReadback>,
    pub clone_readback: Option<McpWorldDocumentReadback>,
    pub ledger_readback: Option<McpWorldToolLedgerReadback>,
}

pub fn call_mcp_template_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpTemplateToolOutput, EngineError> {
    match tool_name {
        LIST_TEMPLATES_TOOL => list_templates_tool(arguments),
        CLONE_TEMPLATE_TOOL => clone_template_tool(arguments),
        other => Err(invalid_template_tool(
            "tool_name",
            format!("{other} is not a registered template MCP tool"),
        )),
    }
}

fn list_templates_tool(arguments: Value) -> Result<McpTemplateToolOutput, EngineError> {
    let _args: ListTemplatesArgs = parse_args(LIST_TEMPLATES_TOOL, arguments)?;
    let summaries: Vec<TemplateSummary> = list_templates()?;
    Ok(McpTemplateToolOutput {
        tool_name: LIST_TEMPLATES_TOOL.to_string(),
        source_of_truth: "editor template gallery (authored via real editor APIs)".to_string(),
        result: serde_json::json!({
            "template_count": summaries.len(),
            "templates": summaries,
        }),
        template_readback: None,
        clone_readback: None,
        ledger_readback: None,
    })
}

fn clone_template_tool(arguments: Value) -> Result<McpTemplateToolOutput, EngineError> {
    let args: CloneTemplateArgs = parse_args(CLONE_TEMPLATE_TOOL, arguments)?;
    validate_path("template_path", &args.template_path)?;
    validate_path("clone_path", &args.clone_path)?;
    validate_path("ledger_path", &args.ledger_path)?;
    validate_single_line("request_id", &args.request_id)?;
    validate_single_line("requested_by", &args.requested_by)?;
    // Name collision: refuse to clone over an existing destination project (typed error).
    if args.clone_path.exists() {
        return Err(invalid_template_tool(
            "clone_path",
            format!(
                "destination project {} already exists; refusing to overwrite",
                args.clone_path.display()
            ),
        ));
    }
    // Materialize the template source-of-truth (idempotent), then read it back through an
    // independent handle so the clone is a copy of the PERSISTED bytes, not an in-memory object.
    let template_readback = materialize_template(&args.template_id, &args.template_path)?;
    let clone = clone_template_document(
        &template_readback.document,
        &args.clone_calyx_document_id,
        &args.clone_revision_id,
    )?;
    let clone_readback = write_mcp_world_document(&args.clone_path, &clone)?;
    let record = build_record(&args, &clone_readback)?;
    let ledger_readback = write_mcp_world_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(McpTemplateToolOutput {
        tool_name: CLONE_TEMPLATE_TOOL.to_string(),
        source_of_truth: "Calyx Aster template vault -> fresh-identity clone vault".to_string(),
        result: serde_json::json!({
            "template_id": args.template_id,
            "template_fingerprint": template_readback.document_fingerprint,
            "clone_fingerprint": clone_readback.document_fingerprint,
            "clone_calyx_document_id": args.clone_calyx_document_id,
        }),
        template_readback: Some(template_readback),
        clone_readback: Some(clone_readback),
        ledger_readback: Some(ledger_readback),
    })
}

fn materialize_template(
    template_id: &str,
    template_path: &Path,
) -> Result<McpWorldDocumentReadback, EngineError> {
    if template_path.exists() {
        return read_mcp_world_document(template_path);
    }
    let document = build_template_document(template_id)?;
    write_mcp_world_document(template_path, &document)
}

fn build_record(
    args: &CloneTemplateArgs,
    readback: &McpWorldDocumentReadback,
) -> Result<McpWorldToolRecord, EngineError> {
    let start_location_count = readback
        .document
        .objects
        .values()
        .filter(|object| object.kind == "StartLocation")
        .count();
    Ok(McpWorldToolRecord {
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        tool_name: CLONE_TEMPLATE_TOOL.to_string(),
        operation_id: CLONE_TEMPLATE_OP.to_string(),
        document_path: readback.path.clone(),
        document_sha256: readback.sha256.clone(),
        document_bytes: readback.bytes,
        document_fingerprint: readback.document_fingerprint.clone(),
        action_summary: format!("cloned template {} into fresh project", args.template_id),
        terrain_cell_count: readback.terrain_cell_count,
        terrain_stamp_count: readback.terrain_stamp_count,
        object_count: readback.object_count,
        region_count: readback.region_count,
        pathing_override_count: readback.pathing_override_count,
        start_location_count,
    })
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_template_tool(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_template_tool(field, "path must be non-empty"));
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_template_tool(
            field,
            "value must be non-empty single-line",
        ));
    }
    Ok(())
}

/// Independent-readback helper used by FSV: re-read a persisted document from disk
/// and return its stable fingerprint (proves the template is untouched after a clone).
pub fn read_document_fingerprint(path: &Path) -> Result<String, EngineError> {
    Ok(read_mcp_world_document(path)?.document_fingerprint)
}

/// Re-read a persisted document from disk through a fresh handle.
pub fn read_document(path: &Path) -> Result<EditorDocument, EngineError> {
    Ok(read_mcp_world_document(path)?.document)
}

pub(crate) fn invalid_template_tool(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.mcp_template_tools.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "mcp_template_tools_tests.rs"]
mod tests;
