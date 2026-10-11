use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::Vec3,
    physics::{
        PhysicsCharacterAutostep, PhysicsCharacterControllerConfig, PhysicsCharacterMoveRequest,
        PhysicsColliderDesc, PhysicsColliderShape, PhysicsRigidBodyDesc, PhysicsRigidBodyKind,
        PhysicsWorld, PhysicsWorldConfig,
    },
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1111_character_controller")?;
    let report = json!({
        "schema": "litd.engine.issue1111.character_controller.v1",
        "schema_version": 1,
        "issue": 1111,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON and hashes"
        },
        "happy_path": happy_path()?,
        "edges": {
            "invalid_up_vector": invalid_up_vector_edge()?,
            "invalid_slope_angle": invalid_slope_angle_edge()?,
            "invalid_snap_distance": invalid_snap_distance_edge()?,
            "invalid_autostep_height": invalid_autostep_height_edge()?,
            "invalid_delta_time": invalid_delta_time_edge()?,
            "invalid_character_shape": invalid_character_shape_edge()?,
        },
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1111_CHARACTER_CONTROLLER_FSV report={} bytes={} sha256={} grounded={} collisions={} requested_x={} effective_x={} final_y={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["movement"]["grounded"],
        readback.json["happy_path"]["movement"]["collision_count"],
        readback.json["happy_path"]["movement"]["requested_translation"][0],
        readback.json["happy_path"]["movement"]["effective_translation"][0],
        readback.json["happy_path"]["movement"]["final_position"][1],
    );
    Ok(())
}

fn happy_path() -> Result<Value, EngineError> {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    let ground =
        world.add_rigid_body(body(PhysicsRigidBodyKind::Fixed, Vec3::new(0.0, -0.1, 0.0)))?;
    let ground_collider = world.add_collider(cuboid(ground, Vec3::new(5.0, 0.1, 5.0)))?;
    let wall = world.add_rigid_body(body(PhysicsRigidBodyKind::Fixed, Vec3::new(1.2, 0.5, 0.0)))?;
    let wall_collider = world.add_collider(cuboid(wall, Vec3::new(0.1, 0.5, 5.0)))?;
    let step_report = world.step()?;
    let config = PhysicsCharacterControllerConfig {
        snap_to_ground: Some(0.3),
        autostep: Some(PhysicsCharacterAutostep {
            max_height: 0.2,
            min_width: 0.1,
            include_dynamic_bodies: false,
        }),
        ..PhysicsCharacterControllerConfig::default()
    };
    let request = baseline_request();
    let movement = world.move_character(config, request)?;
    let wall_blocked_x = movement.effective_translation.x < movement.requested_translation.x;
    Ok(json!({
        "scene": {
            "step_report": step_report,
            "ground_collider": ground_collider,
            "wall_collider": wall_collider,
            "character_shape": request.shape,
            "requested_position": request.position,
            "desired_translation": request.desired_translation,
            "delta_time_secs": request.delta_time_secs,
        },
        "config": config,
        "movement": movement,
        "acceptance": {
            "grounded": movement.grounded,
            "collision_count_positive": movement.collision_count > 0,
            "wall_blocked_x": wall_blocked_x,
            "sliding_down_slope_reported": movement.sliding_down_slope,
            "slope_climb_angle_radians": movement.max_slope_climb_angle_radians,
            "slope_slide_angle_radians": movement.min_slope_slide_angle_radians,
            "expected_slope_angle_radians": std::f32::consts::FRAC_PI_4,
        }
    }))
}

fn invalid_up_vector_edge() -> Result<Value, EngineError> {
    let config = PhysicsCharacterControllerConfig {
        up: Vec3::ZERO,
        ..PhysicsCharacterControllerConfig::default()
    };
    character_edge(config, baseline_request(), "invalid_up_vector")
}

fn invalid_slope_angle_edge() -> Result<Value, EngineError> {
    let config = PhysicsCharacterControllerConfig {
        max_slope_climb_angle_radians: std::f32::consts::PI,
        ..PhysicsCharacterControllerConfig::default()
    };
    character_edge(config, baseline_request(), "invalid_slope_angle")
}

fn invalid_snap_distance_edge() -> Result<Value, EngineError> {
    let config = PhysicsCharacterControllerConfig {
        snap_to_ground: Some(0.0),
        ..PhysicsCharacterControllerConfig::default()
    };
    character_edge(config, baseline_request(), "invalid_snap_distance")
}

fn invalid_autostep_height_edge() -> Result<Value, EngineError> {
    let config = PhysicsCharacterControllerConfig {
        autostep: Some(PhysicsCharacterAutostep {
            max_height: 0.0,
            min_width: 0.1,
            include_dynamic_bodies: false,
        }),
        ..PhysicsCharacterControllerConfig::default()
    };
    character_edge(config, baseline_request(), "invalid_autostep_height")
}

fn invalid_delta_time_edge() -> Result<Value, EngineError> {
    character_edge(
        PhysicsCharacterControllerConfig::default(),
        PhysicsCharacterMoveRequest {
            delta_time_secs: 0.0,
            ..baseline_request()
        },
        "invalid_delta_time",
    )
}

fn invalid_character_shape_edge() -> Result<Value, EngineError> {
    character_edge(
        PhysicsCharacterControllerConfig::default(),
        PhysicsCharacterMoveRequest {
            shape: PhysicsColliderShape::CapsuleY {
                half_height: 0.4,
                radius: 0.0,
            },
            ..baseline_request()
        },
        "invalid_character_shape",
    )
}

fn character_edge(
    config: PhysicsCharacterControllerConfig,
    request: PhysicsCharacterMoveRequest,
    field: &str,
) -> Result<Value, EngineError> {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    Ok(edge(expect_error(
        world.move_character(config, request),
        field,
    )?))
}

fn expect_error<T>(
    result: Result<T, EngineError>,
    field: &str,
) -> Result<EngineError, EngineError> {
    match result {
        Ok(_) => Err(EngineError::PhysicsInvalidConfig {
            field: format!("physics.fsv.{field}"),
            reason: "expected invalid character-controller input to fail".to_string(),
        }),
        Err(error) => Ok(error),
    }
}

fn edge(error: EngineError) -> Value {
    json!({
        "after": {
            "error_code": error.code(),
            "message": error.to_string(),
        }
    })
}

fn baseline_request() -> PhysicsCharacterMoveRequest {
    PhysicsCharacterMoveRequest {
        position: Vec3::new(0.0, 0.72, 0.0),
        shape: PhysicsColliderShape::CapsuleY {
            half_height: 0.4,
            radius: 0.3,
        },
        desired_translation: Vec3::new(2.0, -0.1, 0.0),
        delta_time_secs: 1.0 / 60.0,
    }
}

fn body(kind: PhysicsRigidBodyKind, translation: Vec3) -> PhysicsRigidBodyDesc {
    PhysicsRigidBodyDesc {
        kind,
        translation,
        linear_velocity: Vec3::ZERO,
    }
}

fn cuboid(
    parent: litd_engine::physics::PhysicsBodyHandle,
    half_extents: Vec3,
) -> PhysicsColliderDesc {
    PhysicsColliderDesc {
        parent: Some(parent),
        shape: PhysicsColliderShape::Cuboid { half_extents },
        translation: Vec3::ZERO,
        sensor: false,
        friction: 0.8,
        restitution: 0.0,
    }
}
