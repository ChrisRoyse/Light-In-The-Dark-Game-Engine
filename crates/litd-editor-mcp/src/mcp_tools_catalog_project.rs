//! Project lifecycle and playtest MCP catalog entries.

use super::super::{
    McpToolDomainGroup,
    mcp_tools_builders::{DomainSpec, domain, mut_tool, read_tool},
};

pub(super) fn project_playtest_domain() -> McpToolDomainGroup {
    domain(DomainSpec {
        domain_id: "project_playtest",
        title: "Project lifecycle, playtest, package, delivery, multiplayer, social, ranking, update, moderation, and server browser tools",
        child_issue: 926,
        source_issues: &[
            738, 805, 1042, 1156, 1157, 1158, 1159, 1160, 1161, 1162, 1163,
        ],
        capability_namespaces: &["editor.play", "editor.project"],
        gui_surface: "egui://editor.project",
        mcp_prefixes: &["litd.editor.play", "litd.editor.project"],
        default_readback_tool_name: "litd.editor.project.read_project",
        readme_refs: &[
            "docs/readme/ED09-test-in-editor.png",
            "docs/readme/ED10-publish-ecosystem.png",
            "docs/readme/ED12-play-topdown.png",
            "docs/readme/ED13-play-firstperson.png",
            "docs/readme/S13-multiplayer-host.png",
        ],
        tools: vec![
            mut_tool(
                "editor.project.new_open_save",
                "Create, open, save, and snapshot LitD world projects",
                "egui://editor.project/new_open_save",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.play.run_headless_playtest",
                "Run deterministic headless playtests and return verdict artifacts",
                "egui://editor.project/run_headless_playtest",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.package_litdgame",
                "Package a sealed ecosystem-bound .litdgame artifact",
                "egui://editor.project/package_litdgame",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.publish_litdgame",
                "Package and publish a sealed .litdgame artifact into the local ecosystem catalog",
                "egui://editor.project/publish_litdgame",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.deliver_litdgame",
                "Store and install a published .litdgame by verified content address",
                "egui://editor.project/deliver_litdgame",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.open_hub_launcher",
                "Browse, search, inspect install status, and launch installed community games",
                "egui://editor.project/hub_launcher",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.manage_multiplayer_session",
                "Host, join, party, and matchmake local-first multiplayer sessions",
                "egui://editor.project/multiplayer_session",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.manage_social_graph",
                "Manage local-first accounts, profiles, friends, presence, and game invites",
                "egui://editor.project/social_graph",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.manage_leaderboards_reviews",
                "Manage local-first ladders, leaderboard scores, ratings, reviews, and review votes",
                "egui://editor.project/leaderboards_reviews",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.manage_game_updates",
                "Manage local-first game update releases, compatibility checks, and update notifications",
                "egui://editor.project/game_updates",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.manage_moderation_safety",
                "Manage local-first reports, sanctions, appeals, and server admin controls",
                "egui://editor.project/moderation_safety",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.evaluate_content_trust",
                "Evaluate shared-content trust before ecosystem import and persist the decision ledger",
                "egui://editor.project/content_trust",
                "litd.editor.project.read_content_trust",
            ),
            mut_tool(
                "editor.project.manage_server_browser",
                "Publish and query local-first public/private server browser listings",
                "egui://editor.project/server_browser",
                "litd.editor.project.read_project",
            ),
            mut_tool(
                "editor.project.author_campaign",
                "Author a single-player campaign (metadata, shared object data, hero carry-over slots) and persist it to Calyx (#853)",
                "egui://editor.project/author_campaign",
                "litd.editor.project.read_campaign",
            ),
            mut_tool(
                "editor.project.author_campaign_chapter",
                "Insert or update a campaign chapter (map ref, unlock flow, branch-on-victory) and re-persist the campaign (#853)",
                "egui://editor.project/author_campaign_chapter",
                "litd.editor.project.read_campaign",
            ),
            read_tool(
                "editor.project.read_campaign",
                "Read the persisted campaign structure and write ledger from the Calyx Aster campaign vault (#853)",
                "egui://editor.project/read_campaign",
            ),
            read_tool(
                "editor.project.read_content_trust",
                "Read persisted shared-content trust ledger decisions from Aster",
                "egui://editor.project/content_trust/readback",
            ),
            read_tool(
                "editor.project.read_project",
                "Read project lifecycle, playtest, replay, and package artifacts",
                "egui://editor.project/read_project",
            ),
        ],
    })
}
