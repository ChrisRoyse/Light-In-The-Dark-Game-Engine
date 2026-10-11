use serde_json::json;

use crate::mcp_project_tools::{
    MODERATION_SAFETY_TOOL, SERVER_BROWSER_TOOL, call_mcp_project_tool,
    read_moderation_safety_state, read_multiplayer_session_state, read_server_browser_state,
    read_social_graph_state,
};

#[path = "mcp_project_server_browser_test_support.rs"]
mod server_browser_test_support;

use self::server_browser_test_support::{
    access_rule_args, browser_summary, prepare_browser_fixture, publish_args, query_args,
    write_report,
};

#[test]
fn issue1042_server_browser_public_private_fsv() {
    let paths = prepare_browser_fixture("unit", true, true);
    let before = browser_summary(&paths);
    println!(
        "issue1042 browser before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        publish_args(
            &paths,
            "listing.social.public",
            "lobby.social",
            "public",
            650,
        ),
    )
    .unwrap();
    call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        publish_args(
            &paths,
            "listing.private.invite",
            "lobby.private",
            "private_invite_only",
            900,
        ),
    )
    .unwrap();
    call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        query_args(&paths, "query.public", false, None, "name_asc"),
    )
    .unwrap();
    call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        query_args(
            &paths,
            "query.friend.private",
            true,
            Some("account.friend"),
            "rp_strictness_desc",
        ),
    )
    .unwrap();

    let browser = read_server_browser_state(&paths.browser_state_path).unwrap();
    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).unwrap();
    let moderation = read_moderation_safety_state(&paths.moderation_state_path).unwrap();
    let social = read_social_graph_state(&paths.social_state_path).unwrap();
    assert_eq!(browser.state.listing_count, 2);
    assert_eq!(browser.state.public_listing_count, 1);
    assert_eq!(browser.state.private_listing_count, 1);
    assert_eq!(browser.state.query_count, 2);
    assert_eq!(browser.state.queries[0].result_count, 1);
    assert_eq!(browser.state.queries[0].hidden_private_count, 1);
    assert_eq!(browser.state.queries[1].result_count, 2);
    assert_eq!(browser.state.queries[1].hidden_private_count, 0);
    assert_eq!(
        browser.state.queries[1].results[0].listing_id,
        "listing.private.invite"
    );
    assert_eq!(
        browser.state.queries[1].results[0].invite_id.as_deref(),
        Some("invite.private.001")
    );
    assert!(
        browser.state.queries[1].results[0]
            .connect_string
            .contains("invite=")
    );
    assert_eq!(multiplayer.state.server_count, 2);
    assert_eq!(multiplayer.state.running_server_count, 2);
    assert_eq!(moderation.state.roleplay_policy_count, 2);
    assert_eq!(moderation.state.active_roleplay_policy_count, 2);
    assert_eq!(social.state.invite_count, 1);
    assert_eq!(social.state.connection_count, 1);
    assert!(browser.state.listings.iter().all(|listing| {
        listing.source_multiplayer_sha256 == multiplayer.sha256
            && listing.source_moderation_sha256 == moderation.sha256
            && listing.source_social_sha256 == social.sha256
    }));

    let after = browser_summary(&paths);
    println!(
        "issue1042 browser after {}",
        serde_json::to_string(&after).unwrap()
    );
    write_report(
        &paths,
        "happy_path",
        json!({ "before": before, "after": after, "server_browser_state": browser.state }),
    );
}

#[test]
fn issue1042_server_browser_edges_fail_closed_without_state_drift() {
    let missing_server_paths = prepare_browser_fixture("edge_no_server", false, false);
    let missing_before = browser_summary(&missing_server_paths);
    let missing_error = call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        publish_args(
            &missing_server_paths,
            "listing.no.server",
            "lobby.social",
            "public",
            650,
        ),
    )
    .unwrap_err()
    .to_string();
    let missing_after = browser_summary(&missing_server_paths);
    assert_eq!(missing_before, missing_after);
    println!(
        "issue1042 edge missing_server before={} after={} error={}",
        missing_before, missing_after, missing_error
    );

    let no_invite_paths = prepare_browser_fixture("edge_private_no_invite", true, false);
    let invite_before = browser_summary(&no_invite_paths);
    let invite_error = call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        publish_args(
            &no_invite_paths,
            "listing.private.noinvite",
            "lobby.private",
            "private_invite_only",
            900,
        ),
    )
    .unwrap_err()
    .to_string();
    let invite_after = browser_summary(&no_invite_paths);
    assert_eq!(invite_before, invite_after);
    println!(
        "issue1042 edge private_no_invite before={} after={} error={}",
        invite_before, invite_after, invite_error
    );

    let allow_paths = prepare_browser_fixture("edge_public_allowlist", true, true);
    call_mcp_project_tool(MODERATION_SAFETY_TOOL, access_rule_args(&allow_paths)).unwrap();
    let allow_before = browser_summary(&allow_paths);
    let allow_error = call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        publish_args(
            &allow_paths,
            "listing.public.allowlist",
            "lobby.social",
            "public",
            650,
        ),
    )
    .unwrap_err()
    .to_string();
    let allow_after = browser_summary(&allow_paths);
    assert_eq!(allow_before, allow_after);
    println!(
        "issue1042 edge public_allowlist before={} after={} error={}",
        allow_before, allow_after, allow_error
    );

    let query_paths = prepare_browser_fixture("edge_private_query_viewer", true, true);
    call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        publish_args(
            &query_paths,
            "listing.private.invite",
            "lobby.private",
            "private_invite_only",
            900,
        ),
    )
    .unwrap();
    let query_before = browser_summary(&query_paths);
    let query_error = call_mcp_project_tool(
        SERVER_BROWSER_TOOL,
        query_args(
            &query_paths,
            "query.private.no.viewer",
            true,
            None,
            "name_asc",
        ),
    )
    .unwrap_err()
    .to_string();
    let query_after = browser_summary(&query_paths);
    assert_eq!(query_before, query_after);
    println!(
        "issue1042 edge private_query_viewer before={} after={} error={}",
        query_before, query_after, query_error
    );
    write_report(
        &query_paths,
        "edge_cases",
        json!({
            "missing_server": { "before": missing_before, "after": missing_after, "error": missing_error },
            "private_no_invite": { "before": invite_before, "after": invite_after, "error": invite_error },
            "public_allowlist": { "before": allow_before, "after": allow_after, "error": allow_error },
            "private_query_viewer": { "before": query_before, "after": query_after, "error": query_error }
        }),
    );
}
