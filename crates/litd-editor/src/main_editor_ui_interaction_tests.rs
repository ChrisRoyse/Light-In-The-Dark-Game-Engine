use super::main_editor_world_layers::WorldLayerToggle;
use super::*;

/// Read the shell-state JSON back from the Aster vault at `vault_dir` (the
/// `evidence().state_path`). The shell state is an Aster vault directory, not a flat
/// file, so `fs::read_to_string` on it fails with PermissionDenied (#1489).
fn read_shell_state_json(vault_dir: &str) -> serde_json::Value {
    let bytes =
        litd_editor::editor_state_store::EditorStateStore::new(std::path::PathBuf::from(vault_dir))
            .read_shell_state()
            .expect("read shell state from Aster vault")
            .expect("shell state must exist in the Aster vault");
    serde_json::from_slice(&bytes).expect("shell state must parse as JSON")
}

#[test]
fn minimap_center_click_recenters_camera_and_records_evidence() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let map_rect = egui::Rect::from_min_size(egui::pos2(24.0, 32.0), egui::vec2(240.0, 180.0));
    let expected = runtime.minimap.world_center_xz();

    let navigated = runtime
        .navigate_minimap_from_screen(map_rect, map_rect.center())
        .unwrap();
    let evidence = runtime.evidence();

    assert!(navigated);
    assert!((runtime.plan.viewport.camera.target_world.x - expected[0]).abs() < 0.01);
    assert!((runtime.plan.viewport.camera.target_world.z - expected[1]).abs() < 0.01);
    assert_eq!(runtime.plan.viewport.camera.distance, 150.0);
    assert_eq!(evidence.minimap_navigation_count, 1);
    assert_eq!(
        evidence.last_minimap_navigation,
        format!("{:.1},{:.1}", expected[0], expected[1])
    );
    assert!(
        evidence
            .camera
            .contains(&format!("target={:.1},0.0,{:.1}", expected[0], expected[1]))
    );
    let state_json = read_shell_state_json(&evidence.state_path);
    println!(
        "minimap_click source_of_truth path={} camera={}",
        evidence.state_path, state_json["persistence"]["camera"]
    );
    assert_eq!(
        state_json["persistence"]["camera"]["distance"],
        serde_json::json!(150.0)
    );
}

#[test]
fn world_view_camera_controls_clamp_and_persist_source_of_truth() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();
    let before_json = read_shell_state_json(&before.state_path);
    println!(
        "camera_control before path={} camera={}",
        before.state_path, before_json["persistence"]["camera"]
    );

    let changed = runtime
        .apply_world_view_camera_control(WorldViewCameraControlInput {
            pan_delta_px: EngineVec2::new(20_000.0, -20_000.0),
            orbit_delta_px: EngineVec2::new(120.0, 1_000.0),
            zoom_delta_px: 4_000.0,
        })
        .unwrap();
    let after = runtime.evidence();
    let after_json = read_shell_state_json(&after.state_path);
    println!(
        "camera_control after path={} camera={}",
        after.state_path, after_json["persistence"]["camera"]
    );

    assert!(changed);
    assert_eq!(runtime.plan.viewport.camera.distance, 28.0);
    assert_eq!(runtime.plan.viewport.camera.pitch_degrees, -72.0);
    assert!(runtime.plan.viewport.camera.target_world.x >= -128.0);
    assert!(runtime.plan.viewport.camera.target_world.x <= 128.0);
    assert!(runtime.plan.viewport.camera.target_world.z >= -128.0);
    assert!(runtime.plan.viewport.camera.target_world.z <= 128.0);
    assert_eq!(
        after_json["persistence"]["camera"]["distance"],
        serde_json::json!(28.0)
    );
    assert_eq!(
        after_json["persistence"]["camera"]["pitch_degrees"],
        serde_json::json!(-72.0)
    );
    assert_ne!(
        before_json["persistence"]["camera"],
        after_json["persistence"]["camera"]
    );
    assert!(after.state_save_count > before.state_save_count);
}

#[test]
fn content_browser_asset_drop_persists_world_record_and_evidence() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.create_world_from_draft_command().unwrap();
    let selected = runtime.selected_asset_index();
    let asset_path = runtime.asset_entries[selected].relative_path.clone();
    let pos = viewport_pos_for_world(&runtime, 4.35, 6.6);

    runtime.preview_content_browser_drop(&[selected], pos);
    runtime
        .drop_content_browser_assets(&[selected], pos)
        .unwrap();
    let evidence = runtime.evidence();
    // The world-creation record migrated from a flat JSON file to an Aster vault; read
    // it back through the vault API keyed by the document id (#1489).
    let world_aster_dir = evidence
        .world_creation_store
        .strip_prefix("aster=")
        .expect("world creation store must be an Aster vault")
        .to_string();
    let record_readback = litd_editor::world_creation::read_world_record(
        std::path::Path::new(&world_aster_dir),
        &evidence.world_creation_document_id,
    )
    .unwrap()
    .expect("world record must exist in the Aster vault");
    let record = litd_editor::world_creation::parse_world_record(&record_readback.bytes).unwrap();
    let state_json = serde_json::to_value(&record).unwrap();
    let state_text = serde_json::to_string(&state_json).unwrap();
    let shell_stats = format_shell_stats(&evidence);
    let placed = state_json["document"]["objects"]
        .as_object()
        .unwrap()
        .values()
        .find(|object| object["properties"]["asset.relative_path"] == asset_path)
        .unwrap();

    assert_eq!(evidence.asset_drop_count, 1);
    assert!(evidence.asset_drop_preview.contains("assets=1"));
    assert!(evidence.last_asset_drop.contains("objects=1"));
    assert!(evidence.last_asset_drop_error.is_empty());
    assert!(shell_stats.contains("shell_asset_drop_count=1"));
    assert!(shell_stats.contains("shell_asset_drop_preview=assets=1"));
    assert!(shell_stats.contains("shell_last_asset_drop=objects=1"));
    assert!(shell_stats.contains("shell_last_asset_drop_error=\n"));
    assert!(evidence.world_creation_status.starts_with("placed:"));
    assert!(state_text.contains(&asset_path));
    assert_eq!(
        placed["transform"]["translation"],
        serde_json::json!([4.0, 0.0, 7.0])
    );
    let asset_drop_stats = shell_stats
        .lines()
        .filter(|line| {
            line.starts_with("shell_asset_drop") || line.starts_with("shell_last_asset_drop")
        })
        .collect::<Vec<_>>()
        .join("|");
    println!(
        "issue1294 stats_readback lines={} count={} preview={} last_drop={} world_state_path={}",
        asset_drop_stats,
        evidence.asset_drop_count,
        evidence.asset_drop_preview,
        evidence.last_asset_drop,
        world_aster_dir
    );
}

fn viewport_pos_for_world(runtime: &EditorShellRuntime, x: f32, z: f32) -> egui::Pos2 {
    let rect = runtime.plan.viewport.rect_px;
    let bounds = runtime.plan.viewport.world_bounds;
    let u = (x - bounds.min.x) / (bounds.max.x - bounds.min.x);
    let v = (z - bounds.min.z) / (bounds.max.z - bounds.min.z);
    egui::pos2(
        rect.min.x + rect.size().x * u,
        rect.min.y + rect.size().y * v,
    )
}

#[test]
fn minimap_click_outside_plot_is_noop_with_state_preserved() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.plan.viewport.camera;
    let map_rect = egui::Rect::from_min_size(egui::pos2(24.0, 32.0), egui::vec2(240.0, 180.0));

    let navigated = runtime
        .navigate_minimap_from_screen(
            map_rect,
            egui::pos2(map_rect.left() + 2.0, map_rect.center().y),
        )
        .unwrap();
    let evidence = runtime.evidence();

    assert!(!navigated);
    assert_eq!(runtime.plan.viewport.camera, before);
    assert_eq!(evidence.minimap_navigation_count, 0);
    assert_eq!(evidence.last_minimap_navigation, "none");
}

#[test]
fn minimap_navigation_rejects_degenerate_plot() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let map_rect = egui::Rect::from_min_size(egui::pos2(24.0, 32.0), egui::vec2(8.0, 8.0));

    match runtime.navigate_minimap_from_screen(map_rect, map_rect.center()) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.shell.runtime.minimap.plot");
            assert!(reason.contains("larger than 1 point"));
        }
        other => panic!("expected minimap plot failure, got {other:?}"),
    }
}

#[test]
fn minimap_layer_toggles_change_visible_counts_without_mutating_source() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.minimap_layers.markers = false;
    runtime.minimap_layers.rings = false;
    let evidence = runtime.evidence();

    assert_eq!(
        evidence.minimap_layer_state,
        "areas=true;markers=false;rings=false"
    );
    assert_eq!(evidence.minimap_area_count, 32);
    assert!(evidence.minimap_marker_count > 0);
    assert_eq!(evidence.minimap_ring_count, 2);
    assert_eq!(evidence.minimap_visible_area_count, 32);
    assert_eq!(evidence.minimap_visible_marker_count, 0);
    assert_eq!(evidence.minimap_visible_ring_count, 0);
}

#[test]
fn world_foliage_toggle_changes_visible_layer_state_without_mutating_sources() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();

    runtime.set_world_foliage_visible(false).unwrap();
    let after = runtime.evidence();

    assert_eq!(
        before.world_layer_state,
        WorldLayerVisibility::default().summary()
    );
    assert!(before.world_foliage_visible);
    assert!(after.world_layer_state.contains("foliage=false"));
    assert!(after.world_layer_state.contains("terrain=true"));
    assert!(!after.world_foliage_visible);
    assert_eq!(after.minimap_ed11_foliage_patch_marker_count, 18);
    assert_eq!(after.minimap_marker_count, before.minimap_marker_count);
}

#[test]
fn world_view_toggles_cover_geometry_and_display_state_without_mutating_sources() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();

    runtime
        .set_world_layer_visible(WorldLayerToggle::Terrain, false)
        .unwrap();
    runtime
        .set_world_layer_visible(WorldLayerToggle::FogEffects, false)
        .unwrap();
    runtime
        .set_world_layer_visible(WorldLayerToggle::LetterboxMode, true)
        .unwrap();
    let after = runtime.evidence();

    assert!(before.world_terrain_visible);
    assert!(before.world_fog_effects_visible);
    assert!(!before.world_letterbox_mode);
    assert!(!after.world_terrain_visible);
    assert!(!after.world_fog_effects_visible);
    assert!(after.world_letterbox_mode);
    assert!(after.world_layer_state.contains("terrain=false"));
    assert!(after.world_layer_state.contains("fog_effects=false"));
    assert!(after.world_layer_state.contains("letterbox_mode=true"));
    assert_eq!(after.minimap_feature_count, before.minimap_feature_count);
    assert_eq!(after.minimap_area_count, before.minimap_area_count);
    assert_eq!(after.minimap_marker_count, before.minimap_marker_count);
}

#[test]
fn lock_to_game_camera_snaps_real_play_camera_and_persists_layer_state() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();

    runtime
        .set_world_layer_visible(WorldLayerToggle::LockToGameCamera, true)
        .unwrap();
    let after = runtime.evidence();

    assert!(!before.world_lock_to_game_camera);
    assert!(after.world_lock_to_game_camera);
    assert_ne!(after.camera, before.camera);
    assert_eq!(
        after.camera,
        "target=3.0,0.0,-6.0;distance=104.0;yaw=0.0;pitch=-82.0"
    );
    assert!(after.world_layer_state.contains("lock_to_game_camera=true"));
    assert_eq!(after.play_session, "none");
    assert_eq!(after.state_save_count, before.state_save_count + 2);
}

#[test]
fn inspector_popout_and_reattach_are_persisted_in_plan() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();

    runtime.pop_out_inspector().unwrap();
    let popped = runtime.evidence();
    assert!(!popped.inspector_docked);
    assert_eq!(popped.inspector_popout_count, 1);

    runtime.reattach_inspector().unwrap();
    let docked = runtime.evidence();
    assert!(docked.inspector_docked);
    assert_eq!(docked.inspector_reattach_count, 1);
}

#[test]
fn resize_preserves_mode_and_popout_state() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.switch_mode(EditorMode::Vfx).unwrap();
    runtime.pop_out_inspector().unwrap();

    runtime.rebuild_for_size(960.0, 540.0).unwrap();
    let evidence = runtime.evidence();

    assert_eq!(evidence.active_mode, EditorMode::Vfx);
    assert_eq!(evidence.active_inspector_title, "VFX Graph");
    assert!(!evidence.inspector_docked);
    assert_eq!(evidence.viewport_rect, "205.0,37.0->774.0,410.0");
}

#[test]
fn rebuild_for_wide_window_moves_docked_chrome_to_real_bounds() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();

    runtime.rebuild_for_size(4096.0, 1669.0).unwrap();
    let right = runtime
        .plan
        .panel(DockPanelKind::RightInspector)
        .unwrap()
        .rect_px;
    let status = runtime
        .plan
        .panel(DockPanelKind::StatusStrip)
        .unwrap()
        .rect_px;
    let viewport = runtime.plan.viewport.rect_px;

    assert!((right.max.x - 4096.0).abs() < 0.01);
    assert!((status.max.x - 4096.0).abs() < 0.01);
    assert!((status.max.y - 1669.0).abs() < 0.01);
    assert!(right.min.x > 3000.0);
    assert_eq!(viewport.max.x, right.min.x);
}

#[test]
fn high_dpi_shell_reports_points_and_physical_viewport_separately() {
    let (logical_width, logical_height) =
        logical_surface_size_from_physical(5120, 2054, 2.5).unwrap();
    let report = default_scene_report_for_tests().unwrap();
    let runtime =
        EditorShellRuntime::new_with_world_scene_report(logical_width, logical_height, &report)
            .unwrap();
    let evidence = runtime.evidence_for_render_target(5120, 2054, 2.5).unwrap();
    let viewport = runtime.viewport_rect_for_surface(5120, 2054, 2.5).unwrap();

    assert_eq!(evidence.layout_units, "egui_points");
    assert_eq!(evidence.logical_surface_size, "2048.0x821.6");
    assert_eq!(evidence.physical_surface_size, "5120x2054");
    assert!((evidence.pixels_per_point - 2.5).abs() < f32::EPSILON);
    assert!(evidence.dock_panels.contains("LeftHierarchy:0.0,56.3"));
    assert!(
        evidence
            .dock_panels_physical
            .contains("LeftHierarchy:0.0,140.7")
    );
    assert_eq!(evidence.viewport_rect, "311.9,56.3->1765.0,623.8");
    assert_eq!(
        evidence.viewport_rect_physical,
        "779.8,140.7->4412.5,1559.5"
    );
    assert_eq!(evidence.viewport_capture_region, "780,141,3632x1418");
    assert_eq!(viewport.x, 780);
    assert_eq!(viewport.y, 141);
    assert_eq!(viewport.x + viewport.width, 4412);
    assert_eq!(viewport.y + viewport.height, 1559);
}

#[test]
fn content_browser_tiles_fit_inside_minimum_height() {
    let tile = main_editor_content_browser::content_browser_tile_metrics(96.0);
    let total_height = content_browser_tile_total_height(tile);

    assert!(tile.label_height <= 16.0);
    assert!(total_height <= 96.0);
    assert!(tile.item_width <= 78.0);
}

#[test]
fn content_browser_tiles_keep_two_line_labels_when_roomy() {
    let tile = main_editor_content_browser::content_browser_tile_metrics(160.0);
    let total_height = content_browser_tile_total_height(tile);

    assert_eq!(tile.label_height, 32.0);
    assert!(total_height <= 160.0);
    assert_eq!(tile.item_width, 86.0);
}

#[test]
fn content_browser_tiles_do_not_overflow_zero_height() {
    let tile = main_editor_content_browser::content_browser_tile_metrics(0.0);

    assert_eq!(content_browser_tile_total_height(tile), 0.0);
    assert_eq!(tile.icon_size, 0.0);
    assert_eq!(tile.label_height, 0.0);
}

fn content_browser_tile_total_height(
    tile: main_editor_content_browser::ContentBrowserTileMetrics,
) -> f32 {
    tile.icon_size + tile.label_height + if tile.label_height > 0.0 { 4.0 } else { 0.0 }
}
