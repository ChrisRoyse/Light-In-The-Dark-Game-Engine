use std::{fs, path::PathBuf};

use litd_engine::math::{Transform, Vec3};
use serde_json::json;

use super::*;
use crate::history::{EditorDocument, EditorObject};

#[test]
fn issue1467_mcp_edit_undo_reverts_persisted_document() {
    let paths = temp_paths("edit-undo");
    let apply = call_mcp_editor_session_tool(
        APPLY_DOCUMENT_EDIT_TOOL,
        json!({
            "request_id": "issue1467-apply",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467",
            "document_path": paths.document,
            "session_state_path": paths.session,
            "history_store_path": paths.history,
            "max_entries": 8,
            "initial_document": initial_document(),
            "next_revision_id": "rev.after",
            "edit": {
                "set_object_property": {
                    "object_id": "unit.issue1467.worker",
                    "key": "display_name",
                    "value": "After Undo Target"
                }
            }
        }),
    )
    .unwrap();
    assert_eq!(
        apply.document_readback.as_ref().unwrap().document.objects["unit.issue1467.worker"]
            .properties["display_name"],
        "After Undo Target"
    );

    let undo = call_mcp_editor_session_tool(
        UNDO_TOOL,
        json!({
            "request_id": "issue1467-undo",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467",
            "document_path": paths.document,
            "session_state_path": paths.session,
            "history_store_path": paths.history
        }),
    )
    .unwrap();
    let document = &undo.document_readback.as_ref().unwrap().document;
    assert_eq!(
        document.objects["unit.issue1467.worker"].properties["display_name"],
        "Before Undo Target"
    );
    assert_eq!(undo.session_readback.state.history.undo.len(), 0);
    assert_eq!(undo.session_readback.state.history.redo.len(), 1);
    cleanup(paths.root);
}

#[test]
fn issue1467_mcp_command_execute_models_mode_and_playtest() {
    let paths = temp_paths("commands");
    call_mcp_editor_session_tool(
        APPLY_DOCUMENT_EDIT_TOOL,
        json!({
            "request_id": "issue1467-command-seed",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467.commands",
            "document_path": paths.document,
            "session_state_path": paths.session,
            "history_store_path": paths.history,
            "max_entries": 8,
            "initial_document": initial_document(),
            "next_revision_id": "rev.command.seed",
            "edit": {
                "set_object_property": {
                    "object_id": "unit.issue1467.worker",
                    "key": "display_name",
                    "value": "Command Seed"
                }
            }
        }),
    )
    .unwrap();

    let mode = call_mcp_editor_session_tool(
        EXECUTE_COMMAND_TOOL,
        json!({
            "request_id": "issue1467-command-mode",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467.commands",
            "session_state_path": paths.session,
            "command_id": "editor.mode.vfx"
        }),
    )
    .unwrap();
    assert_eq!(mode.session_readback.state.active_mode, "vfx");

    let playtest = call_mcp_editor_session_tool(
        EXECUTE_COMMAND_TOOL,
        json!({
            "request_id": "issue1467-command-playtest",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467.commands",
            "session_state_path": paths.session,
            "command_id": "editor.playtest.toggle"
        }),
    )
    .unwrap();
    assert!(playtest.session_readback.state.playtest_running);
    cleanup(paths.root);
}

#[test]
fn issue1467_empty_undo_fails_loud_without_state_drift() {
    let paths = temp_paths("empty-undo");
    let before = call_mcp_editor_session_tool(
        APPLY_DOCUMENT_EDIT_TOOL,
        json!({
            "request_id": "issue1467-empty-seed",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467.empty",
            "document_path": paths.document,
            "session_state_path": paths.session,
            "history_store_path": paths.history,
            "max_entries": 8,
            "initial_document": initial_document(),
            "next_revision_id": "rev.empty.seed",
            "edit": {
                "set_object_property": {
                    "object_id": "unit.issue1467.worker",
                    "key": "display_name",
                    "value": "Empty Seed"
                }
            }
        }),
    )
    .unwrap();
    call_mcp_editor_session_tool(
        UNDO_TOOL,
        json!({
            "request_id": "issue1467-empty-undo-1",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467.empty",
            "document_path": paths.document,
            "session_state_path": paths.session,
            "history_store_path": paths.history
        }),
    )
    .unwrap();
    let error = call_mcp_editor_session_tool(
        UNDO_TOOL,
        json!({
            "request_id": "issue1467-empty-undo-2",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1467.empty",
            "document_path": paths.document,
            "session_state_path": paths.session,
            "history_store_path": paths.history
        }),
    )
    .unwrap_err()
    .to_string();
    let after = read_mcp_editor_session_state(&paths.session).unwrap();
    assert_eq!(
        before.session_readback.state.session_id,
        after.state.session_id
    );
    assert!(error.contains("no persisted Undo entry"));
    cleanup(paths.root);
}

#[test]
fn issue1518_editor_session_rejects_out_of_root_paths_without_writing() {
    // #1518 confinement close gate for the editor-session family: a poisoned
    // session_state_path (absolute-escape) and a ..-traversal document_path are both
    // rejected at confine_tool_paths BEFORE load_or_create_session / document read/write,
    // leaving the filesystem untouched.
    let paths = temp_paths("issue1518-confine");
    let edit = json!({
        "set_object_property": {
            "object_id": "unit.issue1467.worker",
            "key": "display_name",
            "value": "should-never-persist"
        }
    });

    // --- EDGE 1: absolute session_state_path OUTSIDE the workspace root ---
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-session");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).ok();
    }
    let escape_error = call_mcp_editor_session_tool(
        APPLY_DOCUMENT_EDIT_TOOL,
        json!({
            "request_id": "issue1518-escape",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1518",
            "document_path": paths.document,
            "session_state_path": escape_dir.join("editor-session"),
            "history_store_path": paths.history,
            "max_entries": 8,
            "initial_document": initial_document(),
            "next_revision_id": "rev.after",
            "edit": edit.clone()
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 session absolute-escape rejected: {escape_error}");
    assert!(
        escape_error.contains("outside the project root"),
        "expected outside-root rejection, got: {escape_error}"
    );
    assert!(
        !escape_dir.exists(),
        "confinement breach: editor-session tool created {} outside the project root",
        escape_dir.display()
    );

    // --- EDGE 2: ..-traversal document_path ---
    let traversal_error = call_mcp_editor_session_tool(
        APPLY_DOCUMENT_EDIT_TOOL,
        json!({
            "request_id": "issue1518-traversal",
            "requested_by": "mcp-agent",
            "session_id": "session.issue1518",
            "document_path": "sess-doc/../../litd-issue1518-session-traversal",
            "session_state_path": paths.session,
            "history_store_path": paths.history,
            "max_entries": 8,
            "initial_document": initial_document(),
            "next_revision_id": "rev.after",
            "edit": edit
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 session traversal rejected: {traversal_error}");
    assert!(
        traversal_error.contains("parent-directory"),
        "expected traversal rejection, got: {traversal_error}"
    );
    assert!(
        !std::path::Path::new("../litd-issue1518-session-traversal").exists(),
        "confinement breach: traversal created a directory outside the workspace"
    );
    cleanup(paths.root);
}

struct SessionTestPaths {
    root: PathBuf,
    document: PathBuf,
    session: PathBuf,
    history: PathBuf,
}

fn temp_paths(label: &str) -> SessionTestPaths {
    // #1518: the editor-session tools now confine every path to LITD_EDITOR_PROJECT_ROOT.
    // Set the root and place fixtures under <workspace>/target/fsv (inside that root) rather
    // than the system temp dir (outside it) so confinement passes for legitimate paths. All
    // three tests funnel through here, so this is the single env + fixture-root anchor.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_run, root) = crate::fsv_support::artifact_dir("issue1467_editor_session", label).unwrap();
    cleanup(root.clone());
    SessionTestPaths {
        document: root.join("world-document"),
        session: root.join("editor-session"),
        history: root.join("history-vault"),
        root,
    }
}

fn initial_document() -> EditorDocument {
    let mut document = EditorDocument::new("calyx://issue1467/session", "rev.before").unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit.issue1467.worker",
                "Unit",
                None,
                Transform::from_translation(Vec3::new(1.0, 0.0, 2.0)),
            )
            .with_property("display_name", "Before Undo Target"),
        )
        .unwrap();
    document
}

fn cleanup(root: PathBuf) {
    fs::remove_dir_all(root).ok();
}
