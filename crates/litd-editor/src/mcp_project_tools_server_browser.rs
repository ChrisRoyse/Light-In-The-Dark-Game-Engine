use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

#[path = "mcp_project_server_browser_ops.rs"]
mod mcp_project_server_browser_ops;
#[path = "mcp_project_server_browser_store.rs"]
mod mcp_project_server_browser_store;
#[path = "mcp_project_server_browser_types.rs"]
mod mcp_project_server_browser_types;
#[path = "mcp_project_server_browser_validation.rs"]
mod mcp_project_server_browser_validation;

pub use self::{
    mcp_project_server_browser_store::read_server_browser_state,
    mcp_project_server_browser_types::{
        SERVER_BROWSER_STATE_SCHEMA, SERVER_BROWSER_TOOL, ServerBrowserAction, ServerBrowserArgs,
        ServerBrowserListingRecord, ServerBrowserQueryRecord, ServerBrowserResultRecord,
        ServerBrowserSort, ServerBrowserState, ServerBrowserStateReadback, ServerVisibility,
    },
};

use self::{
    mcp_project_server_browser_ops::{publish_listing, query_listings},
    mcp_project_server_browser_store::{
        load_or_create_server_browser_state, write_server_browser_state,
    },
    mcp_project_server_browser_validation::validate_server_browser_args,
};
use super::{
    McpProjectToolCallReadback, READ_PROJECT_TOOL, mcp_project_tools_store::McpProjectToolRecord,
    mcp_project_tools_store::load_or_create_mcp_project_state,
    mcp_project_tools_store::write_mcp_project_state,
    mcp_project_tools_store::write_mcp_project_tool_ledger, parse_args, validate_mutating_route,
};

pub(super) fn manage_server_browser(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: ServerBrowserArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        (
            "server_browser_state_path",
            std::path::Path::new(&args.server_browser_state_path),
        ),
        (
            "multiplayer_state_path",
            std::path::Path::new(&args.multiplayer_state_path),
        ),
        (
            "moderation_state_path",
            std::path::Path::new(&args.moderation_state_path),
        ),
        (
            "social_state_path",
            std::path::Path::new(&args.social_state_path),
        ),
    ])?;
    validate_server_browser_args(&args)?;

    let mut browser_state = load_or_create_server_browser_state(
        &args.server_browser_state_path,
        &args.server_browser_state_id,
        &args.multiplayer_state_path,
        &args.moderation_state_path,
        &args.social_state_path,
    )?;
    let result = match args.action {
        ServerBrowserAction::PublishListing => publish_listing(&args, &mut browser_state)?,
        ServerBrowserAction::QueryListings => query_listings(&args, &mut browser_state)?,
    };
    refresh_counts(&mut browser_state);
    let browser_readback =
        write_server_browser_state(&args.server_browser_state_path, &browser_state)?;

    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    state.update_count = state.update_count.saturating_add(1);
    let state_readback = write_mcp_project_state(&args.state_path, &state)?;
    let ledger_readback = write_mcp_project_tool_ledger(
        &args.ledger_path,
        &args.ledger_id,
        McpProjectToolRecord {
            request_id: args.request_id.clone(),
            requested_by: args.requested_by.clone(),
            tool_name: route.tool_name.clone(),
            operation_id: route.operation_id.clone(),
            state_path: state_readback.path.clone(),
            state_sha256: state_readback.sha256.clone(),
            state_bytes: state_readback.bytes,
            action_summary:
                "managed local public/private server browser listings and query readbacks"
                    .to_string(),
            affected_ids: affected_ids(&browser_readback.state),
            project_count: state_readback.project_count,
            playtest_count: state_readback.playtest_count,
            package_count: state_readback.package_count,
            published_count: state_readback.published_count,
        },
    )?;

    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth:
            "Server browser JSON + multiplayer state JSON + moderation state JSON + social graph JSON"
                .to_string(),
        result: json!({
            "server_browser_state_readback": browser_readback,
            "operation": result
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

fn refresh_counts(state: &mut ServerBrowserState) {
    state.listing_count = state.listings.len();
    state.public_listing_count = state
        .listings
        .iter()
        .filter(|listing| listing.visibility == ServerVisibility::Public)
        .count();
    state.private_listing_count = state
        .listings
        .iter()
        .filter(|listing| listing.visibility == ServerVisibility::PrivateInviteOnly)
        .count();
    state.query_count = state.queries.len();
}

fn affected_ids(state: &ServerBrowserState) -> Vec<String> {
    let mut ids = state
        .listings
        .iter()
        .map(|listing| listing.listing_id.clone())
        .chain(state.queries.iter().map(|query| query.query_id.clone()))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.push(state.state_id.clone());
    }
    ids
}
