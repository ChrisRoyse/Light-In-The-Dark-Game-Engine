use super::*;

fn settings(width: u32, height: u32) -> UiRenderSettings {
    UiRenderSettings {
        viewport_width: width as f32,
        viewport_height: height as f32,
    }
}

#[test]
fn world_view_shell_matches_readme_docked_regions() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();

    assert_eq!(
        plan.mode_tab_summary(),
        "Terrain|Objects|Triggers|Cinematics|VFX|AI Co-Author"
    );
    assert_eq!(plan.dock_panels.len(), 6);
    assert!(plan.reference_image_list().contains("ED11-world-view.png"));
    assert_eq!(
        rect_summary(plan.viewport.rect_px),
        "410.0,74.0->1548.0,820.0"
    );
    assert!(plan.viewport.live_engine_view);
    assert_eq!(plan.viewport.camera, ed11_bird_eye_default_camera());
    assert_eq!(plan.active_inspector.mode, EditorMode::Terrain);
    assert_eq!(plan.active_inspector.title, "Doodad Brush");
    assert_eq!(plan.gizmo.mode, TransformGizmoMode::Translate);
    assert_eq!(plan.gizmo.anchor_px, Vec2::new(979.0, 447.0));
}

#[test]
fn shell_scales_down_from_reference_layout() {
    let plan = build_editor_shell_plan(settings(1280, 720)).unwrap();

    assert_eq!(
        rect_summary(plan.viewport.rect_px),
        "273.3,49.3->1032.0,546.7"
    );
    assert_eq!(
        rect_summary(plan.panel(DockPanelKind::ContentBrowser).unwrap().rect_px),
        "273.3,546.7->1032.0,694.7"
    );
}

#[test]
fn wide_shell_keeps_chrome_at_reference_size_and_expands_center() {
    let plan = build_editor_shell_plan(settings(4096, 1669)).unwrap();

    assert_eq!(
        rect_summary(plan.panel(DockPanelKind::LeftHierarchy).unwrap().rect_px),
        "0.0,74.0->410.0,1631.0"
    );
    assert_eq!(
        rect_summary(plan.panel(DockPanelKind::RightInspector).unwrap().rect_px),
        "3724.0,74.0->4096.0,1335.0"
    );
    assert_eq!(
        rect_summary(plan.viewport.rect_px),
        "410.0,74.0->3724.0,1409.0"
    );
    assert_eq!(
        rect_summary(plan.panel(DockPanelKind::ContentBrowser).unwrap().rect_px),
        "410.0,1409.0->3724.0,1631.0"
    );
    assert_eq!(
        rect_summary(plan.panel(DockPanelKind::Minimap).unwrap().rect_px),
        "3820.0,1335.0->4076.0,1607.0"
    );
    assert_eq!(
        rect_summary(plan.panel(DockPanelKind::StatusStrip).unwrap().rect_px),
        "0.0,1631.0->4096.0,1669.0"
    );
}

#[test]
fn restored_docked_panels_use_current_window_size_not_stale_rects() {
    let original = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let persistence = persist_shell_state(&original).unwrap();
    let restored = restore_shell_state(settings(4096, 1669), &persistence).unwrap();

    let right = restored
        .panel(DockPanelKind::RightInspector)
        .unwrap()
        .rect_px;
    let status = restored.panel(DockPanelKind::StatusStrip).unwrap().rect_px;
    let content = restored
        .panel(DockPanelKind::ContentBrowser)
        .unwrap()
        .rect_px;

    assert!((right.max.x - 4096.0).abs() < 0.01);
    assert!((right.size().x - 372.0).abs() < 0.01);
    assert!((status.max.x - 4096.0).abs() < 0.01);
    assert!((right.min.x - 3724.0).abs() < 0.01);
    assert_eq!(restored.viewport.rect_px.max.x, right.min.x);
    assert_eq!(content.max.x, right.min.x);
    assert!(!rect_summary(right).contains("1588.0"));
    for panel in &restored.dock_panels {
        assert!(panel.rect_px.min.x >= 0.0);
        assert!(panel.rect_px.min.y >= 0.0);
        assert!(panel.rect_px.max.x <= 4096.0);
        assert!(panel.rect_px.max.y <= 1669.0);
    }
}

#[test]
fn floating_panel_restore_is_clamped_inside_smaller_window() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let floating = UiRect::new(Vec2::new(1400.0, 200.0), Vec2::new(1900.0, 1000.0)).unwrap();
    let popped = pop_out_panel(&plan, DockPanelKind::RightInspector, floating).unwrap();
    let persistence = persist_shell_state(&popped).unwrap();
    let restored = restore_shell_state(settings(960, 540), &persistence).unwrap();
    let floating = restored
        .panel(DockPanelKind::RightInspector)
        .unwrap()
        .floating_rect_px
        .unwrap();

    assert!(floating.min.x >= 0.0);
    assert!(floating.min.y >= 0.0);
    assert!(floating.max.x <= 960.0);
    assert!(floating.max.y <= 540.0);
}

#[test]
fn dock_panels_expose_egui_rects() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let hierarchy = plan
        .panel(DockPanelKind::LeftHierarchy)
        .unwrap()
        .egui_rect();
    let viewport = plan.viewport.egui_rect();

    assert_eq!(hierarchy.min, egui::pos2(0.0, 74.0));
    assert_eq!(hierarchy.max, egui::pos2(410.0, 1042.0));
    assert_eq!(viewport.min, egui::pos2(410.0, 74.0));
    assert_eq!(viewport.max, egui::pos2(1548.0, 820.0));
}

#[test]
fn mode_switch_loads_tool_inspector_and_preserves_camera() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let camera = plan.viewport.camera;
    let switched = switch_editor_mode(&plan, EditorMode::AiCoAuthor).unwrap();

    assert_eq!(switched.active_mode, EditorMode::AiCoAuthor);
    assert_eq!(switched.active_inspector.title, "AI Co-Author");
    assert_eq!(switched.active_inspector.tool_id, "editor.tool.ai.coauthor");
    assert_eq!(switched.viewport.camera, camera);
    assert_eq!(switched.viewport.rect_px, plan.viewport.rect_px);
}

#[test]
fn tool_panel_popout_and_reattach_preserve_fixed_world_view_chrome() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let floating = UiRect::new(Vec2::new(1200.0, 120.0), Vec2::new(1820.0, 860.0)).unwrap();
    let popped = pop_out_panel(&plan, DockPanelKind::RightInspector, floating).unwrap();

    let inspector = popped.panel(DockPanelKind::RightInspector).unwrap();
    assert!(!inspector.docked);
    assert_eq!(inspector.floating_rect_px, Some(floating));
    assert!(popped.panel(DockPanelKind::TopModeBar).unwrap().docked);
    assert!(popped.panel(DockPanelKind::StatusStrip).unwrap().docked);
    assert_eq!(popped.viewport.rect_px, plan.viewport.rect_px);

    let reattached = reattach_panel(&popped, DockPanelKind::RightInspector).unwrap();
    let inspector = reattached.panel(DockPanelKind::RightInspector).unwrap();
    assert!(inspector.docked);
    assert_eq!(inspector.floating_rect_px, None);
}

#[test]
fn dock_persistence_restores_mode_camera_and_floating_panel() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let switched = switch_editor_mode(&plan, EditorMode::Vfx).unwrap();
    let floating = UiRect::new(Vec2::new(1180.0, 120.0), Vec2::new(1840.0, 840.0)).unwrap();
    let popped = pop_out_panel(&switched, DockPanelKind::RightInspector, floating).unwrap();
    let persistence = persist_shell_state(&popped).unwrap();
    let restored = restore_shell_state(settings(1920, 1080), &persistence).unwrap();

    assert_eq!(restored.active_mode, EditorMode::Vfx);
    assert_eq!(restored.active_inspector.title, "VFX Graph");
    assert_eq!(restored.viewport.camera, popped.viewport.camera);
    let inspector = restored.panel(DockPanelKind::RightInspector).unwrap();
    assert!(!inspector.docked);
    assert_eq!(inspector.floating_rect_px, Some(floating));
}

#[test]
fn small_screen_reflow_keeps_all_regions_visible() {
    let plan = build_editor_shell_plan(settings(960, 540)).unwrap();

    assert_eq!(
        rect_summary(plan.viewport.rect_px),
        "205.0,37.0->774.0,410.0"
    );
    for panel in &plan.dock_panels {
        assert!(panel.rect_px.min.x >= 0.0);
        assert!(panel.rect_px.min.y >= 0.0);
        assert!(panel.rect_px.max.x <= 960.0);
        assert!(panel.rect_px.max.y <= 540.0);
        assert!(panel.rect_px.size().x > 0.0);
        assert!(panel.rect_px.size().y > 0.0);
    }

    let left = plan.panel(DockPanelKind::LeftHierarchy).unwrap().rect_px;
    let status = plan.panel(DockPanelKind::StatusStrip).unwrap().rect_px;
    let right = plan.panel(DockPanelKind::RightInspector).unwrap().rect_px;
    let minimap = plan.panel(DockPanelKind::Minimap).unwrap().rect_px;

    assert!(left.max.y <= status.min.y);
    assert!(right.max.y <= minimap.min.y);
    assert!(minimap.max.y <= status.min.y);
    assert!(plan.viewport.rect_px.size().x >= 480.0);
    assert!(plan.viewport.rect_px.size().y >= 300.0);
}

#[test]
fn fixed_chrome_popout_fails_loud() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let floating = UiRect::new(Vec2::new(100.0, 100.0), Vec2::new(500.0, 300.0)).unwrap();

    match pop_out_panel(&plan, DockPanelKind::TopModeBar, floating) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.shell.dock_panels.popout");
            assert!(reason.contains("fixed"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn snapping_rounds_gizmo_deltas() {
    let plan = build_editor_shell_plan(settings(1920, 1080)).unwrap();
    let snapped = plan
        .gizmo
        .snap
        .snap_translation_delta(Vec3::new(1.24, 0.0, -2.76));

    assert_eq!(snapped, Vec3::new(1.0, 0.0, -3.0));
    assert_eq!(plan.gizmo.snap.snap_rotation_degrees(22.0), 15.0);
    assert!((plan.gizmo.snap.snap_scale_factor(1.16) - 1.2).abs() < 0.001);
}

#[test]
fn invalid_viewport_fails_loud() {
    match build_editor_shell_plan(settings(0, 720)) {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => assert_eq!(field, "viewport"),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}
