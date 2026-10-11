//! Presentation-only VFX effect assets and lifecycle module validation.

mod content_store;
mod cue;
mod events;
mod gpu_particles;
mod light_atlas;
mod store;
mod types;
mod validation;
mod validation_rules;
mod workload;

pub use content_store::{
    VFX_EFFECT_DIGEST_ALGORITHM, VFX_EFFECT_MEDIA_TYPE, VFX_EFFECT_STORE_PREFIX,
    VfxEffectDescriptor, VfxEffectStoreCount, VfxEffectStoreReadback, VfxEffectStoreWriteback,
    count_effects_in_content_store, effect_store_relative_path, read_effect_from_content_store,
    validate_effect_descriptor, write_effect_to_content_store,
};
pub use cue::{
    VfxRenderCue, VfxRenderCueSourceKind, VfxResolvedRenderCue, resolve_render_cue_effect,
    validate_render_cue,
};
pub use events::{
    VfxRenderEvent, VfxRenderEventDrain, VfxRenderEventDrop, VfxRenderEventEnqueueReport,
    VfxRenderEventKind, VfxRenderEventQueue, VfxResolvedRenderEvent, resolve_render_event_effect,
    validate_render_event,
};
pub use gpu_particles::{
    VFX_GPU_PARTICLE_WGSL, VfxGpuParticleFrame, VfxGpuParticleImage, VfxGpuParticleMetrics,
    VfxGpuParticleSettings, VfxGpuParticleStats, render_vfx_particles_gpu,
    render_vfx_particles_gpu_with_context,
};
pub use light_atlas::{
    VFX_MAX_LOCAL_LIGHTS, VfxAtlasCatalog, VfxAtlasSample, VfxAtlasSlot, VfxOutputPlan,
    VfxPresentationTerms, cluster_vfx_output_lights, resolve_vfx_output_plan,
};
pub use store::{VfxEffectFileReadback, effect_to_json_bytes, read_effect_json, write_effect_json};
pub use types::{
    VFX_EFFECT_SCHEMA_VERSION, VFX_MAX_CURVE_POINTS, VFX_MAX_EMITTERS, VFX_MAX_GRADIENT_STOPS,
    VFX_MAX_OUTPUTS_PER_EMITTER, VFX_MAX_PARTICLES_PER_EFFECT, VFX_MAX_PARTICLES_PER_EMITTER,
    VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset, VfxEmitter, VfxGradient,
    VfxGradientStop, VfxInitializeModule, VfxOutputModule, VfxRange, VfxShape, VfxSpawnModule,
    VfxUpdateModule, VfxVelocity,
};
pub use validation::{VfxValidationLimits, VfxValidationReport, validate_effect};
pub use workload::{
    VfxRenderLod, VfxRenderWorkloadSettings, VfxRenderWorkloadStats, VfxSceneEffectInstance,
    build_vfx_render_workload,
};

use crate::EngineError;

pub(crate) fn invalid_vfx_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::VfxInvalidConfig {
        field: format!("render.vfx.{}", field.into()),
        reason: reason.into(),
    }
}

pub(crate) fn invalid_vfx<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_vfx_error(field, reason))
}

#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod gpu_particles_tests;
#[cfg(test)]
mod light_atlas_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod workload_tests;
