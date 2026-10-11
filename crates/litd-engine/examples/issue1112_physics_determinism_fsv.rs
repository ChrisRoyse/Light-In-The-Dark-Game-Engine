use litd_engine::{
    EngineError,
    determinism_boundary::{HotPathPhase, enter_hot_path, reset_for_fsv, snapshot},
    fsv_artifacts::begin_issue_run,
    math::{FixedVec3, Vec3},
    physics::{
        PhysicsAabbQuery, PhysicsBodyHandle, PhysicsCharacterControllerConfig,
        PhysicsCharacterMoveRequest, PhysicsColliderDesc, PhysicsColliderShape, PhysicsRaycast,
        PhysicsRigidBodyDesc, PhysicsRigidBodyKind, PhysicsWorld, PhysicsWorldConfig,
    },
    sim::{FixedTickSim, SimConfig},
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1112_physics_determinism")?;
    let report = json!({
        "schema": "litd.engine.issue1112.physics_determinism.v1",
        "schema_version": 1,
        "issue": 1112,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON and hashes"
        },
        "hot_path_boundary": hot_path_boundary()?,
        "hash_boundary": hash_boundary()?,
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "ISSUE1112_PHYSICS_DETERMINISM_FSV report={} bytes={} sha256={} hot_path_physics={} total_physics={} before_hash={} after_hash={} hash_equal={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["hot_path_boundary"]["inside_hot_path_snapshot"]["hot_path_physics_access_total"],
        readback.json["hot_path_boundary"]["inside_hot_path_snapshot"]["physics_access_total"],
        readback.json["hash_boundary"]["before_hash_hex"],
        readback.json["hash_boundary"]["after_hash_hex"],
        readback.json["hash_boundary"]["hash_equal"],
    );
    Ok(())
}

fn assert_report(report: &Value) -> Result<(), EngineError> {
    let hot = &report["hot_path_boundary"];
    let checks = [
        (
            "guarded_operation_count",
            hot["guarded_operation_count"] == 10,
        ),
        (
            "inside_phase",
            hot["acceptance"]["inside_phase"] == "sim_tick",
        ),
        ("inside_depth", hot["acceptance"]["inside_depth"] == 1),
        ("after_phase", hot["acceptance"]["after_phase"].is_null()),
        ("after_depth", hot["acceptance"]["after_depth"] == 0),
        (
            "hot_path_physics_access_total",
            hot["acceptance"]["hot_path_physics_access_total"] == 10,
        ),
        (
            "physics_access_total",
            hot["acceptance"]["physics_access_total"] == 14,
        ),
        (
            "hash_physics_step_index",
            report["hash_boundary"]["acceptance"]["physics_step_index"] == 1,
        ),
        (
            "hash_hot_path_physics_access_total",
            report["hash_boundary"]["acceptance"]["hot_path_physics_access_total"] == 0,
        ),
        (
            "hash_physics_access_total",
            report["hash_boundary"]["acceptance"]["physics_access_total"] == 4,
        ),
        ("hash_equal", report["hash_boundary"]["hash_equal"] == true),
    ];
    for (field, passed) in checks {
        if !passed {
            return Err(EngineError::PhysicsInvalidConfig {
                field: format!("physics.fsv.acceptance.{field}"),
                reason: "persisted physics determinism verdict was false".to_string(),
            });
        }
    }
    Ok(())
}

fn hot_path_boundary() -> Result<Value, EngineError> {
    reset_for_fsv();
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    let fixed = world.add_rigid_body(body(PhysicsRigidBodyKind::Fixed, Vec3::ZERO))?;
    let dynamic = world.add_rigid_body(body(PhysicsRigidBodyKind::Dynamic, Vec3::Y))?;
    world.add_collider(cuboid(fixed, Vec3::new(5.0, 0.1, 5.0)))?;

    let errors;
    let inside_snapshot;
    {
        let _hot_path = enter_hot_path(HotPathPhase::SimTick);
        errors = vec![
            boundary_error(
                PhysicsWorld::new(PhysicsWorldConfig::default()),
                "physics.world.new",
            )?,
            boundary_error(
                world.add_rigid_body(body(PhysicsRigidBodyKind::Dynamic, Vec3::Y)),
                "physics.world.add_rigid_body",
            )?,
            boundary_error(
                world.add_collider(ball_shape(dynamic, 0.25)),
                "physics.world.add_collider",
            )?,
            boundary_error(
                world.add_fixed_joint(fixed, dynamic),
                "physics.world.add_fixed_joint",
            )?,
            boundary_error(world.step(), "physics.world.step")?,
            boundary_error(world.body_transform(fixed), "physics.world.body_transform")?,
            boundary_error(
                world.raycast(PhysicsRaycast {
                    origin: Vec3::Y,
                    direction: -Vec3::Y,
                    max_toi: 2.0,
                    solid: true,
                }),
                "physics.world.raycast",
            )?,
            boundary_error(
                world.intersect_aabb(PhysicsAabbQuery {
                    center: Vec3::ZERO,
                    half_extents: Vec3::ONE,
                }),
                "physics.world.intersect_aabb",
            )?,
            boundary_error(world.step_report(), "physics.world.step_report")?,
            boundary_error(
                world.move_character(
                    PhysicsCharacterControllerConfig::default(),
                    PhysicsCharacterMoveRequest {
                        position: Vec3::ZERO,
                        shape: PhysicsColliderShape::Ball { radius: 0.5 },
                        desired_translation: Vec3::X,
                        delta_time_secs: 1.0 / 60.0,
                    },
                ),
                "physics.character.move",
            )?,
        ];
        inside_snapshot = snapshot();
    }
    let after_snapshot = snapshot();
    Ok(json!({
        "guarded_operation_count": errors.len(),
        "errors": errors,
        "inside_hot_path_snapshot": inside_snapshot,
        "after_hot_path_snapshot": after_snapshot,
        "acceptance": {
            "inside_phase": inside_snapshot.current_hot_path_phase,
            "inside_depth": inside_snapshot.current_hot_path_depth,
            "after_phase": after_snapshot.current_hot_path_phase,
            "after_depth": after_snapshot.current_hot_path_depth,
            "hot_path_physics_access_total": inside_snapshot.hot_path_physics_access_total,
            "physics_access_total": inside_snapshot.physics_access_total,
        }
    }))
}

fn hash_boundary() -> Result<Value, EngineError> {
    reset_for_fsv();
    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64)?, 1112)?;
    sim.spawn_body(
        FixedVec3::ZERO,
        FixedVec3::from_i32s(64, 0, 0),
        FixedVec3::ZERO,
    )?;
    let before_hash = sim.state_hash();

    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default())?;
    let body = world.add_rigid_body(body(
        PhysicsRigidBodyKind::Dynamic,
        Vec3::new(0.0, 3.0, 0.0),
    ))?;
    world.add_collider(ball_shape(body, 0.5))?;
    let step_report = world.step()?;
    let after_hash = sim.state_hash();
    let boundary = snapshot();

    Ok(json!({
        "before_hash": before_hash,
        "after_hash": after_hash,
        "before_hash_hex": format!("{before_hash:016X}"),
        "after_hash_hex": format!("{after_hash:016X}"),
        "hash_equal": before_hash == after_hash,
        "physics_step_report": step_report,
        "boundary_snapshot": boundary,
        "acceptance": {
            "physics_step_index": step_report.step_index,
            "hot_path_physics_access_total": boundary.hot_path_physics_access_total,
            "physics_access_total": boundary.physics_access_total,
        }
    }))
}

fn boundary_error<T>(result: Result<T, EngineError>, expected: &str) -> Result<Value, EngineError> {
    match result {
        Err(EngineError::DeterminismBoundaryViolation { phase, operation }) => {
            if phase != "sim_tick" || operation != expected {
                return Err(EngineError::PhysicsInvalidConfig {
                    field: format!("physics.fsv.boundary.{expected}"),
                    reason: format!(
                        "expected sim_tick/{expected} boundary error, got {phase}/{operation}"
                    ),
                });
            }
            Ok(json!({
                "operation": operation,
                "phase": phase,
                "error_code": "LITD-DETERMINISM-BOUNDARY-VIOLATION"
            }))
        }
        Err(error) => Err(error),
        Ok(_) => Err(EngineError::PhysicsInvalidConfig {
            field: format!("physics.fsv.boundary.{expected}"),
            reason: "expected Rapier access to fail inside deterministic hot path".to_string(),
        }),
    }
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
