use litd_engine::{
    EngineError,
    math::{Fixed, FixedVec3},
    sim::{SimBodyId, SimBodyState},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};

use crate::history::stable_fingerprint;

use super::{
    DriveBotHarnessArgs, HarnessBotDriveRecord, HarnessBotDriverKind, HarnessBotMetrics,
    HarnessBotObjective, HarnessBotOutcomeStatus, HarnessBotTrajectorySample, HarnessRunRecord,
    mcp_harness_tools_bot_validation::{MAX_BOT_TRAJECTORY_SAMPLES, validate_bot_drive_record},
    mcp_harness_tools_validation::invalid_harness,
};

const SOURCE_OF_TRUTH: &str =
    "MCP harness bot run state JSON + replay orders + trajectory samples + Aster ledger row";

pub(super) fn build_bot_record(
    args: &DriveBotHarnessArgs,
    run: &HarnessRunRecord,
) -> Result<HarnessBotDriveRecord, EngineError> {
    let target_body_id = resolve_target_body_id(args.target_body_id, &run.final_snapshot)?;
    let orders = build_orders(args, run.final_snapshot.tick, target_body_id)?;
    let execution = execute_plan(
        &run.final_snapshot,
        target_body_id,
        &orders,
        args.ticks,
        args.sample_interval_ticks,
        &args.objective,
    )?;
    let replay = execute_plan(
        &run.final_snapshot,
        target_body_id,
        &orders,
        execution.ticks_elapsed,
        args.sample_interval_ticks,
        &args.objective,
    )?;
    if replay.final_state_hash != execution.final_state_hash
        || replay.trajectory != execution.trajectory
        || replay.final_tick != execution.final_tick
    {
        return invalid_harness(
            "bot.replay",
            format!(
                "deterministic bot replay diverged final={:016X}/{:016X} samples={}/{} ticks={}/{}",
                execution.final_state_hash,
                replay.final_state_hash,
                execution.trajectory.len(),
                replay.trajectory.len(),
                execution.final_tick,
                replay.final_tick
            ),
        );
    }
    let replay_record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: run.final_snapshot.clone(),
        orders: orders.clone(),
        run_ticks: execution.ticks_elapsed,
        expected_final_hash: execution.final_state_hash,
    };
    let replay_outcome = replay_record.replay::<16, 8>()?;
    if replay_outcome.final_hash != execution.final_state_hash
        || replay_outcome.final_tick != execution.final_tick
        || replay_outcome.orders_applied != execution.order_count
    {
        return invalid_harness(
            "bot.replay_record",
            "stored replay record did not reproduce bot run",
        );
    }
    let verdict = if execution.status == args.expected_outcome {
        "passed"
    } else {
        "failed"
    };
    let (failure_code, failure_message) =
        failure_details(execution.status, args.expected_outcome, verdict);
    let record = HarnessBotDriveRecord {
        bot_run_id: args.bot_run_id.clone(),
        run_id: run.run_id.clone(),
        bot_id: args.bot_id.clone(),
        driver_kind: args.driver_kind,
        objective: args.objective.clone(),
        expected_outcome: args.expected_outcome,
        status: execution.status,
        verdict: verdict.to_string(),
        termination_reason: execution.termination_reason,
        source_of_truth: SOURCE_OF_TRUTH.to_string(),
        target_body_id,
        tick_budget: args.ticks,
        sample_interval_ticks: args.sample_interval_ticks,
        initial_state_hash: run.final_state_hash,
        final_state_hash: execution.final_state_hash,
        replay_final_state_hash: replay_outcome.final_hash,
        replay_verified: true,
        final_tick: execution.final_tick,
        ticks_elapsed: execution.ticks_elapsed,
        bodies_updated: execution.bodies_updated,
        order_count: execution.order_count,
        orders,
        replay_record,
        metrics: execution.metrics,
        trajectory_sample_count: execution.trajectory.len(),
        trajectory_hash: stable_fingerprint(&execution.trajectory)?,
        trajectory: execution.trajectory,
        failure_code,
        failure_message,
    };
    validate_bot_drive_record(&record)?;
    Ok(record)
}

struct BotExecution {
    status: HarnessBotOutcomeStatus,
    termination_reason: String,
    final_state_hash: u64,
    final_tick: u64,
    ticks_elapsed: u64,
    bodies_updated: u64,
    order_count: u64,
    metrics: HarnessBotMetrics,
    trajectory: Vec<HarnessBotTrajectorySample>,
}

fn execute_plan(
    snapshot: &SimSnapshot,
    target_body_id: u32,
    orders: &[SimOrder],
    tick_budget: u64,
    sample_interval: u64,
    objective: &HarnessBotObjective,
) -> Result<BotExecution, EngineError> {
    let mut sim = snapshot.restore::<16>()?;
    let target = SimBodyId::from_raw(target_body_id);
    let start_body = sim.body_state(target).ok_or_else(|| {
        invalid_harness::<()>("bot.target", "target body is absent from restored snapshot")
            .unwrap_err()
    })?;
    let start_tick = sim.tick();
    let start_x_milli = fixed_to_milli(start_body.position.x)?;
    let start_z_milli = fixed_to_milli(start_body.position.z)?;
    let mut queue = SimOrderQueue::<8>::default();
    for order in orders {
        queue.enqueue(*order)?;
    }
    let mut bodies_updated = 0_u64;
    let mut trajectory = Vec::new();
    push_sample(
        &mut trajectory,
        &sim,
        target,
        start_x_milli,
        start_tick,
        objective,
        queue.applied_total(),
    )?;
    let mut max_progress = objective_progress(
        objective,
        start_x_milli,
        start_tick,
        start_tick,
        &start_body,
    )?;
    let mut status = None;
    let mut termination_reason = None;
    if objective_satisfied(
        objective,
        start_x_milli,
        start_tick,
        start_tick,
        &start_body,
    )? {
        status = Some(HarnessBotOutcomeStatus::Completed);
        termination_reason = Some("objective_satisfied_at_start".to_string());
    }
    while status.is_none() && sim.tick().saturating_sub(start_tick) < tick_budget {
        let step = queue.step(&mut sim)?;
        bodies_updated = bodies_updated
            .checked_add(step.bodies_updated as u64)
            .ok_or(EngineError::SimTickOverflow)?;
        let body = sim.body_state(target).ok_or_else(|| {
            invalid_harness::<()>("bot.target", "target body disappeared during bot run")
                .unwrap_err()
        })?;
        let progress = objective_progress(objective, start_x_milli, start_tick, sim.tick(), &body)?;
        max_progress = max_progress.max(progress);
        let elapsed = sim.tick().saturating_sub(start_tick);
        let completed =
            objective_satisfied(objective, start_x_milli, start_tick, sim.tick(), &body)?;
        if should_sample(elapsed, tick_budget, sample_interval, completed) {
            push_sample(
                &mut trajectory,
                &sim,
                target,
                start_x_milli,
                start_tick,
                objective,
                queue.applied_total(),
            )?;
        }
        if completed {
            status = Some(HarnessBotOutcomeStatus::Completed);
            termination_reason = Some("objective_satisfied".to_string());
        }
    }
    let final_body = sim.body_state(target).ok_or_else(|| {
        invalid_harness::<()>("bot.target", "target body missing at final tick").unwrap_err()
    })?;
    let final_progress = objective_progress(
        objective,
        start_x_milli,
        start_tick,
        sim.tick(),
        &final_body,
    )?;
    let actual_status = status.unwrap_or_else(|| unresolved_status(objective, max_progress));
    let reason = termination_reason.unwrap_or_else(|| {
        match actual_status {
            HarnessBotOutcomeStatus::Unwinnable => "objective_unwinnable_no_forward_progress",
            HarnessBotOutcomeStatus::Timeout => "tick_budget_exhausted",
            HarnessBotOutcomeStatus::Failed => "bot_execution_failed",
            HarnessBotOutcomeStatus::Completed => "objective_satisfied",
        }
        .to_string()
    });
    let final_x_milli = fixed_to_milli(final_body.position.x)?;
    let final_z_milli = fixed_to_milli(final_body.position.z)?;
    Ok(BotExecution {
        status: actual_status,
        termination_reason: reason,
        final_state_hash: sim.state_hash(),
        final_tick: sim.tick(),
        ticks_elapsed: sim.tick().saturating_sub(start_tick),
        bodies_updated,
        order_count: queue.applied_total(),
        metrics: HarnessBotMetrics {
            start_x_milli,
            final_x_milli,
            displacement_x_milli: final_x_milli.saturating_sub(start_x_milli),
            displacement_z_milli: final_z_milli.saturating_sub(start_z_milli),
            objective_progress_milli: final_progress,
            max_objective_progress_milli: max_progress,
            distance_xz_milli: final_x_milli
                .saturating_sub(start_x_milli)
                .abs()
                .saturating_add(final_z_milli.saturating_sub(start_z_milli).abs()),
            body_updates: bodies_updated,
            orders_applied: queue.applied_total(),
        },
        trajectory,
    })
}

fn build_orders(
    args: &DriveBotHarnessArgs,
    initial_tick: u64,
    target_body_id: u32,
) -> Result<Vec<SimOrder>, EngineError> {
    if args.ticks == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![SimOrder::new(
        initial_tick
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?,
        SimOrderSource::lowered_agent(driver_source_id(args.driver_kind)),
        1,
        SimBodyId::from_raw(target_body_id),
        SimOrderKind::SetVelocity(FixedVec3::new(
            Fixed::from_ratio(args.velocity_x_milli, 1000)?,
            Fixed::from_raw(0),
            Fixed::from_ratio(args.velocity_z_milli, 1000)?,
        )),
    )])
}

fn resolve_target_body_id(
    requested: Option<u32>,
    snapshot: &SimSnapshot,
) -> Result<u32, EngineError> {
    let target_body_id = requested
        .or_else(|| snapshot.bodies.first().map(|body| body.id.index()))
        .ok_or_else(|| {
            invalid_harness::<()>("bot.target", "run has no body to drive").unwrap_err()
        })?;
    if !snapshot
        .bodies
        .iter()
        .any(|body| body.id.index() == target_body_id)
    {
        return invalid_harness(
            "bot.target_body_id",
            "target body is absent from source run",
        );
    }
    Ok(target_body_id)
}

fn push_sample(
    trajectory: &mut Vec<HarnessBotTrajectorySample>,
    sim: &litd_engine::sim::FixedTickSim<16>,
    target: SimBodyId,
    start_x_milli: i64,
    start_tick: u64,
    objective: &HarnessBotObjective,
    orders_applied: u64,
) -> Result<(), EngineError> {
    if trajectory.len() == MAX_BOT_TRAJECTORY_SAMPLES {
        return invalid_harness("bot.trajectory", "trajectory sample limit reached");
    }
    let body = sim.body_state(target).ok_or_else(|| {
        invalid_harness::<()>("bot.target", "target body missing during sample").unwrap_err()
    })?;
    trajectory.push(HarnessBotTrajectorySample {
        sample_index: trajectory.len(),
        tick: sim.tick(),
        body_id: target.index(),
        position_x_milli: fixed_to_milli(body.position.x)?,
        position_z_milli: fixed_to_milli(body.position.z)?,
        velocity_x_milli: fixed_to_milli(body.velocity.x)?,
        velocity_z_milli: fixed_to_milli(body.velocity.z)?,
        state_hash: sim.state_hash(),
        orders_applied_total: orders_applied,
        objective_progress_milli: objective_progress(
            objective,
            start_x_milli,
            start_tick,
            sim.tick(),
            &body,
        )?,
    });
    Ok(())
}

fn should_sample(elapsed: u64, tick_budget: u64, interval: u64, completed: bool) -> bool {
    completed || elapsed == tick_budget || elapsed.is_multiple_of(interval)
}

fn objective_satisfied(
    objective: &HarnessBotObjective,
    start_x_milli: i64,
    start_tick: u64,
    current_tick: u64,
    body: &SimBodyState,
) -> Result<bool, EngineError> {
    Ok(match objective {
        HarnessBotObjective::ReachPositionXAtLeast { min_x_milli } => {
            fixed_to_milli(body.position.x)? >= *min_x_milli
        }
        HarnessBotObjective::SurviveTicks { min_ticks } => {
            objective_progress(objective, start_x_milli, start_tick, current_tick, body)?
                >= i64::try_from(*min_ticks).map_err(|_| EngineError::SimTickOverflow)?
        }
    })
}

fn objective_progress(
    objective: &HarnessBotObjective,
    start_x_milli: i64,
    start_tick: u64,
    current_tick: u64,
    body: &SimBodyState,
) -> Result<i64, EngineError> {
    match objective {
        HarnessBotObjective::ReachPositionXAtLeast { .. } => {
            Ok(fixed_to_milli(body.position.x)?.saturating_sub(start_x_milli))
        }
        HarnessBotObjective::SurviveTicks { .. } => current_tick
            .checked_sub(start_tick)
            .and_then(|ticks| i64::try_from(ticks).ok())
            .ok_or(EngineError::SimTickOverflow),
    }
}

fn unresolved_status(
    objective: &HarnessBotObjective,
    max_progress: i64,
) -> HarnessBotOutcomeStatus {
    match objective {
        HarnessBotObjective::ReachPositionXAtLeast { .. } if max_progress <= 0 => {
            HarnessBotOutcomeStatus::Unwinnable
        }
        _ => HarnessBotOutcomeStatus::Timeout,
    }
}

fn failure_details(
    status: HarnessBotOutcomeStatus,
    expected: HarnessBotOutcomeStatus,
    verdict: &str,
) -> (Option<String>, Option<String>) {
    if verdict == "failed" {
        return (
            Some("LITD_HARNESS_BOT_OUTCOME_MISMATCH".to_string()),
            Some(format!("bot outcome was {status:?}, expected {expected:?}")),
        );
    }
    match status {
        HarnessBotOutcomeStatus::Completed => (None, None),
        HarnessBotOutcomeStatus::Unwinnable => (
            Some("LITD_HARNESS_BOT_UNWINNABLE".to_string()),
            Some("bot made no forward progress toward the objective".to_string()),
        ),
        HarnessBotOutcomeStatus::Timeout => (
            Some("LITD_HARNESS_BOT_TIMEOUT".to_string()),
            Some("bot exhausted the tick budget before satisfying the objective".to_string()),
        ),
        HarnessBotOutcomeStatus::Failed => (
            Some("LITD_HARNESS_BOT_FAILED".to_string()),
            Some("bot execution failed before producing a deterministic outcome".to_string()),
        ),
    }
}

fn fixed_to_milli(value: Fixed) -> Result<i64, EngineError> {
    let scaled = (value.raw() as i128)
        .checked_mul(1000)
        .ok_or(EngineError::SimTickOverflow)?
        / (1_i128 << 32);
    i64::try_from(scaled).map_err(|_| EngineError::SimTickOverflow)
}

fn driver_source_id(kind: HarnessBotDriverKind) -> u32 {
    match kind {
        HarnessBotDriverKind::Heuristic => 1,
        HarnessBotDriverKind::BehaviorTree => 2,
        HarnessBotDriverKind::FiniteStateMachine => 3,
        HarnessBotDriverKind::CalyxAgentLowered => 4,
    }
}
