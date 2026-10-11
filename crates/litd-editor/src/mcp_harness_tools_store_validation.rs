use super::*;

pub fn validate_mcp_harness_state(state: &McpHarnessState) -> Result<(), EngineError> {
    if state.schema != MCP_HARNESS_STATE_SCHEMA
        || state.issue != 1178
        || state.state_id.trim().is_empty()
        || state.source_of_truth.trim().is_empty()
        || state.events.len() != state.update_count as usize
    {
        return Err(store_error(
            "state",
            "schema, issue, identity, or event count is invalid",
        ));
    }
    for signal in &state.calyx_signals {
        validate_calyx_signals_record(signal)?;
    }
    for run in &state.runs {
        validate_run_record(run)?;
    }
    for batch in &state.sandbox_batches {
        validate_sandbox_batch_record(batch)?;
    }
    for assertion in &state.assertions {
        validate_assertion_verdict(assertion)?;
    }
    for oracle in &state.oracle_runs {
        validate_oracle_run_record(oracle)?;
    }
    for bot_run in &state.bot_runs {
        validate_bot_drive_record(bot_run)?;
    }
    for diff in &state.screenshot_diffs {
        validate_screenshot_diff_record(diff)?;
    }
    for event in &state.events {
        validate_feedback_stream(&event.feedback)?;
    }
    Ok(())
}

fn validate_assertion_verdict(verdict: &HarnessAssertionVerdict) -> Result<(), EngineError> {
    if verdict.verdict_id.trim().is_empty()
        || verdict.run_id.trim().is_empty()
        || verdict.check_count != verdict.checks.len()
        || verdict.passed_count + verdict.failed_count != verdict.check_count
    {
        return Err(store_error(
            "assertions",
            "assertion verdict identity or counts are invalid",
        ));
    }
    let expected = if verdict.failed_count == 0 {
        "passed"
    } else {
        "failed"
    };
    if verdict.verdict != expected {
        return Err(store_error(
            "assertions",
            "assertion verdict status is invalid",
        ));
    }
    for check in &verdict.checks {
        if check.assertion_id.trim().is_empty()
            || check.expected.trim().is_empty()
            || check.actual.trim().is_empty()
            || check.reason.trim().is_empty()
        {
            return Err(store_error(
                "assertions.check",
                "assertion check details are invalid",
            ));
        }
    }
    Ok(())
}

pub fn validate_mcp_harness_ledger(ledger: &McpHarnessLedger) -> Result<(), EngineError> {
    if ledger.schema != MCP_HARNESS_LEDGER_SCHEMA
        || ledger.issue != 1178
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.record_count != ledger.records.len()
        || ledger.records.is_empty()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and count are invalid",
        ));
    }
    for record in &ledger.records {
        if record.state_sha256.len() != 64
            || record.state_bytes == 0
            || record.state_fingerprint.trim().is_empty()
            || record.event.affected_ids.is_empty()
        {
            return Err(store_error("record", "required record fields are invalid"));
        }
        validate_feedback_stream(&record.event.feedback)?;
    }
    Ok(())
}
