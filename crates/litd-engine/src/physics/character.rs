use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    prelude::Pose,
};
use serde::{Deserialize, Serialize};

use crate::{EngineError, math::Vec3};

use super::{
    PhysicsColliderShape, PhysicsWorld, collider_builder, guard_physics_outside_hot_path,
    invalid_physics, rapier_to_vec3, validate_positive, validate_vec3, vec3_to_rapier,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsCharacterAutostep {
    pub max_height: f32,
    pub min_width: f32,
    pub include_dynamic_bodies: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsCharacterControllerConfig {
    pub up: Vec3,
    pub offset: f32,
    pub slide: bool,
    pub autostep: Option<PhysicsCharacterAutostep>,
    pub max_slope_climb_angle_radians: f32,
    pub min_slope_slide_angle_radians: f32,
    pub snap_to_ground: Option<f32>,
}

impl Default for PhysicsCharacterControllerConfig {
    fn default() -> Self {
        Self {
            up: Vec3::Y,
            offset: 0.01,
            slide: true,
            autostep: None,
            max_slope_climb_angle_radians: std::f32::consts::FRAC_PI_4,
            min_slope_slide_angle_radians: std::f32::consts::FRAC_PI_4,
            snap_to_ground: Some(0.2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsCharacterMoveRequest {
    pub position: Vec3,
    pub shape: PhysicsColliderShape,
    pub desired_translation: Vec3,
    pub delta_time_secs: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsCharacterMove {
    pub schema: String,
    pub requested_translation: Vec3,
    pub effective_translation: Vec3,
    pub final_position: Vec3,
    pub grounded: bool,
    pub sliding_down_slope: bool,
    pub collision_count: usize,
    pub max_slope_climb_angle_radians: f32,
    pub min_slope_slide_angle_radians: f32,
}

impl PhysicsWorld {
    pub fn move_character(
        &self,
        config: PhysicsCharacterControllerConfig,
        request: PhysicsCharacterMoveRequest,
    ) -> Result<PhysicsCharacterMove, EngineError> {
        guard_physics_outside_hot_path("physics.character.move")?;
        validate_config(config)?;
        validate_request(request)?;
        let collider = collider_builder(request.shape)?.build();
        let controller = KinematicCharacterController {
            up: vec3_to_rapier(config.up.normalize()),
            offset: CharacterLength::Absolute(config.offset),
            slide: config.slide,
            autostep: config.autostep.map(|autostep| CharacterAutostep {
                max_height: CharacterLength::Absolute(autostep.max_height),
                min_width: CharacterLength::Absolute(autostep.min_width),
                include_dynamic_bodies: autostep.include_dynamic_bodies,
            }),
            max_slope_climb_angle: config.max_slope_climb_angle_radians,
            min_slope_slide_angle: config.min_slope_slide_angle_radians,
            snap_to_ground: config.snap_to_ground.map(CharacterLength::Absolute),
            ..KinematicCharacterController::default()
        };
        let query = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            rapier3d::prelude::QueryFilter::default(),
        );
        let mut collision_count = 0;
        let movement = controller.move_shape(
            request.delta_time_secs,
            &query,
            collider.shared_shape().as_ref(),
            &Pose::from_translation(vec3_to_rapier(request.position)),
            vec3_to_rapier(request.desired_translation),
            |_| collision_count += 1,
        );
        let effective_translation = rapier_to_vec3(movement.translation);
        Ok(PhysicsCharacterMove {
            schema: "litd.physics_character_move.v1".to_string(),
            requested_translation: request.desired_translation,
            effective_translation,
            final_position: request.position + effective_translation,
            grounded: movement.grounded,
            sliding_down_slope: movement.is_sliding_down_slope,
            collision_count,
            max_slope_climb_angle_radians: config.max_slope_climb_angle_radians,
            min_slope_slide_angle_radians: config.min_slope_slide_angle_radians,
        })
    }
}

fn validate_config(config: PhysicsCharacterControllerConfig) -> Result<(), EngineError> {
    validate_vec3(config.up, "character.up")?;
    if config.up.length() <= f32::EPSILON {
        return invalid_physics("character.up", "up vector must be non-zero");
    }
    validate_positive(config.offset, "character.offset")?;
    validate_angle(
        config.max_slope_climb_angle_radians,
        "character.max_slope_climb_angle_radians",
    )?;
    validate_angle(
        config.min_slope_slide_angle_radians,
        "character.min_slope_slide_angle_radians",
    )?;
    if let Some(snap) = config.snap_to_ground {
        validate_positive(snap, "character.snap_to_ground")?;
    }
    if let Some(autostep) = config.autostep {
        validate_positive(autostep.max_height, "character.autostep.max_height")?;
        validate_positive(autostep.min_width, "character.autostep.min_width")?;
    }
    Ok(())
}

fn validate_request(request: PhysicsCharacterMoveRequest) -> Result<(), EngineError> {
    validate_vec3(request.position, "character.position")?;
    validate_vec3(request.desired_translation, "character.desired_translation")?;
    validate_positive(request.delta_time_secs, "character.delta_time_secs")
}

fn validate_angle(value: f32, field: &str) -> Result<(), EngineError> {
    if !value.is_finite() || !(0.0..=std::f32::consts::FRAC_PI_2).contains(&value) {
        return invalid_physics(field, "angle must be finite and within 0..=pi/2 radians");
    }
    Ok(())
}
