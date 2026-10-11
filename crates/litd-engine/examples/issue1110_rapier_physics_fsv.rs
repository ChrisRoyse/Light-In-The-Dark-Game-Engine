use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::Vec3,
    physics::{
        PhysicsAabbQuery, PhysicsBodyHandle, PhysicsColliderDesc, PhysicsColliderShape,
        PhysicsRaycast, PhysicsRigidBodyDesc, PhysicsRigidBodyKind, PhysicsWorld,
        PhysicsWorldConfig,
    },
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1110_rapier_physics")?;
    let report = json!({
        "schema": "litd.engine.issue1110.rapier_physics.v1",
        "schema_version": 1,
        "issue": 1110,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON and hashes"
        },
        "happy_path": happy_path()?,
        "edges": {
            "invalid_gravity": invalid_gravity_edge()?,
            "invalid_collider_radius": invalid_collider_radius_edge()?,
            "invalid_raycast_direction": invalid_raycast_direction_edge()?,
            "missing_joint_body": missing_joint_body_edge()?,
        },
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "ISSUE1110_RAPIER_PHYSICS_FSV report={} bytes={} sha256={} bodies={} colliders={} joints={} ray_toi={} aabb_hits={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["after"]["step_report"]["body_count"],
        readback.json["happy_path"]["after"]["step_report"]["collider_count"],
        readback.json["happy_path"]["after"]["step_report"]["impulse_joint_count"],
        readback.json["happy_path"]["after"]["raycast"]["time_of_impact"],
        readback.json["happy_path"]["after"]["aabb_hit_count"],
    );
    Ok(())
}

fn assert_report(report: &Value) -> Result<(), EngineError> {
    let after = &report["happy_path"]["after"];
    let actual_toi = after["raycast"]["time_of_impact"].as_f64().ok_or_else(|| {
        EngineError::PhysicsInvalidConfig {
            field: "physics.fsv.acceptance.ray_toi".to_string(),
            reason: "persisted raycast time_of_impact was not numeric".to_string(),
        }
    })?;
    let checks = [
        ("ball_fell", after["ball_fell"] == true),
        ("ray_hit_ground", after["ray_hit_ground"] == true),
        ("ray_toi", (actual_toi - 5.0).abs() <= 1.0e-5),
        (
            "aabb_contains_ground",
            after["aabb_contains_ground"] == true,
        ),
        ("body_count", after["step_report"]["body_count"] == 3),
        (
            "collider_count",
            after["step_report"]["collider_count"] == 3,
        ),
        (
            "impulse_joint_count",
            after["step_report"]["impulse_joint_count"] == 1,
        ),
    ];
    for (field, passed) in checks {
        if !passed {
            return Err(EngineError::PhysicsInvalidConfig {
                field: format!("physics.fsv.acceptance.{field}"),
                reason: format!("persisted Rapier verdict was false; after={after}"),
            });
        }
    }
    Ok(())
}

fn happy_path() -> Result<Value, EngineError> {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    let ground =
        world.add_rigid_body(body(PhysicsRigidBodyKind::Fixed, Vec3::new(0.0, -0.1, 0.0)))?;
    let ground_collider = world.add_collider(cuboid(ground, Vec3::new(5.0, 0.1, 5.0)))?;
    let ball = world.add_rigid_body(body(
        PhysicsRigidBodyKind::Dynamic,
        Vec3::new(0.0, 3.0, 0.0),
    ))?;
    let ball_collider = world.add_collider(ball_shape(ball, 0.5))?;
    let attached = world.add_rigid_body(body(
        PhysicsRigidBodyKind::Dynamic,
        Vec3::new(1.0, 3.0, 0.0),
    ))?;
    let attached_collider = world.add_collider(ball_shape(attached, 0.25))?;
    let joint = world.add_fixed_joint(ball, attached)?;
    let step_report = world.step()?;
    let ball_after = world.body_transform(ball)?;
    let raycast = world
        .raycast(PhysicsRaycast {
            origin: Vec3::new(3.0, 5.0, 0.0),
            direction: Vec3::new(0.0, -1.0, 0.0),
            max_toi: 10.0,
            solid: true,
        })?
        .ok_or_else(|| EngineError::PhysicsInvalidConfig {
            field: "physics.fsv.raycast".to_string(),
            reason: "expected raycast to hit the ground collider".to_string(),
        })?;
    let aabb_hits = world.intersect_aabb(PhysicsAabbQuery {
        center: Vec3::new(0.0, 1.5, 0.0),
        half_extents: Vec3::new(6.0, 5.0, 6.0),
    })?;
    Ok(json!({
        "before": {
            "gravity": [0.0, -9.81, 0.0],
            "bodies_to_create": 3,
            "colliders_to_create": 3,
            "joints_to_create": 1,
            "raycast_origin": [3.0, 5.0, 0.0],
            "aabb_center": [0.0, 1.5, 0.0],
        },
        "after": {
            "step_report": step_report,
            "ball_after_y": ball_after.translation.y,
            "ball_fell": ball_after.translation.y < 3.0,
            "joint": joint,
            "colliders": {
                "ground": ground_collider,
                "ball": ball_collider,
                "attached": attached_collider,
            },
            "raycast": raycast,
            "ray_hit_ground": raycast.collider == ground_collider,
            "ray_toi_expected": 5.0,
            "aabb_hits": aabb_hits,
            "aabb_hit_count": aabb_hits.len(),
            "aabb_contains_ground": aabb_hits.contains(&ground_collider),
        }
    }))
}

fn invalid_gravity_edge() -> Result<Value, EngineError> {
    Ok(edge(expect_error(
        PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec3::new(f32::NAN, 0.0, 0.0),
            timestep_secs: 1.0 / 60.0,
        }),
        "invalid_gravity",
    )?))
}

fn invalid_collider_radius_edge() -> Result<Value, EngineError> {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    let body = world.add_rigid_body(body(PhysicsRigidBodyKind::Fixed, Vec3::ZERO))?;
    Ok(edge(expect_error(
        world.add_collider(ball_shape(body, 0.0)),
        "invalid_collider_radius",
    )?))
}

fn invalid_raycast_direction_edge() -> Result<Value, EngineError> {
    let world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    Ok(edge(expect_error(
        world.raycast(PhysicsRaycast {
            origin: Vec3::ZERO,
            direction: Vec3::ZERO,
            max_toi: 1.0,
            solid: true,
        }),
        "invalid_raycast_direction",
    )?))
}

fn missing_joint_body_edge() -> Result<Value, EngineError> {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    let body = world.add_rigid_body(body(PhysicsRigidBodyKind::Fixed, Vec3::ZERO))?;
    Ok(edge(expect_error(
        world.add_fixed_joint(
            body,
            PhysicsBodyHandle {
                id: u32::MAX,
                generation: u32::MAX,
            },
        ),
        "missing_joint_body",
    )?))
}

fn expect_error<T>(
    result: Result<T, EngineError>,
    field: &str,
) -> Result<EngineError, EngineError> {
    match result {
        Ok(_) => Err(EngineError::PhysicsInvalidConfig {
            field: format!("physics.fsv.{field}"),
            reason: "expected invalid physics input to fail".to_string(),
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

fn body(kind: PhysicsRigidBodyKind, translation: Vec3) -> PhysicsRigidBodyDesc {
    PhysicsRigidBodyDesc {
        kind,
        translation,
        linear_velocity: Vec3::ZERO,
    }
}

fn cuboid(parent: PhysicsBodyHandle, half_extents: Vec3) -> PhysicsColliderDesc {
    PhysicsColliderDesc {
        parent: Some(parent),
        shape: PhysicsColliderShape::Cuboid { half_extents },
        translation: Vec3::ZERO,
        sensor: false,
        friction: 0.8,
        restitution: 0.0,
    }
}

fn ball_shape(parent: PhysicsBodyHandle, radius: f32) -> PhysicsColliderDesc {
    PhysicsColliderDesc {
        parent: Some(parent),
        shape: PhysicsColliderShape::Ball { radius },
        translation: Vec3::ZERO,
        sensor: false,
        friction: 0.5,
        restitution: 0.0,
    }
}
