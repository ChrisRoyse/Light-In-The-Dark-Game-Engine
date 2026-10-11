use super::*;

#[test]
fn callable_registry_matches_domain_catalog() {
    let registry = default_mcp_tool_registry().unwrap();
    // 90 (post #812 evaluate_region_transitions) + the #862 no-code PBR authoring pair
    // (author + read) in render_materials (-> 92) + the #853 project_playtest campaign
    // trio (author_campaign, author_campaign_chapter, read_campaign; -> 95, project 15 -> 18)
    // + four #892/#901 terrain_world tools (author_timeline, evaluate_timeline, list_templates,
    // clone_template; -> 99) + #818/#829 lower_unit + read_lowered_unit in
    // object_game_data (-> 101) + the #835 trigger_runtime pair
    // (set_trigger_organization, query_cross_ref; -> 103) + the #819/#820 object_game_data
    // quartet (lower_ability, read_lowered_ability, lower_hero, read_lowered_hero; -> 107)
    // + the #821/#822 object_game_data quartet (lower_item, read_lowered_item, lower_shop,
    // read_lowered_shop; -> 111) + the #825/#827 object_game_data quartet
    // (lower_tech_requirements, read_lowered_tech_requirements, lower_gameplay_constants,
    // read_lowered_gameplay_constants; -> 115) + the #823/#824/#826/#828 eight object_game_data
    // #829-valve readback tools (four lower_*/read_lowered_* pairs; -> 123).
    assert_eq!(registry.entries.len(), 123);
    assert!(registry.entry(PAINT_TILES_TOOL).is_ok());
    assert!(registry.entry(UNDO_TOOL).is_ok());
    assert!(registry.entry(READ_HARNESS_TOOL).is_ok());
    assert!(registry.entry(AUTHOR_TIMELINE_TOOL).is_ok());
    assert!(registry.entry(CLONE_TEMPLATE_TOOL).is_ok());
    // The #862 material tools land in the render_materials domain alongside the parallel
    // engine.render.set_pbr_material scene-binding path.
    assert_eq!(
        registry.entry(AUTHOR_PBR_MATERIAL_TOOL).unwrap().domain_id,
        "render_materials"
    );
    assert_eq!(
        registry.entry(READ_PBR_MATERIAL_TOOL).unwrap().domain_id,
        "render_materials"
    );
    assert_eq!(
        registry.entry(SET_PBR_MATERIAL_TOOL).unwrap().domain_id,
        "render_materials"
    );
    assert_eq!(
        registry.domain_counts().get("terrain_world").copied(),
        Some(10)
    );
    // object_game_data: copy_base_object, apply_object_delta, upsert_gameplay_tables, author_race,
    // read_object_data (5) + #818/#829 lower_unit + read_lowered_unit (2) + #819/#820 lower_ability,
    // read_lowered_ability, lower_hero, read_lowered_hero (4) + #821 lower_item + read_lowered_item
    // (2) + #822 lower_shop + read_lowered_shop (2) + #825/#827 lower/read_lowered
    // tech_requirements + gameplay_constants (4) + #823/#824/#826/#828 eight #829-valve readback
    // tools (lower_buff/read_lowered_buff, lower_upgrade/..., lower_destructible_doodad/...,
    // lower_game_interface/...) = 27.
    assert_eq!(
        registry.domain_counts().get("object_game_data").copied(),
        Some(27)
    );
    assert_eq!(
        registry.domain_counts().get("editor_session").copied(),
        Some(8)
    );
    assert_eq!(
        registry.domain_counts().get("project_playtest").copied(),
        Some(18)
    );
}

#[test]
fn dispatch_routes_material_tool_to_material_handler_not_render() {
    let registry = default_mcp_tool_registry().unwrap();
    // Malformed args make the material handler's arg parse fail loud. The error FIELD proves the
    // call reached call_mcp_material_tool (editor.material_editor_mcp.*) and not the generic render
    // fan-out (editor.mcp_render_tools.*), so this verifies dispatch routing for a tool that shares
    // the render_materials domain with engine.render.set_pbr_material.
    let error = dispatch_mcp_tool_call(
        &registry,
        AUTHOR_PBR_MATERIAL_TOOL,
        Value::Object(Default::default()),
    )
    .unwrap_err();
    match error {
        EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { field, .. },
        ) => {
            assert_eq!(field, "editor.material_editor_mcp.arguments");
        }
        other => panic!("expected material handler error, got {other:?}"),
    }
}

#[test]
fn renamed_dispatcher_constant_fails_registry_validation() {
    let manifest = default_mcp_domain_toolset_manifest().unwrap();
    let mut implemented = implemented_mcp_tool_names();
    let paint_index = implemented
        .iter()
        .position(|name| name == PAINT_TILES_TOOL)
        .expect("paint tool should be in implemented set");
    implemented[paint_index] = "litd.editor.world.paint_tiles_renamed".to_string();

    let error = validate_mcp_tool_dispatch_surface(&manifest, &implemented).unwrap_err();
    match error {
        EngineError::UiRenderInvalidConfig { field, reason } => {
            assert_eq!(field, "editor.mcp_tool_registry.dispatch_surface");
            assert!(reason.contains(PAINT_TILES_TOOL));
            assert!(reason.contains("paint_tiles_renamed"));
        }
        other => panic!("expected registry drift error, got {other:?}"),
    }
}

#[test]
fn duplicate_dispatcher_name_fails_registry_validation() {
    let manifest = default_mcp_domain_toolset_manifest().unwrap();
    let mut implemented = implemented_mcp_tool_names();
    implemented.push(PAINT_TILES_TOOL.to_string());

    let error = validate_mcp_tool_dispatch_surface(&manifest, &implemented).unwrap_err();
    match error {
        EngineError::UiRenderInvalidConfig { field, reason } => {
            assert_eq!(field, "editor.mcp_tool_registry.implemented_names");
            assert!(reason.contains(PAINT_TILES_TOOL));
        }
        other => panic!("expected duplicate registry error, got {other:?}"),
    }
}
