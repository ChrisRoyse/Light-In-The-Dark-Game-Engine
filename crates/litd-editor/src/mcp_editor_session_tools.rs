//! Callable MCP tools for editor history, clipboard, and command-session parity.

use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    editor_commands::default_editor_command_registry,
    editor_state_store::EditorStateStore,
    history::{
        ClipboardPasteOptions, SelectionSet, autosave_record, copy_selection,
        paste_clipboard_with_options, stable_fingerprint,
    },
    mcp_project_tools::mcp_project_tools_validation::confine_tool_paths,
    mcp_tools::McpDomainTool,
    mcp_world_tools::{
        McpWorldDocumentReadback, read_mcp_world_document, write_mcp_world_document,
    },
};

#[path = "mcp_editor_session_tools_store.rs"]
mod mcp_editor_session_tools_store;
#[path = "mcp_editor_session_tools_support.rs"]
mod mcp_editor_session_tools_support;
#[path = "mcp_editor_session_tools_types.rs"]
mod mcp_editor_session_tools_types;

use mcp_editor_session_tools_support::{
    apply_command_state, apply_edit, assert_paths_match, editor_session_route,
    invalid_mcp_editor_session, load_document_for_mutation, load_existing_session,
    load_or_create_session, parse_args, validate_common, validate_mutating_route, validate_path,
    validate_single_line,
};

pub use mcp_editor_session_tools_store::{
    McpEditorSessionReadback, read_mcp_editor_session_state, write_mcp_editor_session_state,
};
pub use mcp_editor_session_tools_types::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpEditorSessionToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub session_readback: McpEditorSessionReadback,
    #[serde(default)]
    pub document_readback: Option<McpWorldDocumentReadback>,
}

pub fn call_mcp_editor_session_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    let route = editor_session_route(tool_name)?;
    match tool_name {
        APPLY_DOCUMENT_EDIT_TOOL => apply_document_edit(route, arguments),
        UNDO_TOOL => undo_or_redo(route, arguments, HistoryDirection::Undo),
        REDO_TOOL => undo_or_redo(route, arguments, HistoryDirection::Redo),
        AUTOSAVE_TOOL => autosave(route, arguments),
        COPY_SELECTION_TOOL => copy_selection_tool(route, arguments),
        PASTE_CLIPBOARD_TOOL => paste_clipboard_tool(route, arguments),
        EXECUTE_COMMAND_TOOL => execute_command(route, arguments),
        READ_SESSION_TOOL => read_session(route, arguments),
        other => Err(invalid_mcp_editor_session(
            "tool_name",
            format!("{other} is not a registered editor-session MCP tool"),
        )),
    }
}

fn apply_document_edit(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ApplyDocumentEditArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every write sink to the project root before any fs effect.
    confine_tool_paths(&[
        ("session_state_path", args.session_state_path.as_path()),
        ("document_path", args.document_path.as_path()),
        ("history_store_path", args.history_store_path.as_path()),
    ])?;
    validate_common(&args.request_id, &args.requested_by, &args.session_id)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("history_store_path", &args.history_store_path)?;
    validate_single_line("next_revision_id", &args.next_revision_id)?;
    let mut session = load_or_create_session(
        &args.session_state_path,
        &args.session_id,
        &args.document_path,
        &args.history_store_path,
        args.max_entries.unwrap_or(32),
    )?;
    let mut document = load_document_for_mutation(&args.document_path, args.initial_document)?;
    let before = document.clone();
    apply_edit(&mut document, args.edit)?;
    document.revision_id = args.next_revision_id;
    document.validate()?;
    let before_fp = stable_fingerprint(&before)?;
    let after_fp = stable_fingerprint(&document)?;
    if before_fp == after_fp {
        return Err(invalid_mcp_editor_session(
            "edit",
            "document edit produced no state change",
        ));
    }
    session.history.record_with_mvcc(
        &EditorStateStore::new(&args.history_store_path),
        "mcp.editor.apply_document_edit",
        before,
        document.clone(),
    )?;
    let document_readback = write_mcp_world_document(&args.document_path, &document)?;
    finish(
        route,
        &args.session_state_path,
        &mut session,
        RecordInput {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            document_path: &args.document_path,
            action_summary: "applied editor document edit and recorded undo snapshot",
            before_fp: Some(before_fp),
            after_fp: Some(after_fp),
        },
        json!({ "document_readback": document_readback }),
        Some(document_readback),
    )
}

fn undo_or_redo(
    route: McpDomainTool,
    arguments: Value,
    direction: HistoryDirection,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: HistoryCommandArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every write sink to the project root before any fs effect.
    confine_tool_paths(&[
        ("session_state_path", args.session_state_path.as_path()),
        ("document_path", args.document_path.as_path()),
        ("history_store_path", args.history_store_path.as_path()),
    ])?;
    validate_common(&args.request_id, &args.requested_by, &args.session_id)?;
    let mut session = load_existing_session(&args.session_state_path, &args.session_id)?;
    assert_paths_match(&session, &args.document_path, &args.history_store_path)?;
    let mut document = read_mcp_world_document(&args.document_path)?.document;
    let before_fp = stable_fingerprint(&document)?;
    let label = match direction {
        HistoryDirection::Undo => session.history.undo_from_mvcc(
            &EditorStateStore::new(&args.history_store_path),
            &mut document,
        )?,
        HistoryDirection::Redo => session.history.redo_from_mvcc(
            &EditorStateStore::new(&args.history_store_path),
            &mut document,
        )?,
    };
    let Some(label) = label else {
        return Err(invalid_mcp_editor_session(
            "history",
            format!("no persisted {:?} entry is available", direction),
        ));
    };
    let after_fp = stable_fingerprint(&document)?;
    let document_readback = write_mcp_world_document(&args.document_path, &document)?;
    finish(
        route,
        &args.session_state_path,
        &mut session,
        RecordInput {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            document_path: &args.document_path,
            action_summary: &format!("applied persisted history {direction:?}: {label}"),
            before_fp: Some(before_fp),
            after_fp: Some(after_fp),
        },
        json!({ "history_label": label, "document_readback": document_readback }),
        Some(document_readback),
    )
}

fn autosave(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: AutosaveArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every write sink to the project root before any fs effect.
    confine_tool_paths(&[
        ("session_state_path", args.session_state_path.as_path()),
        ("document_path", args.document_path.as_path()),
    ])?;
    validate_common(&args.request_id, &args.requested_by, &args.session_id)?;
    let mut session = load_existing_session(&args.session_state_path, &args.session_id)?;
    let document_readback = read_mcp_world_document(&args.document_path)?;
    let record = autosave_record(&document_readback.document, args.pending_changes)?;
    session.autosaves.push(record.clone());
    if session.autosaves.len() > 32 {
        session.autosaves.remove(0);
    }
    finish(
        route,
        &args.session_state_path,
        &mut session,
        RecordInput {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            document_path: &args.document_path,
            action_summary: "persisted editor autosave record",
            before_fp: None,
            after_fp: Some(document_readback.document_fingerprint.clone()),
        },
        json!({ "autosave": record }),
        Some(document_readback),
    )
}

fn copy_selection_tool(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ClipboardCopyArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every write sink to the project root before any fs effect.
    confine_tool_paths(&[
        ("session_state_path", args.session_state_path.as_path()),
        ("document_path", args.document_path.as_path()),
    ])?;
    validate_common(&args.request_id, &args.requested_by, &args.session_id)?;
    let mut session = load_existing_session(&args.session_state_path, &args.session_id)?;
    let document_readback = read_mcp_world_document(&args.document_path)?;
    let mut selection = SelectionSet::default();
    selection.select_many(args.selected_ids);
    let clipboard = copy_selection(&document_readback.document, &selection)?;
    session.clipboard = Some(clipboard.clone());
    finish(
        route,
        &args.session_state_path,
        &mut session,
        RecordInput {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            document_path: &args.document_path,
            action_summary: "copied selected editor entities into MCP session clipboard",
            before_fp: None,
            after_fp: Some(document_readback.document_fingerprint.clone()),
        },
        json!({ "clipboard": clipboard }),
        Some(document_readback),
    )
}

fn paste_clipboard_tool(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ClipboardPasteArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every write sink to the project root before any fs effect.
    confine_tool_paths(&[
        ("session_state_path", args.session_state_path.as_path()),
        ("document_path", args.document_path.as_path()),
        ("history_store_path", args.history_store_path.as_path()),
    ])?;
    validate_common(&args.request_id, &args.requested_by, &args.session_id)?;
    validate_single_line("next_revision_id", &args.next_revision_id)?;
    let mut session = load_existing_session(&args.session_state_path, &args.session_id)?;
    assert_paths_match(&session, &args.document_path, &args.history_store_path)?;
    let clipboard = session
        .clipboard
        .clone()
        .ok_or_else(|| invalid_mcp_editor_session("clipboard", "session clipboard is empty"))?;
    let mut document = read_mcp_world_document(&args.document_path)?.document;
    let before = document.clone();
    let before_fp = stable_fingerprint(&before)?;
    let report = paste_clipboard_with_options(
        &mut document,
        &clipboard,
        ClipboardPasteOptions::in_place(args.id_suffix),
    )?;
    document.revision_id = args.next_revision_id;
    document.validate()?;
    let after_fp = stable_fingerprint(&document)?;
    session.history.record_with_mvcc(
        &EditorStateStore::new(&args.history_store_path),
        "mcp.editor.clipboard.paste",
        before,
        document.clone(),
    )?;
    let document_readback = write_mcp_world_document(&args.document_path, &document)?;
    finish(
        route,
        &args.session_state_path,
        &mut session,
        RecordInput {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            document_path: &args.document_path,
            action_summary: "pasted MCP session clipboard and recorded undo snapshot",
            before_fp: Some(before_fp),
            after_fp: Some(after_fp),
        },
        json!({ "paste_report": report, "document_readback": document_readback }),
        Some(document_readback),
    )
}

fn execute_command(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ExecuteCommandArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine the session state sink to the project root before any fs effect.
    confine_tool_paths(&[("session_state_path", args.session_state_path.as_path())])?;
    validate_common(&args.request_id, &args.requested_by, &args.session_id)?;
    let registry = default_editor_command_registry()?;
    if registry.get(&args.command_id).is_none() {
        return Err(invalid_mcp_editor_session(
            "command_id",
            format!("{} is not in the editor command registry", args.command_id),
        ));
    }
    let mut session = load_existing_session(&args.session_state_path, &args.session_id)?;
    apply_command_state(&mut session, &args.command_id)?;
    session.last_command_id = Some(args.command_id.clone());
    let document_path = Path::new(&session.document_path).to_path_buf();
    let current_document_fingerprint = session.current_document_fingerprint.clone();
    let active_mode = session.active_mode.clone();
    let playtest_running = session.playtest_running;
    let camera_bookmarks = session.camera_bookmarks.clone();
    finish(
        route,
        &args.session_state_path,
        &mut session,
        RecordInput {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            document_path: &document_path,
            action_summary: "executed MCP-safe editor command state transition",
            before_fp: None,
            after_fp: current_document_fingerprint,
        },
        json!({
            "command_id": args.command_id,
            "active_mode": active_mode,
            "playtest_running": playtest_running,
            "camera_bookmarks": camera_bookmarks
        }),
        None,
    )
}

fn read_session(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    let args: ReadSessionArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every read path to the project root before reading from disk.
    let mut confined = vec![("session_state_path", args.session_state_path.as_path())];
    if let Some(document_path) = args.document_path.as_ref() {
        confined.push(("document_path", document_path.as_path()));
    }
    confine_tool_paths(&confined)?;
    let session_readback = read_mcp_editor_session_state(&args.session_state_path)?;
    let document_readback = args
        .document_path
        .as_ref()
        .map(read_mcp_world_document)
        .transpose()?;
    Ok(output(
        route,
        "Calyx Aster editor session state",
        json!({ "session_readback": session_readback, "document_readback": document_readback }),
        session_readback,
        document_readback,
    ))
}

fn finish(
    route: McpDomainTool,
    session_path: &Path,
    session: &mut McpEditorSessionState,
    record: RecordInput<'_>,
    result: Value,
    document_readback: Option<McpWorldDocumentReadback>,
) -> Result<McpEditorSessionToolCallOutput, EngineError> {
    session.update_count = session.update_count.saturating_add(1);
    session.current_document_fingerprint = record
        .after_fp
        .clone()
        .or_else(|| session.current_document_fingerprint.clone());
    session.records.push(McpEditorSessionRecord {
        request_id: record.request_id.to_string(),
        requested_by: record.requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        action_summary: record.action_summary.to_string(),
        document_path: record.document_path.display().to_string(),
        before_document_fingerprint: record.before_fp,
        after_document_fingerprint: record.after_fp,
        undo_depth: session.history.undo.len(),
        redo_depth: session.history.redo.len(),
        clipboard_fingerprint: session
            .clipboard
            .as_ref()
            .map(|clipboard| clipboard.fingerprint.clone()),
        autosave_count: session.autosaves.len(),
        active_mode: session.active_mode.clone(),
        playtest_running: session.playtest_running,
    });
    session.record_count = session.records.len();
    let session_readback = write_mcp_editor_session_state(session_path, session)?;
    Ok(output(
        route,
        "Calyx Aster editor session state + optional world document readback",
        json!({ "operation_report": result, "session_readback": session_readback }),
        session_readback,
        document_readback,
    ))
}

fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    session_readback: McpEditorSessionReadback,
    document_readback: Option<McpWorldDocumentReadback>,
) -> McpEditorSessionToolCallOutput {
    McpEditorSessionToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        session_readback,
        document_readback,
    }
}

struct RecordInput<'a> {
    request_id: &'a str,
    requested_by: &'a str,
    document_path: &'a Path,
    action_summary: &'a str,
    before_fp: Option<String>,
    after_fp: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum HistoryDirection {
    Undo,
    Redo,
}

#[cfg(test)]
#[path = "mcp_editor_session_tools_tests.rs"]
mod tests;
