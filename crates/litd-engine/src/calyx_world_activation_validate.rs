use std::collections::BTreeSet;

use crate::sim_order::{SimOrder, SimOrderSourceKind};

use super::{
    calyx_world_activation_hash::activation_fingerprint,
    calyx_world_activation_types::{
        ENGINE_CALYX_WORLD_ACTIVATION_HOT_PATH_POLICY, ENGINE_CALYX_WORLD_ACTIVATION_SCHEMA,
        EngineWorldActivationDecision, EngineWorldActivationError, EngineWorldActivationInput,
        EngineWorldActivationPolicy, EngineWorldActivationReadback, EngineWorldAgentProfile,
        EngineWorldAgentState, EngineWorldFact,
    },
};

pub(super) fn validate_input(
    input: &EngineWorldActivationInput,
) -> Result<(), EngineWorldActivationError> {
    validate_id("world_activation.run_id", &input.run_id)?;
    validate_id("world_activation.world_id", &input.world_id)?;
    validate_policy(&input.policy)?;
    if input.agents.is_empty() {
        return invalid("world_activation.agents", "must contain at least one agent");
    }
    if input.facts.is_empty() {
        return invalid("world_activation.facts", "must contain at least one fact");
    }
    let mut agent_ids = BTreeSet::new();
    let mut lowered_ids = BTreeSet::new();
    for agent in &input.agents {
        validate_agent(agent)?;
        if !agent_ids.insert(agent.agent_id.as_str()) {
            return invalid(
                "world_activation.agent_id",
                format!("duplicate {}", agent.agent_id),
            );
        }
        if !lowered_ids.insert(agent.lowered_agent_id) {
            return invalid(
                "world_activation.lowered_agent_id",
                "lowered agent ids must be unique",
            );
        }
    }
    validate_states(&agent_ids, &input.states)?;
    validate_facts(&input.world_id, &agent_ids, &input.facts)?;
    Ok(())
}

pub fn validate_world_activation_readback(
    readback: &EngineWorldActivationReadback,
) -> Result<(), EngineWorldActivationError> {
    if readback.schema != ENGINE_CALYX_WORLD_ACTIVATION_SCHEMA {
        return invalid(
            "world_activation.schema",
            "unsupported world activation schema",
        );
    }
    if readback.agent_count == 0 || readback.fact_count == 0 || readback.fact_kind_count == 0 {
        return invalid(
            "world_activation.counts",
            "fact and agent counts must be non-zero",
        );
    }
    if readback.decision_count != readback.decisions.len()
        || readback.association_count != readback.associations.len()
        || readback.accepted_count + readback.skipped_count != readback.decision_count
        || readback.order_count != readback.orders.len()
    {
        return invalid(
            "world_activation.counts",
            "counts must match readback vectors",
        );
    }
    if readback.hot_path_policy != ENGINE_CALYX_WORLD_ACTIVATION_HOT_PATH_POLICY {
        return invalid(
            "world_activation.hot_path_policy",
            "must be lowered_orders_only",
        );
    }
    if readback.calyx_live_query_allowed_in_hot_path {
        return invalid(
            "world_activation.calyx_live_query_allowed_in_hot_path",
            "live Calyx queries are forbidden in deterministic sim ticks",
        );
    }
    let mut accepted_orders = Vec::new();
    for decision in &readback.decisions {
        validate_decision(decision, &mut accepted_orders)?;
    }
    if accepted_orders != readback.orders {
        return invalid(
            "world_activation.orders",
            "accepted decision orders must match orders",
        );
    }
    let actual = activation_fingerprint(readback).map_err(codec_failed)?;
    if readback.activation_fingerprint != actual {
        return Err(EngineWorldActivationError::ReadbackMismatch {
            field: "world_activation.activation_fingerprint".to_string(),
            expected: readback.activation_fingerprint.clone(),
            actual,
        });
    }
    Ok(())
}

fn validate_policy(policy: &EngineWorldActivationPolicy) -> Result<(), EngineWorldActivationError> {
    validate_id("world_activation.policy_id", &policy.policy_id)?;
    if policy.max_activations_per_tick == 0 {
        return invalid(
            "world_activation.max_activations_per_tick",
            "must be non-zero",
        );
    }
    validate_milli(
        "world_activation.min_support_milli",
        policy.min_support_milli,
    )
}

fn validate_agent(agent: &EngineWorldAgentProfile) -> Result<(), EngineWorldActivationError> {
    validate_id("world_activation.agent_id", &agent.agent_id)?;
    validate_text("world_activation.display_name", &agent.display_name)?;
    validate_id(
        "world_activation.prompt_profile_id",
        &agent.prompt_profile_id,
    )?;
    if agent.lowered_agent_id == 0 {
        return invalid("world_activation.lowered_agent_id", "must be non-zero");
    }
    validate_tag_list("world_activation.trigger_tags", &agent.trigger_tags)?;
    validate_tag_list("world_activation.ward_tags", &agent.ward_tags)?;
    Ok(())
}

fn validate_states(
    agent_ids: &BTreeSet<&str>,
    states: &[EngineWorldAgentState],
) -> Result<(), EngineWorldActivationError> {
    if states.len() != agent_ids.len() {
        return invalid(
            "world_activation.states",
            "must contain one state per agent",
        );
    }
    let mut seen = BTreeSet::new();
    for state in states {
        validate_id("world_activation.state.agent_id", &state.agent_id)?;
        if !agent_ids.contains(state.agent_id.as_str()) {
            return invalid(
                "world_activation.state.agent_id",
                "state references unknown agent",
            );
        }
        if !seen.insert(state.agent_id.as_str()) {
            return invalid("world_activation.state.agent_id", "duplicate state");
        }
    }
    Ok(())
}

fn validate_facts(
    world_id: &str,
    agent_ids: &BTreeSet<&str>,
    facts: &[EngineWorldFact],
) -> Result<(), EngineWorldActivationError> {
    let mut seen = BTreeSet::new();
    for fact in facts {
        validate_id("world_activation.fact_id", &fact.fact_id)?;
        if !seen.insert(fact.fact_id.as_str()) {
            return invalid(
                "world_activation.fact_id",
                format!("duplicate {}", fact.fact_id),
            );
        }
        if fact.world_id != world_id {
            return invalid(
                "world_activation.fact.world_id",
                "must match input world_id",
            );
        }
        validate_tag_list("world_activation.fact.tags", &fact.tags)?;
        validate_text("world_activation.fact.summary", &fact.summary)?;
        validate_milli("world_activation.fact.weight_milli", fact.weight_milli)?;
        if let Some(measure) = fact.measure_milli {
            validate_milli("world_activation.fact.measure_milli", measure)?;
        }
        validate_ref_list(
            "world_activation.fact.provenance_refs",
            &fact.provenance_refs,
        )?;
        if let Some(agent_id) = &fact.target_agent_id
            && !agent_ids.contains(agent_id.as_str())
        {
            return invalid(
                "world_activation.fact.target_agent_id",
                "target agent must exist",
            );
        }
    }
    Ok(())
}

fn validate_decision(
    decision: &EngineWorldActivationDecision,
    accepted_orders: &mut Vec<SimOrder>,
) -> Result<(), EngineWorldActivationError> {
    validate_id("world_activation.decision.agent_id", &decision.agent_id)?;
    if decision.accepted {
        let Some(order) = decision.lowered_order else {
            return invalid(
                "world_activation.decision",
                "accepted decisions need an order",
            );
        };
        if order.source.kind != SimOrderSourceKind::LoweredAgent {
            return invalid("world_activation.order.source", "must be lowered_agent");
        }
        if Some(order.fingerprint()) != decision.order_fingerprint {
            return invalid("world_activation.order_fingerprint", "fingerprint mismatch");
        }
        if decision.prompt.is_none() || decision.prompt_request_id.is_none() {
            return invalid(
                "world_activation.decision",
                "accepted decisions need prompt",
            );
        }
        accepted_orders.push(order);
    } else if decision.lowered_order.is_some()
        || decision.order_fingerprint.is_some()
        || decision.prompt.is_some()
    {
        return invalid(
            "world_activation.decision",
            "skipped decisions must not emit effects",
        );
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineWorldActivationError> {
    validate_text(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return invalid(field, "must contain only stable ASCII id characters");
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineWorldActivationError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    Ok(())
}

fn validate_tag_list(field: &str, tags: &[String]) -> Result<(), EngineWorldActivationError> {
    if tags.is_empty() {
        return invalid(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for tag in tags {
        validate_id(field, tag)?;
        if !seen.insert(tag.as_str()) {
            return invalid(field, "must be unique");
        }
    }
    Ok(())
}

fn validate_ref_list(field: &str, refs: &[String]) -> Result<(), EngineWorldActivationError> {
    if refs.is_empty() {
        return invalid(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for reference in refs {
        validate_text(field, reference)?;
        if !reference.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'#')
        }) {
            return invalid(field, "must contain only stable ASCII reference characters");
        }
        if !seen.insert(reference.as_str()) {
            return invalid(field, "must be unique");
        }
    }
    Ok(())
}

fn validate_milli(field: &str, value: u16) -> Result<(), EngineWorldActivationError> {
    if value == 0 || value > 1000 {
        return invalid(field, "must be within 1..=1000");
    }
    Ok(())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineWorldActivationError> {
    Err(EngineWorldActivationError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    })
}

pub(super) fn codec_failed(error: impl std::fmt::Display) -> EngineWorldActivationError {
    EngineWorldActivationError::Codec(error.to_string())
}
