//! Editor session/history MCP catalog entries for issue #1467.

use super::super::{
    McpToolDomainGroup,
    mcp_tools_builders::{DomainSpec, domain, mut_tool, read_tool},
};

pub(super) fn editor_session_domain() -> McpToolDomainGroup {
    domain(DomainSpec {
        domain_id: "editor_session",
        title: "Editor command, history, autosave, clipboard, mode, and playtest session tools",
        child_issue: 1467,
        source_issues: &[915, 1166, 1167, 1217],
        capability_namespaces: &["editor.commands", "editor.history", "editor.clipboard"],
        gui_surface: "egui://editor.commands",
        mcp_prefixes: &[
            "litd.editor.commands",
            "litd.editor.history",
            "litd.editor.clipboard",
        ],
        default_readback_tool_name: "litd.editor.commands.read_session",
        readme_refs: &[
            "docs/readme/ED01-editor-shell.png",
            "docs/readme/ED09-test-in-editor.png",
            "docs/readme/ED11-world-view.png",
        ],
        tools: vec![
            mut_tool(
                "editor.history.apply_document_edit",
                "Apply a source-tracked editor document edit and record undo history",
                "egui://editor.commands/history/apply_document_edit",
                "litd.editor.commands.read_session",
            ),
            mut_tool(
                "editor.history.undo",
                "Undo the latest persisted editor document mutation",
                "egui://editor.commands/history/undo",
                "litd.editor.commands.read_session",
            ),
            mut_tool(
                "editor.history.redo",
                "Redo the latest persisted editor document mutation",
                "egui://editor.commands/history/redo",
                "litd.editor.commands.read_session",
            ),
            mut_tool(
                "editor.history.autosave",
                "Write a structured autosave record for the current editor document",
                "egui://editor.commands/history/autosave",
                "litd.editor.commands.read_session",
            ),
            mut_tool(
                "editor.clipboard.copy_selection",
                "Copy selected editor entities into the session clipboard",
                "egui://editor.commands/clipboard/copy_selection",
                "litd.editor.commands.read_session",
            ),
            mut_tool(
                "editor.clipboard.paste_clipboard",
                "Paste the session clipboard and record undo history",
                "egui://editor.commands/clipboard/paste_clipboard",
                "litd.editor.commands.read_session",
            ),
            mut_tool(
                "editor.commands.execute",
                "Execute MCP-safe editor commands for modes, camera bookmarks, and playtest toggle",
                "egui://editor.commands/execute",
                "litd.editor.commands.read_session",
            ),
            read_tool(
                "editor.commands.read_session",
                "Read editor command, history, autosave, clipboard, mode, and playtest session state",
                "egui://editor.commands/read_session",
            ),
        ],
    })
}
