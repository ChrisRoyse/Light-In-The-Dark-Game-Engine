use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{
    McpEditorSessionState,
    mcp_editor_session_tools_support::{
        invalid_mcp_editor_session, validate_mcp_editor_session_state,
    },
};

const MCP_EDITOR_SESSION_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FES";
const MCP_EDITOR_SESSION_VAULT_PURPOSE: &[u8] = b"litd-mcp-editor-session-issue1467";
const MCP_EDITOR_SESSION_COLLECTION: &str = "litd_mcp_editor_session";
const MCP_EDITOR_SESSION_ROW: &[u8] = b"editor/session/latest";
const MCP_EDITOR_SESSION_COLUMN: &[u8] = b"session_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpEditorSessionReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: McpEditorSessionState,
}

pub fn write_mcp_editor_session_state(
    path: impl AsRef<Path>,
    state: &McpEditorSessionState,
) -> Result<McpEditorSessionReadback, EngineError> {
    validate_mcp_editor_session_state(state)?;
    let path = path.as_ref();
    validate_path("session_state_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-editor-session-json",
            reason: format!("serialize editor session state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&session_spec(path), &bytes)
        .map_err(|error| store_error("session.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "session.write",
            "Aster session bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_editor_session_state(path)?;
    if readback.state != *state {
        return Err(store_error(
            "session.readback",
            format!("{} readback did not match written state", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_mcp_editor_session_state(
    path: impl AsRef<Path>,
) -> Result<McpEditorSessionReadback, EngineError> {
    let path = path.as_ref();
    validate_path("session_state_path", path)?;
    let readback = read_calyx_json_bytes(&session_spec(path))
        .map_err(|error| store_error("session.read", error))?
        .ok_or_else(|| {
            store_error(
                "session.read",
                format!(
                    "missing Calyx Aster editor session row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let state: McpEditorSessionState =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-editor-session-json",
            reason: format!("parse editor session state {}: {error}", path.display()),
        })?;
    validate_mcp_editor_session_state(&state)?;
    Ok(McpEditorSessionReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        state,
    })
}

pub fn session_state_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

fn session_spec(root: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: root,
        vault_id: MCP_EDITOR_SESSION_VAULT_ID,
        purpose: MCP_EDITOR_SESSION_VAULT_PURPOSE,
        collection: MCP_EDITOR_SESSION_COLLECTION,
        row_key: MCP_EDITOR_SESSION_ROW,
        column: MCP_EDITOR_SESSION_COLUMN,
    }
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_editor_session(field, "path must be non-empty"));
    }
    Ok(())
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    invalid_mcp_editor_session(field, reason.to_string())
}
