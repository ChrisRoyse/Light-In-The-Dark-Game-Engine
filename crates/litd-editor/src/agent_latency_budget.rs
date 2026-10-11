use std::time::Instant;

use litd_engine::{
    EngineError,
    math::FixedVec3,
    sim::{FixedTickSim, SimConfig},
};
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
    world_validation::{WorldValidationUseCase, require_editor_world_valid},
};

#[path = "agent_latency_budget_store.rs"]
mod agent_latency_budget_store;
#[path = "agent_latency_budget_support.rs"]
mod agent_latency_budget_support;

pub use agent_latency_budget_store::{
    AgentLatencyBudgetAsterReadback, parse_agent_latency_budget_ledger,
    read_agent_latency_budget_ledger, write_agent_latency_budget_ledger,
};
use agent_latency_budget_support::{
    invalid_latency_budget, latency_error, push_stage, sim_seed, timed, validate_measurement,
    validate_request, validate_single_line, validate_tier_plan,
};

pub const AGENT_LATENCY_BUDGET_SCHEMA: &str = "litd.agent_latency_budget.readback.v1";
pub const AGENT_LATENCY_BUDGET_LEDGER_SCHEMA: &str = "litd.agent_latency_budget.ledger.v1";
pub const AGENT_LATENCY_BUDGET_GUI_ROUTE: &str = "egui://editor.project/agent_latency_budget";
pub const AGENT_LATENCY_BUDGET_CAPTURE_TOOL: &str = "litd.editor.play.capture_latency_budget";
pub const AGENT_LATENCY_BUDGET_READBACK_TOOL: &str = "litd.editor.play.read_latency_budget";
pub const SINGLE_CHANGE_REVALIDATE_BUDGET_MS: f64 = 1000.0;
pub const FOCUSED_SCENARIO_BUDGET_MS: f64 = 5000.0;
pub const FULL_WORLD_SMOKE_BUDGET_MS: f64 = 30_000.0;
const MAX_TIER_COUNT: usize = 32;
const MAX_SIM_BODIES: usize = 1024;
const MAX_SIM_TICKS: u64 = 120_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLatencyTierKind {
    SingleChangeRevalidate,
    FocusedScenario,
    FullWorldSmoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLatencyCacheMode {
    Cold,
    Warm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLatencyBudgetRequest {
    pub ledger_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub scenario_id: String,
    pub camera_rig: PlayCameraRig,
    pub tiers: Vec<AgentLatencyTierPlan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLatencyTierPlan {
    pub tier_id: String,
    pub kind: AgentLatencyTierKind,
    pub cache_mode: AgentLatencyCacheMode,
    pub budget_ms: f64,
    pub sim_ticks: u64,
    pub body_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLatencyStageMeasurement {
    pub stage_name: String,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLatencyTierMeasurement {
    pub tier_id: String,
    pub kind: AgentLatencyTierKind,
    pub cache_mode: AgentLatencyCacheMode,
    pub budget_ms: f64,
    pub elapsed_ms: f64,
    pub within_budget: bool,
    pub warmup_elapsed_ms: Option<f64>,
    pub slowest_stage_name: String,
    pub slowest_stage_ms: f64,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub validation_error_count: usize,
    pub sim_ticks: u64,
    pub body_count: usize,
    pub sim_bodies_updated: u64,
    pub sim_state_hash: u64,
    pub stages: Vec<AgentLatencyStageMeasurement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLatencyBudgetReadback {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub scenario_id: String,
    pub source_document_fingerprint: String,
    pub tier_count: usize,
    pub cold_tier_count: usize,
    pub warm_tier_count: usize,
    pub max_elapsed_ms: f64,
    pub all_tiers_within_budget: bool,
    pub single_change_under_budget: bool,
    pub focused_scenario_under_budget: bool,
    pub full_world_smoke_under_budget: bool,
    pub cold_and_warm_measured: bool,
    pub slowest_stage_name: String,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLatencyBudgetLedger {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub scenario_id: String,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
    pub camera_rig: PlayCameraRig,
    pub source_document: EditorDocument,
    pub tiers: Vec<AgentLatencyTierPlan>,
    pub measurements: Vec<AgentLatencyTierMeasurement>,
    pub readback: AgentLatencyBudgetReadback,
}

impl AgentLatencyTierKind {
    pub const fn target_budget_ms(self) -> f64 {
        match self {
            Self::SingleChangeRevalidate => SINGLE_CHANGE_REVALIDATE_BUDGET_MS,
            Self::FocusedScenario => FOCUSED_SCENARIO_BUDGET_MS,
            Self::FullWorldSmoke => FULL_WORLD_SMOKE_BUDGET_MS,
        }
    }
}

pub fn build_agent_latency_budget_ledger(
    document: &EditorDocument,
    request: AgentLatencyBudgetRequest,
) -> Result<AgentLatencyBudgetLedger, EngineError> {
    validate_request(&request)?;
    document.validate()?;
    let source_document_fingerprint = stable_fingerprint(document)?;
    let measurements = request
        .tiers
        .iter()
        .map(|tier| measure_latency_tier(document, request.camera_rig, tier))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger = AgentLatencyBudgetLedger {
        schema: AGENT_LATENCY_BUDGET_LEDGER_SCHEMA.to_string(),
        ledger_id: request.ledger_id,
        project_id: request.project_id,
        actor_id: request.actor_id,
        scenario_id: request.scenario_id,
        gui_route: AGENT_LATENCY_BUDGET_GUI_ROUTE.to_string(),
        capture_tool_name: AGENT_LATENCY_BUDGET_CAPTURE_TOOL.to_string(),
        readback_tool_name: AGENT_LATENCY_BUDGET_READBACK_TOOL.to_string(),
        camera_rig: request.camera_rig,
        source_document: document.clone(),
        tiers: request.tiers,
        measurements,
        readback: empty_readback(),
    };
    let ledger = AgentLatencyBudgetLedger {
        readback: build_readback(&ledger, source_document_fingerprint)?,
        ..ledger
    };
    validate_agent_latency_budget_ledger(&ledger)?;
    tracing::info!(
        ledger_id = %ledger.ledger_id,
        max_elapsed_ms = ledger.readback.max_elapsed_ms,
        tier_count = ledger.readback.tier_count,
        code = "LITD-AGENT-LATENCY-BUDGET-BUILT",
        "agent latency budget ledger built"
    );
    Ok(ledger)
}

pub fn validate_agent_latency_budget_ledger(
    ledger: &AgentLatencyBudgetLedger,
) -> Result<(), EngineError> {
    if ledger.schema != AGENT_LATENCY_BUDGET_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.project_id.trim().is_empty()
        || ledger.actor_id.trim().is_empty()
        || ledger.scenario_id.trim().is_empty()
        || ledger.gui_route != AGENT_LATENCY_BUDGET_GUI_ROUTE
        || ledger.capture_tool_name != AGENT_LATENCY_BUDGET_CAPTURE_TOOL
        || ledger.readback_tool_name != AGENT_LATENCY_BUDGET_READBACK_TOOL
        || ledger.tiers.is_empty()
        || ledger.tiers.len() != ledger.measurements.len()
    {
        return invalid_latency_budget("ledger", "required latency-budget fields are invalid");
    }
    validate_single_line("ledger.ledger_id", &ledger.ledger_id)?;
    validate_single_line("ledger.project_id", &ledger.project_id)?;
    validate_single_line("ledger.actor_id", &ledger.actor_id)?;
    validate_single_line("ledger.scenario_id", &ledger.scenario_id)?;
    ledger.source_document.validate()?;
    for (tier, measurement) in ledger.tiers.iter().zip(&ledger.measurements) {
        validate_tier_plan(tier)?;
        validate_measurement(measurement, tier)?;
    }
    let expected = build_readback(ledger, stable_fingerprint(&ledger.source_document)?)?;
    if expected != ledger.readback {
        return invalid_latency_budget("readback", "readback does not match measurements");
    }
    if !ledger.readback.all_tiers_within_budget
        || !ledger.readback.single_change_under_budget
        || !ledger.readback.focused_scenario_under_budget
        || !ledger.readback.full_world_smoke_under_budget
        || !ledger.readback.cold_and_warm_measured
    {
        return invalid_latency_budget(
            "readback.invariants",
            "latency gate requires all tiers under budget with cold and warm measurements",
        );
    }
    Ok(())
}

fn measure_latency_tier(
    document: &EditorDocument,
    camera_rig: PlayCameraRig,
    tier: &AgentLatencyTierPlan,
) -> Result<AgentLatencyTierMeasurement, EngineError> {
    validate_tier_plan(tier)?;
    let warmup_elapsed_ms = if tier.cache_mode == AgentLatencyCacheMode::Warm {
        Some(measure_tier_body(document, camera_rig, tier)?.elapsed_ms)
    } else {
        None
    };
    let mut measurement = measure_tier_body(document, camera_rig, tier)?;
    measurement.warmup_elapsed_ms = warmup_elapsed_ms;
    if !measurement.within_budget {
        return invalid_latency_budget(
            "budget",
            format!(
                "{} exceeded {:.3}ms budget with {:.3}ms; slowest_stage={}",
                measurement.tier_id,
                measurement.budget_ms,
                measurement.elapsed_ms,
                measurement.slowest_stage_name
            ),
        );
    }
    Ok(measurement)
}

fn measure_tier_body(
    document: &EditorDocument,
    camera_rig: PlayCameraRig,
    tier: &AgentLatencyTierPlan,
) -> Result<AgentLatencyTierMeasurement, EngineError> {
    let total_start = Instant::now();
    let mut stages = Vec::new();
    push_stage(&mut stages, "document_validate", || document.validate())?;
    let (validation, stage) = timed("world_play_validation", || {
        require_editor_world_valid(document, WorldValidationUseCase::Play)
    })?;
    stages.push(stage);
    let (document_fingerprint, stage) =
        timed("document_fingerprint", || stable_fingerprint(document))?;
    stages.push(stage);
    let (session, stage) = timed("play_build", || PlaySession::start(document, camera_rig))?;
    stages.push(stage);
    let (sim_report, stage) = timed("fixed_tick_sim", || run_deterministic_sim(tier))?;
    stages.push(stage);
    let elapsed_ms = total_start.elapsed().as_secs_f64() * 1000.0;
    let slowest = stages
        .iter()
        .max_by(|left, right| left.elapsed_ms.total_cmp(&right.elapsed_ms))
        .ok_or_else(|| latency_error("stages", "missing stage measurements"))?;
    Ok(AgentLatencyTierMeasurement {
        tier_id: tier.tier_id.clone(),
        kind: tier.kind,
        cache_mode: tier.cache_mode,
        budget_ms: tier.budget_ms,
        elapsed_ms,
        within_budget: elapsed_ms <= tier.budget_ms,
        warmup_elapsed_ms: None,
        slowest_stage_name: slowest.stage_name.clone(),
        slowest_stage_ms: slowest.elapsed_ms,
        document_fingerprint,
        play_build_fingerprint: stable_fingerprint(&session.build)?,
        validation_error_count: validation.error_count,
        sim_ticks: sim_report.steps,
        body_count: tier.body_count,
        sim_bodies_updated: sim_report.bodies_updated,
        sim_state_hash: sim_report.state_hash,
        stages,
    })
}

fn run_deterministic_sim(
    tier: &AgentLatencyTierPlan,
) -> Result<litd_engine::sim::SimRunReport, EngineError> {
    let mut sim = FixedTickSim::<MAX_SIM_BODIES>::new(SimConfig::new(64)?, sim_seed(tier))?;
    for index in 0..tier.body_count {
        let lane = (index % 64) as i32;
        sim.spawn_body(
            FixedVec3::from_i32s(index as i32, 0, -lane),
            FixedVec3::from_i32s(1 + (lane % 3), 0, -1),
            FixedVec3::ZERO,
        )?;
    }
    sim.run_ticks(tier.sim_ticks)
}

fn build_readback(
    ledger: &AgentLatencyBudgetLedger,
    source_document_fingerprint: String,
) -> Result<AgentLatencyBudgetReadback, EngineError> {
    let max_elapsed_ms = ledger
        .measurements
        .iter()
        .map(|measurement| measurement.elapsed_ms)
        .fold(0.0_f64, f64::max);
    let slowest = ledger
        .measurements
        .iter()
        .max_by(|left, right| left.slowest_stage_ms.total_cmp(&right.slowest_stage_ms))
        .ok_or_else(|| latency_error("measurements", "missing latency measurements"))?;
    Ok(AgentLatencyBudgetReadback {
        schema: AGENT_LATENCY_BUDGET_SCHEMA.to_string(),
        ledger_id: ledger.ledger_id.clone(),
        project_id: ledger.project_id.clone(),
        scenario_id: ledger.scenario_id.clone(),
        source_document_fingerprint,
        tier_count: ledger.measurements.len(),
        cold_tier_count: cache_count(&ledger.measurements, AgentLatencyCacheMode::Cold),
        warm_tier_count: cache_count(&ledger.measurements, AgentLatencyCacheMode::Warm),
        max_elapsed_ms,
        all_tiers_within_budget: ledger
            .measurements
            .iter()
            .all(|measurement| measurement.within_budget),
        single_change_under_budget: kind_under_budget(
            &ledger.measurements,
            AgentLatencyTierKind::SingleChangeRevalidate,
        ),
        focused_scenario_under_budget: kind_under_budget(
            &ledger.measurements,
            AgentLatencyTierKind::FocusedScenario,
        ),
        full_world_smoke_under_budget: kind_under_budget(
            &ledger.measurements,
            AgentLatencyTierKind::FullWorldSmoke,
        ),
        cold_and_warm_measured: ledger
            .measurements
            .iter()
            .any(|measurement| measurement.cache_mode == AgentLatencyCacheMode::Cold)
            && ledger
                .measurements
                .iter()
                .any(|measurement| measurement.cache_mode == AgentLatencyCacheMode::Warm),
        slowest_stage_name: slowest.slowest_stage_name.clone(),
        gui_route: ledger.gui_route.clone(),
        capture_tool_name: ledger.capture_tool_name.clone(),
        readback_tool_name: ledger.readback_tool_name.clone(),
    })
}

fn kind_under_budget(
    measurements: &[AgentLatencyTierMeasurement],
    kind: AgentLatencyTierKind,
) -> bool {
    let mut found = false;
    let mut all_under = true;
    for measurement in measurements
        .iter()
        .filter(|measurement| measurement.kind == kind)
    {
        found = true;
        all_under &= measurement.within_budget;
    }
    found && all_under
}

fn cache_count(
    measurements: &[AgentLatencyTierMeasurement],
    cache_mode: AgentLatencyCacheMode,
) -> usize {
    measurements
        .iter()
        .filter(|measurement| measurement.cache_mode == cache_mode)
        .count()
}

fn empty_readback() -> AgentLatencyBudgetReadback {
    AgentLatencyBudgetReadback {
        schema: String::new(),
        ledger_id: String::new(),
        project_id: String::new(),
        scenario_id: String::new(),
        source_document_fingerprint: String::new(),
        tier_count: 0,
        cold_tier_count: 0,
        warm_tier_count: 0,
        max_elapsed_ms: 0.0,
        all_tiers_within_budget: false,
        single_change_under_budget: false,
        focused_scenario_under_budget: false,
        full_world_smoke_under_budget: false,
        cold_and_warm_measured: false,
        slowest_stage_name: String::new(),
        gui_route: String::new(),
        capture_tool_name: String::new(),
        readback_tool_name: String::new(),
    }
}

#[cfg(test)]
#[path = "agent_latency_budget_tests.rs"]
mod tests;
