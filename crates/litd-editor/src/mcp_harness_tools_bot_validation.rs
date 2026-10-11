use litd_engine::EngineError;

use super::{
    DriveBotHarnessArgs, HarnessBotDriveRecord, HarnessBotObjective, HarnessBotOutcomeStatus,
    mcp_harness_tools_validation::{
        invalid_harness, validate_context, validate_path_string, validate_token,
    },
};

pub(super) const MAX_BOT_TRAJECTORY_SAMPLES: usize = 256;

pub(super) fn validate_bot_args(args: &DriveBotHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("bot_run_id", &args.bot_run_id)?;
    validate_token("run_id", &args.run_id)?;
    validate_token("bot_id", &args.bot_id)?;
    validate_bot_ticks(args.ticks, "bot.ticks")?;
    if args.sample_interval_ticks == 0 {
        return invalid_harness(
            "bot.sample_interval_ticks",
            "sample interval must be non-zero",
        );
    }
    if max_sample_count(args.ticks, args.sample_interval_ticks)? > MAX_BOT_TRAJECTORY_SAMPLES {
        return invalid_harness(
            "bot.trajectory",
            "trajectory sample count exceeds the bot-driver safety limit",
        );
    }
    validate_objective(&args.objective)?;
    validate_path_string("state_path", &args.context.state_path)
}

pub(super) fn validate_bot_drive_record(record: &HarnessBotDriveRecord) -> Result<(), EngineError> {
    validate_token("bot.bot_run_id", &record.bot_run_id)?;
    validate_token("bot.run_id", &record.run_id)?;
    validate_token("bot.bot_id", &record.bot_id)?;
    if record.source_of_truth.trim().is_empty()
        || record.sample_interval_ticks == 0
        || record.trajectory.is_empty()
        || record.trajectory_sample_count != record.trajectory.len()
        || record.replay_record.expected_final_hash != record.final_state_hash
        || record.replay_record.run_ticks != record.ticks_elapsed
        || record.replay_record.orders != record.orders
        || record.replay_final_state_hash != record.final_state_hash
        || !record.replay_verified
        || record.order_count != record.orders.len() as u64
    {
        return invalid_harness(
            "bot.record",
            "bot record counts or replay fields are invalid",
        );
    }
    let expected_verdict = if record.status == record.expected_outcome {
        "passed"
    } else {
        "failed"
    };
    if record.verdict != expected_verdict {
        return invalid_harness("bot.verdict", "bot verdict does not match expected outcome");
    }
    if record.verdict == "failed" && record.failure_code.is_none() {
        return invalid_harness(
            "bot.failure_code",
            "failed bot run must carry a failure code",
        );
    }
    if record.status != HarnessBotOutcomeStatus::Completed && record.failure_code.is_none() {
        return invalid_harness(
            "bot.failure_code",
            "non-completed bot run must report a failure code",
        );
    }
    validate_trajectory(record)
}

fn validate_trajectory(record: &HarnessBotDriveRecord) -> Result<(), EngineError> {
    let mut previous_tick = None;
    for (index, sample) in record.trajectory.iter().enumerate() {
        if sample.sample_index != index || sample.body_id != record.target_body_id {
            return invalid_harness("bot.trajectory", "sample index or body id is invalid");
        }
        if let Some(tick) = previous_tick
            && sample.tick < tick
        {
            return invalid_harness("bot.trajectory", "sample ticks must be monotonic");
        }
        previous_tick = Some(sample.tick);
    }
    let first = record.trajectory.first().ok_or_else(|| {
        invalid_harness::<()>("bot.trajectory", "trajectory is empty").unwrap_err()
    })?;
    let last = record.trajectory.last().ok_or_else(|| {
        invalid_harness::<()>("bot.trajectory", "trajectory is empty").unwrap_err()
    })?;
    if first.state_hash != record.initial_state_hash || last.state_hash != record.final_state_hash {
        return invalid_harness(
            "bot.trajectory_hash",
            "trajectory endpoints do not match bot state hashes",
        );
    }
    Ok(())
}

fn validate_objective(objective: &HarnessBotObjective) -> Result<(), EngineError> {
    match objective {
        HarnessBotObjective::ReachPositionXAtLeast { .. } => Ok(()),
        HarnessBotObjective::SurviveTicks { min_ticks } => {
            validate_bot_ticks(*min_ticks, "bot.objective.min_ticks")
        }
    }
}

fn validate_bot_ticks(ticks: u64, field: &str) -> Result<(), EngineError> {
    if ticks > 1_000_000 {
        return invalid_harness(field, "tick count exceeds the harness safety limit");
    }
    Ok(())
}

fn max_sample_count(ticks: u64, interval: u64) -> Result<usize, EngineError> {
    let samples = ticks
        .checked_div(interval)
        .and_then(|value| value.checked_add(2))
        .ok_or(EngineError::SimTickOverflow)?;
    usize::try_from(samples).map_err(|_| EngineError::SimTickOverflow)
}
