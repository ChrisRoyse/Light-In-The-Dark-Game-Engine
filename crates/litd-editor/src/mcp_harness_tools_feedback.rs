//! Structured feedback stream records for the agent harness.

use std::collections::BTreeSet;

use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

use super::{
    EmitFeedbackHarnessArgs, HarnessAssertionVerdict, HarnessBotDriveRecord,
    HarnessBotOutcomeStatus, HarnessCalyxSignalStatus, HarnessCalyxSignalsRecord,
    HarnessFeedbackInput, HarnessFeedbackRecord, HarnessOracleKind, HarnessOracleRunRecord,
    HarnessPevTranscriptRecord, finish_mutation_with_feedback, load_state,
    mcp_harness_tools_helpers::{parse_args, validate_mutating_route},
    mcp_harness_tools_validation::{invalid_harness, validate_emit_feedback_args},
};

pub(super) const UNKNOWN_FEEDBACK_CODE: &str = "LITD_HARNESS_UNKNOWN_FAILURE";
const SOURCE_OF_TRUTH: &str =
    "MCP harness Loom event stream + state JSON + ledger JSON + Aster ledger row";

pub(super) fn emit_feedback(
    route: McpDomainTool,
    arguments: Value,
) -> Result<super::McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: EmitFeedbackHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_emit_feedback_args(&args)?;
    let state = load_state(&args.context)?;
    let records = build_feedback_stream(&args.feedback_id, "explicit_feedback", args.feedback)?;
    finish_mutation_with_feedback(
        route,
        args.context,
        state,
        "emitted structured fix-oriented harness feedback into the Loom stream",
        records
            .iter()
            .map(|record| record.feedback_id.clone())
            .collect(),
        json!({ "feedback": records }),
        records,
    )
}

pub(super) fn feedback_from_assertion_verdict(
    verdict: &HarnessAssertionVerdict,
) -> Result<Vec<HarnessFeedbackRecord>, EngineError> {
    let feedback = verdict
        .checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| HarnessFeedbackInput {
            code: check.code.clone(),
            message: check.reason.clone(),
            remediation: remediation_for_code(&check.code),
            actual: check.actual.clone(),
            expected: check.expected.clone(),
            location: check.location.clone(),
        })
        .collect();
    build_feedback_stream(&verdict.verdict_id, "assertion_verdict", feedback)
}

pub(super) fn feedback_from_oracle_run(
    record: &HarnessOracleRunRecord,
) -> Result<Vec<HarnessFeedbackRecord>, EngineError> {
    let feedback = record
        .checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| {
            let code = oracle_code(check.oracle);
            HarnessFeedbackInput {
                code: code.to_string(),
                message: check.reason.clone(),
                remediation: remediation_for_code(code),
                actual: check.actual.clone(),
                expected: format!("{} passes", oracle_location(check.oracle)),
                location: oracle_location(check.oracle),
            }
        })
        .collect();
    build_feedback_stream(&record.oracle_run_id, "oracle_run", feedback)
}

pub(super) fn feedback_from_bot_drive_record(
    record: &HarnessBotDriveRecord,
) -> Result<Vec<HarnessFeedbackRecord>, EngineError> {
    let mut feedback = Vec::new();
    if record.status != HarnessBotOutcomeStatus::Completed {
        let code = bot_status_code(record.status);
        feedback.push(HarnessFeedbackInput {
            code: code.to_string(),
            message: record
                .failure_message
                .clone()
                .unwrap_or_else(|| format!("bot ended with status {:?}", record.status)),
            remediation: remediation_for_code(code),
            actual: bot_actual(record),
            expected: format!("expected_outcome={:?}", record.expected_outcome),
            location: format!("bot_run.{}.status", record.bot_run_id),
        });
    }
    if record.verdict != "passed" {
        feedback.push(HarnessFeedbackInput {
            code: "LITD_HARNESS_BOT_OUTCOME_MISMATCH".to_string(),
            message: format!(
                "bot outcome was {:?}, expected {:?}",
                record.status, record.expected_outcome
            ),
            remediation: remediation_for_code("LITD_HARNESS_BOT_OUTCOME_MISMATCH"),
            actual: bot_actual(record),
            expected: format!("expected_outcome={:?}", record.expected_outcome),
            location: format!("bot_run.{}.verdict", record.bot_run_id),
        });
    }
    if !record.replay_verified {
        feedback.push(HarnessFeedbackInput {
            code: "LITD_HARNESS_BOT_REPLAY_DIVERGED".to_string(),
            message: "bot replay did not reproduce the persisted trajectory".to_string(),
            remediation: remediation_for_code("LITD_HARNESS_BOT_REPLAY_DIVERGED"),
            actual: format!(
                "final_hash={:016X} replay_hash={:016X}",
                record.final_state_hash, record.replay_final_state_hash
            ),
            expected: "replay_hash equals final_hash and trajectory samples match".to_string(),
            location: format!("bot_run.{}.replay", record.bot_run_id),
        });
    }
    build_feedback_stream(&record.bot_run_id, "bot_run", feedback)
}

pub(super) fn feedback_from_calyx_signals(
    record: &HarnessCalyxSignalsRecord,
) -> Result<Vec<HarnessFeedbackRecord>, EngineError> {
    let feedback = record
        .signals
        .iter()
        .filter(|signal| signal.status != HarnessCalyxSignalStatus::Admitted)
        .map(|signal| HarnessFeedbackInput {
            code: signal.code.clone(),
            message: signal.message.clone(),
            remediation: signal.remediation.clone(),
            actual: format!(
                "status={:?} evidence={}",
                signal.status,
                signal.evidence.join("|")
            ),
            expected: "status=admitted".to_string(),
            location: format!(
                "calyx_signals.{}. {:?}",
                signal.content_id, signal.subsystem
            )
            .replace(". ", "."),
        })
        .collect();
    build_feedback_stream(&record.signal_batch_id, "calyx_signals", feedback)
}

pub(super) fn feedback_from_pev_transcript(
    transcript: &HarnessPevTranscriptRecord,
) -> Result<Vec<HarnessFeedbackRecord>, EngineError> {
    let feedback = transcript
        .turns
        .iter()
        .filter(|turn| !turn.passed || turn.no_op || turn.regression)
        .map(|turn| HarnessFeedbackInput {
            code: turn.feedback_code.clone(),
            message: turn.feedback_message.clone(),
            remediation: turn.remediation.clone(),
            actual: format!(
                "passed={} no_op={} regression={} applied_to_state={}",
                turn.passed, turn.no_op, turn.regression, turn.applied_to_state
            ),
            expected: "passed=true no_op=false regression=false".to_string(),
            location: format!(
                "pev_transcript.{}.turn.{}",
                transcript.transcript_id, turn.turn_id
            ),
        })
        .collect();
    build_feedback_stream(&transcript.transcript_id, "pev_transcript", feedback)
}

pub(super) fn build_feedback_stream(
    prefix: &str,
    source: &str,
    inputs: Vec<HarnessFeedbackInput>,
) -> Result<Vec<HarnessFeedbackRecord>, EngineError> {
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for input in inputs {
        let normalized = normalize_input(input)?;
        let key = feedback_key(&normalized);
        if !seen.insert(key) {
            continue;
        }
        let index = records.len() as u64 + 1;
        records.push(HarnessFeedbackRecord {
            feedback_id: format!("{prefix}:{index}"),
            stream_index: index,
            source: source.to_string(),
            severity: "error".to_string(),
            code: normalized.code,
            message: normalized.message,
            remediation: normalized.remediation,
            actual: normalized.actual,
            expected: normalized.expected,
            location: normalized.location,
            source_of_truth: SOURCE_OF_TRUTH.to_string(),
        });
    }
    validate_feedback_stream(&records)?;
    Ok(records)
}

pub(super) fn validate_feedback_stream(
    records: &[HarnessFeedbackRecord],
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for (index, record) in records.iter().enumerate() {
        if record.feedback_id.trim().is_empty()
            || record.stream_index != index as u64 + 1
            || record.source.trim().is_empty()
            || record.severity.trim().is_empty()
            || record.code.trim().is_empty()
            || record.message.trim().is_empty()
            || record.remediation.trim().is_empty()
            || record.actual.trim().is_empty()
            || record.expected.trim().is_empty()
            || record.location.trim().is_empty()
            || record.source_of_truth.trim().is_empty()
        {
            return invalid_harness(
                "feedback.record",
                "feedback records require identity, ordered index, code, message, remediation, actual, expected, location, and source of truth",
            );
        }
        if !valid_code(&record.code) {
            return invalid_harness(
                "feedback.code",
                "feedback code must be non-empty and single-line",
            );
        }
        if !seen.insert(feedback_record_key(record)) {
            return invalid_harness(
                "feedback.dedupe",
                "duplicate feedback record survived dedupe",
            );
        }
    }
    Ok(())
}

fn normalize_input(input: HarnessFeedbackInput) -> Result<HarnessFeedbackInput, EngineError> {
    let code = normalize_code(&input.code)?;
    Ok(HarnessFeedbackInput {
        remediation: non_empty_or(input.remediation, || remediation_for_code(&code)),
        message: non_empty_or(input.message, || {
            if code == UNKNOWN_FEEDBACK_CODE {
                "Unclassified harness failure captured by the feedback stream".to_string()
            } else {
                format!("{code} failed")
            }
        }),
        actual: non_empty_or(input.actual, || "unclassified actual state".to_string()),
        expected: non_empty_or(input.expected, || "classified successful state".to_string()),
        location: non_empty_or(input.location, || "unknown.harness.location".to_string()),
        code,
    })
}

fn normalize_code(raw: &str) -> Result<String, EngineError> {
    let code = raw.trim();
    if code.is_empty() {
        return Ok(UNKNOWN_FEEDBACK_CODE.to_string());
    }
    if !valid_code(code) {
        return invalid_harness(
            "feedback.code",
            "feedback code must be non-empty and single-line",
        );
    }
    Ok(code.to_string())
}

fn non_empty_or(value: String, fallback: impl FnOnce() -> String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback()
    } else {
        trimmed.to_string()
    }
}

fn valid_code(code: &str) -> bool {
    let trimmed = code.trim();
    !trimmed.is_empty()
        && !trimmed.contains(['\n', '\r', '\t'])
        && trimmed.chars().all(|ch| !ch.is_control())
}

fn feedback_key(input: &HarnessFeedbackInput) -> (String, String, String, String, String) {
    (
        input.code.clone(),
        input.message.clone(),
        input.actual.clone(),
        input.expected.clone(),
        input.location.clone(),
    )
}

fn feedback_record_key(record: &HarnessFeedbackRecord) -> (String, String, String, String, String) {
    (
        record.code.clone(),
        record.message.clone(),
        record.actual.clone(),
        record.expected.clone(),
        record.location.clone(),
    )
}

fn oracle_code(oracle: HarnessOracleKind) -> &'static str {
    match oracle {
        HarnessOracleKind::DeterministicReplay => "LITD_HARNESS_ORACLE_REPLAY_DIVERGED",
        HarnessOracleKind::PresentationNonEmpty => "LITD_HARNESS_ORACLE_PRESENTATION_EMPTY",
        HarnessOracleKind::NoErrorVerdict => "LITD_HARNESS_ORACLE_ERROR_VERDICT",
        HarnessOracleKind::CrashFree => "LITD_HARNESS_ORACLE_CRASH_DETECTED",
        HarnessOracleKind::SoftLockFree => "LITD_HARNESS_ORACLE_SOFT_LOCK",
        HarnessOracleKind::PhysicsBounds => "LITD_HARNESS_ORACLE_PHYSICS_BOUNDS",
        HarnessOracleKind::PerfBudget => "LITD_HARNESS_ORACLE_PERF_BUDGET",
        HarnessOracleKind::EconomyRewardSanity => "LITD_HARNESS_ORACLE_ECONOMY_REWARD",
        HarnessOracleKind::SoakStable => "LITD_HARNESS_ORACLE_SOAK_DEGRADATION",
    }
}

fn oracle_location(oracle: HarnessOracleKind) -> String {
    match oracle {
        HarnessOracleKind::DeterministicReplay => "oracle.deterministic_replay",
        HarnessOracleKind::PresentationNonEmpty => "oracle.presentation_non_empty",
        HarnessOracleKind::NoErrorVerdict => "oracle.no_error_verdict",
        HarnessOracleKind::CrashFree => "oracle.crash_free",
        HarnessOracleKind::SoftLockFree => "oracle.soft_lock_free",
        HarnessOracleKind::PhysicsBounds => "oracle.physics_bounds",
        HarnessOracleKind::PerfBudget => "oracle.perf_budget",
        HarnessOracleKind::EconomyRewardSanity => "oracle.economy_reward_sanity",
        HarnessOracleKind::SoakStable => "oracle.soak_stable",
    }
    .to_string()
}

fn bot_status_code(status: HarnessBotOutcomeStatus) -> &'static str {
    match status {
        HarnessBotOutcomeStatus::Completed => "LITD_HARNESS_BOT_COMPLETED",
        HarnessBotOutcomeStatus::Unwinnable => "LITD_HARNESS_BOT_UNWINNABLE",
        HarnessBotOutcomeStatus::Timeout => "LITD_HARNESS_BOT_TIMEOUT",
        HarnessBotOutcomeStatus::Failed => "LITD_HARNESS_BOT_FAILED",
    }
}

fn bot_actual(record: &HarnessBotDriveRecord) -> String {
    format!(
        "status={:?} verdict={} ticks_elapsed={} final_tick={} progress_milli={} max_progress_milli={} final_hash={:016X} trajectory_hash={}",
        record.status,
        record.verdict,
        record.ticks_elapsed,
        record.final_tick,
        record.metrics.objective_progress_milli,
        record.metrics.max_objective_progress_milli,
        record.final_state_hash,
        record.trajectory_hash
    )
}

fn remediation_for_code(code: &str) -> String {
    match code {
        "LITD_HARNESS_ASSERT_BODY_NOT_FOUND" => {
            "Spawn the expected body in the authored scenario or update the assertion body_id to the persisted body that should be checked.".to_string()
        }
        "LITD_HARNESS_ASSERT_BODY_SCALAR_OUT_OF_RANGE" => {
            "Adjust the authored transform/movement data so the persisted body scalar lands inside the expected range, then rerun the same assertion.".to_string()
        }
        "LITD_HARNESS_ASSERT_VALUE_MISMATCH" => {
            "Read the persisted run snapshot at the feedback location, then either fix authored content to produce the expected value or correct the assertion if the design intent changed.".to_string()
        }
        "LITD_HARNESS_ORACLE_REPLAY_DIVERGED" => {
            "Reproduce from the initial snapshot, remove nondeterministic state from the sim path, and rerun the deterministic replay oracle.".to_string()
        }
        "LITD_HARNESS_ORACLE_PRESENTATION_EMPTY" => {
            "Ensure the authored scenario creates visible instances and draw batches before the oracle tick budget completes.".to_string()
        }
        "LITD_HARNESS_ORACLE_ERROR_VERDICT" => {
            "Inspect the run verdict and fix the failing execution path before treating the run as accepted.".to_string()
        }
        "LITD_HARNESS_ORACLE_CRASH_DETECTED" => {
            "Inspect the run error/verdict source and fix the crash before rerunning the same frame-watch oracle.".to_string()
        }
        "LITD_HARNESS_ORACLE_SOFT_LOCK" => {
            "Inspect sim progress, body spawn/progress conditions, and authored victory/fail triggers; ensure the scenario can advance before rerunning.".to_string()
        }
        "LITD_HARNESS_ORACLE_PHYSICS_BOUNDS" => {
            "Read the reported body/vector/component, clamp authored transforms or movement values to the declared physics bounds, then rerun.".to_string()
        }
        "LITD_HARNESS_ORACLE_PERF_BUDGET" => {
            "Reduce draw batches/visible instances or raise the explicit oracle perf budget in the project if the higher budget is intentional.".to_string()
        }
        "LITD_HARNESS_ORACLE_ECONOMY_REWARD" => {
            "Inspect authored reward/economy delta properties and rebalance values or raise the explicit oracle reward budget if intended.".to_string()
        }
        "LITD_HARNESS_ORACLE_SOAK_DEGRADATION" => {
            "Run the required soak duration and inspect update growth; fix leaks/unbounded work or adjust the explicit soak budget if intended.".to_string()
        }
        "LITD_HARNESS_BOT_UNWINNABLE" => {
            "Inspect the bot trajectory and authored objective; add a reachable path/objective trigger or mark the scenario intentionally unwinnable with an explicit expected outcome.".to_string()
        }
        "LITD_HARNESS_BOT_TIMEOUT" => {
            "Read the trajectory samples and either extend the explicit tick budget or fix the authored objective/path so the bot can complete within budget.".to_string()
        }
        "LITD_HARNESS_BOT_FAILED" => {
            "Inspect the bot target body, lowered orders, and replay source of truth, then fix the deterministic execution error before rerunning.".to_string()
        }
        "LITD_HARNESS_BOT_OUTCOME_MISMATCH" => {
            "Compare expected_outcome with the persisted bot status and either fix the scenario so the desired outcome happens or update the explicit expectation if the design intent changed.".to_string()
        }
        "LITD_HARNESS_BOT_REPLAY_DIVERGED" => {
            "Remove nondeterministic bot input or sim state, then rerun and verify the persisted replay record reproduces the trajectory hash.".to_string()
        }
        UNKNOWN_FEEDBACK_CODE => {
            "Inspect the actual/location fields, add a domain-specific feedback code for this failure source, and rerun so future agents receive a precise prompt.".to_string()
        }
        _ => {
            "Use the actual, expected, and location fields to patch the authored source of truth, then rerun the same harness operation.".to_string()
        }
    }
}
