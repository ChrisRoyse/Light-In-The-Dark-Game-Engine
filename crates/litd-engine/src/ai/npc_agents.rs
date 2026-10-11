use std::collections::{BTreeMap, BTreeSet};

use crate::{
    EngineError,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

use super::{
    npc_agents_hash::activation_id,
    npc_agents_types::{
        AI_NPC_AGENT_ACTIVATION_SCHEMA, AI_NPC_AGENT_HOT_PATH_POLICY, NpcAgentActivationDecision,
        NpcAgentActivationGrade, NpcAgentActivationInput, NpcAgentActivationReadback,
        NpcAgentDefinition, NpcAgentIntent, NpcAgentRuntimeState, NpcWorldSignal,
    },
    npc_agents_validate::{invalid_config_error, validate_input},
};

pub fn evaluate_npc_agent_activations(
    input: &NpcAgentActivationInput,
) -> Result<NpcAgentActivationReadback, EngineError> {
    validate_input(input)?;
    let npcs = npc_index(&input.npcs);
    let mut states = input
        .states
        .iter()
        .map(|state| (state.npc_id.as_str(), state.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut signals = input.signals.clone();
    signals.sort_by(|left, right| {
        (left.tick, left.signal_id.as_str()).cmp(&(right.tick, right.signal_id.as_str()))
    });

    let mut accepted_count = 0_usize;
    let mut decisions = Vec::with_capacity(signals.len());
    let mut orders = Vec::new();
    for signal in &signals {
        let decision = decision_for_signal(
            input,
            &npcs,
            &mut states,
            signal,
            accepted_count,
            orders.len() as u64,
        )?;
        if let Some(order) = decision.lowered_order {
            orders.push(order);
        }
        if decision.accepted {
            accepted_count += 1;
        }
        decisions.push(decision);
    }

    let prompt_count = decisions
        .iter()
        .filter(|decision| decision.prompt_request_id.is_some())
        .count();
    let spawn_request_count = decisions
        .iter()
        .filter(|decision| decision.spawn_requested)
        .count();
    let mut readback = NpcAgentActivationReadback {
        schema: AI_NPC_AGENT_ACTIVATION_SCHEMA.to_string(),
        runtime_id: input.runtime_id.clone(),
        catalog_id: input.catalog_id.clone(),
        npc_count: input.npcs.len(),
        default_agent_count: input
            .npcs
            .iter()
            .filter(|npc| npc.default_agent_id != 0)
            .count(),
        signal_count: input.signals.len(),
        decision_count: decisions.len(),
        accepted_count,
        skipped_count: decisions.len().saturating_sub(accepted_count),
        prompt_count,
        order_count: orders.len(),
        spawn_request_count,
        hot_path_policy: AI_NPC_AGENT_HOT_PATH_POLICY.to_string(),
        calyx_live_query_allowed_in_hot_path: false,
        lowering_boundary:
            "Calyx selects and prompts NPC agents out-of-band; sim consumes deterministic lowered orders"
                .to_string(),
        decisions,
        orders,
        fnv1a64: String::new(),
    };
    readback.fnv1a64 = compute_npc_agent_activation_fnv1a64(&readback);
    verify_npc_agent_activation_integrity(&readback)?;
    Ok(readback)
}

pub use super::npc_agents_hash::compute_npc_agent_activation_fnv1a64;
pub use super::npc_agents_validate::verify_npc_agent_activation_integrity;

fn decision_for_signal(
    input: &NpcAgentActivationInput,
    npcs: &BTreeMap<&str, &NpcAgentDefinition>,
    states: &mut BTreeMap<&str, NpcAgentRuntimeState>,
    signal: &NpcWorldSignal,
    accepted_count: usize,
    order_sequence: u64,
) -> Result<NpcAgentActivationDecision, EngineError> {
    let intent = signal.kind.intent();
    if signal.strength_milli < input.rules.min_strength_milli {
        return Ok(skipped(
            input,
            signal,
            intent,
            NpcAgentActivationGrade::SignalTooWeak,
        ));
    }
    if signal.relevance_milli < input.rules.min_relevance_milli {
        return Ok(skipped(
            input,
            signal,
            intent,
            NpcAgentActivationGrade::LowRelevance,
        ));
    }
    let Some((npc, relevance_score)) = select_candidate(signal, intent, npcs) else {
        return Ok(skipped(
            input,
            signal,
            intent,
            NpcAgentActivationGrade::NoCandidate,
        ));
    };
    if !ward_allows(npc, signal) {
        return Ok(skipped_for_npc(
            input,
            signal,
            npc,
            intent,
            relevance_score,
            NpcAgentActivationGrade::WardBlocked,
        ));
    }
    if !npc.allowed_intents.contains(&intent) {
        return Ok(skipped_for_npc(
            input,
            signal,
            npc,
            intent,
            relevance_score,
            NpcAgentActivationGrade::IntentNotAllowed,
        ));
    }
    let state = states
        .get_mut(npc.npc_id.as_str())
        .ok_or_else(|| invalid_config_error("npc_agents.states", "missing runtime state"))?;
    if let Some(last_tick) = state.last_activation_tick
        && signal.tick.saturating_sub(last_tick) < input.rules.cooldown_ticks
    {
        return Ok(skipped_for_npc(
            input,
            signal,
            npc,
            intent,
            relevance_score,
            NpcAgentActivationGrade::CooldownActive,
        ));
    }
    if accepted_count >= input.rules.max_activations_per_tick {
        return Ok(skipped_for_npc(
            input,
            signal,
            npc,
            intent,
            relevance_score,
            NpcAgentActivationGrade::BudgetExceeded,
        ));
    }

    let execute_tick = signal
        .tick
        .checked_add(1)
        .ok_or(EngineError::SimTickOverflow)?;
    let order = SimOrder::new(
        execute_tick,
        SimOrderSource::lowered_agent(npc.default_agent_id),
        order_sequence + 1,
        npc.body_id,
        SimOrderKind::SetVelocity(npc.order_velocity),
    );
    let prompt_request_id = format!(
        "prompt.{}.{}.{}",
        input.runtime_id, npc.npc_id, signal.signal_id
    );
    let prompt = format!(
        "NPC {} ({}) intent={} signal={} world={} tags={} summary={}",
        npc.display_name,
        npc.npc_id,
        intent.as_str(),
        signal.signal_id,
        signal.world_id,
        signal.tags.join(","),
        signal.summary
    );
    let spawn_requested = !state.spawned;
    state.last_activation_tick = Some(signal.tick);
    Ok(NpcAgentActivationDecision {
        activation_id: activation_id(&input.runtime_id, &npc.npc_id, signal, intent),
        signal_id: signal.signal_id.clone(),
        npc_id: Some(npc.npc_id.clone()),
        default_agent_id: Some(npc.default_agent_id),
        prompt_profile_id: Some(npc.prompt_profile_id.clone()),
        intent,
        accepted: true,
        grade: NpcAgentActivationGrade::Accepted,
        reason: "accepted for out-of-band prompt and deterministic lowering".to_string(),
        relevance_score,
        spawn_requested,
        prompt_request_id: Some(prompt_request_id),
        prompt: Some(prompt),
        execute_tick: Some(execute_tick),
        order_fingerprint: Some(order.fingerprint()),
        lowered_order: Some(order),
        provenance_refs: merged_refs(
            &[format!("npc://{}/{}", input.catalog_id, npc.npc_id)],
            &signal.provenance_refs,
        ),
    })
}

fn select_candidate<'a>(
    signal: &NpcWorldSignal,
    intent: NpcAgentIntent,
    npcs: &BTreeMap<&'a str, &'a NpcAgentDefinition>,
) -> Option<(&'a NpcAgentDefinition, u16)> {
    if let Some(target_npc_id) = signal.target_npc_id.as_deref() {
        return npcs
            .get(target_npc_id)
            .map(|npc| (*npc, relevance_score(npc, signal)));
    }
    npcs.values()
        .filter(|npc| npc.allowed_intents.contains(&intent))
        .filter(|npc| tag_overlap(&npc.activation_tags, &signal.tags) > 0)
        .filter(|npc| ward_allows(npc, signal))
        .map(|npc| (*npc, relevance_score(npc, signal)))
        .max_by(|(left_npc, left_score), (right_npc, right_score)| {
            left_score
                .cmp(right_score)
                .then_with(|| right_npc.npc_id.cmp(&left_npc.npc_id))
        })
}

fn skipped(
    input: &NpcAgentActivationInput,
    signal: &NpcWorldSignal,
    intent: NpcAgentIntent,
    grade: NpcAgentActivationGrade,
) -> NpcAgentActivationDecision {
    NpcAgentActivationDecision {
        activation_id: activation_id(&input.runtime_id, "none", signal, intent),
        signal_id: signal.signal_id.clone(),
        npc_id: None,
        default_agent_id: None,
        prompt_profile_id: None,
        intent,
        accepted: false,
        grade,
        reason: grade_reason(grade).to_string(),
        relevance_score: 0,
        spawn_requested: false,
        prompt_request_id: None,
        prompt: None,
        execute_tick: None,
        lowered_order: None,
        order_fingerprint: None,
        provenance_refs: signal.provenance_refs.clone(),
    }
}

fn skipped_for_npc(
    input: &NpcAgentActivationInput,
    signal: &NpcWorldSignal,
    npc: &NpcAgentDefinition,
    intent: NpcAgentIntent,
    relevance_score: u16,
    grade: NpcAgentActivationGrade,
) -> NpcAgentActivationDecision {
    NpcAgentActivationDecision {
        activation_id: activation_id(&input.runtime_id, &npc.npc_id, signal, intent),
        signal_id: signal.signal_id.clone(),
        npc_id: Some(npc.npc_id.clone()),
        default_agent_id: Some(npc.default_agent_id),
        prompt_profile_id: Some(npc.prompt_profile_id.clone()),
        intent,
        accepted: false,
        grade,
        reason: grade_reason(grade).to_string(),
        relevance_score,
        spawn_requested: false,
        prompt_request_id: None,
        prompt: None,
        execute_tick: None,
        lowered_order: None,
        order_fingerprint: None,
        provenance_refs: merged_refs(
            &[format!("npc://{}/{}", input.catalog_id, npc.npc_id)],
            &signal.provenance_refs,
        ),
    }
}

fn npc_index(npcs: &[NpcAgentDefinition]) -> BTreeMap<&str, &NpcAgentDefinition> {
    npcs.iter().map(|npc| (npc.npc_id.as_str(), npc)).collect()
}

fn ward_allows(npc: &NpcAgentDefinition, signal: &NpcWorldSignal) -> bool {
    signal.tags.iter().all(|tag| npc.ward_tags.contains(tag))
}

fn relevance_score(npc: &NpcAgentDefinition, signal: &NpcWorldSignal) -> u16 {
    let overlap = tag_overlap(&npc.activation_tags, &signal.tags) as u16;
    signal
        .relevance_milli
        .saturating_add(overlap.saturating_mul(25))
        .min(1000)
}

fn tag_overlap(left: &[String], right: &[String]) -> usize {
    left.iter().filter(|tag| right.contains(tag)).count()
}

fn grade_reason(grade: NpcAgentActivationGrade) -> &'static str {
    match grade {
        NpcAgentActivationGrade::Accepted => "accepted",
        NpcAgentActivationGrade::SignalTooWeak => "signal strength below activation threshold",
        NpcAgentActivationGrade::LowRelevance => "signal relevance below activation threshold",
        NpcAgentActivationGrade::NoCandidate => "no NPC matched the signal tags",
        NpcAgentActivationGrade::WardBlocked => "signal tags exceed NPC Ward envelope",
        NpcAgentActivationGrade::IntentNotAllowed => "NPC default agent does not allow this intent",
        NpcAgentActivationGrade::CooldownActive => "NPC prompt cooldown is still active",
        NpcAgentActivationGrade::BudgetExceeded => "activation budget exhausted for tick",
    }
}

fn merged_refs(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .chain(right)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
