//! Real trigger/ECA and runtime MCP tool dispatch over persisted editor state.

use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths;
use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use self::mcp_trigger_tools_store::{
    McpSoundRecordReadback, McpSoundRuntimeReadback, McpTriggerStateReadback,
    McpTriggerToolLedgerReadback, McpTriggerToolRecord, load_or_create_mcp_trigger_state,
    write_mcp_trigger_state, write_mcp_trigger_tool_ledger,
};

#[path = "mcp_trigger_tools_sound.rs"]
mod mcp_trigger_tools_sound;
#[path = "mcp_trigger_tools_store.rs"]
pub(crate) mod mcp_trigger_tools_store;
#[path = "mcp_trigger_tools_types.rs"]
mod mcp_trigger_tools_types;
#[path = "mcp_trigger_tools_validation.rs"]
mod mcp_trigger_tools_validation;

pub use mcp_trigger_tools_store::{
    MCP_TRIGGER_LEDGER_SCHEMA, MCP_TRIGGER_STATE_SCHEMA, McpTriggerRuntimeState,
    McpTriggerToolLedger, read_mcp_sound_record, read_mcp_sound_runtime, read_mcp_trigger_state,
    read_mcp_trigger_tool_ledger, validate_mcp_trigger_state, validate_mcp_trigger_tool_ledger,
};
pub use mcp_trigger_tools_types::{
    CrossRefObjectKind, CrossRefQuery, EcaGraph, EcaNode, ImportSoundArgs, PreviewSoundArgs,
    QueryCrossRefArgs, ReadSoundEditorArgs, ReadTriggerRuntimeArgs, RuntimeCue, RuntimeCueKind,
    SetTriggerOrganizationArgs, TriggerValueType, TriggerVariable, TriggerVariableScope,
    UpsertEcaGraphArgs, UpsertRuntimeCueArgs, UpsertSoundVariableArgs, UpsertVariableArgs,
    WasmEscape,
};
pub(crate) use mcp_trigger_tools_validation::validate_trigger_variable;
use mcp_trigger_tools_validation::{
    validate_eca_graph, validate_request_identity, validate_runtime_cue,
};

#[path = "mcp_trigger_tools_organization.rs"]
mod mcp_trigger_tools_organization;
use mcp_trigger_tools_organization::{query_cross_ref, set_trigger_organization};

pub const UPSERT_VARIABLE_TOOL: &str = "litd.editor.triggers.upsert_variable";
pub const UPSERT_ECA_GRAPH_TOOL: &str = "litd.editor.triggers.upsert_eca_graph";
pub const UPSERT_RUNTIME_CUE_TOOL: &str = "litd.editor.runtime.upsert_runtime_cue";
pub const IMPORT_SOUND_TOOL: &str = "litd.editor.runtime.import_sound";
pub const UPSERT_SOUND_VARIABLE_TOOL: &str = "litd.editor.runtime.upsert_sound_variable";
pub const PREVIEW_SOUND_TOOL: &str = "litd.editor.runtime.preview_sound";
pub const READ_TRIGGER_RUNTIME_TOOL: &str = "litd.editor.triggers.read_trigger_runtime";
pub const READ_SOUND_EDITOR_TOOL: &str = "litd.editor.runtime.read_sound_editor";
pub const SET_TRIGGER_ORGANIZATION_TOOL: &str = "litd.editor.triggers.set_trigger_organization";
pub const QUERY_CROSS_REF_TOOL: &str = "litd.editor.triggers.query_cross_ref";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTriggerToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub trigger_readback: Option<McpTriggerStateReadback>,
    pub sound_readback: Option<McpSoundRecordReadback>,
    pub sound_runtime_readback: Option<McpSoundRuntimeReadback>,
    pub ledger_readback: Option<McpTriggerToolLedgerReadback>,
}

pub fn call_mcp_trigger_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    let route = trigger_route(tool_name)?;
    match tool_name {
        UPSERT_VARIABLE_TOOL => upsert_variable(route, arguments),
        UPSERT_ECA_GRAPH_TOOL => upsert_eca_graph(route, arguments),
        UPSERT_RUNTIME_CUE_TOOL => upsert_runtime_cue(route, arguments),
        IMPORT_SOUND_TOOL => mcp_trigger_tools_sound::import_sound(route, arguments),
        UPSERT_SOUND_VARIABLE_TOOL => {
            mcp_trigger_tools_sound::upsert_sound_variable(route, arguments)
        }
        PREVIEW_SOUND_TOOL => mcp_trigger_tools_sound::preview_sound(route, arguments),
        READ_TRIGGER_RUNTIME_TOOL => read_trigger_runtime(route, arguments),
        READ_SOUND_EDITOR_TOOL => mcp_trigger_tools_sound::read_sound_editor(route, arguments),
        SET_TRIGGER_ORGANIZATION_TOOL => set_trigger_organization(route, arguments),
        QUERY_CROSS_REF_TOOL => query_cross_ref(route, arguments),
        other => Err(invalid_mcp_trigger(
            "tool_name",
            format!("{other} is not a registered trigger/runtime MCP tool"),
        )),
    }
}

fn upsert_variable(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertVariableArgs = parse_args(UPSERT_VARIABLE_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_trigger_variable(&args.variable)?;
    let mut state = load_or_create_mcp_trigger_state(&args.state_path, &args.state_id)?;
    upsert_by(
        &mut state.variables,
        |variable| variable.name == args.variable.name,
        args.variable.clone(),
    );
    state.update_count += 1;
    let variable_name = args.variable.name;
    finish_trigger_mutation(FinishTriggerMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted typed trigger variable",
        state,
        affected_ids: vec![variable_name],
    })
}

fn upsert_eca_graph(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertEcaGraphArgs = parse_args(UPSERT_ECA_GRAPH_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_trigger_state(&args.state_path, &args.state_id)?;
    let mut graph = args.graph;
    if let Some(escape) = graph.wasm_escape.as_mut() {
        crate::trigger_wasm_authoring::materialize_wasm_escape(escape)?;
    }
    validate_eca_graph(&state, &graph)?;
    upsert_by(
        &mut state.graphs,
        |existing| existing.trigger_id == graph.trigger_id,
        graph.clone(),
    );
    state.update_count += 1;
    let trigger_id = graph.trigger_id;
    finish_trigger_mutation(FinishTriggerMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted validated ECA trigger graph",
        state,
        affected_ids: vec![trigger_id],
    })
}

fn upsert_runtime_cue(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertRuntimeCueArgs = parse_args(UPSERT_RUNTIME_CUE_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_trigger_state(&args.state_path, &args.state_id)?;
    validate_runtime_cue(&state, &args.cue)?;
    upsert_by(
        &mut state.runtime_cues,
        |cue| cue.cue_id == args.cue.cue_id,
        args.cue.clone(),
    );
    state.update_count += 1;
    let cue_id = args.cue.cue_id;
    finish_trigger_mutation(FinishTriggerMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted validated runtime cue",
        state,
        affected_ids: vec![cue_id],
    })
}

fn read_trigger_runtime(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    let args: ReadTriggerRuntimeArgs = parse_args(READ_TRIGGER_RUNTIME_TOOL, arguments)?;
    // #1518: confine the read path to the project root before reading from disk.
    confine_tool_paths(&[("state_path", args.state_path.as_path())])?;
    let trigger_readback = read_mcp_trigger_state(&args.state_path)?;
    Ok(output(
        route,
        "MCP trigger/runtime state JSON",
        json!({ "trigger_readback": trigger_readback }),
        Some(trigger_readback),
        None,
        None,
        None,
    ))
}

struct FinishTriggerMutationArgs<'a> {
    route: McpDomainTool,
    state_path: &'a Path,
    ledger_path: &'a Path,
    ledger_id: String,
    request_id: &'a str,
    requested_by: &'a str,
    action_summary: &'a str,
    state: McpTriggerRuntimeState,
    affected_ids: Vec<String>,
}

fn finish_trigger_mutation(
    args: FinishTriggerMutationArgs<'_>,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    let FinishTriggerMutationArgs {
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
    let trigger_readback = write_mcp_trigger_state(state_path, &state)?;
    let record = trigger_record(TriggerRecordArgs {
        request_id,
        requested_by,
        route: &route,
        primary_path: state_path,
        primary_sha256: &trigger_readback.sha256,
        primary_bytes: trigger_readback.bytes,
        action_summary,
        affected_ids,
        trigger_counts: &trigger_readback,
        sound_counts: None,
    });
    let ledger_readback = write_mcp_trigger_tool_ledger(ledger_path, ledger_id, record)?;
    Ok(output(
        route,
        "MCP trigger/runtime state JSON + trigger MCP ledger JSON",
        json!({ "trigger_readback": trigger_readback }),
        Some(trigger_readback),
        None,
        None,
        Some(ledger_readback),
    ))
}

pub(crate) struct TriggerRecordArgs<'a> {
    pub(crate) request_id: &'a str,
    pub(crate) requested_by: &'a str,
    pub(crate) route: &'a McpDomainTool,
    pub(crate) primary_path: &'a Path,
    pub(crate) primary_sha256: &'a str,
    pub(crate) primary_bytes: u64,
    pub(crate) action_summary: &'a str,
    pub(crate) affected_ids: Vec<String>,
    pub(crate) trigger_counts: &'a McpTriggerStateReadback,
    pub(crate) sound_counts: Option<&'a McpSoundRecordReadback>,
}

pub(super) fn trigger_record(args: TriggerRecordArgs<'_>) -> McpTriggerToolRecord {
    let TriggerRecordArgs {
        request_id,
        requested_by,
        route,
        primary_path,
        primary_sha256,
        primary_bytes,
        action_summary,
        affected_ids,
        trigger_counts,
        sound_counts,
    } = args;
    McpTriggerToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        primary_path: primary_path.display().to_string(),
        primary_sha256: primary_sha256.to_string(),
        primary_bytes,
        action_summary: action_summary.to_string(),
        affected_ids,
        variable_count: trigger_counts.variable_count,
        graph_count: trigger_counts.graph_count,
        runtime_cue_count: trigger_counts.runtime_cue_count,
        sound_library_count: sound_counts
            .map(|readback| readback.library_count)
            .unwrap_or(0),
        named_sound_count: sound_counts
            .map(|readback| readback.named_sound_count)
            .unwrap_or(0),
        sound_binding_count: sound_counts
            .map(|readback| readback.trigger_binding_count)
            .unwrap_or(0),
    }
}

pub(super) fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    trigger_readback: Option<McpTriggerStateReadback>,
    sound_readback: Option<McpSoundRecordReadback>,
    sound_runtime_readback: Option<McpSoundRuntimeReadback>,
    ledger_readback: Option<McpTriggerToolLedgerReadback>,
) -> McpTriggerToolCallOutput {
    McpTriggerToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        trigger_readback,
        sound_readback,
        sound_runtime_readback,
        ledger_readback,
    }
}

fn trigger_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "trigger_runtime")
        .ok_or_else(|| invalid_mcp_trigger("domain", "missing trigger_runtime domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_trigger(
                "tool_name",
                format!("{tool_name} is not registered in trigger_runtime"),
            )
        })
}

pub(super) fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
    {
        return Err(invalid_mcp_trigger(
            "route",
            format!("{} is not a guarded trigger/runtime route", route.tool_name),
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

pub(super) fn empty_trigger_counts() -> McpTriggerStateReadback {
    McpTriggerStateReadback {
        path: String::new(),
        bytes: 1,
        seq: 0,
        sha256: "0".repeat(64),
        state_fingerprint: String::new(),
        state: McpTriggerRuntimeState {
            schema: MCP_TRIGGER_STATE_SCHEMA.to_string(),
            issue: 920,
            state_id: "sound".to_string(),
            source_of_truth: "sound editor record".to_string(),
            update_count: 0,
            variables: Vec::new(),
            graphs: Vec::new(),
            runtime_cues: Vec::new(),
        },
        variable_count: 0,
        graph_count: 0,
        enabled_graph_count: 0,
        runtime_cue_count: 0,
        wasm_escape_count: 0,
    }
}

pub(super) fn parse_args<T: DeserializeOwned>(
    tool_name: &str,
    value: Value,
) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_trigger(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

pub(super) fn invalid_mcp_trigger(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_trigger_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_trigger_tools",
        field = %field,
        reason = %reason,
        "invalid trigger/runtime MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "mcp_trigger_tools_tests.rs"]
mod tests;
