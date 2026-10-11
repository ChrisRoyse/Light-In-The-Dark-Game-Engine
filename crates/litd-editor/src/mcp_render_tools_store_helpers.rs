use std::path::Path;

use litd_engine::{EngineError, calyx_json_store::CalyxJsonStoreSpec};

use super::{
    MCP_RENDER_COLLECTION, MCP_RENDER_LEDGER_COLUMN, MCP_RENDER_LEDGER_ROW,
    MCP_RENDER_STATE_COLUMN, MCP_RENDER_STATE_ROW, MCP_RENDER_VAULT_ID, MCP_RENDER_VAULT_PURPOSE,
};

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
    Ok(())
}

pub(super) fn render_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_RENDER_VAULT_ID,
        purpose: MCP_RENDER_VAULT_PURPOSE,
        collection: MCP_RENDER_COLLECTION,
        row_key: MCP_RENDER_STATE_ROW,
        column: MCP_RENDER_STATE_COLUMN,
    }
}

pub(super) fn render_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_RENDER_VAULT_ID,
        purpose: MCP_RENDER_VAULT_PURPOSE,
        collection: MCP_RENDER_COLLECTION,
        row_key: MCP_RENDER_LEDGER_ROW,
        column: MCP_RENDER_LEDGER_COLUMN,
    }
}

pub(super) fn store_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.mcp_render_tools.store.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_render_tools::store",
        field = %field,
        reason = %reason,
        "invalid render MCP state or ledger"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
