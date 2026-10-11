//! Multi-turn Plan-Execute-Verify loop transcript builder.

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde_json::json;

use super::{
    HarnessCalyxSignalsRecord, HarnessPevRunRecord, HarnessPevTranscriptRecord,
    HarnessPevTranscriptTurn, HarnessRunRecord, RunPevHarnessArgs,
    mcp_harness_tools_assertions::build_assertion_verdict,
    mcp_harness_tools_build::build_run_record,
    mcp_harness_tools_calyx_signals::build_calyx_signals_record,
    mcp_harness_tools_helpers::upsert_by, mcp_harness_tools_oracles::build_oracle_record,
    mcp_harness_tools_store::McpHarnessState, mcp_harness_tools_validation::invalid_harness,
};

pub(super) struct PevLoopBuildOutcome {
    pub state: McpHarnessState,
    pub transcript: HarnessPevTranscriptRecord,
    pub transcript_turns: Vec<HarnessPevTranscriptTurn>,
}

struct AcceptedTurn {
    turn_id: String,
    run: HarnessRunRecord,
    pev: HarnessPevRunRecord,
}

pub(super) fn build_pev_loop_transcript(
    args: &RunPevHarnessArgs,
    mut state: McpHarnessState,
) -> Result<PevLoopBuildOutcome, EngineError> {
    let turns = args.turns.as_ref().ok_or_else(|| {
        invalid_harness::<()>("pev.turns", "PEV loop requires turns").unwrap_err()
    })?;
    let transcript_id = args.transcript_id.as_deref().ok_or_else(|| {
        invalid_harness::<()>("pev.transcript_id", "PEV loop requires transcript_id").unwrap_err()
    })?;
    let intent = args.intent.as_deref().ok_or_else(|| {
        invalid_harness::<()>("pev.intent", "PEV loop requires intent").unwrap_err()
    })?;
    let criteria = args.acceptance_criteria.clone().ok_or_else(|| {
        invalid_harness::<()>(
            "pev.acceptance_criteria",
            "PEV loop requires acceptance_criteria",
        )
        .unwrap_err()
    })?;
    let max_turns = args.max_turns.ok_or_else(|| {
        invalid_harness::<()>("pev.max_turns", "PEV loop requires max_turns").unwrap_err()
    })?;
    let mut previous_turn_hash = "0".repeat(64);
    let mut previous_proposal = None::<String>;
    let mut accepted = None::<AcceptedTurn>;
    let mut transcript_turns = Vec::with_capacity(turns.len());
    let mut final_verdict = "halted_non_converging".to_string();
    let mut halt_reason = Some("provided turns ended without a passing verdict".to_string());
    let mut reverted_to_turn_id = None::<String>;

    for (index, turn) in turns.iter().enumerate() {
        let turn_index = index as u32 + 1;
        let proposal_fingerprint = proposal_fingerprint(turn)?;
        let no_op = previous_proposal.as_deref() == Some(proposal_fingerprint.as_str());
        let rerun_scope = if index == 0 {
            "full_initial"
        } else {
            "changed_only"
        };
        if no_op {
            let record = transcript_turn(TranscriptTurnArgs {
                turn_index,
                turn_id: &turn.turn_id,
                author_delta: &turn.author_delta,
                changed_paths: &turn.changed_paths,
                rerun_scope,
                proposal_fingerprint: &proposal_fingerprint,
                records: None,
                passed: false,
                no_op: true,
                regression: false,
                applied_to_state: false,
                feedback_code: "halted_no_op",
                feedback_message: "turn repeated the previous proposal without changing the authored inputs",
                remediation: "change the failing proposal before rerunning the loop",
                previous_turn_hash: &previous_turn_hash,
            })?;
            previous_turn_hash = record.turn_hash.clone();
            transcript_turns.push(record);
            final_verdict = "halted_no_op".to_string();
            halt_reason = Some("no-op turn detected before convergence".to_string());
            break;
        }
        previous_proposal = Some(proposal_fingerprint.clone());
        let run_id = format!("{}-{}", args.run_id, turn.turn_id);
        let run = build_run_record(
            &run_id,
            &args.project_path,
            args.camera_rig,
            turn.ticks,
            None,
        )?;
        let assertions = build_assertion_verdict(
            &format!("{}-{}-assert", args.pev_id, turn.turn_id),
            &run,
            &turn.assertions,
        )?;
        let oracles = build_oracle_record(
            &format!("{}-{}-oracle", args.pev_id, turn.turn_id),
            &run,
            &turn.oracles,
        )?;
        let signals = build_calyx_signals_record(
            &format!("{}-{}-signals", args.pev_id, turn.turn_id),
            Some(&run),
            &[],
        )?;
        let passed = assertions.failed_count == 0 && oracles.failed_count == 0 && signals.admitted;
        let regression = accepted.is_some() && !passed;
        let applied_to_state = passed && !regression;
        let record = transcript_turn(TranscriptTurnArgs {
            turn_index,
            turn_id: &turn.turn_id,
            author_delta: &turn.author_delta,
            changed_paths: &turn.changed_paths,
            rerun_scope,
            proposal_fingerprint: &proposal_fingerprint,
            records: Some((
                run.clone(),
                assertions.clone(),
                oracles.clone(),
                signals.clone(),
            )),
            passed,
            no_op: false,
            regression,
            applied_to_state,
            feedback_code: feedback_code(passed, regression),
            feedback_message: feedback_message(
                passed,
                regression,
                assertions.failed_count,
                oracles.failed_count,
            ),
            remediation: remediation(passed, regression),
            previous_turn_hash: &previous_turn_hash,
        })?;
        previous_turn_hash = record.turn_hash.clone();
        transcript_turns.push(record);
        if regression {
            final_verdict = "halted_regression_reverted".to_string();
            halt_reason = Some("regression caught; reverted to last passing turn".to_string());
            reverted_to_turn_id = accepted.as_ref().map(|item| item.turn_id.clone());
            break;
        }
        if passed {
            let pev = HarnessPevRunRecord {
                pev_id: format!("{}-{}", args.pev_id, turn.turn_id),
                run_id: run.run_id.clone(),
                run_record: run.clone(),
                assertion_verdict: assertions,
                oracle_verdict: oracles,
                calyx_signals: signals,
                verdict: "passed".to_string(),
            };
            accepted = Some(AcceptedTurn {
                turn_id: turn.turn_id.clone(),
                run,
                pev,
            });
            final_verdict = "passed".to_string();
            halt_reason = None;
        }
    }

    let accepted_turn_id = accepted.as_ref().map(|item| item.turn_id.clone());
    if let Some(accepted) = accepted {
        persist_accepted(&mut state, &accepted);
    }
    let transcript = HarnessPevTranscriptRecord {
        issue: 1177,
        transcript_id: transcript_id.to_string(),
        pev_id: args.pev_id.clone(),
        intent: intent.to_string(),
        acceptance_criteria: criteria,
        max_turns,
        turn_count: transcript_turns.len(),
        final_verdict,
        halt_reason,
        accepted_turn_id,
        reverted_to_turn_id,
        final_turn_hash: previous_turn_hash,
        turns: transcript_turns.clone(),
    };
    validate_harness_pev_transcript(&transcript)?;
    upsert_by(
        &mut state.pev_transcripts,
        |item| item.transcript_id == transcript.transcript_id,
        transcript.clone(),
    );
    Ok(PevLoopBuildOutcome {
        state,
        transcript,
        transcript_turns,
    })
}

fn persist_accepted(state: &mut McpHarnessState, accepted: &AcceptedTurn) {
    upsert_by(
        &mut state.runs,
        |item| item.run_id == accepted.run.run_id,
        accepted.run.clone(),
    );
    upsert_by(
        &mut state.assertions,
        |item| item.verdict_id == accepted.pev.assertion_verdict.verdict_id,
        accepted.pev.assertion_verdict.clone(),
    );
    upsert_by(
        &mut state.oracle_runs,
        |item| item.oracle_run_id == accepted.pev.oracle_verdict.oracle_run_id,
        accepted.pev.oracle_verdict.clone(),
    );
    upsert_by(
        &mut state.calyx_signals,
        |item| item.signal_batch_id == accepted.pev.calyx_signals.signal_batch_id,
        accepted.pev.calyx_signals.clone(),
    );
    upsert_by(
        &mut state.pev_runs,
        |item| item.pev_id == accepted.pev.pev_id,
        accepted.pev.clone(),
    );
}

pub fn validate_harness_pev_transcript(
    transcript: &HarnessPevTranscriptRecord,
) -> Result<(), EngineError> {
    if transcript.issue != 1177
        || transcript.transcript_id.trim().is_empty()
        || transcript.turn_count != transcript.turns.len()
        || transcript.turns.is_empty()
    {
        return invalid_harness("pev.transcript", "transcript identity or count is invalid");
    }
    let mut previous = "0".repeat(64);
    for turn in &transcript.turns {
        if turn.previous_turn_hash != previous {
            return invalid_harness("pev.transcript.hash_chain", "previous turn hash mismatch");
        }
        let expected = turn_hash(turn)?;
        if turn.turn_hash != expected {
            return invalid_harness("pev.transcript.hash_chain", "turn hash mismatch");
        }
        previous = turn.turn_hash.clone();
    }
    if transcript.final_turn_hash != previous {
        return invalid_harness("pev.transcript.final_hash", "final turn hash mismatch");
    }
    match transcript.final_verdict.as_str() {
        "passed" if transcript.accepted_turn_id.is_some() && transcript.halt_reason.is_none() => {
            Ok(())
        }
        "halted_non_converging" if transcript.accepted_turn_id.is_none() => Ok(()),
        "halted_no_op" if transcript.turns.iter().any(|turn| turn.no_op) => Ok(()),
        "halted_regression_reverted"
            if transcript.accepted_turn_id.is_some()
                && transcript.reverted_to_turn_id.is_some() =>
        {
            Ok(())
        }
        _ => invalid_harness(
            "pev.transcript.verdict",
            "final verdict fields are internally inconsistent",
        ),
    }
}

struct TranscriptTurnArgs<'a, T0: Into<String>, T1: Into<String>, T2: Into<String>> {
    turn_index: u32,
    turn_id: &'a str,
    author_delta: &'a str,
    changed_paths: &'a [String],
    rerun_scope: &'a str,
    proposal_fingerprint: &'a str,
    records: Option<(
        HarnessRunRecord,
        super::HarnessAssertionVerdict,
        super::HarnessOracleRunRecord,
        HarnessCalyxSignalsRecord,
    )>,
    passed: bool,
    no_op: bool,
    regression: bool,
    applied_to_state: bool,
    feedback_code: T0,
    feedback_message: T1,
    remediation: T2,
    previous_turn_hash: &'a str,
}

fn transcript_turn(
    args: TranscriptTurnArgs<'_, impl Into<String>, impl Into<String>, impl Into<String>>,
) -> Result<HarnessPevTranscriptTurn, EngineError> {
    let TranscriptTurnArgs {
        turn_index,
        turn_id,
        author_delta,
        changed_paths,
        rerun_scope,
        proposal_fingerprint,
        records,
        passed,
        no_op,
        regression,
        applied_to_state,
        feedback_code,
        feedback_message,
        remediation,
        previous_turn_hash,
    } = args;
    let (run_record, assertion_verdict, oracle_verdict, calyx_signals) = records.map_or(
        (None, None, None, None),
        |(run, assertions, oracles, signals)| {
            (Some(run), Some(assertions), Some(oracles), Some(signals))
        },
    );
    let mut record = HarnessPevTranscriptTurn {
        turn_index,
        turn_id: turn_id.to_string(),
        author_delta: author_delta.to_string(),
        changed_paths: changed_paths.to_vec(),
        rerun_scope: rerun_scope.to_string(),
        proposal_fingerprint: proposal_fingerprint.to_string(),
        run_id: run_record.as_ref().map(|run| run.run_id.clone()),
        run_record,
        assertion_verdict,
        oracle_verdict,
        calyx_signals,
        passed,
        no_op,
        regression,
        applied_to_state,
        feedback_code: feedback_code.into(),
        feedback_message: feedback_message.into(),
        remediation: remediation.into(),
        previous_turn_hash: previous_turn_hash.to_string(),
        turn_hash: String::new(),
    };
    record.turn_hash = turn_hash(&record)?;
    Ok(record)
}

fn proposal_fingerprint(turn: &super::HarnessPevLoopTurnInput) -> Result<String, EngineError> {
    let bytes = serde_json::to_vec(&json!({
        "changed_paths": &turn.changed_paths,
        "ticks": turn.ticks,
        "assertions": &turn.assertions,
        "oracles": &turn.oracles,
    }))
    .map_err(|error| {
        invalid_harness::<()>("pev.turn.fingerprint", error.to_string()).unwrap_err()
    })?;
    Ok(sha256_hex(&bytes))
}

fn turn_hash(record: &HarnessPevTranscriptTurn) -> Result<String, EngineError> {
    let bytes = serde_json::to_vec(&json!({
        "turn_index": record.turn_index,
        "turn_id": &record.turn_id,
        "author_delta": &record.author_delta,
        "changed_paths": &record.changed_paths,
        "rerun_scope": &record.rerun_scope,
        "proposal_fingerprint": &record.proposal_fingerprint,
        "run_id": &record.run_id,
        "run_record": &record.run_record,
        "assertion_verdict": &record.assertion_verdict,
        "oracle_verdict": &record.oracle_verdict,
        "calyx_signals": &record.calyx_signals,
        "passed": record.passed,
        "no_op": record.no_op,
        "regression": record.regression,
        "applied_to_state": record.applied_to_state,
        "feedback_code": &record.feedback_code,
        "feedback_message": &record.feedback_message,
        "remediation": &record.remediation,
        "previous_turn_hash": &record.previous_turn_hash,
    }))
    .map_err(|error| invalid_harness::<()>("pev.turn.hash", error.to_string()).unwrap_err())?;
    Ok(sha256_hex(&bytes))
}

fn feedback_code(passed: bool, regression: bool) -> &'static str {
    if passed {
        "green"
    } else if regression {
        "regression_reverted"
    } else {
        "verification_failed"
    }
}

fn feedback_message(
    passed: bool,
    regression: bool,
    assertion_failures: usize,
    oracle_failures: usize,
) -> String {
    if passed {
        "all assertions, oracles, and Calyx signals passed".to_string()
    } else if regression {
        "candidate failed after a prior green turn; last passing turn remains authoritative"
            .to_string()
    } else {
        format!(
            "candidate failed verification: assertions_failed={assertion_failures} oracles_failed={oracle_failures}"
        )
    }
}

fn remediation(passed: bool, regression: bool) -> &'static str {
    if passed {
        "promote the green candidate"
    } else if regression {
        "inspect the failed delta and keep the previous green turn"
    } else {
        "change only the failing authored inputs and rerun the affected turn"
    }
}
