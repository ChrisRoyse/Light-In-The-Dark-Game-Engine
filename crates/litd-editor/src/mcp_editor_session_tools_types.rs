use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::history::{
    AutosaveRecord, ClipboardPayload, EditorDocument, EditorObject, UndoRedoStack,
};

pub const MCP_EDITOR_SESSION_SCHEMA: &str = "litd.mcp.editor_session.state.v1";
pub const APPLY_DOCUMENT_EDIT_TOOL: &str = "litd.editor.history.apply_document_edit";
pub const UNDO_TOOL: &str = "litd.editor.history.undo";
pub const REDO_TOOL: &str = "litd.editor.history.redo";
pub const AUTOSAVE_TOOL: &str = "litd.editor.history.autosave";
pub const COPY_SELECTION_TOOL: &str = "litd.editor.clipboard.copy_selection";
pub const PASTE_CLIPBOARD_TOOL: &str = "litd.editor.clipboard.paste_clipboard";
pub const EXECUTE_COMMAND_TOOL: &str = "litd.editor.commands.execute";
pub const READ_SESSION_TOOL: &str = "litd.editor.commands.read_session";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpEditorSessionState {
    pub schema: String,
    pub issue: u32,
    pub session_id: String,
    pub source_of_truth: String,
    pub document_path: String,
    pub history_store_path: String,
    pub max_entries: usize,
    pub history: UndoRedoStack,
    #[serde(default)]
    pub clipboard: Option<ClipboardPayload>,
    #[serde(default)]
    pub autosaves: Vec<AutosaveRecord>,
    pub active_mode: String,
    pub playtest_running: bool,
    #[serde(default)]
    pub camera_bookmarks: BTreeMap<u8, String>,
    pub update_count: u64,
    pub record_count: usize,
    pub records: Vec<McpEditorSessionRecord>,
    #[serde(default)]
    pub current_document_fingerprint: Option<String>,
    #[serde(default)]
    pub last_command_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpEditorSessionRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub action_summary: String,
    pub document_path: String,
    #[serde(default)]
    pub before_document_fingerprint: Option<String>,
    #[serde(default)]
    pub after_document_fingerprint: Option<String>,
    pub undo_depth: usize,
    pub redo_depth: usize,
    #[serde(default)]
    pub clipboard_fingerprint: Option<String>,
    pub autosave_count: usize,
    pub active_mode: String,
    pub playtest_running: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyDocumentEditArgs {
    pub request_id: String,
    pub requested_by: String,
    pub session_id: String,
    pub document_path: PathBuf,
    pub session_state_path: PathBuf,
    pub history_store_path: PathBuf,
    #[serde(default)]
    pub max_entries: Option<usize>,
    #[serde(default)]
    pub initial_document: Option<EditorDocument>,
    pub next_revision_id: String,
    pub edit: EditorSessionDocumentEdit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorSessionDocumentEdit {
    UpsertObject {
        object: EditorObject,
    },
    SetObjectProperty {
        object_id: String,
        key: String,
        value: String,
    },
    RemoveObject {
        object_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryCommandArgs {
    pub request_id: String,
    pub requested_by: String,
    pub session_id: String,
    pub document_path: PathBuf,
    pub session_state_path: PathBuf,
    pub history_store_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutosaveArgs {
    pub request_id: String,
    pub requested_by: String,
    pub session_id: String,
    pub document_path: PathBuf,
    pub session_state_path: PathBuf,
    pub pending_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardCopyArgs {
    pub request_id: String,
    pub requested_by: String,
    pub session_id: String,
    pub document_path: PathBuf,
    pub session_state_path: PathBuf,
    pub selected_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPasteArgs {
    pub request_id: String,
    pub requested_by: String,
    pub session_id: String,
    pub document_path: PathBuf,
    pub session_state_path: PathBuf,
    pub history_store_path: PathBuf,
    pub id_suffix: String,
    pub next_revision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecuteCommandArgs {
    pub request_id: String,
    pub requested_by: String,
    pub session_id: String,
    pub session_state_path: PathBuf,
    pub command_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadSessionArgs {
    pub session_state_path: PathBuf,
    #[serde(default)]
    pub document_path: Option<PathBuf>,
}
