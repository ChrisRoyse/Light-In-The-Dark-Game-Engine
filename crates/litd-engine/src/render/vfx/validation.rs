use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VFX_MAX_CURVE_POINTS, VFX_MAX_EMITTERS, VFX_MAX_GRADIENT_STOPS,
        VFX_MAX_OUTPUTS_PER_EMITTER, VFX_MAX_PARTICLES_PER_EFFECT, VFX_MAX_PARTICLES_PER_EMITTER,
        VfxEffectAsset, VfxEmitter, invalid_vfx, invalid_vfx_error,
    },
};

use super::validation_rules::{
    EmitterLifecyclePresence, validate_bounds, validate_initialize_module, validate_output_module,
    validate_spawn_module, validate_stage_counts, validate_update_module,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VfxValidationLimits {
    pub max_emitters: usize,
    pub max_particles_per_emitter: u32,
    pub max_particles_per_effect: u32,
    pub max_outputs_per_emitter: usize,
    pub max_gradient_stops: usize,
    pub max_curve_points: usize,
}

impl Default for VfxValidationLimits {
    fn default() -> Self {
        Self {
            max_emitters: VFX_MAX_EMITTERS,
            max_particles_per_emitter: VFX_MAX_PARTICLES_PER_EMITTER,
            max_particles_per_effect: VFX_MAX_PARTICLES_PER_EFFECT,
            max_outputs_per_emitter: VFX_MAX_OUTPUTS_PER_EMITTER,
            max_gradient_stops: VFX_MAX_GRADIENT_STOPS,
            max_curve_points: VFX_MAX_CURVE_POINTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxValidationReport {
    pub schema_version: u32,
    pub effect_id: String,
    pub emitter_count: usize,
    pub total_max_particles: u32,
    pub spawn_module_count: usize,
    pub initialize_module_count: usize,
    pub update_module_count: usize,
    pub output_module_count: usize,
    pub sprite_output_count: usize,
    pub mesh_output_count: usize,
    pub trail_output_count: usize,
    pub light_output_count: usize,
    pub draw_group_count: usize,
    pub presentation_only: bool,
    pub source_of_truth: String,
}

pub fn validate_effect(effect: &VfxEffectAsset) -> Result<VfxValidationReport, EngineError> {
    validate_effect_with_limits(effect, VfxValidationLimits::default())
}

pub fn validate_effect_with_limits(
    effect: &VfxEffectAsset,
    limits: VfxValidationLimits,
) -> Result<VfxValidationReport, EngineError> {
    if effect.schema_version != VFX_EFFECT_SCHEMA_VERSION {
        return invalid_vfx(
            "schema_version",
            format!(
                "expected {}, got {}",
                VFX_EFFECT_SCHEMA_VERSION, effect.schema_version
            ),
        );
    }
    validate_non_empty("effect_id", &effect.effect_id)?;
    validate_non_empty("display_name", &effect.display_name)?;
    if !effect.presentation_only {
        return invalid_vfx(
            "presentation_only",
            "must be true; VFX assets never mutate deterministic sim state",
        );
    }
    if effect.emitters.is_empty() {
        return invalid_vfx("emitters", "effect must contain at least one emitter");
    }
    if effect.emitters.len() > limits.max_emitters {
        return invalid_vfx(
            "emitters",
            format!(
                "effect has {} emitters but limit is {}",
                effect.emitters.len(),
                limits.max_emitters
            ),
        );
    }

    let mut report = VfxValidationReport {
        schema_version: effect.schema_version,
        effect_id: effect.effect_id.clone(),
        emitter_count: effect.emitters.len(),
        total_max_particles: 0,
        spawn_module_count: 0,
        initialize_module_count: 0,
        update_module_count: 0,
        output_module_count: 0,
        sprite_output_count: 0,
        mesh_output_count: 0,
        trail_output_count: 0,
        light_output_count: 0,
        draw_group_count: 0,
        presentation_only: effect.presentation_only,
        source_of_truth: "Calyx Aster VFX effect/content row".to_string(),
    };
    let mut emitter_ids = BTreeSet::new();
    for (index, emitter) in effect.emitters.iter().enumerate() {
        validate_emitter(emitter, index, limits, &mut report)?;
        if !emitter_ids.insert(emitter.emitter_id.clone()) {
            return invalid_vfx(
                format!("emitters[{index}].emitter_id"),
                "emitter ids must be unique",
            );
        }
    }
    if report.total_max_particles > limits.max_particles_per_effect {
        return invalid_vfx(
            "total_max_particles",
            format!(
                "effect particle cap {} exceeds limit {}",
                report.total_max_particles, limits.max_particles_per_effect
            ),
        );
    }
    Ok(report)
}

fn validate_emitter(
    emitter: &VfxEmitter,
    index: usize,
    limits: VfxValidationLimits,
    report: &mut VfxValidationReport,
) -> Result<(), EngineError> {
    let prefix = format!("emitters[{index}]");
    validate_non_empty(format!("{prefix}.emitter_id"), &emitter.emitter_id)?;
    validate_non_empty(format!("{prefix}.display_name"), &emitter.display_name)?;
    if emitter.max_particles == 0 || emitter.max_particles > limits.max_particles_per_emitter {
        return invalid_vfx(
            format!("{prefix}.max_particles"),
            format!("must be 1..={}", limits.max_particles_per_emitter),
        );
    }
    report.total_max_particles = report
        .total_max_particles
        .checked_add(emitter.max_particles)
        .ok_or_else(|| invalid_vfx_error("total_max_particles", "particle cap overflowed u32"))?;
    validate_bounds(format!("{prefix}.local_bounds"), emitter.local_bounds)?;
    validate_stage_counts(emitter, &prefix, limits)?;
    let mut seen_lifecycle = EmitterLifecyclePresence::default();
    for (module_index, module) in emitter.spawn.iter().enumerate() {
        validate_spawn_module(module, &format!("{prefix}.spawn[{module_index}]"))?;
    }
    for (module_index, module) in emitter.initialize.iter().enumerate() {
        validate_initialize_module(
            module,
            &format!("{prefix}.initialize[{module_index}]"),
            &mut seen_lifecycle,
        )?;
    }
    for (module_index, module) in emitter.update.iter().enumerate() {
        validate_update_module(module, &format!("{prefix}.update[{module_index}]"), limits)?;
    }
    for (module_index, module) in emitter.output.iter().enumerate() {
        validate_output_module(module, &format!("{prefix}.output[{module_index}]"), report)?;
    }
    seen_lifecycle.validate(&prefix)?;
    report.spawn_module_count += emitter.spawn.len();
    report.initialize_module_count += emitter.initialize.len();
    report.update_module_count += emitter.update.len();
    report.output_module_count += emitter.output.len();
    report.draw_group_count += emitter.output.len();
    Ok(())
}

fn validate_non_empty(field: impl Into<String>, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_vfx(field, "must be non-empty");
    }
    Ok(())
}
