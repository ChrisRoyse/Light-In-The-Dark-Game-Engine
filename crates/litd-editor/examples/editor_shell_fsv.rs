use std::{env, fs, path::PathBuf};

use litd_editor::shell::{
    DockPanelKind, EditorMode, EditorShellPersistence, EditorShellPlan, build_editor_shell_plan,
    persist_shell_state, pop_out_panel, reattach_panel, rect_summary, restore_shell_state,
    switch_editor_mode,
};
use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::ui::{UiRect, UiRenderSettings},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ShellFsvSnapshot {
    base: EditorShellPlan,
    switched: EditorShellPlan,
    popped: EditorShellPlan,
    reattached: EditorShellPlan,
    restored: EditorShellPlan,
    small_screen: EditorShellPlan,
    persistence: EditorShellPersistence,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env::var("LITD_EDITOR_SHELL_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1138-editor-shell-fsv.txt"));
    let plan_path = env::var("LITD_EDITOR_SHELL_FSV_PLAN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1138-editor-shell.ron"));

    let plan = build_editor_shell_plan(UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    })?;
    let switched = switch_editor_mode(&plan, EditorMode::AiCoAuthor)?;
    let camera_preserved = switched.viewport.camera == plan.viewport.camera;
    let floating_rect = UiRect::new(Vec2::new(1200.0, 120.0), Vec2::new(1820.0, 860.0))?;
    let popped = pop_out_panel(&switched, DockPanelKind::RightInspector, floating_rect)?;
    let reattached = reattach_panel(&popped, DockPanelKind::RightInspector)?;
    let persistence = persist_shell_state(&popped)?;
    let restored = restore_shell_state(
        UiRenderSettings {
            viewport_width: 1920.0,
            viewport_height: 1080.0,
        },
        &persistence,
    )?;
    let small = build_editor_shell_plan(UiRenderSettings {
        viewport_width: 960.0,
        viewport_height: 540.0,
    })?;
    let snapped_translation = plan
        .gizmo
        .snap
        .snap_translation_delta(Vec3::new(1.24, 0.0, -2.76));
    let invalid_viewport_code = match invalid_probe() {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => err.code(),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = plan_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let snapshot = ShellFsvSnapshot {
        base: plan.clone(),
        switched: switched.clone(),
        popped: popped.clone(),
        reattached: reattached.clone(),
        restored: restored.clone(),
        small_screen: small.clone(),
        persistence,
    };
    fs::write(
        &plan_path,
        ron::ser::to_string_pretty(
            &snapshot,
            ron::ser::PrettyConfig::default()
                .new_line("\n".to_string())
                .indentor("  ".to_string())
                .compact_arrays(true),
        )?,
    )?;

    if let Some(parent) = stats_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &stats_path,
        format!(
            "viewport={}x{}\n\
             active_mode={:?}\n\
             mode_tabs={}\n\
             egui_root_id={}\n\
             egui_dock_area_id={}\n\
             egui_central_viewport_id={}\n\
             dock_panel_count={}\n\
             dock_panels={}\n\
             viewport_rect={}\n\
             viewport_live_engine_view={}\n\
             camera={:.1},{:.1},{:.1};distance={:.1};yaw={:.1};pitch={:.1}\n\
             viewport_egui_min={:.1},{:.1}\n\
             viewport_egui_max={:.1},{:.1}\n\
             switch_before_mode={:?}\n\
             switch_after_mode={:?}\n\
             switch_after_inspector={}\n\
             switch_after_tool_id={}\n\
             switch_preserved_camera={}\n\
             popout_before=RightInspector:docked:true\n\
             popout_after=RightInspector:docked:{},floating:{}\n\
             reattach_after=RightInspector:docked:{},floating:{}\n\
             persistence_restored_mode={:?}\n\
             persistence_restored_floating={}\n\
             small_screen_viewport={}\n\
             small_screen_panel_count={}\n\
             gizmo_visible={}\n\
             gizmo_mode={:?}\n\
             gizmo_selected_entities={}\n\
             gizmo_anchor={:.1},{:.1}\n\
             snap_grid_enabled={}\n\
             snap_grid_size={:.1}\n\
             snap_rotation_degrees={:.1}\n\
             snap_scale_step={:.1}\n\
             snap_translation_probe={:.3},{:.3},{:.3}\n\
             reference_images={}\n\
             plan_file={}\n\
             plan_file_bytes={}\n\
             invalid_viewport_code={}\n\
             window_opened=false\n\
             renderer_touched=false\n",
            plan.settings.viewport_width,
            plan.settings.viewport_height,
            plan.active_mode,
            plan.mode_tab_summary(),
            plan.egui.root_id,
            plan.egui.dock_area_id,
            plan.egui.central_viewport_id,
            plan.dock_panels.len(),
            plan.dock_panel_summary(),
            rect_summary(plan.viewport.rect_px),
            plan.viewport.live_engine_view,
            plan.viewport.camera.target_world.x,
            plan.viewport.camera.target_world.y,
            plan.viewport.camera.target_world.z,
            plan.viewport.camera.distance,
            plan.viewport.camera.yaw_degrees,
            plan.viewport.camera.pitch_degrees,
            plan.viewport.egui_rect().min.x,
            plan.viewport.egui_rect().min.y,
            plan.viewport.egui_rect().max.x,
            plan.viewport.egui_rect().max.y,
            plan.active_mode,
            switched.active_mode,
            switched.active_inspector.title,
            switched.active_inspector.tool_id,
            camera_preserved,
            popped
                .panel(DockPanelKind::RightInspector)
                .expect("right inspector exists")
                .docked,
            popped
                .panel(DockPanelKind::RightInspector)
                .expect("right inspector exists")
                .floating_rect_px
                .map(rect_summary)
                .unwrap_or_default(),
            reattached
                .panel(DockPanelKind::RightInspector)
                .expect("right inspector exists")
                .docked,
            reattached
                .panel(DockPanelKind::RightInspector)
                .expect("right inspector exists")
                .floating_rect_px
                .map(rect_summary)
                .unwrap_or_default(),
            restored.active_mode,
            restored
                .panel(DockPanelKind::RightInspector)
                .expect("right inspector exists")
                .floating_rect_px
                .map(rect_summary)
                .unwrap_or_default(),
            rect_summary(small.viewport.rect_px),
            small.dock_panels.len(),
            plan.gizmo.visible,
            plan.gizmo.mode,
            plan.gizmo.selected_entities,
            plan.gizmo.anchor_px.x,
            plan.gizmo.anchor_px.y,
            plan.gizmo.snap.grid_enabled,
            plan.gizmo.snap.grid_size,
            plan.gizmo.snap.rotation_degrees,
            plan.gizmo.snap.scale_step,
            snapped_translation.x,
            snapped_translation.y,
            snapped_translation.z,
            plan.reference_image_list(),
            plan_path.display(),
            fs::metadata(&plan_path)?.len(),
            invalid_viewport_code,
        ),
    )?;
    println!("wrote editor shell FSV stats to {}", stats_path.display());
    Ok(())
}

fn invalid_probe() -> Result<(), EngineError> {
    build_editor_shell_plan(UiRenderSettings {
        viewport_width: 0.0,
        viewport_height: 1080.0,
    })
    .map(|_| ())
}
