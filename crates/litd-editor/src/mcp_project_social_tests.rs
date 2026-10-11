use std::{fs, path::PathBuf};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::mcp_project_tools::{
    FriendshipStatus, GameInviteStatus, SOCIAL_GRAPH_TOOL, call_mcp_project_tool,
    read_multiplayer_session_state, read_social_graph_state,
};

use crate::mcp_project_social_test_support::{
    FixturePaths, accept_friendship, accept_invite, create_account_args, create_friend,
    create_host, friend_args, invite_args, prepare_multiplayer_fixture, presence_args,
    send_friendship, send_invite, set_friend_presence, set_host_presence, social_summary,
};

#[test]
fn issue1160_social_graph_profiles_presence_invites_fsv() {
    let paths = prepare_multiplayer_fixture("unit");
    let before = social_summary(&paths);
    println!(
        "issue1160 social before {}",
        serde_json::to_string(&before).unwrap()
    );

    create_host(&paths);
    create_friend(&paths);
    send_friendship(&paths);
    accept_friendship(&paths);
    set_host_presence(&paths);
    set_friend_presence(&paths);
    send_invite(&paths);
    accept_invite(&paths);

    let social = read_social_graph_state(&paths.social_state_path).unwrap();
    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).unwrap();
    let lobby = &multiplayer.state.lobbies[0];
    let installed_path = PathBuf::from(&lobby.installed_package_path);
    let installed_bytes = fs::read(&installed_path).unwrap();
    let friendship = &social.state.friendships[0];
    let invite = &social.state.invites[0];

    assert_eq!(social.state.account_count, 2);
    assert_eq!(social.state.profile_count, 2);
    assert_eq!(social.state.friendship_count, 1);
    assert_eq!(social.state.presence_count, 2);
    assert_eq!(social.state.invite_count, 1);
    assert_eq!(friendship.status, FriendshipStatus::Accepted);
    assert_eq!(invite.status, GameInviteStatus::Accepted);
    assert_eq!(invite.lobby_id, lobby.lobby_id);
    assert_eq!(invite.content_address, lobby.content_address);
    assert_eq!(
        invite.connect_string,
        "litd://join/lobby.social?invite=invite.social.001"
    );
    assert_eq!(sha256_hex(&installed_bytes), lobby.package_sha256);
    assert!(
        lobby
            .member_ids
            .iter()
            .any(|member| member == "player.host")
    );

    let after = social_summary(&paths);
    println!(
        "issue1160 social after {}",
        serde_json::to_string(&after).unwrap()
    );
    write_report(
        &paths,
        "happy_path",
        json!({ "before": before, "after": after }),
    );
}

#[test]
fn issue1160_social_edges_fail_closed_without_state_drift() {
    let paths = prepare_multiplayer_fixture("edges");

    let tags_before = social_summary(&paths);
    let tags_error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        create_account_args(
            &paths,
            "account.bad",
            "player.bad",
            "profile.bad",
            "bad",
            &[],
        ),
    )
    .unwrap_err()
    .to_string();
    let tags_after = social_summary(&paths);
    assert_eq!(tags_before, tags_after);
    println!(
        "issue1160 edge empty_tags before={} after={} error={}",
        tags_before, tags_after, tags_error
    );

    create_host(&paths);
    create_friend(&paths);

    let self_before = social_summary(&paths);
    let self_error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        friend_args(
            &paths,
            "send_friend_request",
            "account.host",
            "account.host",
        ),
    )
    .unwrap_err()
    .to_string();
    let self_after = social_summary(&paths);
    assert_eq!(self_before, self_after);
    println!(
        "issue1160 edge self_friend before={} after={} error={}",
        self_before, self_after, self_error
    );

    let presence_before = social_summary(&paths);
    let presence_error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        presence_args(&paths, "account.friend", "in_game", Some("lobby.social")),
    )
    .unwrap_err()
    .to_string();
    let presence_after = social_summary(&paths);
    assert_eq!(presence_before, presence_after);
    println!(
        "issue1160 edge non_member_presence before={} after={} error={}",
        presence_before, presence_after, presence_error
    );

    let invite_before = social_summary(&paths);
    let invite_error =
        call_mcp_project_tool(SOCIAL_GRAPH_TOOL, invite_args(&paths, "send_lobby_invite"))
            .unwrap_err()
            .to_string();
    let invite_after = social_summary(&paths);
    assert_eq!(invite_before, invite_after);
    println!(
        "issue1160 edge invite_without_friendship before={} after={} error={}",
        invite_before, invite_after, invite_error
    );
    write_report(
        &paths,
        "edge_cases",
        json!({
            "empty_tags": { "before": tags_before, "after": tags_after, "error": tags_error },
            "self_friend": { "before": self_before, "after": self_after, "error": self_error },
            "non_member_presence": { "before": presence_before, "after": presence_after, "error": presence_error },
            "invite_without_friendship": { "before": invite_before, "after": invite_after, "error": invite_error }
        }),
    );
}

fn write_report(paths: &FixturePaths, label: &str, details: Value) {
    let social = read_social_graph_state(&paths.social_state_path).unwrap();
    let report = json!({
        "issue": 1160,
        "label": label,
        "source_of_truth": "social-graph-state.json persisted readback",
        "social_state_path": paths.social_state_path,
        "social_state_sha256": fs::read(&paths.social_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "social_state": social.state,
        "details": details
    });
    paths
        .fsv_run
        .write_json_file(&paths.root.join("manual-fsv-report.json"), &report)
        .unwrap();
}
