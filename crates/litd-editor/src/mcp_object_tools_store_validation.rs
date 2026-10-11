use std::path::Path;

use litd_engine::EngineError;

use super::{MCP_OBJECT_LEDGER_SCHEMA, McpObjectToolLedger, McpObjectToolRecord, store_error};

pub fn validate_mcp_object_tool_ledger(ledger: &McpObjectToolLedger) -> Result<(), EngineError> {
    validate_single_line("ledger_id", &ledger.ledger_id)?;
    if ledger.schema != MCP_OBJECT_LEDGER_SCHEMA
        || ledger.issue != 919
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

fn validate_record(record: &McpObjectToolRecord) -> Result<(), EngineError> {
    validate_single_line("record.request_id", &record.request_id)?;
    validate_single_line("record.requested_by", &record.requested_by)?;
    validate_single_line("record.document_path", &record.document_path)?;
    validate_single_line("record.document_fingerprint", &record.document_fingerprint)?;
    validate_single_line("record.action_summary", &record.action_summary)?;
    if !record.tool_name.starts_with("litd.editor.objects.")
        && !record.tool_name.starts_with("litd.editor.rpg.")
    {
        return Err(store_error(
            "record.tool_name",
            "unexpected object tool prefix",
        ));
    }
    if !(record.operation_id.starts_with("editor.objects.")
        || record.operation_id.starts_with("editor.rpg."))
        || record.document_sha256.len() != 64
        || record.document_bytes == 0
        || record.object_count == 0
        || record.affected_object_ids.is_empty()
    {
        return Err(store_error(
            "record",
            "operation, document hash, bytes, object count, and affected ids must be concrete",
        ));
    }
    for object_id in &record.affected_object_ids {
        validate_single_line("record.affected_object_id", object_id)?;
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(store_error(
            field,
            "value must be non-empty single-line text",
        ));
    }
    Ok(())
}

pub(super) fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(store_error(field, "path must be non-empty"));
    }
    if path.is_file() {
        return Err(store_error(
            field,
            format!(
                "{} is a file; object/gameplay state must be a Calyx Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}
