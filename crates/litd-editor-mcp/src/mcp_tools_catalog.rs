//! Default MCP domain tool catalog for issue #1145.

#[path = "mcp_tools_catalog_editor_session.rs"]
mod mcp_tools_catalog_editor_session;
#[path = "mcp_tools_catalog_harness.rs"]
mod mcp_tools_catalog_harness;
#[path = "mcp_tools_catalog_object_game_data.rs"]
mod mcp_tools_catalog_object_game_data;
#[path = "mcp_tools_catalog_project.rs"]
mod mcp_tools_catalog_project;
#[path = "mcp_tools_catalog_trigger_runtime.rs"]
mod mcp_tools_catalog_trigger_runtime;

use super::{
    McpToolDomainGroup,
    mcp_tools_builders::{DomainSpec, domain, mut_tool, read_tool},
};

pub(super) fn default_domains() -> Vec<McpToolDomainGroup> {
    vec![
        domain(DomainSpec {
            domain_id: "terrain_world",
            title: "Terrain and world-building tools",
            child_issue: 918,
            source_issues: &[801],
            capability_namespaces: &["editor.world"],
            gui_surface: "egui://editor.world",
            mcp_prefixes: &["litd.editor.world"],
            default_readback_tool_name: "litd.editor.world.read_world",
            readme_refs: &[
                "docs/readme/04-editor.png",
                "docs/readme/ED02-terrain-editor.png",
                "docs/readme/ED11-world-view.png",
            ],
            tools: vec![
                mut_tool(
                    "editor.world.paint_tiles",
                    "Paint tiles, cliffs, water, and pathing",
                    "egui://editor.world/paint_tiles",
                    "litd.editor.world.read_world",
                ),
                mut_tool(
                    "editor.world.bulk_edit_terrain",
                    "Run bulk terrain and world-object operations",
                    "egui://editor.world/bulk_edit_terrain",
                    "litd.editor.world.read_world",
                ),
                mut_tool(
                    "editor.world.place_actor_region_camera",
                    "Place doodads, destructibles, regions, and cameras",
                    "egui://editor.world/place_actor_region_camera",
                    "litd.editor.world.read_world",
                ),
                mut_tool(
                    "editor.world.set_start_locations",
                    "Configure player start locations",
                    "egui://editor.world/set_start_locations",
                    "litd.editor.world.read_world",
                ),
                read_tool(
                    "editor.world.read_world",
                    "Read lowered terrain, doodads, regions, cameras, and starts",
                    "egui://editor.world/read_world",
                ),
                read_tool(
                    "editor.world.evaluate_region_transitions",
                    "Evaluate deterministic region enter/leave events for a movement",
                    "egui://editor.world/evaluate_region_transitions",
                ),
                mut_tool(
                    "editor.world.author_timeline",
                    "Author a no-code cinematic timeline (camera moves, transmissions, fades, subtitles, cues)",
                    "egui://editor.world/author_timeline",
                    "litd.editor.world.read_world",
                ),
                read_tool(
                    "editor.world.evaluate_timeline",
                    "Deterministically sample an authored cinematic timeline at given times",
                    "egui://editor.world/evaluate_timeline",
                ),
                read_tool(
                    "editor.world.list_templates",
                    "List opinionated starter templates to browse, clone, and dissect",
                    "egui://editor.world/list_templates",
                ),
                mut_tool(
                    "editor.world.clone_template",
                    "Deep-copy a starter template into a fresh editable project",
                    "egui://editor.world/clone_template",
                    "litd.editor.world.read_world",
                ),
            ],
        }),
        mcp_tools_catalog_object_game_data::object_game_data_domain(),
        mcp_tools_catalog_trigger_runtime::trigger_runtime_domain(),
        domain(DomainSpec {
            domain_id: "visual_story",
            title: "Visual-story and narrative tools",
            child_issue: 921,
            source_issues: &[886, 1048, 1049, 1050, 1051, 1052, 1053, 1054],
            capability_namespaces: &["editor.story", "editor.lore"],
            gui_surface: "egui://editor.story",
            mcp_prefixes: &["litd.editor.story", "litd.editor.lore"],
            default_readback_tool_name: "litd.editor.story.read_story",
            readme_refs: &[
                "docs/readme/02-storytelling.png",
                "docs/readme/ED07-cinematic-director.png",
                "docs/readme/GP10-dialogue.png",
                "docs/readme/GP11-quest.png",
                "docs/readme/S16-quest-designer.png",
            ],
            tools: vec![
                mut_tool(
                    "editor.story.upsert_dialogue_quest",
                    "Author branching dialogue, quests, and character relationships",
                    "egui://editor.story/upsert_dialogue_quest",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.story.stage_cinematic",
                    "Direct staging, mood, camera, and cinematic beats",
                    "egui://editor.story/stage_cinematic",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.lore.upsert_lore_context",
                    "Ground lore constellations and surfaced story context",
                    "egui://editor.story/upsert_lore_context",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.lore.upsert_lore_entry",
                    "Author structured world-bible entries with links, metadata, and timelines",
                    "egui://editor.story/upsert_lore_entry",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.lore.build_relationship_graph",
                    "Build a navigable lore relationship graph from world-bible links",
                    "egui://editor.story/build_lore_relationship_graph",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.lore.upsert_lore_surface",
                    "Surface lore through books, notes, inscriptions, dialogue, codex, regions, and props",
                    "egui://editor.story/upsert_lore_surface",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.lore.query_canon",
                    "Search canon, flag contradictions, and answer grounded lore questions",
                    "egui://editor.story/query_lore_canon",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.lore.co_write",
                    "Draft original lore grounded in canon, Ward-gated, and lowered to data",
                    "egui://editor.story/co_write_lore",
                    "litd.editor.story.read_story",
                ),
                mut_tool(
                    "editor.lore.generate_template",
                    "Generate original pantheon, cabal, myth, and fallen-empire lore templates",
                    "egui://editor.story/generate_lore_template",
                    "litd.editor.story.read_story",
                ),
                read_tool(
                    "editor.story.read_story",
                    "Read dialogue, quest, cinematic, lore, and world-bible state",
                    "egui://editor.story/read_story",
                ),
            ],
        }),
        domain(DomainSpec {
            domain_id: "vfx_particles",
            title: "VFX and particle-authoring tools",
            child_issue: 922,
            source_issues: &[744, 1128],
            capability_namespaces: &["engine.vfx"],
            gui_surface: "egui://editor.vfx",
            mcp_prefixes: &["litd.engine.vfx"],
            default_readback_tool_name: "litd.engine.vfx.read_vfx",
            readme_refs: &["docs/readme/09-vfx.png", "docs/readme/ED08-vfx-editor.png"],
            tools: vec![
                mut_tool(
                    "engine.vfx.describe_effect_asset",
                    "Create an effect asset from a textual description",
                    "egui://editor.vfx/describe_effect_asset",
                    "litd.engine.vfx.read_vfx",
                ),
                mut_tool(
                    "engine.vfx.set_particle_stack",
                    "Set emitter stacks, curves, color ramps, and blend modes",
                    "egui://editor.vfx/set_particle_stack",
                    "litd.engine.vfx.read_vfx",
                ),
                mut_tool(
                    "engine.vfx.attach_effect",
                    "Attach effects to abilities, events, scene beats, or objects",
                    "egui://editor.vfx/attach_effect",
                    "litd.engine.vfx.read_vfx",
                ),
                read_tool(
                    "engine.vfx.read_vfx",
                    "Read effect assets, emitters, attachments, and budgets",
                    "egui://editor.vfx/read_vfx",
                ),
            ],
        }),
        domain(DomainSpec {
            domain_id: "render_materials",
            title: "Custom Rust renderer, material, animation, and scene tools",
            child_issue: 923,
            source_issues: &[872, 1085, 1098, 1104, 862, 915],
            // engine.render is the runtime scene-binding path; editor.material (#862/#915) is the
            // no-code PBR authoring path that persists through the real Calyx put_content vault and
            // lowers to a fingerprinted preview uniform. Both live in this domain and cross-reference
            // each other: engine.render.set_pbr_material binds a material into an MCP render state for
            // scene composition, while editor.material.author_pbr_material authors the canonical
            // material record itself (see the tool titles below and mcp_render_tools.rs).
            capability_namespaces: &["engine.render", "editor.material"],
            gui_surface: "egui://editor.render",
            mcp_prefixes: &["litd.engine.render", "litd.editor.material"],
            default_readback_tool_name: "litd.engine.render.read_render",
            readme_refs: &[
                "docs/readme/ED06-material-assets.png",
                "docs/readme/ED11-world-view.png",
                "docs/readme/ED12-play-topdown.png",
                "docs/readme/ED13-play-firstperson.png",
            ],
            tools: vec![
                mut_tool(
                    "engine.render.set_pbr_material",
                    "Set stylized PBR material channels",
                    "egui://editor.render/set_pbr_material",
                    "litd.engine.render.read_render",
                ),
                mut_tool(
                    "engine.render.set_lighting_animation_post",
                    "Set lighting, animation blends, and post effects",
                    "egui://editor.render/set_lighting_animation_post",
                    "litd.engine.render.read_render",
                ),
                mut_tool(
                    "engine.render.compose_scene",
                    "Compose renderable scenes from lowered editor data",
                    "egui://editor.render/compose_scene",
                    "litd.engine.render.read_render",
                ),
                mut_tool(
                    "engine.render.set_shader_gallery",
                    "Pick curated stylized shaders and validate a WGSL escape",
                    "egui://editor.render/set_shader_gallery",
                    "litd.engine.render.read_render",
                ),
                mut_tool(
                    "engine.render.set_primitive_gallery",
                    "Build procedural blockout primitives with triangle, line, and point topology",
                    "egui://editor.render/set_primitive_gallery",
                    "litd.engine.render.read_render",
                ),
                mut_tool(
                    "engine.render.set_sprite_billboard_library",
                    "Set content-addressed sprite materials, sheets, and billboard placements",
                    "egui://editor.render/set_sprite_billboard_library",
                    "litd.engine.render.read_render",
                ),
                mut_tool(
                    "engine.render.set_hud_ui_builder",
                    "Set no-code HUD UI builder regions, widgets, labels, and policy-checked minimap markers",
                    "egui://editor.render/set_hud_ui_builder",
                    "litd.engine.render.read_render",
                ),
                read_tool(
                    "engine.render.read_render",
                    "Read render materials, lights, shaders, primitives, sprites, animation assignments, batches, and budgets",
                    "egui://editor.render/read_render",
                ),
                // No-code PBR material editor (#862/#915). Parallel to engine.render.set_pbr_material
                // above: that tool binds a material into MCP render state for scene composition; this
                // tool authors the canonical material record through the real Calyx put_content path
                // and lowers it to a fingerprinted GPU preview uniform.
                mut_tool(
                    "editor.material.author_pbr_material",
                    "Author a PBR material through the real Calyx put_content path and lower it to a preview uniform (see engine.render.set_pbr_material for scene binding)",
                    "egui://editor.render/author_pbr_material",
                    "litd.editor.material.read_pbr_material",
                ),
                read_tool(
                    "editor.material.read_pbr_material",
                    "Read a persisted PBR material record back from the Calyx content vault",
                    "egui://editor.render/read_pbr_material",
                ),
            ],
        }),
        domain(DomainSpec {
            domain_id: "in_game_agents",
            title: "In-game agent binding tools",
            child_issue: 924,
            source_issues: &[781, 860],
            capability_namespaces: &["editor.agents"],
            gui_surface: "egui://editor.agents",
            mcp_prefixes: &["litd.editor.agents"],
            default_readback_tool_name: "litd.editor.agents.read_agents",
            readme_refs: &[
                "docs/readme/03-mcp-agent.png",
                "docs/readme/06-living-agents.png",
                "docs/readme/S17-agent-binding.png",
            ],
            tools: vec![
                mut_tool(
                    "editor.agents.author_agent",
                    "No-code prompt, model, object, visual, and Ward authoring in one guarded action",
                    "egui://editor.agents/author_agent",
                    "litd.editor.agents.read_agents",
                ),
                mut_tool(
                    "editor.agents.bind_agent",
                    "Bind an AI agent brain to a world object",
                    "egui://editor.agents/bind_agent",
                    "litd.editor.agents.read_agents",
                ),
                mut_tool(
                    "editor.agents.set_prompt_model",
                    "Set prompt, model, personality, and activation policy",
                    "egui://editor.agents/set_prompt_model",
                    "litd.editor.agents.read_agents",
                ),
                mut_tool(
                    "editor.agents.set_ward_envelope",
                    "Configure Ward physics, visual, and behavior envelopes",
                    "egui://editor.agents/set_ward_envelope",
                    "litd.editor.agents.read_agents",
                ),
                read_tool(
                    "editor.agents.read_agents",
                    "Read agent bindings, prompts, models, and Ward envelopes",
                    "egui://editor.agents/read_agents",
                ),
            ],
        }),
        domain(DomainSpec {
            domain_id: "asset_pipeline",
            title: "AI asset-generation and ingest tools",
            child_issue: 925,
            source_issues: &[782, 1098, 1104],
            capability_namespaces: &["editor.assets"],
            gui_surface: "egui://editor.assets",
            mcp_prefixes: &["litd.editor.assets"],
            default_readback_tool_name: "litd.editor.assets.read_assets",
            readme_refs: &[
                "docs/readme/07-asset-pipeline.png",
                "docs/readme/ED06-material-assets.png",
            ],
            tools: vec![
                mut_tool(
                    "editor.assets.generate_import_asset",
                    "Generate or import source art, models, textures, and rigs",
                    "egui://editor.assets/generate_import_asset",
                    "litd.editor.assets.read_assets",
                ),
                mut_tool(
                    "editor.assets.rig_texture_animate",
                    "Rig, texture, animate, and prepare gameplay-ready assets",
                    "egui://editor.assets/rig_texture_animate",
                    "litd.editor.assets.read_assets",
                ),
                mut_tool(
                    "editor.assets.conform_ingest_budget",
                    "Conform render budgets and ingest asset metadata to Calyx",
                    "egui://editor.assets/conform_ingest_budget",
                    "litd.editor.assets.read_assets",
                ),
                read_tool(
                    "editor.assets.read_assets",
                    "Read generated assets, import records, budgets, and Calyx ingest ids",
                    "egui://editor.assets/read_assets",
                ),
            ],
        }),
        mcp_tools_catalog_editor_session::editor_session_domain(),
        mcp_tools_catalog_project::project_playtest_domain(),
        mcp_tools_catalog_harness::fsv_introspection_domain(),
        mcp_tools_catalog_harness::agent_harness_domain(),
    ]
}
