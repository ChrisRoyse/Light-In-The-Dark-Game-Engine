//! Real asset-pipeline MCP tool dispatch over persisted asset state.

use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use self::mcp_asset_tools_store::{
    McpAssetState, McpAssetStateReadback, McpAssetToolLedgerReadback, McpAssetToolRecord,
    load_or_create_mcp_asset_state, write_mcp_asset_state, write_mcp_asset_tool_ledger,
};

#[path = "mcp_asset_tools_store.rs"]
pub(crate) mod mcp_asset_tools_store;
#[path = "mcp_asset_tools_types.rs"]
mod mcp_asset_tools_types;
#[path = "mcp_asset_tools_validation.rs"]
mod mcp_asset_tools_validation;

pub use mcp_asset_tools_store::{
    MCP_ASSET_LEDGER_SCHEMA, MCP_ASSET_STATE_SCHEMA, McpAssetToolLedger, read_mcp_asset_state,
    read_mcp_asset_tool_ledger, validate_mcp_asset_state, validate_mcp_asset_tool_ledger,
};
pub use mcp_asset_tools_types::{
    AssetBudgetConformance, AssetPipelineRecord, AssetPreparation, ConformIngestBudgetArgs,
    GenerateImportAssetArgs, ReadAssetsArgs, RigTextureAnimateArgs,
};
use mcp_asset_tools_validation::{
    validate_asset, validate_budget, validate_preparation, validate_request_identity,
};

pub const GENERATE_IMPORT_ASSET_TOOL: &str = "litd.editor.assets.generate_import_asset";
pub const RIG_TEXTURE_ANIMATE_TOOL: &str = "litd.editor.assets.rig_texture_animate";
pub const CONFORM_INGEST_BUDGET_TOOL: &str = "litd.editor.assets.conform_ingest_budget";
pub const READ_ASSETS_TOOL: &str = "litd.editor.assets.read_assets";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpAssetToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub state_readback: Option<McpAssetStateReadback>,
    pub ledger_readback: Option<McpAssetToolLedgerReadback>,
}

pub fn call_mcp_asset_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpAssetToolCallOutput, EngineError> {
    let route = asset_route(tool_name)?;
    match tool_name {
        GENERATE_IMPORT_ASSET_TOOL => generate_import_asset(route, arguments),
        RIG_TEXTURE_ANIMATE_TOOL => rig_texture_animate(route, arguments),
        CONFORM_INGEST_BUDGET_TOOL => conform_ingest_budget(route, arguments),
        READ_ASSETS_TOOL => read_assets(route, arguments),
        other => Err(invalid_mcp_asset(
            "tool_name",
            format!("{other} is not a registered asset-pipeline MCP tool"),
        )),
    }
}

fn generate_import_asset(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAssetToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: GenerateImportAssetArgs = parse_args(GENERATE_IMPORT_ASSET_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root BEFORE load_or_create (which can
    // create state_path) or any other fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", &args.state_path),
        ("ledger_path", &args.ledger_path),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_asset(&args.asset)?;
    let mut state = load_or_create_mcp_asset_state(&args.state_path, &args.state_id)?;
    upsert_by(
        &mut state.assets,
        |item| item.asset_id == args.asset.asset_id,
        args.asset.clone(),
    );
    state.update_count += 1;
    finish_asset_mutation(FinishAssetMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "generated or imported content-addressed asset with provenance",
        state,
        affected_ids: vec![args.asset.asset_id],
    })
}

fn rig_texture_animate(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAssetToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: RigTextureAnimateArgs = parse_args(RIG_TEXTURE_ANIMATE_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", &args.state_path),
        ("ledger_path", &args.ledger_path),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_asset_state(&args.state_path, &args.state_id)?;
    validate_preparation(&state, &args.preparation)?;
    upsert_by(
        &mut state.preparations,
        |item| item.asset_id == args.preparation.asset_id,
        args.preparation.clone(),
    );
    state.update_count += 1;
    finish_asset_mutation(FinishAssetMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "rigged, textured, animated, and marked asset runtime-ready",
        state,
        affected_ids: vec![args.preparation.asset_id],
    })
}

fn conform_ingest_budget(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAssetToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ConformIngestBudgetArgs = parse_args(CONFORM_INGEST_BUDGET_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", &args.state_path),
        ("ledger_path", &args.ledger_path),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_asset_state(&args.state_path, &args.state_id)?;
    validate_budget(&state, &args.budget)?;
    upsert_by(
        &mut state.budgets,
        |item| item.asset_id == args.budget.asset_id,
        args.budget.clone(),
    );
    state.update_count += 1;
    finish_asset_mutation(FinishAssetMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "validated render budget and Calyx ingest reference",
        state,
        affected_ids: vec![args.budget.asset_id],
    })
}

fn read_assets(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpAssetToolCallOutput, EngineError> {
    let args: ReadAssetsArgs = parse_args(READ_ASSETS_TOOL, arguments)?;
    // #1518: confine the read path to the project root before reading from disk.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[(
        "state_path",
        &args.state_path,
    )])?;
    let state_readback = read_mcp_asset_state(&args.state_path)?;
    Ok(output(
        route,
        "MCP asset-pipeline state JSON",
        json!({ "state_readback": state_readback }),
        Some(state_readback),
        None,
    ))
}

struct FinishAssetMutationArgs<'a> {
    route: McpDomainTool,
    state_path: &'a Path,
    ledger_path: &'a Path,
    ledger_id: String,
    request_id: &'a str,
    requested_by: &'a str,
    action_summary: &'a str,
    state: McpAssetState,
    affected_ids: Vec<String>,
}

fn finish_asset_mutation(
    args: FinishAssetMutationArgs<'_>,
) -> Result<McpAssetToolCallOutput, EngineError> {
    let FinishAssetMutationArgs {
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
    let state_readback = write_mcp_asset_state(state_path, &state)?;
    let record = McpAssetToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: state_path.display().to_string(),
        state_sha256: state_readback.sha256.clone(),
        state_bytes: state_readback.bytes,
        action_summary: action_summary.to_string(),
        affected_ids,
        asset_count: state_readback.asset_count,
        preparation_count: state_readback.preparation_count,
        budget_count: state_readback.budget_count,
    };
    let ledger_readback = write_mcp_asset_tool_ledger(ledger_path, ledger_id, record)?;
    Ok(output(
        route,
        "MCP asset-pipeline state JSON + asset MCP ledger JSON",
        json!({ "state_readback": state_readback }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}

fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    state_readback: Option<McpAssetStateReadback>,
    ledger_readback: Option<McpAssetToolLedgerReadback>,
) -> McpAssetToolCallOutput {
    McpAssetToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        state_readback,
        ledger_readback,
    }
}

fn asset_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "asset_pipeline")
        .ok_or_else(|| invalid_mcp_asset("domain", "missing asset_pipeline domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_asset(
                "tool_name",
                format!("{tool_name} is not registered in asset_pipeline"),
            )
        })
}

fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != READ_ASSETS_TOOL
    {
        return Err(invalid_mcp_asset(
            "route",
            format!("{} is not a guarded asset-pipeline route", route.tool_name),
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
        invalid_mcp_asset(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

pub(super) fn invalid_mcp_asset(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_asset_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_asset_tools",
        field = %field,
        reason = %reason,
        "invalid asset-pipeline MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "mcp_asset_tools_tests.rs"]
mod tests;
