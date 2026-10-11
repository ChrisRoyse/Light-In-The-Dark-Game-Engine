//! Rapier-backed presentation/gameplay physics wrapper.

#[path = "physics/character.rs"]
mod character;

use rapier3d::prelude::{
    Aabb, BroadPhaseBvh, CCDSolver, ColliderBuilder, ColliderHandle, ColliderSet,
    FixedJointBuilder, ImpulseJointHandle, ImpulseJointSet, IntegrationParameters, IslandManager,
    MultibodyJointSet, NarrowPhase, PhysicsPipeline, QueryFilter, Ray, RigidBodyBuilder,
    RigidBodyHandle, RigidBodySet, Vector,
};
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    determinism_boundary::record_physics_access,
    math::{Transform, Vec3},
};

pub use character::{
    PhysicsCharacterAutostep, PhysicsCharacterControllerConfig, PhysicsCharacterMove,
    PhysicsCharacterMoveRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsWorldConfig {
    pub gravity: Vec3,
    pub timestep_secs: f32,
}

impl Default for PhysicsWorldConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            timestep_secs: 1.0 / 60.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsRigidBodyKind {
    Dynamic,
    Fixed,
    KinematicPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsRigidBodyDesc {
    pub kind: PhysicsRigidBodyKind,
    pub translation: Vec3,
    pub linear_velocity: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PhysicsColliderShape {
    Cuboid { half_extents: Vec3 },
    Ball { radius: f32 },
    CapsuleY { half_height: f32, radius: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsColliderDesc {
    pub parent: Option<PhysicsBodyHandle>,
    pub shape: PhysicsColliderShape,
    pub translation: Vec3,
    pub sensor: bool,
    pub friction: f32,
    pub restitution: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsRaycast {
    pub origin: Vec3,
    pub direction: Vec3,
    pub max_toi: f32,
    pub solid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsAabbQuery {
    pub center: Vec3,
    pub half_extents: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PhysicsBodyHandle {
    pub id: u32,
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PhysicsColliderHandle {
    pub id: u32,
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PhysicsJointHandle {
    pub id: u32,
    pub generation: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsRayHit {
    pub collider: PhysicsColliderHandle,
    pub time_of_impact: f32,
    pub point: Vec3,
    pub normal: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsStepReport {
    pub schema: String,
    pub step_index: u64,
    pub body_count: usize,
    pub collider_count: usize,
    pub impulse_joint_count: usize,
    pub timestep_secs: f32,
}

pub struct PhysicsWorld {
    gravity: Vector,
    integration: IntegrationParameters,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    step_index: u64,
}

impl PhysicsWorld {
    pub fn new(config: PhysicsWorldConfig) -> Result<Self, EngineError> {
        guard_physics_outside_hot_path("physics.world.new")?;
        validate_vec3(config.gravity, "config.gravity")?;
        if !config.timestep_secs.is_finite() || config.timestep_secs <= 0.0 {
            return invalid_physics("config.timestep_secs", "must be finite and positive");
        }
        Ok(Self {
            gravity: vec3_to_rapier(config.gravity),
            integration: IntegrationParameters {
                dt: config.timestep_secs,
                ..IntegrationParameters::default()
            },
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            step_index: 0,
        })
    }

    pub fn add_rigid_body(
        &mut self,
        desc: PhysicsRigidBodyDesc,
    ) -> Result<PhysicsBodyHandle, EngineError> {
        guard_physics_outside_hot_path("physics.world.add_rigid_body")?;
        validate_vec3(desc.translation, "rigid_body.translation")?;
        validate_vec3(desc.linear_velocity, "rigid_body.linear_velocity")?;
        let builder = match desc.kind {
            PhysicsRigidBodyKind::Dynamic => RigidBodyBuilder::dynamic(),
            PhysicsRigidBodyKind::Fixed => RigidBodyBuilder::fixed(),
            PhysicsRigidBodyKind::KinematicPosition => RigidBodyBuilder::kinematic_position_based(),
        }
        .translation(vec3_to_rapier(desc.translation))
        .linvel(vec3_to_rapier(desc.linear_velocity));
        Ok(PhysicsBodyHandle::from(self.bodies.insert(builder)))
    }

    pub fn add_collider(
        &mut self,
        desc: PhysicsColliderDesc,
    ) -> Result<PhysicsColliderHandle, EngineError> {
        guard_physics_outside_hot_path("physics.world.add_collider")?;
        validate_collider_desc(desc)?;
        let builder = collider_builder(desc.shape)?
            .translation(vec3_to_rapier(desc.translation))
            .sensor(desc.sensor)
            .friction(desc.friction)
            .restitution(desc.restitution);
        let handle = match desc.parent {
            Some(parent) => {
                let parent = parent.to_rapier();
                if self.bodies.get(parent).is_none() {
                    return invalid_physics("collider.parent", "parent rigid body does not exist");
                }
                self.colliders
                    .insert_with_parent(builder, parent, &mut self.bodies)
            }
            None => self.colliders.insert(builder),
        };
        Ok(PhysicsColliderHandle::from(handle))
    }

    pub fn add_fixed_joint(
        &mut self,
        body1: PhysicsBodyHandle,
        body2: PhysicsBodyHandle,
    ) -> Result<PhysicsJointHandle, EngineError> {
        guard_physics_outside_hot_path("physics.world.add_fixed_joint")?;
        let body1 = body1.to_rapier();
        let body2 = body2.to_rapier();
        if self.bodies.get(body1).is_none() || self.bodies.get(body2).is_none() {
            return invalid_physics("joint.body", "joint body handle does not exist");
        }
        let joint = self
            .impulse_joints
            .insert(body1, body2, FixedJointBuilder::new(), true);
        Ok(PhysicsJointHandle::from(joint))
    }

    pub fn step(&mut self) -> Result<PhysicsStepReport, EngineError> {
        guard_physics_outside_hot_path("physics.world.step")?;
        self.pipeline.step(
            self.gravity,
            &self.integration,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &(),
            &(),
        );
        self.step_index =
            self.step_index
                .checked_add(1)
                .ok_or_else(|| EngineError::PhysicsInvalidConfig {
                    field: "physics.step_index".to_string(),
                    reason: "step index overflowed".to_string(),
                })?;
        Ok(self.build_step_report())
    }

    pub fn body_transform(&self, handle: PhysicsBodyHandle) -> Result<Transform, EngineError> {
        guard_physics_outside_hot_path("physics.world.body_transform")?;
        let body = self.bodies.get(handle.to_rapier()).ok_or_else(|| {
            EngineError::PhysicsInvalidConfig {
                field: "physics.body_handle".to_string(),
                reason: "rigid body does not exist".to_string(),
            }
        })?;
        Ok(Transform::from_translation(rapier_to_vec3(
            body.translation(),
        )))
    }

    pub fn raycast(&self, request: PhysicsRaycast) -> Result<Option<PhysicsRayHit>, EngineError> {
        guard_physics_outside_hot_path("physics.world.raycast")?;
        validate_raycast(request)?;
        let ray = Ray::new(
            point3(request.origin),
            vec3_to_rapier(request.direction.normalize()),
        );
        let query = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default(),
        );
        Ok(query
            .cast_ray_and_get_normal(&ray, request.max_toi, request.solid)
            .map(|(collider, hit)| PhysicsRayHit {
                collider: PhysicsColliderHandle::from(collider),
                time_of_impact: hit.time_of_impact,
                point: rapier_to_vec3(ray.point_at(hit.time_of_impact)),
                normal: rapier_to_vec3(hit.normal),
            }))
    }

    pub fn intersect_aabb(
        &self,
        request: PhysicsAabbQuery,
    ) -> Result<Vec<PhysicsColliderHandle>, EngineError> {
        guard_physics_outside_hot_path("physics.world.intersect_aabb")?;
        validate_aabb_query(request)?;
        let min = request.center - request.half_extents;
        let max = request.center + request.half_extents;
        let query = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default(),
        );
        let mut hits = query
            .intersect_aabb_conservative(Aabb::new(point3(min), point3(max)))
            .map(|(handle, _)| PhysicsColliderHandle::from(handle))
            .collect::<Vec<_>>();
        hits.sort_unstable();
        Ok(hits)
    }

    pub fn step_report(&self) -> Result<PhysicsStepReport, EngineError> {
        guard_physics_outside_hot_path("physics.world.step_report")?;
        Ok(self.build_step_report())
    }

    fn build_step_report(&self) -> PhysicsStepReport {
        PhysicsStepReport {
            schema: "litd.physics_step_report.v1".to_string(),
            step_index: self.step_index,
            body_count: self.bodies.len(),
            collider_count: self.colliders.len(),
            impulse_joint_count: self.impulse_joints.len(),
            timestep_secs: self.integration.dt,
        }
    }
}

impl From<RigidBodyHandle> for PhysicsBodyHandle {
    fn from(value: RigidBodyHandle) -> Self {
        let (id, generation) = value.into_raw_parts();
        Self { id, generation }
    }
}

impl PhysicsBodyHandle {
    fn to_rapier(self) -> RigidBodyHandle {
        RigidBodyHandle::from_raw_parts(self.id, self.generation)
    }
}

impl From<ColliderHandle> for PhysicsColliderHandle {
    fn from(value: ColliderHandle) -> Self {
        let (id, generation) = value.into_raw_parts();
        Self { id, generation }
    }
}

impl From<ImpulseJointHandle> for PhysicsJointHandle {
    fn from(value: ImpulseJointHandle) -> Self {
        let (id, generation) = value.into_raw_parts();
        Self { id, generation }
    }
}

fn collider_builder(shape: PhysicsColliderShape) -> Result<ColliderBuilder, EngineError> {
    match shape {
        PhysicsColliderShape::Cuboid { half_extents } => {
            validate_vec3(half_extents, "collider.shape.cuboid.half_extents")?;
            if half_extents.x <= 0.0 || half_extents.y <= 0.0 || half_extents.z <= 0.0 {
                return invalid_physics(
                    "collider.shape.cuboid.half_extents",
                    "half extents must be positive",
                );
            }
            Ok(ColliderBuilder::cuboid(
                half_extents.x,
                half_extents.y,
                half_extents.z,
            ))
        }
        PhysicsColliderShape::Ball { radius } => {
            validate_positive(radius, "collider.shape.ball.radius")?;
            Ok(ColliderBuilder::ball(radius))
        }
        PhysicsColliderShape::CapsuleY {
            half_height,
            radius,
        } => {
            validate_positive(half_height, "collider.shape.capsule_y.half_height")?;
            validate_positive(radius, "collider.shape.capsule_y.radius")?;
            Ok(ColliderBuilder::capsule_y(half_height, radius))
        }
    }
}

fn validate_collider_desc(desc: PhysicsColliderDesc) -> Result<(), EngineError> {
    validate_vec3(desc.translation, "collider.translation")?;
    if !desc.friction.is_finite() || desc.friction < 0.0 {
        return invalid_physics("collider.friction", "must be finite and non-negative");
    }
    if !desc.restitution.is_finite() || desc.restitution < 0.0 {
        return invalid_physics("collider.restitution", "must be finite and non-negative");
    }
    Ok(())
}

fn validate_raycast(request: PhysicsRaycast) -> Result<(), EngineError> {
    validate_vec3(request.origin, "raycast.origin")?;
    validate_vec3(request.direction, "raycast.direction")?;
    if request.direction.length() <= f32::EPSILON {
        return invalid_physics("raycast.direction", "direction must be non-zero");
    }
    validate_positive(request.max_toi, "raycast.max_toi")
}

fn validate_aabb_query(request: PhysicsAabbQuery) -> Result<(), EngineError> {
    validate_vec3(request.center, "aabb.center")?;
    validate_vec3(request.half_extents, "aabb.half_extents")?;
    if request.half_extents.x <= 0.0
        || request.half_extents.y <= 0.0
        || request.half_extents.z <= 0.0
    {
        return invalid_physics("aabb.half_extents", "half extents must be positive");
    }
    Ok(())
}

fn validate_vec3(value: Vec3, field: &str) -> Result<(), EngineError> {
    if !value.is_finite() {
        return invalid_physics(field, "vector must contain finite values");
    }
    Ok(())
}

fn validate_positive(value: f32, field: &str) -> Result<(), EngineError> {
    if !value.is_finite() || value <= 0.0 {
        return invalid_physics(field, "must be finite and positive");
    }
    Ok(())
}

fn invalid_physics<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::PhysicsInvalidConfig {
        field: format!("physics.{}", field.into()),
        reason: reason.into(),
    })
}

pub(crate) fn guard_physics_outside_hot_path(operation: &'static str) -> Result<(), EngineError> {
    record_physics_access(operation)
}

fn vec3_to_rapier(value: Vec3) -> Vector {
    Vector::new(value.x, value.y, value.z)
}

fn rapier_to_vec3(value: Vector) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

fn point3(value: Vec3) -> Vector {
    Vector::new(value.x, value.y, value.z)
}
