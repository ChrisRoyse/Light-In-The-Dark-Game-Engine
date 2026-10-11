use crate::{
    EngineError,
    determinism_boundary::{HotPathPhase, enter_hot_path, reset_for_fsv, snapshot},
    math::{FixedVec3, Vec3},
    physics::{
        PhysicsAabbQuery, PhysicsBodyHandle, PhysicsCharacterControllerConfig,
        PhysicsCharacterMoveRequest, PhysicsColliderDesc, PhysicsColliderShape, PhysicsRaycast,
        PhysicsRigidBodyDesc, PhysicsRigidBodyKind, PhysicsWorld, PhysicsWorldConfig,
    },
    sim::{FixedTickSim, SimConfig},
};

#[test]
fn rapier_world_steps_bodies_joints_raycast_and_scene_queries() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default()).unwrap();
    let ground = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Fixed,
            translation: Vec3::new(0.0, -0.1, 0.0),
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();
    let ground_collider = world
        .add_collider(cuboid(ground, Vec3::new(5.0, 0.1, 5.0)))
        .unwrap();
    let ball = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Dynamic,
            translation: Vec3::new(0.0, 3.0, 0.0),
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();
    let _ball_collider = world.add_collider(ball_shape(ball, 0.5)).unwrap();
    let attached = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Dynamic,
            translation: Vec3::new(1.0, 3.0, 0.0),
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();
    let _attached_collider = world.add_collider(ball_shape(attached, 0.25)).unwrap();
    let joint = world.add_fixed_joint(ball, attached).unwrap();

    let report = world.step().unwrap();
    let ball_after = world.body_transform(ball).unwrap();
    assert_eq!(report.body_count, 3);
    assert_eq!(report.collider_count, 3);
    assert_eq!(report.impulse_joint_count, 1);
    assert!(ball_after.translation.y < 3.0);

    let hit = world
        .raycast(PhysicsRaycast {
            origin: Vec3::new(3.0, 5.0, 0.0),
            direction: Vec3::new(0.0, -1.0, 0.0),
            max_toi: 10.0,
            solid: true,
        })
        .unwrap()
        .expect("ray should hit the ground collider");
    assert_eq!(hit.collider, ground_collider);
    assert!(hit.normal.y > 0.5);

    let query_hits = world
        .intersect_aabb(PhysicsAabbQuery {
            center: Vec3::new(0.0, 1.5, 0.0),
            half_extents: Vec3::new(6.0, 5.0, 6.0),
        })
        .unwrap();
    assert_eq!(query_hits.len(), 3);
    assert!(query_hits.contains(&ground_collider));
    println!(
        "issue1110_physics step={} bodies={} colliders={} joint={:?} ray_toi={:.3} aabb_hits={}",
        report.step_index,
        report.body_count,
        report.collider_count,
        joint,
        hit.time_of_impact,
        query_hits.len()
    );
}

#[test]
fn kinematic_character_controller_reports_grounding_and_wall_collision() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default()).unwrap();
    let ground = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Fixed,
            translation: Vec3::new(0.0, -0.1, 0.0),
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();
    world
        .add_collider(cuboid(ground, Vec3::new(5.0, 0.1, 5.0)))
        .unwrap();
    let wall = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Fixed,
            translation: Vec3::new(1.2, 0.5, 0.0),
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();
    world
        .add_collider(cuboid(wall, Vec3::new(0.1, 0.5, 5.0)))
        .unwrap();
    world.step().unwrap();

    let movement = world
        .move_character(
            PhysicsCharacterControllerConfig {
                snap_to_ground: Some(0.3),
                ..PhysicsCharacterControllerConfig::default()
            },
            PhysicsCharacterMoveRequest {
                position: Vec3::new(0.0, 0.72, 0.0),
                shape: PhysicsColliderShape::CapsuleY {
                    half_height: 0.4,
                    radius: 0.3,
                },
                desired_translation: Vec3::new(2.0, -0.1, 0.0),
                delta_time_secs: 1.0 / 60.0,
            },
        )
        .unwrap();

    assert!(movement.grounded);
    assert!(movement.collision_count > 0);
    assert!(movement.effective_translation.x < movement.requested_translation.x);
    assert_eq!(
        movement.max_slope_climb_angle_radians,
        std::f32::consts::FRAC_PI_4
    );
    println!(
        "issue1111_character grounded={} collisions={} requested_x={:.3} effective_x={:.3} final_y={:.3}",
        movement.grounded,
        movement.collision_count,
        movement.requested_translation.x,
        movement.effective_translation.x,
        movement.final_position.y
    );
}

#[test]
fn invalid_physics_inputs_fail_loud() {
    let bad_world = error_string(PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec3::new(f32::NAN, 0.0, 0.0),
        timestep_secs: 1.0 / 60.0,
    }));
    assert!(bad_world.contains("physics.config.gravity"));

    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default()).unwrap();
    let body = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Fixed,
            translation: Vec3::ZERO,
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();
    let bad_collider = world
        .add_collider(PhysicsColliderDesc {
            parent: Some(body),
            shape: PhysicsColliderShape::Ball { radius: 0.0 },
            translation: Vec3::ZERO,
            sensor: false,
            friction: 0.5,
            restitution: 0.0,
        })
        .unwrap_err()
        .to_string();
    assert!(bad_collider.contains("physics.collider.shape.ball.radius"));

    let bad_ray = world
        .raycast(PhysicsRaycast {
            origin: Vec3::ZERO,
            direction: Vec3::ZERO,
            max_toi: 1.0,
            solid: true,
        })
        .unwrap_err()
        .to_string();
    assert!(bad_ray.contains("physics.raycast.direction"));

    let bad_character = world
        .move_character(
            PhysicsCharacterControllerConfig {
                up: Vec3::ZERO,
                ..PhysicsCharacterControllerConfig::default()
            },
            PhysicsCharacterMoveRequest {
                position: Vec3::ZERO,
                shape: PhysicsColliderShape::Ball { radius: 0.5 },
                desired_translation: Vec3::X,
                delta_time_secs: 1.0 / 60.0,
            },
        )
        .unwrap_err()
        .to_string();
    assert!(bad_character.contains("physics.character.up"));
}

#[test]
fn rapier_physics_is_rejected_inside_deterministic_hot_path() {
    reset_for_fsv();
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default()).unwrap();
    let body = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Fixed,
            translation: Vec3::ZERO,
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();

    let _hot_path = enter_hot_path(HotPathPhase::SimTick);
    assert_boundary_error(
        PhysicsWorld::new(PhysicsWorldConfig::default()),
        "physics.world.new",
    );
    assert_boundary_error(
        world.add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Dynamic,
            translation: Vec3::Y,
            linear_velocity: Vec3::ZERO,
        }),
        "physics.world.add_rigid_body",
    );
    assert_boundary_error(world.step(), "physics.world.step");
    assert_boundary_error(world.body_transform(body), "physics.world.body_transform");
    assert_boundary_error(
        world.raycast(PhysicsRaycast {
            origin: Vec3::ZERO,
            direction: -Vec3::Y,
            max_toi: 1.0,
            solid: true,
        }),
        "physics.world.raycast",
    );
    assert_boundary_error(world.step_report(), "physics.world.step_report");
    assert_boundary_error(
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
    );

    let boundary = snapshot();
    assert_eq!(boundary.current_hot_path_phase, Some(HotPathPhase::SimTick));
    assert_eq!(boundary.hot_path_physics_access_total, 7);
    println!(
        "issue1112_boundary hot_path_physics={} total_physics={} phase={:?}",
        boundary.hot_path_physics_access_total,
        boundary.physics_access_total,
        boundary.current_hot_path_phase
    );
}

#[test]
fn rapier_world_state_is_not_part_of_fixed_tick_sim_hash() {
    reset_for_fsv();
    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64).unwrap(), 1112).unwrap();
    sim.spawn_body(
        FixedVec3::ZERO,
        FixedVec3::from_i32s(64, 0, 0),
        FixedVec3::ZERO,
    )
    .unwrap();
    let before_hash = sim.state_hash();

    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default()).unwrap();
    let body = world
        .add_rigid_body(PhysicsRigidBodyDesc {
            kind: PhysicsRigidBodyKind::Dynamic,
            translation: Vec3::new(0.0, 3.0, 0.0),
            linear_velocity: Vec3::ZERO,
        })
        .unwrap();
    world.add_collider(ball_shape(body, 0.5)).unwrap();
    let report = world.step().unwrap();
    let after_hash = sim.state_hash();

    assert_eq!(before_hash, after_hash);
    assert_eq!(report.step_index, 1);
    println!(
        "issue1112_hash before={:016X} after={:016X} physics_step={}",
        before_hash, after_hash, report.step_index
    );
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

fn error_string<T>(result: Result<T, EngineError>) -> String {
    match result {
        Ok(_) => panic!("expected physics operation to fail"),
        Err(error) => error.to_string(),
    }
}

fn assert_boundary_error<T>(result: Result<T, EngineError>, operation: &str) {
    match result {
        Err(EngineError::DeterminismBoundaryViolation {
            phase,
            operation: actual_operation,
        }) => {
            assert_eq!(phase, "sim_tick");
            assert_eq!(actual_operation, operation);
        }
        Err(error) => panic!("expected determinism boundary error, got {error}"),
        Ok(_) => panic!("expected determinism boundary error"),
    }
}
