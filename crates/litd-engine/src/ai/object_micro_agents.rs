use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::{
    object_micro_agents_hash::{decision_id, effect_fingerprint, effect_id, readback_fingerprint},
    object_micro_agents_types::*,
    object_micro_agents_validate::{validate_input, validate_object_micro_agent_readback},
};

pub fn evaluate_object_micro_agents(
    input: &ObjectMicroAgentInput,
) -> Result<ObjectMicroAgentReadback, EngineError> {
    validate_input(input)?;
    let definitions = input
        .definitions
        .iter()
        .map(|definition| (definition.agent_id.as_str(), definition))
        .collect::<BTreeMap<_, _>>();
    let mut states = input
        .states
        .iter()
        .map(|state| (state.agent_id.as_str(), state.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut signals = input.signals.clone();
    signals.sort_by(|left, right| {
        (left.tick, left.signal_id.as_str()).cmp(&(right.tick, right.signal_id.as_str()))
    });

    let mut accepted_count = 0_usize;
    let mut particles_reserved = 0_u32;
    let mut decisions = Vec::with_capacity(signals.len());
    let mut lowered_effects = Vec::new();
    for signal in &signals {
        let decision = decision_for_signal(
            input,
            &definitions,
            &mut states,
            signal,
            accepted_count,
            particles_reserved,
        )?;
        if let Some(effect) = decision.lowered_effect.clone() {
            accepted_count += 1;
            particles_reserved = particles_reserved.saturating_add(effect.particle_budget);
            lowered_effects.push(effect);
        }
        decisions.push(decision);
    }
    let mut readback = ObjectMicroAgentReadback {
        schema: AI_OBJECT_MICRO_AGENT_SCHEMA.to_string(),
        run_id: input.run_id.clone(),
        world_id: input.world_id.clone(),
        agent_count: input.definitions.len(),
        signal_count: input.signals.len(),
        decision_count: decisions.len(),
        accepted_count,
        skipped_count: decisions.len().saturating_sub(accepted_count),
        embedded_model_count: input
            .definitions
            .iter()
            .map(|definition| definition.embedded_model_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        total_embedded_model_bytes: total_model_bytes(&input.definitions),
        visual_effect_count: lowered_effects.len(),
        total_particles_reserved: particles_reserved,
        spawnable_object_count: input.definitions.len(),
        hot_path_policy: AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY.to_string(),
        calyx_live_query_allowed_in_hot_path: false,
        lowering_boundary:
            "Calyx activates object micro-agents out-of-band; render consumes fingerprinted visual effects"
                .to_string(),
        decisions,
        lowered_effects,
        readback_fingerprint: String::new(),
    };
    readback.readback_fingerprint = readback_fingerprint(&readback)?;
    validate_object_micro_agent_readback(&readback)?;
    Ok(readback)
}

fn decision_for_signal(
    input: &ObjectMicroAgentInput,
    definitions: &BTreeMap<&str, &ObjectMicroAgentDefinition>,
    states: &mut BTreeMap<&str, ObjectMicroAgentState>,
    signal: &CalyxObjectVisualSignal,
    accepted_count: usize,
    particles_reserved: u32,
) -> Result<ObjectMicroAgentDecision, EngineError> {
    if signal.strength_milli < input.budget.min_signal_strength_milli {
        return Ok(skipped(
            input,
            signal,
            None,
            ObjectMicroAgentActivationGrade::SignalTooWeak,
        ));
    }
    let Some(definition) = select_definition(signal, definitions) else {
        return Ok(skipped(
            input,
            signal,
            None,
            ObjectMicroAgentActivationGrade::NoCandidate,
        ));
    };
    if !signal
        .tags
        .iter()
        .all(|tag| definition.ward_tags.contains(tag))
    {
        return Ok(skipped(
            input,
            signal,
            Some(definition),
            ObjectMicroAgentActivationGrade::WardBlocked,
        ));
    }
    let state = states
        .get_mut(definition.agent_id.as_str())
        .ok_or_else(|| micro_agent_invalid("state", "missing object micro-agent state"))?;
    if let Some(last_tick) = state.last_activation_tick
        && signal.tick.saturating_sub(last_tick) < input.budget.cooldown_ticks
    {
        return Ok(skipped(
            input,
            signal,
            Some(definition),
            ObjectMicroAgentActivationGrade::CooldownActive,
        ));
    }
    if accepted_count >= input.budget.max_activations_per_tick {
        return Ok(skipped(
            input,
            signal,
            Some(definition),
            ObjectMicroAgentActivationGrade::TickBudgetExceeded,
        ));
    }
    if particles_reserved.saturating_add(definition.particle_budget)
        > input.budget.max_particles_per_tick
    {
        return Ok(skipped(
            input,
            signal,
            Some(definition),
            ObjectMicroAgentActivationGrade::ParticleBudgetExceeded,
        ));
    }
    state.last_activation_tick = Some(signal.tick);
    state.spawned_instances = state.spawned_instances.saturating_add(1);
    accepted_decision(input, definition, signal)
}

fn accepted_decision(
    input: &ObjectMicroAgentInput,
    definition: &ObjectMicroAgentDefinition,
    signal: &CalyxObjectVisualSignal,
) -> Result<ObjectMicroAgentDecision, EngineError> {
    let support = signal
        .strength_milli
        .saturating_add((tag_overlap(&definition.trigger_tags, &signal.tags) as u16) * 10)
        .min(1000);
    let mut effect = ObjectMicroAgentLoweredEffect {
        effect_id: effect_id(&input.run_id, definition, signal),
        agent_id: definition.agent_id.clone(),
        object_id: definition.object_id.clone(),
        target_body: definition.target_body,
        visual_intent: definition.visual_intent,
        vfx_effect_id: definition.vfx_effect_id.clone(),
        material_variant: definition.material_variant.clone(),
        intensity_milli: support,
        particle_budget: definition.particle_budget,
        provenance_refs: merged_refs(
            &[format!("object://{}", definition.object_id)],
            &signal.provenance_refs,
        ),
        source: "lowered_object_micro_agent".to_string(),
        deterministic: true,
        fingerprint: String::new(),
    };
    effect.fingerprint = effect_fingerprint(&effect)?;
    Ok(ObjectMicroAgentDecision {
        decision_id: decision_id(&input.run_id, definition, signal),
        signal_id: signal.signal_id.clone(),
        agent_id: Some(definition.agent_id.clone()),
        object_id: Some(definition.object_id.clone()),
        accepted: true,
        grade: ObjectMicroAgentActivationGrade::Accepted,
        reason: "accepted from Calyx object signal into deterministic visual effect".to_string(),
        support_milli: support,
        lowered_effect: Some(effect),
    })
}

fn select_definition<'a>(
    signal: &CalyxObjectVisualSignal,
    definitions: &BTreeMap<&'a str, &'a ObjectMicroAgentDefinition>,
) -> Option<&'a ObjectMicroAgentDefinition> {
    if let Some(object_id) = signal.target_object_id.as_deref() {
        return definitions
            .values()
            .find(|definition| definition.object_id == object_id)
            .copied();
    }
    definitions
        .values()
        .filter(|definition| tag_overlap(&definition.trigger_tags, &signal.tags) > 0)
        .max_by_key(|definition| {
            (
                tag_overlap(&definition.trigger_tags, &signal.tags),
                std::cmp::Reverse(definition.agent_id.as_str()),
            )
        })
        .copied()
}

fn skipped(
    input: &ObjectMicroAgentInput,
    signal: &CalyxObjectVisualSignal,
    definition: Option<&ObjectMicroAgentDefinition>,
    grade: ObjectMicroAgentActivationGrade,
) -> ObjectMicroAgentDecision {
    ObjectMicroAgentDecision {
        decision_id: format!("skip.{}.{}.{}", input.run_id, signal.signal_id, grade as u8),
        signal_id: signal.signal_id.clone(),
        agent_id: definition.map(|definition| definition.agent_id.clone()),
        object_id: definition.map(|definition| definition.object_id.clone()),
        accepted: false,
        grade,
        reason: grade_reason(grade).to_string(),
        support_milli: 0,
        lowered_effect: None,
    }
}

pub(super) fn total_model_bytes(definitions: &[ObjectMicroAgentDefinition]) -> u64 {
    definitions
        .iter()
        .map(|definition| definition.embedded_model_bytes)
        .sum()
}

fn merged_refs(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .chain(right)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn tag_overlap(left: &[String], right: &[String]) -> usize {
    left.iter().filter(|tag| right.contains(tag)).count()
}

fn grade_reason(grade: ObjectMicroAgentActivationGrade) -> &'static str {
    match grade {
        ObjectMicroAgentActivationGrade::Accepted => "accepted",
        ObjectMicroAgentActivationGrade::SignalTooWeak => "signal strength below threshold",
        ObjectMicroAgentActivationGrade::NoCandidate => "no object micro-agent matched signal",
        ObjectMicroAgentActivationGrade::WardBlocked => "signal exceeds object Ward envelope",
        ObjectMicroAgentActivationGrade::CooldownActive => "object micro-agent cooldown active",
        ObjectMicroAgentActivationGrade::TickBudgetExceeded => "activation budget exhausted",
        ObjectMicroAgentActivationGrade::ParticleBudgetExceeded => "particle budget exhausted",
    }
}

fn micro_agent_invalid(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::AiInvalidConfig {
        field: format!("object_micro_agent.{field}"),
        reason: reason.into(),
    }
}
