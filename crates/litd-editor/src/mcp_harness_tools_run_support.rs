use litd_engine::EngineError;

use super::{
    HarnessRunMode, HarnessRunRecord, HarnessRunRenderMode,
    mcp_harness_tools_validation::invalid_harness,
};

pub(super) fn validate_run_record(run: &HarnessRunRecord) -> Result<(), EngineError> {
    let Some(expected_final_tick) = run.start_tick.checked_add(run.steps) else {
        return invalid_harness("run.final_tick", "run tick range overflowed");
    };
    if run.render_mode != HarnessRunRenderMode::HeadlessNoRender
        || run.termination_reason.trim().is_empty()
        || run.final_tick != expected_final_tick
        || run.steps > run.ticks_requested
        || run.final_snapshot.state_hash != run.final_state_hash
    {
        return invalid_harness("run", "run timing, render mode, or hash is invalid");
    }
    match run.run_mode {
        HarnessRunMode::FixedTicks => validate_fixed_run(run),
        HarnessRunMode::UntilCondition => validate_until_run(run),
    }
}

fn validate_fixed_run(run: &HarnessRunRecord) -> Result<(), EngineError> {
    if run.until_condition.is_some()
        || run.steps != run.ticks_requested
        || run.termination_reason != "ticks_completed"
    {
        return invalid_harness("run.fixed_ticks", "fixed-tick run fields are invalid");
    }
    Ok(())
}

fn validate_until_run(run: &HarnessRunRecord) -> Result<(), EngineError> {
    if run.until_condition.is_none() || run.termination_reason != "until_condition_satisfied" {
        return invalid_harness(
            "run.until_condition",
            "until-condition run fields are invalid",
        );
    }
    Ok(())
}
