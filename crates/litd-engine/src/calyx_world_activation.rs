//! Calyx world-state association and agent activation brain.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

pub use crate::calyx_world_activation_types::*;
use crate::{
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxRecord},
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

use super::{
    calyx_persistence::EngineCalyxRecordKind,
    calyx_world_activation_hash::{activation_fingerprint, association_id, decision_id},
    calyx_world_activation_validate::{codec_failed, invalid, validate_input},
};

pub use super::calyx_world_activation_validate::validate_world_activation_readback;

pub fn evaluate_calyx_world_activation(
    input: &EngineWorldActivationInput,
) -> Result<EngineWorldActivationReadback, EngineWorldActivationError> {
    validate_input(input)?;
    let mut facts = input.facts.iter().collect::<Vec<_>>();
    facts.sort_by(|left, right| {
        (left.tick, left.fact_id.as_str()).cmp(&(right.tick, right.fact_id.as_str()))
    });
    let mut agents = input.agents.iter().collect::<Vec<_>>();
    agents.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    let mut states = input
        .states
        .iter()
        .map(|state| (state.agent_id.as_str(), state.clone()))
        .collect::<BTreeMap<_, _>>();

    let associations = agents
        .iter()
        .filter_map(|agent| association_for_agent(&input.run_id, agent, &facts))
        .collect::<Vec<_>>();
    let association_index = associations
        .iter()
        .map(|association| (association.agent_id.as_str(), association.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut decisions = Vec::with_capacity(agents.len());
    let mut accepted_count = 0_usize;
    let mut orders = Vec::new();
    for agent in agents {
        let decision = decision_for_agent(
            input,
            agent,
            association_index.get(agent.agent_id.as_str()).cloned(),
            &mut states,
            accepted_count,
            orders.len() as u64,
        )?;
        if decision.accepted {
            accepted_count += 1;
        }
        if let Some(order) = decision.lowered_order {
            orders.push(order);
        }
        decisions.push(decision);
    }
    decisions.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    let prompt_count = decisions
        .iter()
        .filter(|decision| decision.prompt_request_id.is_some())
        .count();
    let spawn_request_count = decisions
        .iter()
        .filter(|decision| decision.spawn_requested)
        .count();
    let mut readback = EngineWorldActivationReadback {
        schema: ENGINE_CALYX_WORLD_ACTIVATION_SCHEMA.to_string(),
        run_id: input.run_id.clone(),
        world_id: input.world_id.clone(),
        policy: input.policy.clone(),
        fact_count: input.facts.len(),
        fact_kind_count: fact_kind_count(&input.facts),
        agent_count: input.agents.len(),
        association_count: associations.len(),
        decision_count: decisions.len(),
        accepted_count,
        skipped_count: decisions.len().saturating_sub(accepted_count),
        prompt_count,
        order_count: orders.len(),
        spawn_request_count,
        hot_path_policy: ENGINE_CALYX_WORLD_ACTIVATION_HOT_PATH_POLICY.to_string(),
        calyx_live_query_allowed_in_hot_path: false,
        lowering_boundary:
            "Calyx ingests world facts and associates agents out-of-band; sim consumes deterministic lowered orders"
                .to_string(),
        associations,
        decisions,
        orders,
        activation_fingerprint: String::new(),
    };
    readback.activation_fingerprint = activation_fingerprint(&readback).map_err(codec_failed)?;
    validate_world_activation_readback(&readback)?;
    Ok(readback)
}

pub fn store_calyx_world_activation(
    database: &EngineCalyxDatabase,
    activation: &EngineWorldActivationReadback,
) -> Result<EngineWorldActivationStoredReadback, EngineWorldActivationError> {
    validate_world_activation_readback(activation)?;
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::WorldActivationLedger,
        activation.run_id.clone(),
        serde_json::to_value(activation).map_err(codec_failed)?,
        json!({
            "source_issue": 968,
            "activation_fingerprint": activation.activation_fingerprint,
            "accepted_count": activation.accepted_count,
            "fact_count": activation.fact_count,
            "agent_count": activation.agent_count
        }),
    );
    parse_stored_activation(database.put_record(&record)?)
}

pub fn read_calyx_world_activation(
    database: &EngineCalyxDatabase,
    run_id: &str,
) -> Result<EngineWorldActivationStoredReadback, EngineWorldActivationError> {
    if run_id.trim().is_empty() {
        return invalid("run_id", "run id must be non-empty");
    }
    parse_stored_activation(
        database.get_record(EngineCalyxRecordKind::WorldActivationLedger, run_id)?,
    )
}

fn association_for_agent(
    run_id: &str,
    agent: &EngineWorldAgentProfile,
    facts: &[&EngineWorldFact],
) -> Option<EngineWorldAssociation> {
    let matched = facts
        .iter()
        .copied()
        .filter(|fact| fact_applies_to_agent(fact, agent))
        .filter(|fact| tag_overlap(&agent.trigger_tags, &fact.tags) > 0)
        .collect::<Vec<_>>();
    if matched.is_empty() {
        return None;
    }
    let mut matched_fact_ids = matched
        .iter()
        .map(|fact| fact.fact_id.clone())
        .collect::<Vec<_>>();
    matched_fact_ids.sort();
    let mut matched_tags = matched
        .iter()
        .flat_map(|fact| intersection(&agent.trigger_tags, &fact.tags))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    matched_tags.sort();
    let mut all_fact_tags = matched
        .iter()
        .flat_map(|fact| fact.tags.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    all_fact_tags.sort();
    let support_milli = matched
        .iter()
        // Grounding honesty (#1348/#1457): a measured strength is authoritative
        // when present, so weak measurement can *reduce* support below the
        // authored weight. The prior `.max(weight_milli)` floored support at the
        // author's intent, letting an agent activate on evidence the measurement
        // calls weak — a silent fail-open against the measurement (#1484-4).
        // Absent a measurement we fall back to the authored weight.
        .map(|fact| fact.measure_milli.unwrap_or(fact.weight_milli))
        .fold(0_u16, |acc, value| acc.saturating_add(value))
        .min(1000);
    let max_tick = matched.iter().map(|fact| fact.tick).max().unwrap_or(0);
    let evidence_summary = matched
        .iter()
        .map(|fact| format!("{}:{}", fact.kind.as_str(), fact.summary))
        .collect::<Vec<_>>()
        .join(" | ");
    let provenance_refs = matched
        .iter()
        .flat_map(|fact| fact.provenance_refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(EngineWorldAssociation {
        association_id: association_id(run_id, &agent.agent_id, &matched),
        agent_id: agent.agent_id.clone(),
        matched_fact_ids,
        matched_tags,
        all_fact_tags,
        support_milli,
        max_tick,
        evidence_summary,
        provenance_refs,
    })
}

fn decision_for_agent(
    input: &EngineWorldActivationInput,
    agent: &EngineWorldAgentProfile,
    association: Option<EngineWorldAssociation>,
    states: &mut BTreeMap<&str, EngineWorldAgentState>,
    accepted_count: usize,
    order_sequence: u64,
) -> Result<EngineWorldActivationDecision, EngineWorldActivationError> {
    let Some(association) = association else {
        return Ok(skipped(
            input,
            agent,
            None,
            EngineWorldActivationGrade::NoAssociation,
        ));
    };
    if association.support_milli < input.policy.min_support_milli {
        return Ok(skipped(
            input,
            agent,
            Some(association),
            EngineWorldActivationGrade::InsufficientSupport,
        ));
    }
    if !association
        .all_fact_tags
        .iter()
        .all(|tag| agent.ward_tags.contains(tag))
    {
        return Ok(skipped(
            input,
            agent,
            Some(association),
            EngineWorldActivationGrade::WardBlocked,
        ));
    }
    let state = states
        .get_mut(agent.agent_id.as_str())
        .expect("validated state exists");
    if let Some(last_tick) = state.last_activation_tick
        && association.max_tick.saturating_sub(last_tick) < input.policy.cooldown_ticks
    {
        return Ok(skipped(
            input,
            agent,
            Some(association),
            EngineWorldActivationGrade::CooldownActive,
        ));
    }
    if accepted_count >= input.policy.max_activations_per_tick {
        return Ok(skipped(
            input,
            agent,
            Some(association),
            EngineWorldActivationGrade::BudgetExceeded,
        ));
    }
    let execute_tick =
        association
            .max_tick
            .checked_add(1)
            .ok_or(EngineWorldActivationError::InvalidInput {
                field: "world_activation.association.max_tick".to_string(),
                reason: "execute tick overflow".to_string(),
            })?;
    let order = SimOrder::new(
        execute_tick,
        SimOrderSource::lowered_agent(agent.lowered_agent_id),
        order_sequence + 1,
        agent.target_body,
        SimOrderKind::SetVelocity(agent.order_velocity),
    );
    let prompt_request_id = format!(
        "prompt.{}.{}.{}",
        input.run_id, agent.agent_id, association.association_id
    );
    let prompt = format!(
        "Agent {} ({}) intent={} support={} tags={} evidence={}",
        agent.display_name,
        agent.agent_id,
        agent.intent.as_str(),
        association.support_milli,
        association.matched_tags.join(","),
        association.evidence_summary
    );
    let spawn_requested = !state.spawned;
    state.last_activation_tick = Some(association.max_tick);
    Ok(EngineWorldActivationDecision {
        decision_id: decision_id(&input.run_id, &agent.agent_id, agent.intent),
        agent_id: agent.agent_id.clone(),
        prompt_profile_id: agent.prompt_profile_id.clone(),
        intent: agent.intent,
        accepted: true,
        grade: EngineWorldActivationGrade::Accepted,
        reason: "accepted from grounded world-state association".to_string(),
        support_milli: association.support_milli,
        spawn_requested,
        association: Some(association),
        prompt_request_id: Some(prompt_request_id),
        prompt: Some(prompt),
        execute_tick: Some(execute_tick),
        order_fingerprint: Some(order.fingerprint()),
        lowered_order: Some(order),
    })
}

fn skipped(
    input: &EngineWorldActivationInput,
    agent: &EngineWorldAgentProfile,
    association: Option<EngineWorldAssociation>,
    grade: EngineWorldActivationGrade,
) -> EngineWorldActivationDecision {
    let support_milli = association
        .as_ref()
        .map(|association| association.support_milli)
        .unwrap_or_default();
    EngineWorldActivationDecision {
        decision_id: decision_id(&input.run_id, &agent.agent_id, agent.intent),
        agent_id: agent.agent_id.clone(),
        prompt_profile_id: agent.prompt_profile_id.clone(),
        intent: agent.intent,
        accepted: false,
        grade,
        reason: grade_reason(grade).to_string(),
        support_milli,
        spawn_requested: false,
        association,
        prompt_request_id: None,
        prompt: None,
        execute_tick: None,
        lowered_order: None,
        order_fingerprint: None,
    }
}

fn parse_stored_activation(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineWorldActivationStoredReadback, EngineWorldActivationError> {
    let activation =
        serde_json::from_value::<EngineWorldActivationReadback>(database.payload.clone())
            .map_err(codec_failed)?;
    validate_world_activation_readback(&activation)?;
    Ok(EngineWorldActivationStoredReadback {
        schema: ENGINE_CALYX_WORLD_ACTIVATION_READBACK_SCHEMA.to_string(),
        database,
        activation,
    })
}

fn fact_kind_count(facts: &[EngineWorldFact]) -> usize {
    facts
        .iter()
        .map(|fact| fact.kind)
        .collect::<BTreeSet<_>>()
        .len()
}

fn fact_applies_to_agent(fact: &EngineWorldFact, agent: &EngineWorldAgentProfile) -> bool {
    fact.target_agent_id
        .as_deref()
        .map(|target| target == agent.agent_id)
        .unwrap_or(true)
}

fn tag_overlap(left: &[String], right: &[String]) -> usize {
    left.iter().filter(|tag| right.contains(tag)).count()
}

fn intersection(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|tag| right.contains(tag))
        .cloned()
        .collect()
}

fn grade_reason(grade: EngineWorldActivationGrade) -> &'static str {
    match grade {
        EngineWorldActivationGrade::Accepted => "accepted",
        EngineWorldActivationGrade::NoAssociation => "no grounded association matched agent",
        EngineWorldActivationGrade::InsufficientSupport => {
            "association support below activation threshold"
        }
        EngineWorldActivationGrade::WardBlocked => "association exceeds agent Ward envelope",
        EngineWorldActivationGrade::CooldownActive => "agent cooldown is still active",
        EngineWorldActivationGrade::BudgetExceeded => "activation budget exhausted",
    }
}
