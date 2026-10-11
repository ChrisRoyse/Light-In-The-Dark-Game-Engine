use super::*;

#[test]
fn playtest_snaps_to_game_camera_and_stop_restores_edit_view() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let edit_camera = runtime.plan.viewport.camera;
    runtime.selected_asset = 3;

    runtime.toggle_playtest().unwrap();
    let running = runtime.evidence();

    assert_eq!(running.play_state, "Running");
    assert_eq!(running.play_camera_rig, "Top-down");
    assert_eq!(runtime.plan.viewport.camera.distance, 104.0);
    assert_eq!(runtime.plan.viewport.camera.pitch_degrees, -82.0);
    assert_ne!(runtime.plan.viewport.camera, edit_camera);
    assert!(running.saved_edit_snapshot.contains("distance=150.0"));
    assert!(running.play_session.contains("bodies=1"));
    assert!(running.play_session.contains("visible_instances=1"));
    assert!(running.play_session.contains("rig=TopDown"));
    assert!(running.play_session.contains("player=player_1"));
    assert!(running.play_session.contains("fov=42.0"));
    assert_eq!(running.selected_asset_index, 3);

    runtime.toggle_playtest().unwrap();
    let stopped = runtime.evidence();

    assert_eq!(stopped.play_state, "Stopped");
    assert_eq!(stopped.saved_edit_snapshot, "none");
    assert_eq!(runtime.plan.viewport.camera, edit_camera);
    assert_eq!(runtime.selected_asset, 3);
    assert_eq!(stopped.play_transition_count, 2);
}

#[test]
fn playtest_supports_all_rigs_and_mid_play_perspective_swap() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let edit_camera = runtime.plan.viewport.camera;

    runtime
        .set_play_camera_rig(PlayCameraRig::FirstPerson)
        .unwrap();
    runtime.toggle_playtest().unwrap();
    assert_eq!(runtime.evidence().play_camera_rig, "First-person");
    assert_eq!(runtime.plan.viewport.camera.distance, 1.0);
    assert_eq!(runtime.plan.viewport.camera.yaw_degrees, 0.0);
    assert_eq!(runtime.plan.viewport.camera.pitch_degrees, -2.0);
    assert!(runtime.evidence().play_session.contains("fov=74.0"));

    runtime
        .set_play_camera_rig(PlayCameraRig::Isometric)
        .unwrap();
    assert_eq!(runtime.evidence().play_camera_rig, "Isometric");
    assert_eq!(runtime.plan.viewport.camera.yaw_degrees, 45.0);
    assert_eq!(runtime.plan.viewport.camera.pitch_degrees, -55.0);

    runtime
        .set_play_camera_rig(PlayCameraRig::OverShoulder)
        .unwrap();
    assert_eq!(runtime.evidence().play_camera_rig, "Over-shoulder");
    assert_eq!(runtime.plan.viewport.camera.distance, 28.0);
    assert_eq!(runtime.plan.viewport.camera.yaw_degrees, 0.0);
    assert_eq!(runtime.plan.viewport.camera.pitch_degrees, -15.0);

    runtime.set_play_camera_rig(PlayCameraRig::TopDown).unwrap();
    let top_down = runtime.evidence();
    assert_eq!(top_down.play_camera_rig, "Top-down");
    assert_eq!(top_down.play_rig_switch_count, 3);
    assert!(top_down.play_session.contains("generation=3"));
    assert!(top_down.play_session.contains("fov=42.0"));

    runtime.toggle_playtest().unwrap();
    assert_eq!(runtime.evidence().play_state, "Stopped");
    assert_eq!(runtime.plan.viewport.camera, edit_camera);
}

#[test]
fn mode_switch_updates_inspector_source_of_truth() {
    let mut runtime = EditorShellRuntime::new(1280, 720).unwrap();

    runtime.switch_mode(EditorMode::AiCoAuthor).unwrap();
    let evidence = runtime.evidence();

    assert_eq!(evidence.active_mode, EditorMode::AiCoAuthor);
    assert_eq!(evidence.active_inspector_title, "AI Co-Author");
    assert_eq!(evidence.mode_switch_count, 1);
    assert!(evidence.camera.contains("distance=150.0"));
    assert!(evidence.camera.contains("yaw=38.0"));
    assert!(evidence.camera.contains("pitch=-42.0"));
}
