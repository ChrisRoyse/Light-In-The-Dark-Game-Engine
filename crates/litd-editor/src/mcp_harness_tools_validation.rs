use std::collections::HashSet;

use litd_engine::{EngineError, math::Fixed};

use super::{
    AssertHarnessArgs, CalyxSignalsHarnessArgs, EmitFeedbackHarnessArgs, HarnessAssertion,
    HarnessCalyxAuthoredContent, HarnessMutationContext, HarnessOracleKind,
    HarnessPevLoopTurnInput, HarnessRunUntilCondition, ReadHarnessEventsArgs,
    RunHeadlessHarnessArgs, RunOraclesHarnessArgs, RunPevHarnessArgs, ScreenshotDiffHarnessArgs,
    SnapshotBranchHarnessArgs,
};

pub fn validate_context(context: &HarnessMutationContext) -> Result<(), EngineError> {
    validate_token("request_id", &context.request_id)?;
    validate_token("requested_by", &context.requested_by)?;
    validate_token("state_id", &context.state_id)?;
    validate_token("ledger_id", &context.ledger_id)?;
    validate_path_string("state_path", &context.state_path)?;
    validate_path_string("ledger_path", &context.ledger_path)?;
    validate_path_string("aster_dir", &context.aster_dir)
}

pub fn validate_run_args(args: &RunHeadlessHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("run_id", &args.run_id)?;
    validate_path_string("project_path", &args.project_path)?;
    validate_ticks(args.ticks, "ticks")?;
    if let Some(condition) = &args.until_condition {
        validate_until_condition(condition)?;
    }
    Ok(())
}

pub fn validate_snapshot_args(args: &SnapshotBranchHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("run_id", &args.run_id)?;
    validate_token("snapshot_id", &args.snapshot_id)?;
    validate_token("branch_id", &args.branch_id)?;
    validate_ticks(args.branch_ticks, "branch_ticks")
}

pub fn validate_assert_args(args: &AssertHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("verdict_id", &args.verdict_id)?;
    validate_token("run_id", &args.run_id)?;
    if args.assertions.is_empty() {
        return invalid_harness("assertions", "at least one assertion is required");
    }
    for assertion in &args.assertions {
        validate_assertion(assertion)?;
    }
    Ok(())
}

pub fn validate_oracle_args(args: &RunOraclesHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("oracle_run_id", &args.oracle_run_id)?;
    validate_token("run_id", &args.run_id)?;
    if args.oracles.is_empty() {
        return invalid_harness("oracles", "at least one oracle is required");
    }
    for oracle in &args.oracles {
        validate_oracle(*oracle)?;
    }
    Ok(())
}

pub fn validate_screenshot_args(args: &ScreenshotDiffHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("diff_id", &args.diff_id)?;
    validate_path_string("baseline_path", &args.baseline_path)?;
    validate_path_string("candidate_path", &args.candidate_path)?;
    if !args.max_mean_abs_error.is_finite() || args.max_mean_abs_error < 0.0 {
        return invalid_harness(
            "screenshot.max_mean_abs_error",
            "max_mean_abs_error must be finite and non-negative",
        );
    }
    if args.render_reference_scene {
        let width = args.render_width.ok_or_else(|| {
            invalid_harness::<()>("screenshot.render_width", "render_width is required")
                .unwrap_err()
        })?;
        let height = args.render_height.ok_or_else(|| {
            invalid_harness::<()>("screenshot.render_height", "render_height is required")
                .unwrap_err()
        })?;
        if width < 320 || height < 180 {
            return invalid_harness(
                "screenshot.render_dimensions",
                "rendered reference-scene screenshots require at least 320x180",
            );
        }
    }
    Ok(())
}

pub fn validate_calyx_args(args: &CalyxSignalsHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("signal_batch_id", &args.signal_batch_id)?;
    if args.run_id.is_none() && args.authored_content.is_empty() {
        return invalid_harness(
            "calyx_signals.input",
            "run_id or authored_content is required",
        );
    }
    if let Some(run_id) = &args.run_id {
        validate_token("run_id", run_id)?;
    }
    if args.authored_content.len() > 32 {
        return invalid_harness(
            "calyx_signals.authored_content",
            "at most 32 authored content checks are allowed in one batch",
        );
    }
    for content in &args.authored_content {
        validate_calyx_authored_content(content)?;
    }
    Ok(())
}

pub fn validate_emit_feedback_args(args: &EmitFeedbackHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("feedback_id", &args.feedback_id)?;
    if args.feedback.is_empty() {
        return invalid_harness("feedback", "at least one feedback record is required");
    }
    if args.feedback.len() > 64 {
        return invalid_harness("feedback", "at most 64 feedback records are allowed");
    }
    Ok(())
}

pub fn validate_pev_args(args: &RunPevHarnessArgs) -> Result<(), EngineError> {
    validate_context(&args.context)?;
    validate_token("pev_id", &args.pev_id)?;
    validate_token("run_id", &args.run_id)?;
    validate_path_string("project_path", &args.project_path)?;
    validate_ticks(args.ticks, "ticks")?;
    if args.assertions.is_empty() || args.oracles.is_empty() {
        return invalid_harness("pev.criteria", "PEV requires assertions and oracles");
    }
    for assertion in &args.assertions {
        validate_assertion(assertion)?;
    }
    for oracle in &args.oracles {
        validate_oracle(*oracle)?;
    }
    if args.turns.is_some()
        || args.transcript_id.is_some()
        || args.intent.is_some()
        || args.acceptance_criteria.is_some()
        || args.max_turns.is_some()
    {
        validate_pev_loop_args(args)?;
    }
    Ok(())
}

pub fn validate_read_events_args(args: &ReadHarnessEventsArgs) -> Result<(), EngineError> {
    validate_path_string("state_path", &args.state_path)?;
    if matches!(args.limit, Some(0)) {
        return invalid_harness("events.limit", "limit must be greater than zero");
    }
    Ok(())
}

pub fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_harness(field, "value must be non-empty and single-line");
    }
    Ok(())
}

pub fn validate_path_string(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
        return invalid_harness(field, "path must be non-empty and single-line");
    }
    Ok(())
}

fn validate_ticks(ticks: u64, field: &str) -> Result<(), EngineError> {
    if ticks > 1_000_000 {
        return invalid_harness(field, "tick count exceeds the harness safety limit");
    }
    Ok(())
}

fn validate_until_condition(condition: &HarnessRunUntilCondition) -> Result<(), EngineError> {
    match condition {
        HarnessRunUntilCondition::TickAtLeast { tick } => validate_ticks(*tick, "until.tick"),
        HarnessRunUntilCondition::BodyCountAtLeast { body_count } => {
            if *body_count > 16 {
                return invalid_harness(
                    "until.body_count",
                    "body_count exceeds the fixed headless sim capacity",
                );
            }
            Ok(())
        }
        HarnessRunUntilCondition::StateHashEquals { .. } => Ok(()),
    }
}

fn validate_pev_loop_args(args: &RunPevHarnessArgs) -> Result<(), EngineError> {
    let transcript_id = required_option("pev.transcript_id", args.transcript_id.as_deref())?;
    validate_token("pev.transcript_id", transcript_id)?;
    validate_required_text("pev.intent", args.intent.as_deref())?;
    let criteria = args
        .acceptance_criteria
        .as_ref()
        .ok_or_else(|| invalid_harness::<()>("pev.acceptance_criteria", "required").unwrap_err())?;
    if criteria.is_empty() {
        return invalid_harness(
            "pev.acceptance_criteria",
            "at least one acceptance criterion is required",
        );
    }
    for (index, criterion) in criteria.iter().enumerate() {
        validate_text(&format!("pev.acceptance_criteria[{index}]"), criterion)?;
    }
    let max_turns = args
        .max_turns
        .ok_or_else(|| invalid_harness::<()>("pev.max_turns", "required").unwrap_err())?;
    if max_turns == 0 || max_turns > 64 {
        return invalid_harness("pev.max_turns", "max_turns must be between 1 and 64");
    }
    let turns = args
        .turns
        .as_ref()
        .ok_or_else(|| invalid_harness::<()>("pev.turns", "required").unwrap_err())?;
    if turns.is_empty() {
        return invalid_harness("pev.turns", "at least one loop turn is required");
    }
    if turns.len() > max_turns as usize {
        return invalid_harness("pev.turns", "turn count exceeds max_turns");
    }
    let mut ids = HashSet::new();
    for turn in turns {
        validate_pev_loop_turn(turn, &mut ids)?;
    }
    Ok(())
}

fn validate_pev_loop_turn(
    turn: &HarnessPevLoopTurnInput,
    ids: &mut HashSet<String>,
) -> Result<(), EngineError> {
    validate_token("pev.turn.turn_id", &turn.turn_id)?;
    if !ids.insert(turn.turn_id.clone()) {
        return invalid_harness("pev.turn.turn_id", "duplicate turn_id");
    }
    validate_text("pev.turn.author_delta", &turn.author_delta)?;
    if turn.changed_paths.is_empty() {
        return invalid_harness(
            "pev.turn.changed_paths",
            "at least one changed path is required",
        );
    }
    for path in &turn.changed_paths {
        validate_path_string("pev.turn.changed_path", path)?;
    }
    validate_ticks(turn.ticks, "pev.turn.ticks")?;
    if turn.assertions.is_empty() || turn.oracles.is_empty() {
        return invalid_harness("pev.turn.criteria", "turn requires assertions and oracles");
    }
    for assertion in &turn.assertions {
        validate_assertion(assertion)?;
    }
    for oracle in &turn.oracles {
        validate_oracle(*oracle)?;
    }
    Ok(())
}

fn required_option<'a>(field: &str, value: Option<&'a str>) -> Result<&'a str, EngineError> {
    value.ok_or_else(|| invalid_harness::<()>(field, "required").unwrap_err())
}

fn validate_required_text(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_text(field, required_option(field, value)?)
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\r', '\0']) {
        return invalid_harness(
            field,
            "text must be non-empty and must not contain control bytes",
        );
    }
    Ok(())
}

fn validate_assertion(assertion: &HarnessAssertion) -> Result<(), EngineError> {
    validate_token("assertion.assertion_id", &assertion.assertion_id)?;
    match assertion.kind {
        super::HarnessAssertionKind::FinalTickEquals
        | super::HarnessAssertionKind::FinalHashEquals
        | super::HarnessAssertionKind::SpawnedBodiesEquals => {
            if assertion.expected_u64.is_none() {
                return invalid_harness("assertion.expected_u64", "expected_u64 is required");
            }
        }
        super::HarnessAssertionKind::BodiesUpdatedAtLeast => {
            if assertion.min_u64.is_none() {
                return invalid_harness("assertion.min_u64", "min_u64 is required");
            }
        }
        super::HarnessAssertionKind::VerdictEquals => {
            if assertion
                .expected_string
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return invalid_harness("assertion.expected_string", "expected_string is required");
            }
        }
        super::HarnessAssertionKind::BodyExists => {
            if assertion.body_id.is_none() {
                return invalid_harness("assertion.body_id", "body_id is required");
            }
        }
        super::HarnessAssertionKind::BodyScalarInRange => {
            validate_body_scalar_assertion(assertion)?;
        }
    }
    Ok(())
}

fn validate_body_scalar_assertion(assertion: &HarnessAssertion) -> Result<(), EngineError> {
    if assertion.body_id.is_none() {
        return invalid_harness("assertion.body_id", "body_id is required");
    }
    if assertion.metric.is_none() {
        return invalid_harness("assertion.metric", "metric is required");
    }
    if assertion.axis.is_none() {
        return invalid_harness("assertion.axis", "axis is required");
    }
    let min = assertion.min_milli.ok_or_else(|| {
        invalid_harness::<()>("assertion.min_milli", "min_milli is required").unwrap_err()
    })?;
    let max = assertion.max_milli.ok_or_else(|| {
        invalid_harness::<()>("assertion.max_milli", "max_milli is required").unwrap_err()
    })?;
    if min > max {
        return invalid_harness("assertion.range", "min_milli must be <= max_milli");
    }
    Fixed::from_ratio(min, 1000).map_err(|error| {
        invalid_harness::<()>("assertion.min_milli", error.to_string()).unwrap_err()
    })?;
    Fixed::from_ratio(max, 1000).map_err(|error| {
        invalid_harness::<()>("assertion.max_milli", error.to_string()).unwrap_err()
    })?;
    Ok(())
}

fn validate_oracle(oracle: HarnessOracleKind) -> Result<(), EngineError> {
    match oracle {
        HarnessOracleKind::DeterministicReplay
        | HarnessOracleKind::PresentationNonEmpty
        | HarnessOracleKind::NoErrorVerdict
        | HarnessOracleKind::CrashFree
        | HarnessOracleKind::SoftLockFree
        | HarnessOracleKind::PhysicsBounds
        | HarnessOracleKind::PerfBudget
        | HarnessOracleKind::EconomyRewardSanity
        | HarnessOracleKind::SoakStable => Ok(()),
    }
}

fn validate_calyx_authored_content(
    content: &HarnessCalyxAuthoredContent,
) -> Result<(), EngineError> {
    validate_token("calyx_signals.content_id", &content.content_id)?;
    validate_token("calyx_signals.request_id", &content.request.request_id)?;
    validate_token("calyx_signals.requested_by", &content.request.requested_by)?;
    validate_text("calyx_signals.prompt", &content.request.prompt)?;
    validate_text("calyx_signals.corpus.world_id", &content.corpus.world_id)?;
    validate_text(
        "calyx_signals.corpus.revision_id",
        &content.corpus.revision_id,
    )?;
    validate_text("calyx_signals.corpus.region", &content.corpus.region)?;
    if content.corpus.factions.is_empty()
        || content.corpus.lore_terms.is_empty()
        || content.corpus.anchors.is_empty()
    {
        return invalid_harness(
            "calyx_signals.corpus",
            "corpus requires factions, lore terms, and anchors",
        );
    }
    validate_token("calyx_signals.assay_id", &content.assay.assay_id)?;
    validate_token("calyx_signals.assay_profile", &content.assay.profile_id)?;
    if content.assay.min_damage > content.assay.max_damage
        || content.assay.min_cooldown_seconds > content.assay.max_cooldown_seconds
        || content.assay.max_mana_cost == 0
        || content.assay.allowed_damage_types.is_empty()
    {
        return invalid_harness("calyx_signals.assay", "Assay bounds must be valid");
    }
    validate_token("calyx_signals.ward_policy", &content.ward.policy_id)?;
    if content.ward.min_source_anchors == 0 || content.ward.min_grounded_terms == 0 {
        return invalid_harness("calyx_signals.ward", "Ward thresholds must be non-zero");
    }
    validate_token("calyx_signals.ability_id", &content.candidate.ability_id)?;
    validate_text(
        "calyx_signals.ability_name",
        &content.candidate.display_name,
    )?;
    validate_text("calyx_signals.summary", &content.candidate.summary)?;
    validate_text("calyx_signals.damage_type", &content.candidate.damage_type)?;
    if content.candidate.tags.is_empty() {
        return invalid_harness("calyx_signals.tags", "candidate requires at least one tag");
    }
    if content.oracle.min_honesty_milli > 1000 {
        return invalid_harness("calyx_signals.oracle", "min_honesty_milli must be <= 1000");
    }
    validate_text("calyx_signals.panel_template", &content.panel.template_name)?;
    validate_text("calyx_signals.panel_source", &content.panel.source_ref)?;
    validate_text(
        "calyx_signals.lodestar_question",
        &content.lodestar_question,
    )?;
    validate_text("calyx_signals.sextant_query", &content.sextant_query)?;
    if let Some(id) = &content.similar_to_content_id {
        validate_token("calyx_signals.similar_to_content_id", id)?;
    }
    Ok(())
}

pub fn invalid_harness<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    let field = format!("editor.mcp_harness_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_harness_tools",
        field = %field,
        reason = %reason,
        "invalid agent harness MCP tool call"
    );
    Err(EngineError::UiRenderInvalidConfig { field, reason })
}
