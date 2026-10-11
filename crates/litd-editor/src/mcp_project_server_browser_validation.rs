use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::super::mcp_project_tools_validation::{
    validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_server_browser_types::{
    SERVER_BROWSER_STATE_SCHEMA, ServerBrowserAction, ServerBrowserArgs,
    ServerBrowserListingRecord, ServerBrowserQueryRecord, ServerBrowserResultRecord,
    ServerBrowserState, ServerVisibility,
};

pub fn validate_server_browser_args(args: &ServerBrowserArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path(
        "server_browser_state_path",
        Path::new(&args.server_browser_state_path),
    )?;
    validate_path(
        "multiplayer_state_path",
        Path::new(&args.multiplayer_state_path),
    )?;
    validate_path(
        "moderation_state_path",
        Path::new(&args.moderation_state_path),
    )?;
    validate_path("social_state_path", Path::new(&args.social_state_path))?;
    validate_id("server_browser_state_id", &args.server_browser_state_id)?;
    match args.action {
        ServerBrowserAction::PublishListing => {
            require_id("listing_id", args.listing_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            require_line("server_name", args.server_name.as_deref())?;
            require_line("ruleset_summary", args.ruleset_summary.as_deref())?;
            args.visibility.ok_or_else(|| {
                server_browser_error("visibility", "server visibility is required")
            })?;
        }
        ServerBrowserAction::QueryListings => {
            require_id("query_id", args.query_id.as_deref())?;
            args.sort
                .ok_or_else(|| server_browser_error("sort", "server browser sort is required"))?;
            if args.include_private {
                require_id("viewer_account_id", args.viewer_account_id.as_deref())?;
            }
            for tag in &args.required_tags {
                validate_id("required_tags", tag)?;
            }
        }
    }
    Ok(())
}

pub fn validate_server_browser_state(state: &ServerBrowserState) -> Result<(), EngineError> {
    if state.schema != SERVER_BROWSER_STATE_SCHEMA
        || state.issue != 1042
        || state.state_id.trim().is_empty()
        || state.source_of_truth != "Calyx Aster server browser state"
        || state.multiplayer_state_path.trim().is_empty()
        || state.moderation_state_path.trim().is_empty()
        || state.social_state_path.trim().is_empty()
        || state.listing_count != state.listings.len()
        || state.public_listing_count
            != state
                .listings
                .iter()
                .filter(|listing| listing.visibility == ServerVisibility::Public)
                .count()
        || state.private_listing_count
            != state
                .listings
                .iter()
                .filter(|listing| listing.visibility == ServerVisibility::PrivateInviteOnly)
                .count()
        || state.query_count != state.queries.len()
    {
        return Err(server_browser_error(
            "state",
            "schema, issue, source, paths, and counts are invalid",
        ));
    }
    let mut listings = BTreeSet::new();
    let mut listing_lobbies = BTreeSet::new();
    for listing in &state.listings {
        validate_listing(listing)?;
        if !listings.insert(listing.listing_id.as_str()) {
            return Err(server_browser_error("listings", "duplicate listing_id"));
        }
        if !listing_lobbies.insert(listing.lobby_id.as_str()) {
            return Err(server_browser_error("listings", "duplicate lobby_id"));
        }
    }
    let mut queries = BTreeSet::new();
    for query in &state.queries {
        validate_query(query, &listings)?;
        if !queries.insert(query.query_id.as_str()) {
            return Err(server_browser_error("queries", "duplicate query_id"));
        }
    }
    Ok(())
}

pub fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(server_browser_error(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

pub fn server_browser_error(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> EngineError {
    let field = format!("editor.mcp_project_tools.server_browser.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::server_browser",
        field = %field,
        reason = %reason,
        code = "LITD-SERVER-BROWSER-VALIDATION",
        "server browser operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn validate_listing(record: &ServerBrowserListingRecord) -> Result<(), EngineError> {
    validate_id("listing.listing_id", &record.listing_id)?;
    validate_id("listing.lobby_id", &record.lobby_id)?;
    validate_id("listing.server_id", &record.server_id)?;
    validate_single_line("listing.server_name", &record.server_name)?;
    validate_single_line("listing.title", &record.title)?;
    validate_single_line("listing.version", &record.version)?;
    validate_id("listing.content_address", &record.content_address)?;
    validate_id("listing.project_id", &record.project_id)?;
    validate_id("listing.package_id", &record.package_id)?;
    validate_single_line("listing.ruleset_summary", &record.ruleset_summary)?;
    validate_id("listing.rp_policy_id", &record.rp_policy_id)?;
    validate_single_line("listing.rp_policy_mode", &record.rp_policy_mode)?;
    validate_id("listing.host_player_id", &record.host_player_id)?;
    validate_single_line("listing.endpoint_host", &record.endpoint_host)?;
    validate_single_line("listing.endpoint_path", &record.endpoint_path)?;
    validate_id("listing.listed_by_account_id", &record.listed_by_account_id)?;
    validate_sha256(
        "listing.source_multiplayer_sha256",
        &record.source_multiplayer_sha256,
    )?;
    validate_sha256(
        "listing.source_moderation_sha256",
        &record.source_moderation_sha256,
    )?;
    validate_sha256("listing.source_social_sha256", &record.source_social_sha256)?;
    for tag in &record.tags {
        validate_id("listing.tags", tag)?;
    }
    if record.server_name.len() > 80
        || record.ruleset_summary.len() > 160
        || record.max_players == 0
        || record.current_players > record.max_players
        || record.open_slots != record.max_players - record.current_players
        || record.endpoint_port == 0
        || !record.endpoint_path.starts_with("/lobby/")
    {
        return Err(server_browser_error(
            "listing",
            "listing metadata, counts, or endpoint are invalid",
        ));
    }
    match record.visibility {
        ServerVisibility::Public if record.public_connect_string.is_none() => {
            return Err(server_browser_error(
                "listing.public_connect_string",
                "public listings require public connect strings",
            ));
        }
        ServerVisibility::PrivateInviteOnly if record.private_invite_count == 0 => {
            return Err(server_browser_error(
                "listing.private_invite_count",
                "private listings require at least one invite",
            ));
        }
        _ => {}
    }
    if let Some(connect) = record.public_connect_string.as_deref() {
        validate_connect_string("listing.public_connect_string", connect)?;
    }
    Ok(())
}

fn validate_query(
    query: &ServerBrowserQueryRecord,
    listings: &BTreeSet<&str>,
) -> Result<(), EngineError> {
    validate_id("query.query_id", &query.query_id)?;
    if let Some(account) = query.viewer_account_id.as_deref() {
        validate_id("query.viewer_account_id", account)?;
    }
    for tag in &query.required_tags {
        validate_id("query.required_tags", tag)?;
    }
    if query.result_count != query.results.len() {
        return Err(server_browser_error(
            "query.result_count",
            "query result_count must match results",
        ));
    }
    for result in &query.results {
        validate_result(result)?;
        if !listings.contains(result.listing_id.as_str()) {
            return Err(server_browser_error(
                "query.listing_id",
                "query result must reference a listing",
            ));
        }
    }
    Ok(())
}

fn validate_result(record: &ServerBrowserResultRecord) -> Result<(), EngineError> {
    validate_id("result.listing_id", &record.listing_id)?;
    validate_id("result.lobby_id", &record.lobby_id)?;
    validate_id("result.server_id", &record.server_id)?;
    validate_single_line("result.server_name", &record.server_name)?;
    validate_single_line("result.title", &record.title)?;
    validate_single_line("result.ruleset_summary", &record.ruleset_summary)?;
    validate_single_line("result.rp_policy_mode", &record.rp_policy_mode)?;
    validate_connect_string("result.connect_string", &record.connect_string)?;
    if let Some(invite_id) = record.invite_id.as_deref() {
        validate_id("result.invite_id", invite_id)?;
    }
    if record.max_players == 0
        || record.current_players > record.max_players
        || record.open_slots != record.max_players - record.current_players
    {
        return Err(server_browser_error("result", "result counts are invalid"));
    }
    Ok(())
}

fn validate_connect_string(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !value.starts_with("litd://join/") || value.chars().any(char::is_whitespace) {
        return Err(server_browser_error(
            field,
            "connect string must be a parseable litd join URI",
        ));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !litd_hash::is_sha256_hex(value) {
        return Err(server_browser_error(
            field,
            "value must be a 64-character lowercase SHA-256 hex",
        ));
    }
    Ok(())
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_id(
        field,
        value.ok_or_else(|| server_browser_error(field, "required field missing"))?,
    )
}

fn require_line(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_single_line(
        field,
        value.ok_or_else(|| server_browser_error(field, "required field missing"))?,
    )
}
