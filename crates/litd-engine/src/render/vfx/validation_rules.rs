use crate::{
    EngineError,
    math::{Aabb, Vec3},
    render::vfx::{
        VfxColor, VfxCurve, VfxGradient, VfxInitializeModule, VfxOutputModule, VfxRange, VfxShape,
        VfxSpawnModule, VfxUpdateModule, VfxValidationLimits, VfxValidationReport, VfxVelocity,
        invalid_vfx,
    },
};

pub(super) fn validate_stage_counts(
    emitter: &crate::render::vfx::VfxEmitter,
    prefix: &str,
    limits: VfxValidationLimits,
) -> Result<(), EngineError> {
    if emitter.spawn.is_empty() {
        return invalid_vfx(format!("{prefix}.spawn"), "must contain spawn modules");
    }
    if emitter.initialize.is_empty() {
        return invalid_vfx(
            format!("{prefix}.initialize"),
            "must contain initialize modules",
        );
    }
    if emitter.update.is_empty() {
        return invalid_vfx(format!("{prefix}.update"), "must contain update modules");
    }
    if emitter.output.is_empty() {
        return invalid_vfx(format!("{prefix}.output"), "must contain output modules");
    }
    if emitter.output.len() > limits.max_outputs_per_emitter {
        return invalid_vfx(
            format!("{prefix}.output"),
            format!(
                "has {} outputs but limit is {}",
                emitter.output.len(),
                limits.max_outputs_per_emitter
            ),
        );
    }
    Ok(())
}

pub(super) fn validate_spawn_module(
    module: &VfxSpawnModule,
    field: &str,
) -> Result<(), EngineError> {
    match module {
        VfxSpawnModule::ConstantRate {
            particles_per_second,
        } => validate_positive(
            format!("{field}.particles_per_second"),
            *particles_per_second,
        ),
        VfxSpawnModule::Burst {
            count,
            time_seconds,
        } => {
            if *count == 0 {
                return invalid_vfx(format!("{field}.count"), "burst count must be non-zero");
            }
            validate_finite_non_negative(format!("{field}.time_seconds"), *time_seconds)
        }
    }
}

pub(super) fn validate_initialize_module(
    module: &VfxInitializeModule,
    field: &str,
    seen: &mut EmitterLifecyclePresence,
) -> Result<(), EngineError> {
    match module {
        VfxInitializeModule::Shape(shape) => {
            seen.shape = true;
            validate_shape(shape, field)
        }
        VfxInitializeModule::Velocity(velocity) => {
            seen.velocity = true;
            validate_velocity(velocity, field)
        }
        VfxInitializeModule::Color { color } => {
            seen.color = true;
            validate_color(format!("{field}.color"), *color)
        }
        VfxInitializeModule::Size { pixels } => {
            seen.size = true;
            validate_range(format!("{field}.pixels"), *pixels, true)
        }
        VfxInitializeModule::Lifetime { seconds } => {
            seen.lifetime = true;
            validate_range(format!("{field}.seconds"), *seconds, true)
        }
    }
}

pub(super) fn validate_update_module(
    module: &VfxUpdateModule,
    field: &str,
    limits: VfxValidationLimits,
) -> Result<(), EngineError> {
    match module {
        VfxUpdateModule::Acceleration { world_acceleration } => {
            validate_vec3(format!("{field}.world_acceleration"), *world_acceleration)
        }
        VfxUpdateModule::Drag { coefficient } => {
            validate_finite_non_negative(format!("{field}.coefficient"), *coefficient)
        }
        VfxUpdateModule::Noise {
            amplitude,
            frequency,
        } => {
            validate_finite_non_negative(format!("{field}.amplitude"), *amplitude)?;
            validate_positive(format!("{field}.frequency"), *frequency)
        }
        VfxUpdateModule::ColorOverLife { gradient } => {
            validate_gradient(gradient, &format!("{field}.gradient"), limits)
        }
        VfxUpdateModule::SizeOverLife { curve } => {
            validate_curve(curve, &format!("{field}.curve"), limits)
        }
        VfxUpdateModule::Collision { restitution } => {
            validate_normalized(format!("{field}.restitution"), *restitution)
        }
    }
}

pub(super) fn validate_output_module(
    module: &VfxOutputModule,
    field: &str,
    report: &mut VfxValidationReport,
) -> Result<(), EngineError> {
    match module {
        VfxOutputModule::Sprite {
            material_id,
            texture_atlas_slot,
            ..
        } => {
            validate_non_empty(format!("{field}.material_id"), material_id)?;
            validate_non_empty(format!("{field}.texture_atlas_slot"), texture_atlas_slot)?;
            report.sprite_output_count += 1;
        }
        VfxOutputModule::Mesh {
            mesh_id,
            material_id,
            ..
        } => {
            validate_non_empty(format!("{field}.mesh_id"), mesh_id)?;
            validate_non_empty(format!("{field}.material_id"), material_id)?;
            report.mesh_output_count += 1;
        }
        VfxOutputModule::Trail {
            material_id,
            width_pixels,
            ..
        } => {
            validate_non_empty(format!("{field}.material_id"), material_id)?;
            validate_positive(format!("{field}.width_pixels"), *width_pixels)?;
            report.trail_output_count += 1;
        }
        VfxOutputModule::Light {
            color,
            intensity_lumens,
            radius_meters,
        } => {
            validate_color(format!("{field}.color"), *color)?;
            validate_positive(format!("{field}.intensity_lumens"), *intensity_lumens)?;
            validate_positive(format!("{field}.radius_meters"), *radius_meters)?;
            report.light_output_count += 1;
        }
    }
    Ok(())
}

pub(super) fn validate_bounds(field: impl Into<String>, bounds: Aabb) -> Result<(), EngineError> {
    if !bounds.min.is_finite()
        || !bounds.max.is_finite()
        || bounds.min.x >= bounds.max.x
        || bounds.min.y >= bounds.max.y
        || bounds.min.z >= bounds.max.z
    {
        return invalid_vfx(
            field,
            "bounds must be finite and have positive extent on every axis",
        );
    }
    Ok(())
}

fn validate_shape(shape: &VfxShape, field: &str) -> Result<(), EngineError> {
    match shape {
        VfxShape::Point => Ok(()),
        VfxShape::Sphere { radius_meters } => {
            validate_positive(format!("{field}.radius_meters"), *radius_meters)
        }
        VfxShape::Cone {
            radius_meters,
            angle_degrees,
        } => {
            validate_positive(format!("{field}.radius_meters"), *radius_meters)?;
            validate_range_scalar(format!("{field}.angle_degrees"), *angle_degrees, 0.0, 180.0)
        }
        VfxShape::Box {
            half_extents_meters,
        } => validate_positive_vec3(format!("{field}.half_extents_meters"), *half_extents_meters),
    }
}

fn validate_velocity(velocity: &VfxVelocity, field: &str) -> Result<(), EngineError> {
    match velocity {
        VfxVelocity::Radial {
            speed_meters_per_second,
        }
        | VfxVelocity::Random {
            speed_meters_per_second,
        } => validate_range(
            format!("{field}.speed_meters_per_second"),
            *speed_meters_per_second,
            true,
        ),
        VfxVelocity::Directional {
            direction,
            speed_meters_per_second,
        } => {
            validate_nonzero_vec3(format!("{field}.direction"), *direction)?;
            validate_range(
                format!("{field}.speed_meters_per_second"),
                *speed_meters_per_second,
                true,
            )
        }
    }
}

fn validate_gradient(
    gradient: &VfxGradient,
    field: &str,
    limits: VfxValidationLimits,
) -> Result<(), EngineError> {
    if gradient.stops.len() < 2 || gradient.stops.len() > limits.max_gradient_stops {
        return invalid_vfx(
            field,
            format!(
                "gradient must contain 2..={} stops",
                limits.max_gradient_stops
            ),
        );
    }
    let mut previous = -f32::INFINITY;
    for (index, stop) in gradient.stops.iter().enumerate() {
        validate_normalized(format!("{field}.stops[{index}].t"), stop.t)?;
        validate_color(format!("{field}.stops[{index}].color"), stop.color)?;
        if stop.t <= previous {
            return invalid_vfx(
                format!("{field}.stops[{index}].t"),
                "gradient stop positions must be strictly increasing",
            );
        }
        previous = stop.t;
    }
    validate_endpoints(
        field,
        gradient.stops.first().unwrap().t,
        gradient.stops.last().unwrap().t,
    )
}

fn validate_curve(
    curve: &VfxCurve,
    field: &str,
    limits: VfxValidationLimits,
) -> Result<(), EngineError> {
    if curve.points.len() < 2 || curve.points.len() > limits.max_curve_points {
        return invalid_vfx(
            field,
            format!("curve must contain 2..={} points", limits.max_curve_points),
        );
    }
    let mut previous = -f32::INFINITY;
    for (index, point) in curve.points.iter().enumerate() {
        validate_normalized(format!("{field}.points[{index}].t"), point.t)?;
        validate_finite_non_negative(format!("{field}.points[{index}].value"), point.value)?;
        if point.t <= previous {
            return invalid_vfx(
                format!("{field}.points[{index}].t"),
                "curve points must be strictly increasing",
            );
        }
        previous = point.t;
    }
    validate_endpoints(
        field,
        curve.points.first().unwrap().t,
        curve.points.last().unwrap().t,
    )
}

fn validate_endpoints(field: &str, first: f32, last: f32) -> Result<(), EngineError> {
    if first != 0.0 || last != 1.0 {
        return invalid_vfx(
            field,
            "curve/gradient endpoints must start at t=0 and end at t=1",
        );
    }
    Ok(())
}

fn validate_range(
    field: impl Into<String>,
    range: VfxRange,
    strictly_positive: bool,
) -> Result<(), EngineError> {
    let field = field.into();
    if !range.min.is_finite() || !range.max.is_finite() || range.min > range.max {
        return invalid_vfx(field, "range min/max must be finite and ordered");
    }
    if strictly_positive && range.min <= 0.0 {
        return invalid_vfx(field, "range minimum must be positive");
    }
    Ok(())
}

fn validate_color(field: impl Into<String>, color: VfxColor) -> Result<(), EngineError> {
    let field = field.into();
    validate_normalized(format!("{field}.r"), color.r)?;
    validate_normalized(format!("{field}.g"), color.g)?;
    validate_normalized(format!("{field}.b"), color.b)?;
    validate_normalized(format!("{field}.a"), color.a)
}

fn validate_vec3(field: impl Into<String>, value: Vec3) -> Result<(), EngineError> {
    if !value.is_finite() {
        return invalid_vfx(field, "vector must be finite");
    }
    Ok(())
}

fn validate_positive_vec3(field: impl Into<String>, value: Vec3) -> Result<(), EngineError> {
    let field = field.into();
    validate_vec3(field.clone(), value)?;
    if value.x <= 0.0 || value.y <= 0.0 || value.z <= 0.0 {
        return invalid_vfx(field, "vector components must be positive");
    }
    Ok(())
}

fn validate_nonzero_vec3(field: impl Into<String>, value: Vec3) -> Result<(), EngineError> {
    let field = field.into();
    validate_vec3(field.clone(), value)?;
    if value.length_squared() <= f32::EPSILON {
        return invalid_vfx(field, "direction vector must be non-zero");
    }
    Ok(())
}

fn validate_positive(field: impl Into<String>, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value <= 0.0 {
        return invalid_vfx(field, "must be finite and positive");
    }
    Ok(())
}

fn validate_finite_non_negative(field: impl Into<String>, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value < 0.0 {
        return invalid_vfx(field, "must be finite and non-negative");
    }
    Ok(())
}

fn validate_normalized(field: impl Into<String>, value: f32) -> Result<(), EngineError> {
    validate_range_scalar(field, value, 0.0, 1.0)
}

fn validate_range_scalar(
    field: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
) -> Result<(), EngineError> {
    if !value.is_finite() || value < min || value > max {
        return invalid_vfx(field, format!("must be finite inside {min}..={max}"));
    }
    Ok(())
}

fn validate_non_empty(field: impl Into<String>, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_vfx(field, "must be non-empty");
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct EmitterLifecyclePresence {
    pub(super) shape: bool,
    pub(super) velocity: bool,
    pub(super) color: bool,
    pub(super) size: bool,
    pub(super) lifetime: bool,
}

impl EmitterLifecyclePresence {
    pub(super) fn validate(&self, prefix: &str) -> Result<(), EngineError> {
        for (name, present) in [
            ("shape", self.shape),
            ("velocity", self.velocity),
            ("color", self.color),
            ("size", self.size),
            ("lifetime", self.lifetime),
        ] {
            if !present {
                return invalid_vfx(
                    format!("{prefix}.initialize.{name}"),
                    "initialize stage must include shape, velocity, color, size, and lifetime",
                );
            }
        }
        Ok(())
    }
}
