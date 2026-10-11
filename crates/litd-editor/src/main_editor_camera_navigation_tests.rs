use std::path::PathBuf;

use litd_engine::math::Vec3;

use super::super::*;

fn state_path(runtime: &EditorShellRuntime) -> PathBuf {
    PathBuf::from(runtime.evidence().state_path)
}

fn read_state(runtime: &EditorShellRuntime) -> serde_json::Value {
    runtime.persisted_shell_state_json()
}

fn camera_from_state(state: &serde_json::Value) -> &serde_json::Value {
    &state["persistence"]["camera"]
}

fn assert_camera_target(state: &serde_json::Value, expected: Vec3) {
    let target = camera_from_state(state)["target_world"]
        .as_array()
        .expect("camera target must be persisted as an array");
    assert_eq!(target[0].as_f64().unwrap() as f32, expected.x);
    assert_eq!(target[1].as_f64().unwrap() as f32, expected.y);
    assert_eq!(target[2].as_f64().unwrap() as f32, expected.z);
}

fn assert_persisted_camera_matches_runtime(
    state: &serde_json::Value,
    camera: litd_editor::shell::WorldViewCameraState,
) {
    let persisted = camera_from_state(state);
    let target = persisted["target_world"]
        .as_array()
        .expect("camera target must be persisted as an array");
    assert!((target[0].as_f64().unwrap() as f32 - camera.target_world.x).abs() <= 0.0001);
    assert!((target[1].as_f64().unwrap() as f32 - camera.target_world.y).abs() <= 0.0001);
    assert!((target[2].as_f64().unwrap() as f32 - camera.target_world.z).abs() <= 0.0001);
    assert!((persisted["distance"].as_f64().unwrap() as f32 - camera.distance).abs() <= 0.0001);
    assert!(
        (persisted["yaw_degrees"].as_f64().unwrap() as f32 - camera.yaw_degrees).abs() <= 0.0001
    );
    assert!(
        (persisted["pitch_degrees"].as_f64().unwrap() as f32 - camera.pitch_degrees).abs()
            <= 0.0001
    );
}

#[test]
fn frame_selected_centers_selection_and_persists_source_of_truth() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();
    println!(
        "issue1216 frame_selected before path={} camera={} save_count={}",
        before.state_path, before.camera, before.state_save_count
    );

    let expected_pivot = Vec3::new(32.0, 0.0, -48.0);
    runtime.plan.gizmo.selected_entities = 1;
    runtime.plan.gizmo.pivot_world = expected_pivot;
    let changed = runtime.frame_selected_world_view_camera().unwrap();
    let after = runtime.evidence();
    let state = read_state(&runtime);
    println!(
        "issue1216 frame_selected after changed={} camera={} persisted_camera={}",
        changed,
        after.camera,
        camera_from_state(&state)
    );

    assert!(changed);
    assert!(after.camera.contains("target=32.0,0.0,-48.0"));
    assert!(after.camera.contains("distance=115.2"));
    assert_camera_target(&state, expected_pivot);
}

#[test]
fn bookmark_save_restore_round_trips_exact_camera_from_shell_state() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.plan.viewport.camera.target_world = Vec3::new(24.0, 0.0, -64.0);
    runtime.plan.viewport.camera.distance = 88.0;
    runtime.plan.viewport.camera.yaw_degrees = 72.0;
    runtime.plan.viewport.camera.pitch_degrees = -52.0;
    let saved = runtime.plan.viewport.camera;
    println!(
        "issue1216 bookmark before path={} camera={:?} bookmarks={}",
        state_path(&runtime).display(),
        saved,
        runtime.shell_state.camera_bookmark_summary()
    );

    runtime.save_world_view_camera_bookmark(1).unwrap();
    runtime.plan.viewport.camera.target_world = Vec3::new(-40.0, 0.0, 20.0);
    runtime.plan.viewport.camera.distance = 160.0;
    runtime.shell_state.save_plan(&runtime.plan).unwrap();
    let moved_state = read_state(&runtime);
    println!(
        "issue1216 bookmark moved persisted_camera={}",
        camera_from_state(&moved_state)
    );

    let changed = runtime.restore_world_view_camera_bookmark(1).unwrap();
    let restored_state = read_state(&runtime);
    println!(
        "issue1216 bookmark after changed={} bookmarks={} persisted_camera={}",
        changed,
        runtime.shell_state.camera_bookmark_summary(),
        camera_from_state(&restored_state)
    );

    assert!(changed);
    assert_eq!(runtime.plan.viewport.camera, saved);
    assert_camera_target(&restored_state, saved.target_world);
    assert_eq!(
        restored_state["camera_bookmarks"][0]["camera"],
        restored_state["persistence"]["camera"]
    );
}

#[test]
fn explicit_navigation_controls_persist_orbit_pan_zoom_source_of_truth() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();
    let before_state = read_state(&runtime);
    println!(
        "issue1216 navigation_control before path={} camera={} persisted_camera={} save_count={}",
        before.state_path,
        before.camera,
        camera_from_state(&before_state),
        before.state_save_count
    );

    runtime
        .apply_world_view_camera_control(WorldViewCameraControlInput {
            pan_delta_px: EngineVec2::new(-96.0, 0.0),
            orbit_delta_px: EngineVec2::new(120.0, 0.0),
            zoom_delta_px: 300.0,
        })
        .unwrap();
    let after = runtime.evidence();
    let after_state = read_state(&runtime);
    println!(
        "issue1216 navigation_control after camera={} persisted_camera={} save_count={}",
        after.camera,
        camera_from_state(&after_state),
        after.state_save_count
    );

    assert_ne!(
        camera_from_state(&after_state),
        camera_from_state(&before_state)
    );
    assert_persisted_camera_matches_runtime(&after_state, runtime.plan.viewport.camera);
    assert_ne!(
        camera_from_state(&after_state)["distance"],
        camera_from_state(&before_state)["distance"]
    );
    assert!(after.state_save_count > before.state_save_count);
}

#[test]
fn empty_selection_frame_is_noop_with_identical_file_readback() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.plan.gizmo.selected_entities = 0;
    runtime.plan.gizmo.pivot_world = Vec3::new(90.0, 0.0, 90.0);
    let path = state_path(&runtime);
    let before = runtime.persisted_shell_state_bytes();
    let before_camera = runtime.evidence().camera;
    println!(
        "issue1216 empty_frame before path={} camera={} bytes={}",
        path.display(),
        before_camera,
        before.len()
    );

    let changed = runtime.frame_selected_world_view_camera().unwrap();
    let after = runtime.persisted_shell_state_bytes();
    println!(
        "issue1216 empty_frame after changed={} camera={} bytes={}",
        changed,
        runtime.evidence().camera,
        after.len()
    );

    assert!(!changed);
    assert_eq!(runtime.evidence().camera, before_camera);
    assert_eq!(after, before);
}

#[test]
fn missing_bookmark_restore_fails_loud_without_mutating_shell_state() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let path = state_path(&runtime);
    let before = runtime.persisted_shell_state_bytes();
    let before_camera = runtime.plan.viewport.camera;
    println!(
        "issue1216 missing_bookmark before path={} camera={:?} bookmarks={}",
        path.display(),
        before_camera,
        runtime.shell_state.camera_bookmark_summary()
    );

    let error = runtime
        .restore_world_view_camera_bookmark(4)
        .expect_err("missing bookmark restore must fail loud")
        .to_string();
    let after = runtime.persisted_shell_state_bytes();
    println!(
        "issue1216 missing_bookmark after error={} camera={:?} bytes={}",
        error,
        runtime.plan.viewport.camera,
        after.len()
    );

    assert!(error.contains("editor.shell.camera.bookmark.restore"));
    assert_eq!(runtime.plan.viewport.camera, before_camera);
    assert_eq!(after, before);
}
