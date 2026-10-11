//! Real in-game agent MCP tool dispatch over persisted agent binding state.

use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use self::mcp_agent_tools_store::{
    McpAgentState, McpAgentStateReadback, McpAgentToolLedgerReadback, McpAgentToolRecord,
    load_or_create_mcp_agent_state, write_mcp_agent_state, write_mcp_agent_tool_ledger,
};

#[path = "mcp_agent_tools_authoring.rs"]
mod mcp_agent_tools_authoring;
#[path = "mcp_agent_tools_store.rs"]
pub(crate) mod mcp_agent_tools_store;
#[path = "mcp_agent_tools_types.rs"]
mod mcp_agent_tools_types;
#[path = "mcp_agent_tools_validation.rs"]
mod mcp_agent_tools_validation;

use mcp_agent_tools_authoring::author_agent;
pub use mcp_agent_tools_store::{
    MCP_AGENT_LEDGER_SCHEMA, MCP_AGENT_STATE_SCHEMA, McpAgentToolLedger, read_mcp_agent_state,
    read_mcp_agent_tool_ledger, validate_mcp_agent_state, validate_mcp_agent_tool_ledger,
};
pub use mcp_agent_tools_types::{
    AgentActivationPolicy, AgentAuthoringVisualBinding, AgentBinding, AgentPromptModel,
    AgentWardEnvelope, AuthorAgentArgs, BindAgentArgs, ReadAgentsArgs, SetPromptModelArgs,
    SetWardEnvelopeArgs, WardBehaviorEnvelope, WardPhysicsEnvelope, WardVisualEnvelope,
};
use mcp_agent_tools_validation::{
    validate_binding, validate_prompt_model, validate_request_identity, validate_ward_envelope,
};

pub const AUTHOR_AGENT_TOOL: &str = "litd.editor.agents.author_agent";
pub const BIND_AGENT_TOOL: &str = "litd.editor.agents.bind_agent";
pub const SET_PROMPT_MODEL_TOOL: &str = "litd.editor.agents.set_prompt_model";
pub const SET_WARD_ENVELOPE_TOOL: &str = "litd.editor.agents.set_ward_envelope";
pub const READ_AGENTS_TOOL: &str = "litd.editor.agents.read_agents";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAgentToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub state_readback: Option<McpAgentStateReadback>,
    pub ledger_readback: Option<McpAgentToolLedgerReadback>,
}

pub fn call_mcp_agent_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpAgentToolCallOutput, EngineError> {
    let route = agent_route(tool_name)?;
    match tool_name {
        AUTHOR_AGENT_TOOL => author_agent(route, arguments, AUTHOR_AGENT_TOOL),
        BIND_AGENT_TOOL => bind_agent(route, arguments),
        SET_PROMPT_MODEL_TOOL => set_prompt_model(route, arguments),
        SET_WARD_ENVELOPE_TOOL => set_ward_envelope(route, arguments),
        READ_AGENTS_TOOL => read_agents(route, arguments),
        other => Err(invalid_mcp_agent(
            "tool_name",
            format!("{other} is not a registered in-game agent MCP tool"),
        )),
    }
}

fn bind_agent(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAgentToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: BindAgentArgs = parse_args(BIND_AGENT_TOOL, arguments)?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_binding(&args.binding)?;
    confine_agent_paths(&args.state_path, Some(&args.ledger_path))?;
    let mut state = load_or_create_mcp_agent_state(&args.state_path, &args.state_id)?;
    if state
        .bindings
        .iter()
        .any(|binding| binding.agent_id == args.binding.agent_id)
    {
        return Err(invalid_mcp_agent(
            "binding.agent_id",
            format!("{} is already bound", args.binding.agent_id),
        ));
    }
    upsert_by(
        &mut state.bindings,
        |item| item.binding_id == args.binding.binding_id,
        args.binding.clone(),
    );
    state.update_count += 1;
    finish_agent_mutation(FinishAgentMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "bound agent brain to deterministic object/order sink",
        state,
        affected_ids: vec![args.binding.agent_id],
    })
}

fn set_prompt_model(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAgentToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetPromptModelArgs = parse_args(SET_PROMPT_MODEL_TOOL, arguments)?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    confine_agent_paths(&args.state_path, Some(&args.ledger_path))?;
    let mut state = load_or_create_mcp_agent_state(&args.state_path, &args.state_id)?;
    validate_prompt_model(&state, &args.prompt_model)?;
    upsert_by(
        &mut state.prompt_models,
        |item| item.agent_id == args.prompt_model.agent_id,
        args.prompt_model.clone(),
    );
    state.update_count += 1;
    finish_agent_mutation(FinishAgentMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "set agent prompt, model, personality, and memory scopes",
        state,
        affected_ids: vec![args.prompt_model.agent_id],
    })
}

fn set_ward_envelope(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAgentToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetWardEnvelopeArgs = parse_args(SET_WARD_ENVELOPE_TOOL, arguments)?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    confine_agent_paths(&args.state_path, Some(&args.ledger_path))?;
    let mut state = load_or_create_mcp_agent_state(&args.state_path, &args.state_id)?;
    validate_ward_envelope(&state, &args.envelope)?;
    upsert_by(
        &mut state.ward_envelopes,
        |item| item.agent_id == args.envelope.agent_id,
        args.envelope.clone(),
    );
    state.update_count += 1;
    finish_agent_mutation(FinishAgentMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "set Ward physics, visual, and behavior envelope",
        state,
        affected_ids: vec![args.envelope.agent_id],
    })
}

fn read_agents(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAgentToolCallOutput, EngineError> {
    let args: ReadAgentsArgs = parse_args(READ_AGENTS_TOOL, arguments)?;
    confine_agent_paths(&args.state_path, None)?;
    let state_readback = read_mcp_agent_state(&args.state_path)?;
    Ok(output(
        route,
        "MCP in-game agent state JSON",
        json!({ "state_readback": state_readback }),
        Some(state_readback),
        None,
    ))
}

struct FinishAgentMutationArgs<'a> {
    route: McpDomainTool,
    state_path: &'a Path,
    ledger_path: &'a Path,
    ledger_id: String,
    request_id: &'a str,
    requested_by: &'a str,
    action_summary: &'a str,
    state: McpAgentState,
    affected_ids: Vec<String>,
}

fn finish_agent_mutation(
    args: FinishAgentMutationArgs<'_>,
) -> Result<McpAgentToolCallOutput, EngineError> {
    let FinishAgentMutationArgs {
        route,
        state_path,
        ledger_path,
        ledger_id,
        request_id,
        requested_by,
        action_summary,
        state,
        affected_ids,
    } = args;
    let state_readback = write_mcp_agent_state(state_path, &state)?;
    let record = McpAgentToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: state_path.display().to_string(),
        state_sha256: state_readback.sha256.clone(),
        state_bytes: state_readback.bytes,
        action_summary: action_summary.to_string(),
        affected_ids,
        binding_count: state_readback.binding_count,
        prompt_model_count: state_readback.prompt_model_count,
        ward_envelope_count: state_readback.ward_envelope_count,
    };
    let ledger_readback = write_mcp_agent_tool_ledger(ledger_path, ledger_id, record)?;
    Ok(output(
        route,
        "MCP in-game agent state JSON + agent MCP ledger JSON",
        json!({ "state_readback": state_readback }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}

fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    state_readback: Option<McpAgentStateReadback>,
    ledger_readback: Option<McpAgentToolLedgerReadback>,
) -> McpAgentToolCallOutput {
    McpAgentToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        state_readback,
        ledger_readback,
    }
}

fn agent_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "in_game_agents")
        .ok_or_else(|| invalid_mcp_agent("domain", "missing in_game_agents domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_agent(
                "tool_name",
                format!("{tool_name} is not registered in in_game_agents"),
            )
        })
}

pub(super) fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != READ_AGENTS_TOOL
    {
        return Err(invalid_mcp_agent(
            "route",
            format!("{} is not a guarded in-game agent route", route.tool_name),
        ));
    }
    Ok(())
}

fn upsert_by<T, F>(values: &mut Vec<T>, mut predicate: F, value: T)
where
    F: FnMut(&T) -> bool,
{
    if let Some(existing) = values.iter_mut().find(|existing| predicate(existing)) {
        *existing = value;
    } else {
        values.push(value);
    }
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_agent(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

/// Confine the tool-supplied `state_path` (and optional `ledger_path`) within the trusted
/// project root BEFORE any filesystem effect (#1518). Every agent MCP handler takes these
/// paths straight from arguments and then reads/writes them via the store; without this
/// gate a poisoned request turns the agent tools into an arbitrary read/write primitive
/// outside the project directory. Same policy as `confine_delivery_paths` (#1487).
pub(super) fn confine_agent_paths(
    state_path: &Path,
    ledger_path: Option<&Path>,
) -> Result<(), EngineError> {
    use crate::mcp_project_tools::mcp_project_tools_validation::{
        confine_path, resolve_project_root,
    };
    let project_root = resolve_project_root()?;
    confine_path("state_path", state_path, &project_root)?;
    if let Some(ledger_path) = ledger_path {
        confine_path("ledger_path", ledger_path, &project_root)?;
    }
    Ok(())
}

pub(super) fn invalid_mcp_agent(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_agent_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_agent_tools",
        field = %field,
        reason = %reason,
        "invalid in-game agent MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "mcp_agent_tools_tests.rs"]
mod tests;
