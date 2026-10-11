use litd_hash::sha256_hex;
use serde::Serialize;

use crate::EngineError;

use super::object_micro_agents_types::{
    CalyxObjectVisualSignal, ObjectMicroAgentDefinition, ObjectMicroAgentLoweredEffect,
    ObjectMicroAgentReadback, ObjectMicroAgentVisualIntent,
};

pub(super) fn effect_fingerprint(
    effect: &ObjectMicroAgentLoweredEffect,
) -> Result<String, EngineError> {
    #[derive(Serialize)]
    struct Payload<'a> {
        effect_id: &'a str,
        agent_id: &'a str,
        object_id: &'a str,
        target_body: u32,
        visual_intent: ObjectMicroAgentVisualIntent,
        vfx_effect_id: &'a str,
        material_variant: &'a str,
        intensity_milli: u16,
        particle_budget: u32,
        provenance_refs: &'a [String],
        source: &'a str,
        deterministic: bool,
    }
    sha256_json(&Payload {
        effect_id: &effect.effect_id,
        agent_id: &effect.agent_id,
        object_id: &effect.object_id,
        target_body: effect.target_body.index(),
        visual_intent: effect.visual_intent,
        vfx_effect_id: &effect.vfx_effect_id,
        material_variant: &effect.material_variant,
        intensity_milli: effect.intensity_milli,
        particle_budget: effect.particle_budget,
        provenance_refs: &effect.provenance_refs,
        source: &effect.source,
        deterministic: effect.deterministic,
    })
}

pub(super) fn readback_fingerprint(
    readback: &ObjectMicroAgentReadback,
) -> Result<String, EngineError> {
    #[derive(Serialize)]
    struct Payload<'a> {
        schema: &'a str,
        run_id: &'a str,
        world_id: &'a str,
        agent_count: usize,
        accepted_count: usize,
        total_embedded_model_bytes: u64,
        total_particles_reserved: u32,
        hot_path_policy: &'a str,
        lowered_effects: &'a [ObjectMicroAgentLoweredEffect],
    }
    sha256_json(&Payload {
        schema: &readback.schema,
        run_id: &readback.run_id,
        world_id: &readback.world_id,
        agent_count: readback.agent_count,
        accepted_count: readback.accepted_count,
        total_embedded_model_bytes: readback.total_embedded_model_bytes,
        total_particles_reserved: readback.total_particles_reserved,
        hot_path_policy: &readback.hot_path_policy,
        lowered_effects: &readback.lowered_effects,
    })
}

pub(super) fn effect_id(
    run_id: &str,
    definition: &ObjectMicroAgentDefinition,
    signal: &CalyxObjectVisualSignal,
) -> String {
    sha256_hex(format!("{run_id}:{}:{}", definition.agent_id, signal.signal_id).as_bytes())
}

pub(super) fn decision_id(
    run_id: &str,
    definition: &ObjectMicroAgentDefinition,
    signal: &CalyxObjectVisualSignal,
) -> String {
    sha256_hex(
        format!(
            "decision:{run_id}:{}:{}",
            definition.agent_id, signal.signal_id
        )
        .as_bytes(),
    )
}

fn sha256_json(value: &impl Serialize) -> Result<String, EngineError> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "object-micro-agent-fingerprint-json",
            reason: error.to_string(),
        })
}
