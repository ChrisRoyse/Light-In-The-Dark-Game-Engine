//! Harness and full-state-verification MCP catalog entries.

use super::super::{
    McpToolDomainGroup,
    mcp_tools_builders::{DomainSpec, domain, mut_tool, read_tool},
};

pub(super) fn agent_harness_domain() -> McpToolDomainGroup {
    domain(DomainSpec {
        domain_id: "agent_harness",
        title: "Agent playtest harness tools",
        child_issue: 1178,
        source_issues: &[1167, 1169, 1170, 1171, 1172, 1173, 1174, 1175, 1176, 1177],
        capability_namespaces: &["editor.play", "editor.fsv", "editor.agents"],
        gui_surface: "egui://editor.harness",
        mcp_prefixes: &["litd.editor.harness"],
        default_readback_tool_name: "litd.editor.harness.read_harness",
        readme_refs: &[
            "docs/readme/03-mcp-agent.png",
            "docs/readme/10-architecture.png",
            "docs/readme/ED09-test-in-editor.png",
        ],
        tools: vec![
            mut_tool(
                "editor.harness.run_headless",
                "Run a deterministic no-render fixed-tick or until-condition harness simulation",
                "egui://editor.harness/run_headless",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.run_sandboxes",
                "Run isolated deterministic sandbox variants from one stored run",
                "egui://editor.harness/run_sandboxes",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.snapshot_restore_branch",
                "Restore a verified snapshot and branch the simulation",
                "egui://editor.harness/snapshot_restore_branch",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.assert_state",
                "Evaluate declarative assertions against stored run state",
                "egui://editor.harness/assert_state",
                "litd.editor.harness.read_harness",
            ),
            read_tool(
                "editor.harness.read_events",
                "Read harness event records from persisted state",
                "egui://editor.harness/read_events",
            ),
            mut_tool(
                "editor.harness.run_oracles",
                "Run deterministic and frame-watch oracles against stored state",
                "egui://editor.harness/run_oracles",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.drive_bot",
                "Drive deterministic virtual-player bots and persist metrics, trajectories, and replay proof",
                "egui://editor.harness/drive_bot",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.screenshot_diff",
                "Render or read screenshots and persist perceptual pixel-diff verdicts",
                "egui://editor.harness/screenshot_diff",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.calyx_signals",
                "Persist typed Calyx pre-run signals over authored content",
                "egui://editor.harness/calyx_signals",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.emit_feedback",
                "Emit structured fix-oriented feedback into the harness Loom stream",
                "egui://editor.harness/emit_feedback",
                "litd.editor.harness.read_harness",
            ),
            mut_tool(
                "editor.harness.run_pev",
                "Run a transactional plan-execute-verify harness loop",
                "egui://editor.harness/run_pev",
                "litd.editor.harness.read_harness",
            ),
            read_tool(
                "editor.harness.read_harness",
                "Read persisted harness state, verdicts, events, and ledgers",
                "egui://editor.harness/read_harness",
            ),
        ],
    })
}

pub(super) fn fsv_introspection_domain() -> McpToolDomainGroup {
    domain(DomainSpec {
        domain_id: "fsv_introspection",
        title: "FSV and introspection readback tools",
        child_issue: 927,
        source_issues: &[800, 927],
        capability_namespaces: &["editor.fsv"],
        gui_surface: "egui://editor.fsv",
        mcp_prefixes: &["litd.editor.fsv"],
        default_readback_tool_name: "litd.editor.fsv.read_project_artifacts",
        readme_refs: &[
            "docs/readme/03-mcp-agent.png",
            "docs/readme/10-architecture.png",
            "docs/readme/ED09-test-in-editor.png",
        ],
        tools: vec![
            read_tool(
                "editor.fsv.read_world_state",
                "Read the real world-state source of truth",
                "egui://editor.fsv/read_world_state",
            ),
            read_tool(
                "editor.fsv.read_object_state",
                "Read object, RPG, and economy state",
                "egui://editor.fsv/read_object_state",
            ),
            read_tool(
                "editor.fsv.read_trigger_runtime_state",
                "Read trigger and runtime state",
                "egui://editor.fsv/read_trigger_runtime_state",
            ),
            read_tool(
                "editor.fsv.read_story_render_state",
                "Read story, VFX, render, asset, and agent state",
                "egui://editor.fsv/read_story_render_state",
            ),
            read_tool(
                "editor.fsv.read_project_artifacts",
                "Read project, playtest, package, hash, and ledger artifacts",
                "egui://editor.fsv/read_project_artifacts",
            ),
        ],
    })
}
