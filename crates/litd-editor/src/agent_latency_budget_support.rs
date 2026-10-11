use std::time::Instant;

use litd_engine::EngineError;

use super::{
    AgentLatencyBudgetRequest, AgentLatencyCacheMode, AgentLatencyStageMeasurement,
    AgentLatencyTierKind, AgentLatencyTierMeasurement, AgentLatencyTierPlan, MAX_SIM_BODIES,
    MAX_SIM_TICKS, MAX_TIER_COUNT,
};

pub(super) fn validate_request(request: &AgentLatencyBudgetRequest) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.project_id", &request.project_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_single_line("request.scenario_id", &request.scenario_id)?;
    if request.tiers.is_empty() || request.tiers.len() > MAX_TIER_COUNT {
        return invalid_latency_budget("request.tiers", "tier count must be 1..=32");
    }
    for tier in &request.tiers {
        validate_tier_plan(tier)?;
    }
    Ok(())
}

pub(super) fn validate_tier_plan(tier: &AgentLatencyTierPlan) -> Result<(), EngineError> {
    validate_single_line("tier.tier_id", &tier.tier_id)?;
    if !tier.budget_ms.is_finite()
        || tier.budget_ms <= 0.0
        || tier.budget_ms > tier.kind.target_budget_ms()
    {
        return invalid_latency_budget(
            "tier.budget_ms",
            format!(
                "budget must be within 0..={:.0}ms for {:?}",
                tier.kind.target_budget_ms(),
                tier.kind
            ),
        );
    }
    if tier.sim_ticks == 0 || tier.sim_ticks > MAX_SIM_TICKS {
        return invalid_latency_budget("tier.sim_ticks", "sim ticks must be 1..=120000");
    }
    if tier.body_count == 0 || tier.body_count > MAX_SIM_BODIES {
        return invalid_latency_budget("tier.body_count", "body count must be 1..=1024");
    }
    Ok(())
}

pub(super) fn validate_measurement(
    measurement: &AgentLatencyTierMeasurement,
    tier: &AgentLatencyTierPlan,
) -> Result<(), EngineError> {
    if measurement.tier_id != tier.tier_id
        || measurement.kind != tier.kind
        || measurement.cache_mode != tier.cache_mode
        || measurement.budget_ms != tier.budget_ms
        || !measurement.elapsed_ms.is_finite()
        || measurement.elapsed_ms < 0.0
        || measurement.within_budget != (measurement.elapsed_ms <= measurement.budget_ms)
        || measurement.document_fingerprint.trim().is_empty()
        || measurement.play_build_fingerprint.trim().is_empty()
        || measurement.validation_error_count != 0
        || measurement.sim_ticks != tier.sim_ticks
        || measurement.body_count != tier.body_count
        || measurement.stages.is_empty()
    {
        return invalid_latency_budget("measurement", "tier measurement is invalid");
    }
    if measurement.cache_mode == AgentLatencyCacheMode::Warm
        && measurement.warmup_elapsed_ms.is_none()
    {
        return invalid_latency_budget(
            "measurement.warmup",
            "warm cache tiers require warmup timing",
        );
    }
    if measurement.cache_mode == AgentLatencyCacheMode::Cold
        && measurement.warmup_elapsed_ms.is_some()
    {
        return invalid_latency_budget(
            "measurement.warmup",
            "cold cache tiers must not carry warmup timing",
        );
    }
    Ok(())
}

pub(super) fn timed<T>(
    stage_name: &str,
    operation: impl FnOnce() -> Result<T, EngineError>,
) -> Result<(T, AgentLatencyStageMeasurement), EngineError> {
    let start = Instant::now();
    let value = operation()?;
    Ok((
        value,
        AgentLatencyStageMeasurement {
            stage_name: stage_name.to_string(),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        },
    ))
}

pub(super) fn push_stage(
    stages: &mut Vec<AgentLatencyStageMeasurement>,
    stage_name: &str,
    operation: impl FnOnce() -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    let (_, stage) = timed(stage_name, operation)?;
    stages.push(stage);
    Ok(())
}

pub(super) fn sim_seed(tier: &AgentLatencyTierPlan) -> u64 {
    (tier.sim_ticks ^ (tier.body_count as u64)).wrapping_add(match tier.kind {
        AgentLatencyTierKind::SingleChangeRevalidate => 0x1179_0001,
        AgentLatencyTierKind::FocusedScenario => 0x1179_0002,
        AgentLatencyTierKind::FullWorldSmoke => 0x1179_0003,
    })
}

pub(super) fn invalid_latency_budget<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(latency_error(field, reason.into()))
}

pub(super) fn latency_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.agent_latency_budget.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-AGENT-LATENCY-BUDGET-INVALID",
        "agent latency budget rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_latency_budget(field, "value must be non-empty and single-line");
    }
    Ok(())
}
