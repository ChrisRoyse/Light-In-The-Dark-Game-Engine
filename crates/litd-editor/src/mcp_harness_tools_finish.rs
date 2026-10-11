//! Shared mutation finalization for harness tools.

use litd_engine::EngineError;
use serde_json::Value;

use crate::mcp_tools::McpDomainTool;

use super::{
    HarnessEventRecord, HarnessFeedbackRecord, HarnessMutationContext, HarnessPevTranscriptTurn,
    McpHarnessState, McpHarnessToolCallReadback,
    mcp_harness_tools_helpers::output,
    mcp_harness_tools_store::{write_mcp_harness_ledger, write_mcp_harness_state},
    mcp_harness_tools_validation::validate_context,
};

pub(super) fn finish_mutation(
    route: McpDomainTool,
    context: HarnessMutationContext,
    state: McpHarnessState,
    summary: &str,
    affected_ids: Vec<String>,
    result: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    finish_mutation_with_transcript_and_feedback(FinishMutationArgs {
        route,
        context,
        state,
        summary,
        affected_ids,
        result,
        transcript_turns: Vec::new(),
        feedback: Vec::new(),
    })
}

pub(super) fn finish_mutation_with_feedback(
    route: McpDomainTool,
    context: HarnessMutationContext,
    state: McpHarnessState,
    summary: &str,
    affected_ids: Vec<String>,
    result: Value,
    feedback: Vec<HarnessFeedbackRecord>,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    finish_mutation_with_transcript_and_feedback(FinishMutationArgs {
        route,
        context,
        state,
        summary,
        affected_ids,
        result,
        transcript_turns: Vec::new(),
        feedback,
    })
}

pub(crate) struct FinishMutationArgs<'a> {
    pub(crate) route: McpDomainTool,
    pub(crate) context: HarnessMutationContext,
    pub(crate) state: McpHarnessState,
    pub(crate) summary: &'a str,
    pub(crate) affected_ids: Vec<String>,
    pub(crate) result: Value,
    pub(crate) transcript_turns: Vec<HarnessPevTranscriptTurn>,
    pub(crate) feedback: Vec<HarnessFeedbackRecord>,
}

pub(super) fn finish_mutation_with_transcript_and_feedback(
    args: FinishMutationArgs<'_>,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    let FinishMutationArgs {
        route,
        context,
        mut state,
        summary,
        affected_ids,
        result,
        transcript_turns,
        feedback,
    } = args;
    validate_context(&context)?;
    state.update_count = state.update_count.saturating_add(1);
    let event = HarnessEventRecord {
        event_index: state.update_count,
        request_id: context.request_id.clone(),
        requested_by: context.requested_by.clone(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        summary: summary.to_string(),
        affected_ids,
        state_update_count: state.update_count,
        feedback,
    };
    state.events.push(event.clone());
    let state_readback = write_mcp_harness_state(&context.state_path, &state)?;
    let ledger_readback = write_mcp_harness_ledger(
        &context.ledger_path,
        &context.aster_dir,
        context.ledger_id,
        event,
        &state_readback,
        transcript_turns,
    )?;
    Ok(output(
        route,
        "MCP agent harness state JSON + ledger JSON + Aster ledger row",
        result,
        Some(state_readback),
        Some(ledger_readback),
    ))
}
