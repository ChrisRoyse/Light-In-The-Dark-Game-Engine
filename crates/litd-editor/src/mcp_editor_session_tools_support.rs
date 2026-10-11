use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{
    editor_commands::{
        COMMAND_MODE_AI_COAUTHOR, COMMAND_MODE_CINEMATICS, COMMAND_MODE_OBJECTS,
        COMMAND_MODE_TERRAIN, COMMAND_MODE_TRIGGERS, COMMAND_MODE_VFX, COMMAND_RESTORE_BOOKMARK_1,
        COMMAND_RESTORE_BOOKMARK_2, COMMAND_RESTORE_BOOKMARK_3, COMMAND_SAVE_BOOKMARK_1,
        COMMAND_SAVE_BOOKMARK_2, COMMAND_SAVE_BOOKMARK_3, COMMAND_TOGGLE_PLAYTEST,
    },
    history::{AutosaveRecord, ClipboardPayload, EditorDocument, UndoRedoStack},
    mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest},
    mcp_world_tools::read_mcp_world_document,
};

use super::{
    EditorSessionDocumentEdit, MCP_EDITOR_SESSION_SCHEMA, McpEditorSessionState,
    mcp_editor_session_tools_store::{self, read_mcp_editor_session_state},
};

pub(super) fn apply_edit(
    document: &mut EditorDocument,
    edit: EditorSessionDocumentEdit,
) -> Result<(), EngineError> {
    match edit {
        EditorSessionDocumentEdit::UpsertObject { object } => document.upsert_object(object),
        EditorSessionDocumentEdit::SetObjectProperty {
            object_id,
            key,
            value,
        } => {
            validate_single_line("edit.object_id", &object_id)?;
            validate_single_line("edit.key", &key)?;
            validate_single_line("edit.value", &value)?;
            let object = document.objects.get_mut(&object_id).ok_or_else(|| {
                invalid_mcp_editor_session("edit.object_id", format!("missing object {object_id}"))
            })?;
            object.properties.insert(key, value);
            document.validate()
        }
        EditorSessionDocumentEdit::RemoveObject { object_id } => {
            document.remove_object(&object_id)?;
            document.validate()
        }
    }
}

pub(super) fn load_document_for_mutation(
    document_path: &Path,
    initial_document: Option<EditorDocument>,
) -> Result<EditorDocument, EngineError> {
    if document_path.exists() {
        return Ok(read_mcp_world_document(document_path)?.document);
    }
    let document = initial_document.ok_or_else(|| {
        invalid_mcp_editor_session(
            "document.missing",
            format!(
                "{} does not exist and no explicit initial_document was supplied",
                document_path.display()
            ),
        )
    })?;
    document.validate()?;
    Ok(document)
}

pub(super) fn load_or_create_session(
    path: &Path,
    session_id: &str,
    document_path: &Path,
    history_store_path: &Path,
    max_entries: usize,
) -> Result<McpEditorSessionState, EngineError> {
    validate_path("session_state_path", path)?;
    if mcp_editor_session_tools_store::session_state_exists(path) {
        let state = read_mcp_editor_session_state(path)?.state;
        assert_session_id(&state, session_id)?;
        return Ok(state);
    }
    let history = UndoRedoStack::new(max_entries)?;
    let state = McpEditorSessionState {
        schema: MCP_EDITOR_SESSION_SCHEMA.to_string(),
        issue: 1467,
        session_id: session_id.to_string(),
        source_of_truth: "Calyx Aster editor session + world document + history MVCC snapshots"
            .to_string(),
        document_path: document_path.display().to_string(),
        history_store_path: history_store_path.display().to_string(),
        max_entries,
        history,
        clipboard: None,
        autosaves: Vec::new(),
        active_mode: "terrain".to_string(),
        playtest_running: false,
        camera_bookmarks: Default::default(),
        update_count: 0,
        record_count: 0,
        records: Vec::new(),
        current_document_fingerprint: None,
        last_command_id: None,
    };
    validate_mcp_editor_session_state(&state)?;
    Ok(state)
}

pub(super) fn load_existing_session(
    path: &Path,
    session_id: &str,
) -> Result<McpEditorSessionState, EngineError> {
    let state = read_mcp_editor_session_state(path)?.state;
    assert_session_id(&state, session_id)?;
    Ok(state)
}

pub(super) fn apply_command_state(
    session: &mut McpEditorSessionState,
    command_id: &str,
) -> Result<(), EngineError> {
    match command_id {
        COMMAND_MODE_TERRAIN => session.active_mode = "terrain".to_string(),
        COMMAND_MODE_OBJECTS => session.active_mode = "objects".to_string(),
        COMMAND_MODE_TRIGGERS => session.active_mode = "triggers".to_string(),
        COMMAND_MODE_CINEMATICS => session.active_mode = "cinematics".to_string(),
        COMMAND_MODE_VFX => session.active_mode = "vfx".to_string(),
        COMMAND_MODE_AI_COAUTHOR => session.active_mode = "ai_coauthor".to_string(),
        COMMAND_TOGGLE_PLAYTEST => session.playtest_running = !session.playtest_running,
        COMMAND_SAVE_BOOKMARK_1 => save_bookmark(session, 1),
        COMMAND_SAVE_BOOKMARK_2 => save_bookmark(session, 2),
        COMMAND_SAVE_BOOKMARK_3 => save_bookmark(session, 3),
        COMMAND_RESTORE_BOOKMARK_1 => require_bookmark(session, 1)?,
        COMMAND_RESTORE_BOOKMARK_2 => require_bookmark(session, 2)?,
        COMMAND_RESTORE_BOOKMARK_3 => require_bookmark(session, 3)?,
        other => {
            return Err(invalid_mcp_editor_session(
                "command_id",
                format!("{other} is not MCP-modeled by #1467 editor-session commands"),
            ));
        }
    }
    Ok(())
}

pub(super) fn editor_session_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "editor_session")
        .ok_or_else(|| invalid_mcp_editor_session("domain", "missing editor_session domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_editor_session(
                "tool_name",
                format!("{tool_name} is not registered in editor_session"),
            )
        })
}

pub(super) fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != super::READ_SESSION_TOOL
    {
        return Err(invalid_mcp_editor_session(
            "route",
            format!("{} is not a guarded editor-session route", route.tool_name),
        ));
    }
    Ok(())
}

pub(crate) fn validate_mcp_editor_session_state(
    state: &McpEditorSessionState,
) -> Result<(), EngineError> {
    validate_single_line("session_id", &state.session_id)?;
    if state.schema != MCP_EDITOR_SESSION_SCHEMA
        || state.issue != 1467
        || state.source_of_truth.trim().is_empty()
        || state.max_entries == 0
        || state.history.max_entries != state.max_entries
        || state.record_count != state.records.len()
        || state.update_count < state.record_count as u64
    {
        return Err(invalid_mcp_editor_session(
            "state",
            "schema, issue, max entries, records, or source of truth drifted",
        ));
    }
    validate_single_line("active_mode", &state.active_mode)?;
    let mut request_ids = BTreeSet::new();
    for record in &state.records {
        validate_single_line("record.request_id", &record.request_id)?;
        validate_single_line("record.requested_by", &record.requested_by)?;
        validate_single_line("record.tool_name", &record.tool_name)?;
        validate_single_line("record.operation_id", &record.operation_id)?;
        if !request_ids.insert(record.request_id.as_str()) {
            return Err(invalid_mcp_editor_session(
                "record.request_id",
                format!("duplicate request id {}", record.request_id),
            ));
        }
    }
    for autosave in &state.autosaves {
        validate_autosave(autosave)?;
    }
    if let Some(clipboard) = &state.clipboard {
        validate_clipboard_summary(clipboard)?;
    }
    Ok(())
}

pub(super) fn assert_paths_match(
    session: &McpEditorSessionState,
    document_path: &Path,
    history_store_path: &Path,
) -> Result<(), EngineError> {
    if session.document_path != document_path.display().to_string()
        || session.history_store_path != history_store_path.display().to_string()
    {
        return Err(invalid_mcp_editor_session(
            "paths",
            "session document/history paths do not match request paths",
        ));
    }
    Ok(())
}

pub(super) fn validate_common(
    request_id: &str,
    requested_by: &str,
    session_id: &str,
) -> Result<(), EngineError> {
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)?;
    validate_single_line("session_id", session_id)
}

pub(super) fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_editor_session(field, "path must be non-empty"));
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_editor_session(
            field,
            "value must be non-empty single-line text",
        ));
    }
    Ok(())
}

pub(super) fn parse_args<T: DeserializeOwned>(
    tool_name: &str,
    value: Value,
) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_editor_session(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

pub(crate) fn invalid_mcp_editor_session(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_editor_session_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_editor_session_tools",
        field = %field,
        reason = %reason,
        "invalid editor-session MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn save_bookmark(session: &mut McpEditorSessionState, slot: u8) {
    session.camera_bookmarks.insert(
        slot,
        format!(
            "session={} update={} mode={}",
            session.session_id, session.update_count, session.active_mode
        ),
    );
}

fn require_bookmark(session: &McpEditorSessionState, slot: u8) -> Result<(), EngineError> {
    if session.camera_bookmarks.contains_key(&slot) {
        Ok(())
    } else {
        Err(invalid_mcp_editor_session(
            "camera_bookmark",
            format!("camera bookmark slot {slot} has not been saved"),
        ))
    }
}

fn validate_autosave(record: &AutosaveRecord) -> Result<(), EngineError> {
    validate_single_line("autosave.document_id", &record.calyx_document_id)?;
    validate_single_line("autosave.revision_id", &record.calyx_revision_id)?;
    validate_single_line("autosave.fingerprint", &record.stable_fingerprint)
}

fn validate_clipboard_summary(clipboard: &ClipboardPayload) -> Result<(), EngineError> {
    validate_single_line(
        "clipboard.source_document",
        &clipboard.source_calyx_document_id,
    )?;
    validate_single_line("clipboard.fingerprint", &clipboard.fingerprint)
}

fn assert_session_id(state: &McpEditorSessionState, session_id: &str) -> Result<(), EngineError> {
    validate_single_line("session_id", session_id)?;
    if state.session_id != session_id {
        return Err(invalid_mcp_editor_session(
            "session_id",
            format!("expected {}, got {session_id}", state.session_id),
        ));
    }
    Ok(())
}
