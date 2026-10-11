//! Trigger, ECA, and runtime-system MCP catalog entries (#803/#804, #835).

use super::super::{
    McpToolDomainGroup,
    mcp_tools_builders::{DomainSpec, domain, mut_tool, read_tool},
};

pub(super) fn trigger_runtime_domain() -> McpToolDomainGroup {
    domain(DomainSpec {
        domain_id: "trigger_runtime",
        title: "Trigger, ECA, and runtime-system tools",
        child_issue: 920,
        source_issues: &[803, 804],
        capability_namespaces: &["editor.triggers", "editor.runtime"],
        gui_surface: "egui://editor.triggers",
        mcp_prefixes: &["litd.editor.triggers", "litd.editor.runtime"],
        default_readback_tool_name: "litd.editor.triggers.read_trigger_runtime",
        readme_refs: &[
            "docs/readme/04-editor.png",
            "docs/readme/ED04-trigger-editor.png",
        ],
        tools: vec![
            mut_tool(
                "editor.triggers.upsert_variable",
                "Create or update typed trigger variables",
                "egui://editor.triggers/upsert_variable",
                "litd.editor.triggers.read_trigger_runtime",
            ),
            mut_tool(
                "editor.triggers.upsert_eca_graph",
                "Author event, condition, and action graphs",
                "egui://editor.triggers/upsert_eca_graph",
                "litd.editor.triggers.read_trigger_runtime",
            ),
            mut_tool(
                "editor.runtime.upsert_runtime_cue",
                "Attach runtime timers, camera cues, fog, dialogs, and effects",
                "egui://editor.triggers/upsert_runtime_cue",
                "litd.editor.triggers.read_trigger_runtime",
            ),
            mut_tool(
                "editor.runtime.import_sound",
                "Import a decoded sound asset into the sound library",
                "egui://editor.triggers/import_sound",
                "litd.editor.runtime.read_sound_editor",
            ),
            mut_tool(
                "editor.runtime.upsert_sound_variable",
                "Create or update named sound variables and playback params",
                "egui://editor.triggers/upsert_sound_variable",
                "litd.editor.runtime.read_sound_editor",
            ),
            mut_tool(
                "editor.runtime.preview_sound",
                "Start, stop, or seek sound editor preview transport",
                "egui://editor.triggers/preview_sound",
                "litd.editor.runtime.read_sound_editor",
            ),
            mut_tool(
                "editor.triggers.set_trigger_organization",
                "Set trigger folder/category, enable/disable, initially-on, and comment",
                "egui://editor.triggers/set_trigger_organization",
                "litd.editor.triggers.read_trigger_runtime",
            ),
            read_tool(
                "editor.triggers.query_cross_ref",
                "Cross-reference triggers and objects from persisted trigger state",
                "egui://editor.triggers/query_cross_ref",
            ),
            read_tool(
                "editor.triggers.read_trigger_runtime",
                "Read trigger variables, ECA graphs, and runtime cues",
                "egui://editor.triggers/read_trigger_runtime",
            ),
            read_tool(
                "editor.runtime.read_sound_editor",
                "Read sound library, named sound variables, trigger bindings, and preview transport",
                "egui://editor.triggers/read_sound_editor",
            ),
        ],
    })
}
