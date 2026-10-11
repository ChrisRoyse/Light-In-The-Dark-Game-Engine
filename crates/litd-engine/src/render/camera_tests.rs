use super::*;

#[test]
fn controller_applies_pan_zoom_rotate_and_clamps_to_bounds() {
    let mut camera = fixture_camera();
    camera
        .apply_input(
            CameraControlInput {
                pan_axis: Vec2::new(1.0, 0.5),
                zoom_axis: 1.0,
                rotate_axis: 0.25,
                pitch_axis: 0.0,
            },
            1.0,
        )
        .unwrap();
    assert!(camera.state.target.x > 0.0);
    assert!(camera.state.distance < 32.0);
    assert!(camera.state.yaw_rad > 45.0_f32.to_radians());
}

#[test]
fn locks_prevent_control_axes() {
    let mut camera = fixture_camera();
    camera.locks = CameraControlLocks {
        pan: true,
        zoom: true,
        rotate: true,
    };
    let before = camera.state;
    camera
        .apply_input(
            CameraControlInput {
                pan_axis: Vec2::ONE,
                zoom_axis: 1.0,
                rotate_axis: 1.0,
                pitch_axis: 1.0,
            },
            1.0,
        )
        .unwrap();
    assert_eq!(camera.state, before);
}

#[test]
fn edge_scroll_generates_viewport_edge_pan_and_respects_pan_lock() {
    let mut camera = fixture_camera();
    let axis = camera
        .edge_scroll_axis(EdgeScrollInput {
            cursor_px: Vec2::new(1910.0, 12.0),
            margin_px: 64.0,
        })
        .unwrap();
    assert!(axis.x > 0.0);
    assert!(axis.y > 0.0);

    let before = camera.state.target;
    camera
        .apply_edge_scroll(
            EdgeScrollInput {
                cursor_px: Vec2::new(1910.0, 12.0),
                margin_px: 64.0,
            },
            0.5,
        )
        .unwrap();
    assert_ne!(camera.state.target, before);

    camera.locks.pan = true;
    let locked = camera.state.target;
    camera
        .apply_edge_scroll(
            EdgeScrollInput {
                cursor_px: Vec2::new(4.0, 1076.0),
                margin_px: 64.0,
            },
            0.5,
        )
        .unwrap();
    assert_eq!(camera.state.target, locked);
}

#[test]
fn invalid_edge_scroll_input_fails_loud() {
    let camera = fixture_camera();
    match camera.edge_scroll_axis(EdgeScrollInput {
        cursor_px: Vec2::new(1921.0, 10.0),
        margin_px: 64.0,
    }) {
        Err(EngineError::Rendering(litd_engine::error::RenderingError::CameraInvalidConfig {
            field,
            ..
        })) => {
            assert_eq!(field, "edge_scroll.cursor_px");
        }
        other => panic!("expected CameraInvalidConfig, got {other:?}"),
    }
}

#[test]
fn frustum_culls_behind_camera_bounds() {
    let camera = fixture_camera();
    let frustum = camera.frustum().unwrap();
    let visible = Aabb::from_center_half_extents(camera.state.target, Vec3::splat(1.0)).unwrap();
    let behind =
        Aabb::from_center_half_extents(camera.eye() - camera.forward() * 8.0, Vec3::ONE).unwrap();
    assert!(frustum.contains_aabb(visible));
    assert!(!frustum.contains_aabb(behind));
}

#[test]
fn cinematic_path_samples_between_keyframes() {
    let camera = fixture_camera();
    let path = CinematicCameraPath {
        keyframes: vec![
            CinematicKeyframe {
                time_seconds: 0.0,
                state: camera.state,
            },
            CinematicKeyframe {
                time_seconds: 2.0,
                state: RtsCameraState {
                    target: Vec3::new(10.0, 0.0, 0.0),
                    distance: 48.0,
                    ..camera.state
                },
            },
        ],
    };
    let sampled = path.sample(&camera.config, 1.0).unwrap();
    assert!((sampled.target.x - 5.0).abs() < 0.001);
    assert!((sampled.distance - 40.0).abs() < 0.001);
}

#[test]
fn invalid_clip_planes_fail_loud() {
    let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(32.0)).unwrap();
    let mut config = RtsCameraConfig::standard(1280, 720, bounds);
    config.near = 4.0;
    config.far = 2.0;
    match config.validate() {
        Err(EngineError::Rendering(litd_engine::error::RenderingError::CameraInvalidConfig {
            field,
            ..
        })) => assert_eq!(field, "near_far"),
        other => panic!("expected CameraInvalidConfig, got {other:?}"),
    }
}

fn fixture_camera() -> RtsCamera {
    let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(64.0)).unwrap();
    let config = RtsCameraConfig::standard(1920, 1080, bounds);
    RtsCamera::new(config, RtsCameraState::isometric(Vec3::ZERO)).unwrap()
}
