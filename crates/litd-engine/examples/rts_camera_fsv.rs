use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, HzbOccluder, RenderInstance, ScreenRect, build_batch_plan},
        camera::{
            CameraControlInput, CameraControlLocks, CinematicCameraPath, CinematicKeyframe,
            EdgeScrollInput, RtsCamera, RtsCameraConfig, RtsCameraState,
        },
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_RTS_CAMERA_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue877-rts-camera-fsv.txt"));

    let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(64.0))?;
    let config = RtsCameraConfig::standard(1920, 1080, bounds);
    let mut camera = RtsCamera::new(config, RtsCameraState::isometric(Vec3::ZERO))?;
    camera.apply_input(
        CameraControlInput {
            pan_axis: Vec2::new(1.0, 0.5),
            zoom_axis: 0.5,
            rotate_axis: 0.25,
            pitch_axis: 0.0,
        },
        1.0,
    )?;

    let unlocked_distance = camera.state.distance;
    let manual_target = camera.state.target;
    let edge_input = EdgeScrollInput {
        cursor_px: Vec2::new(1910.0, 12.0),
        margin_px: 64.0,
    };
    let edge_axis = camera.edge_scroll_axis(edge_input)?;
    camera.apply_edge_scroll(edge_input, 0.5)?;
    let edge_target = camera.state.target;
    camera.locks = CameraControlLocks {
        pan: true,
        zoom: true,
        rotate: false,
    };
    let locked_edge_target = camera.state.target;
    camera.apply_edge_scroll(
        EdgeScrollInput {
            cursor_px: Vec2::new(4.0, 1076.0),
            margin_px: 64.0,
        },
        0.5,
    )?;
    let locked_edge_preserved = camera.state.target == locked_edge_target;
    camera.apply_input(
        CameraControlInput {
            zoom_axis: 1.0,
            ..CameraControlInput::ZERO
        },
        1.0,
    )?;
    let locked_distance = camera.state.distance;
    let frustum = camera.frustum()?;
    let cull_bounds = vec![
        Aabb::from_center_half_extents(camera.state.target, Vec3::splat(1.0))?,
        Aabb::from_center_half_extents(
            camera.state.target + Vec3::new(6.0, 0.0, 4.0),
            Vec3::splat(1.5),
        )?,
        Aabb::from_center_half_extents(camera.eye() - camera.forward() * 8.0, Vec3::ONE)?,
        Aabb::from_center_half_extents(Vec3::new(900.0, 0.0, 900.0), Vec3::splat(2.0))?,
    ];
    let visible = frustum.visible_indices(&cull_bounds);
    let batching_frustum = frustum.to_batching_frustum();
    let occluders = vec![HzbOccluder::new(rect(0.42, 0.42, 0.58, 0.58)?, 0.40)];
    let batch_instances = vec![
        batch_instance(
            1,
            "hero",
            camera.state.target,
            rect(0.22, 0.22, 0.30, 0.34)?,
            0.28,
        )?,
        batch_instance(
            2,
            "doodad",
            camera.state.target + Vec3::new(5.0, 0.0, 3.0),
            rect(0.62, 0.40, 0.72, 0.52)?,
            0.32,
        )?,
        batch_instance(
            3,
            "outside",
            Vec3::new(900.0, 0.0, 900.0),
            rect(0.02, 0.02, 0.08, 0.10)?,
            0.12,
        )?,
        batch_instance(
            4,
            "hidden",
            camera.state.target,
            rect(0.46, 0.46, 0.54, 0.54)?,
            0.82,
        )?,
    ];
    let batch_plan = build_batch_plan(
        BatchingSettings::default(),
        &batching_frustum,
        &occluders,
        &batch_instances,
    )?;
    let path_sample = cinematic_sample(&camera)?;
    let invalid_config_code = match invalid_config_probe() {
        Err(
            err @ EngineError::Rendering(litd_engine::error::RenderingError::CameraInvalidConfig {
                ..
            }),
        ) => err.code(),
        other => panic!("expected CameraInvalidConfig, got {other:?}"),
    };
    let invalid_edge_scroll_code = match invalid_edge_scroll_probe(&camera) {
        Err(
            err @ EngineError::Rendering(litd_engine::error::RenderingError::CameraInvalidConfig {
                ..
            }),
        ) => err.code(),
        other => panic!("expected CameraInvalidConfig, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "viewport={}x{}\n\
             aspect_ratio={:.6}\n\
             target={:.3},{:.3},{:.3}\n\
             eye={:.3},{:.3},{:.3}\n\
             yaw_deg={:.3}\n\
             pitch_deg={:.3}\n\
             unlocked_distance={:.3}\n\
             locked_distance={:.3}\n\
             manual_target={:.3},{:.3},{:.3}\n\
             edge_scroll_axis={:.3},{:.3}\n\
             edge_scroll_target={:.3},{:.3},{:.3}\n\
             edge_scroll_moved={}\n\
             edge_scroll_lock_preserved={}\n\
             frustum_planes={}\n\
             batching_frustum_planes={}\n\
             visible_indices={}\n\
             visible_count={}\n\
             batch_visible_ids={}\n\
             batch_frustum_culled_ids={}\n\
             batch_occlusion_culled_ids={}\n\
             batch_draw_batches={}\n\
             cinematic_target={:.3},{:.3},{:.3}\n\
             cinematic_distance={:.3}\n\
             invalid_config_code={}\n\
             invalid_edge_scroll_code={}\n",
            camera.config.viewport_width,
            camera.config.viewport_height,
            camera.config.aspect_ratio(),
            camera.state.target.x,
            camera.state.target.y,
            camera.state.target.z,
            camera.eye().x,
            camera.eye().y,
            camera.eye().z,
            camera.state.yaw_rad.to_degrees(),
            camera.state.pitch_rad.to_degrees(),
            unlocked_distance,
            locked_distance,
            manual_target.x,
            manual_target.y,
            manual_target.z,
            edge_axis.x,
            edge_axis.y,
            edge_target.x,
            edge_target.y,
            edge_target.z,
            edge_target != manual_target,
            locked_edge_preserved,
            frustum.planes.len(),
            batching_frustum.planes.len(),
            visible
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(","),
            visible.len(),
            join_ids(&batch_plan.visible_instance_ids),
            join_ids(&batch_plan.frustum_culled_ids),
            join_ids(&batch_plan.occlusion_culled_ids),
            batch_plan.draw_batches.len(),
            path_sample.target.x,
            path_sample.target.y,
            path_sample.target.z,
            path_sample.distance,
            invalid_config_code,
            invalid_edge_scroll_code,
        ),
    )?;
    println!("wrote RTS camera FSV stats to {}", path.display());
    Ok(())
}

fn cinematic_sample(camera: &RtsCamera) -> Result<RtsCameraState, EngineError> {
    let path = CinematicCameraPath {
        keyframes: vec![
            CinematicKeyframe {
                time_seconds: 0.0,
                state: camera.state,
            },
            CinematicKeyframe {
                time_seconds: 2.0,
                state: RtsCameraState {
                    target: camera.state.target + Vec3::new(12.0, 0.0, 0.0),
                    distance: 48.0,
                    ..camera.state
                },
            },
        ],
    };
    path.sample(&camera.config, 1.0)
}

fn batch_instance(
    id: u64,
    mesh_id: &str,
    center: Vec3,
    screen_rect: ScreenRect,
    nearest_depth: f32,
) -> Result<RenderInstance, EngineError> {
    Ok(RenderInstance {
        id,
        mesh_id: mesh_id.to_string(),
        material_id: "reference".to_string(),
        world_bounds: Aabb::from_center_half_extents(center, Vec3::splat(1.0))?,
        screen_rect,
        nearest_depth,
        index_count: 96,
    })
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}

fn join_ids(ids: &[u64]) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

fn invalid_config_probe() -> Result<(), EngineError> {
    let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(16.0))?;
    let mut config = RtsCameraConfig::standard(1280, 720, bounds);
    config.near = 8.0;
    config.far = 4.0;
    config.validate()
}

fn invalid_edge_scroll_probe(camera: &RtsCamera) -> Result<(), EngineError> {
    camera
        .edge_scroll_axis(EdgeScrollInput {
            cursor_px: Vec2::new(camera.config.viewport_width as f32 + 1.0, 10.0),
            margin_px: 64.0,
        })
        .map(|_| ())
}
