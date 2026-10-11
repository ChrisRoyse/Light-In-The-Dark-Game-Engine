use std::collections::BTreeSet;

use crate::{
    EngineError,
    sim_order::{SimOrder, SimOrderSourceKind},
};

use super::{
    npc_agents_hash::compute_npc_agent_activation_fnv1a64,
    npc_agents_types::{
        AI_NPC_AGENT_ACTIVATION_SCHEMA, AI_NPC_AGENT_HOT_PATH_POLICY, NpcAgentActivationDecision,
        NpcAgentActivationGrade, NpcAgentActivationInput, NpcAgentActivationReadback,
        NpcAgentDefinition, NpcAgentRules, NpcAgentRuntimeState, NpcWorldSignal,
    },
};

pub(super) fn validate_input(input: &NpcAgentActivationInput) -> Result<(), EngineError> {
    validate_id("npc_agents.runtime_id", &input.runtime_id)?;
    validate_id("npc_agents.catalog_id", &input.catalog_id)?;
    validate_rules(&input.rules)?;
    if input.npcs.is_empty() {
        return invalid_config("npc_agents.npcs", "must contain at least one NPC");
    }
    let mut ids = BTreeSet::new();
    let mut bodies = BTreeSet::new();
    let mut agents = BTreeSet::new();
    for npc in &input.npcs {
        validate_npc(npc)?;
        if !ids.insert(npc.npc_id.as_str()) {
            return invalid_config("npc_agents.npc_id", format!("duplicate {}", npc.npc_id));
        }
        if !bodies.insert(npc.body_id.index()) {
            return invalid_config("npc_agents.body_id", "NPC body ids must be unique");
        }
        if !agents.insert(npc.default_agent_id) {
            return invalid_config(
                "npc_agents.default_agent_id",
                "default agent ids must be unique",
            );
        }
    }
    validate_states(&ids, &input.states)?;
    for signal in &input.signals {
        validate_signal(signal, &ids)?;
    }
    Ok(())
}

pub fn verify_npc_agent_activation_integrity(
    readback: &NpcAgentActivationReadback,
) -> Result<(), EngineError> {
    if readback.schema != AI_NPC_AGENT_ACTIVATION_SCHEMA {
        return invalid_config("npc_agents.schema", "unsupported npc agent schema");
    }
    if readback.npc_count == 0 || readback.default_agent_count != readback.npc_count {
        return invalid_config(
            "npc_agents.default_agent_count",
            "all NPCs must have a default agent",
        );
    }
    if readback.signal_count != readback.decision_count
        || readback.decision_count != readback.decisions.len()
        || readback.accepted_count + readback.skipped_count != readback.decision_count
        || readback.order_count != readback.orders.len()
    {
        return invalid_config("npc_agents.counts", "readback counts must match vectors");
    }
    if readback.hot_path_policy != AI_NPC_AGENT_HOT_PATH_POLICY {
        return invalid_config("npc_agents.hot_path_policy", "must be lowered_orders_only");
    }
    if readback.calyx_live_query_allowed_in_hot_path {
        return invalid_config(
            "npc_agents.calyx_live_query_allowed_in_hot_path",
            "live Calyx queries are forbidden in deterministic sim ticks",
        );
    }
    let mut accepted_orders = Vec::new();
    for decision in &readback.decisions {
        verify_decision(decision, &mut accepted_orders)?;
    }
    if accepted_orders != readback.orders {
        return invalid_config(
            "npc_agents.orders",
            "accepted decision orders must match orders",
        );
    }
    let actual = compute_npc_agent_activation_fnv1a64(readback);
    if readback.fnv1a64 != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "npc_agents.fnv1a64".to_string(),
            expected: readback.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

fn validate_rules(rules: &NpcAgentRules) -> Result<(), EngineError> {
    if rules.max_activations_per_tick == 0 {
        return invalid_config(
            "npc_agents.rules.max_activations_per_tick",
            "must be non-zero",
        );
    }
    validate_milli(
        "npc_agents.rules.min_strength_milli",
        rules.min_strength_milli,
    )?;
    validate_milli(
        "npc_agents.rules.min_relevance_milli",
        rules.min_relevance_milli,
    )?;
    Ok(())
}

fn validate_npc(npc: &NpcAgentDefinition) -> Result<(), EngineError> {
    validate_id("npc_agents.npc_id", &npc.npc_id)?;
    validate_text("npc_agents.display_name", &npc.display_name)?;
    if npc.default_agent_id == 0 {
        return invalid_config("npc_agents.default_agent_id", "must be non-zero");
    }
    validate_id("npc_agents.prompt_profile_id", &npc.prompt_profile_id)?;
    validate_tag_list("npc_agents.ward_tags", &npc.ward_tags)?;
    validate_tag_list("npc_agents.activation_tags", &npc.activation_tags)?;
    if npc.allowed_intents.is_empty() {
        return invalid_config("npc_agents.allowed_intents", "must not be empty");
    }
    let mut intents = BTreeSet::new();
    for intent in &npc.allowed_intents {
        if !intents.insert(*intent) {
            return invalid_config("npc_agents.allowed_intents", "must be unique");
        }
    }
    Ok(())
}

fn validate_states(
    npc_ids: &BTreeSet<&str>,
    states: &[NpcAgentRuntimeState],
) -> Result<(), EngineError> {
    if states.len() != npc_ids.len() {
        return invalid_config("npc_agents.states", "must contain one state per NPC");
    }
    let mut seen = BTreeSet::new();
    for state in states {
        validate_id("npc_agents.state.npc_id", &state.npc_id)?;
        if !npc_ids.contains(state.npc_id.as_str()) {
            return invalid_config("npc_agents.state.npc_id", "state references unknown NPC");
        }
        if !seen.insert(state.npc_id.as_str()) {
            return invalid_config("npc_agents.state.npc_id", "duplicate runtime state");
        }
    }
    Ok(())
}

fn validate_signal(signal: &NpcWorldSignal, npc_ids: &BTreeSet<&str>) -> Result<(), EngineError> {
    validate_id("npc_agents.signal_id", &signal.signal_id)?;
    validate_id("npc_agents.world_id", &signal.world_id)?;
    validate_milli("npc_agents.signal.strength_milli", signal.strength_milli)?;
    validate_milli("npc_agents.signal.relevance_milli", signal.relevance_milli)?;
    validate_tag_list("npc_agents.signal.tags", &signal.tags)?;
    validate_text("npc_agents.signal.summary", &signal.summary)?;
    validate_ref_list("npc_agents.signal.provenance_refs", &signal.provenance_refs)?;
    if let Some(target_npc_id) = &signal.target_npc_id {
        validate_id("npc_agents.signal.target_npc_id", target_npc_id)?;
        if !npc_ids.contains(target_npc_id.as_str()) {
            return invalid_config(
                "npc_agents.signal.target_npc_id",
                "target NPC must exist in catalog",
            );
        }
    }
    Ok(())
}

fn verify_decision(
    decision: &NpcAgentActivationDecision,
    accepted_orders: &mut Vec<SimOrder>,
) -> Result<(), EngineError> {
    if decision.accepted {
        if decision.grade != NpcAgentActivationGrade::Accepted
            || decision.prompt_request_id.is_none()
            || decision.prompt.is_none()
            || decision.execute_tick.is_none()
            || decision.lowered_order.is_none()
            || decision.order_fingerprint.is_none()
            || decision.default_agent_id.is_none()
        {
            return invalid_config(
                "npc_agents.decision",
                "accepted decisions need prompt and order",
            );
        }
        let order = decision.lowered_order.expect("checked above");
        if order.source.kind != SimOrderSourceKind::LoweredAgent {
            return invalid_config("npc_agents.order.source", "must be lowered_agent");
        }
        if Some(order.fingerprint()) != decision.order_fingerprint {
            return invalid_config("npc_agents.order_fingerprint", "fingerprint mismatch");
        }
        accepted_orders.push(order);
    } else if decision.prompt_request_id.is_some()
        || decision.prompt.is_some()
        || decision.lowered_order.is_some()
        || decision.order_fingerprint.is_some()
    {
        return invalid_config(
            "npc_agents.decision",
            "skipped decisions must not emit effects",
        );
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    validate_text(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return invalid_config(field, "must contain only stable ASCII id characters");
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_config(field, "must be non-empty");
    }
    Ok(())
}

fn validate_tag_list(field: &str, tags: &[String]) -> Result<(), EngineError> {
    if tags.is_empty() {
        return invalid_config(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for tag in tags {
        validate_id(field, tag)?;
        if !seen.insert(tag.as_str()) {
            return invalid_config(field, "must be unique");
        }
    }
    Ok(())
}

fn validate_ref_list(field: &str, refs: &[String]) -> Result<(), EngineError> {
    if refs.is_empty() {
        return invalid_config(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for reference in refs {
        validate_text(field, reference)?;
        if !reference.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'#')
        }) {
            return invalid_config(field, "must contain only stable ASCII reference characters");
        }
        if !seen.insert(reference.as_str()) {
            return invalid_config(field, "must be unique");
        }
    }
    Ok(())
}

fn validate_milli(field: &str, value: u16) -> Result<(), EngineError> {
    if value == 0 || value > 1000 {
        return invalid_config(field, "must be within 1..=1000");
    }
    Ok(())
}

pub(super) fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_config_error(field, reason))
}

pub(super) fn invalid_config_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::AiInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
