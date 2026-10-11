use std::fs;

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::mcp_project_tools::{
    JoinConnectionMode, SOCIAL_GRAPH_TOOL, call_mcp_project_tool, read_multiplayer_session_state,
    read_social_graph_state,
};

use crate::mcp_project_social_test_support::{
    FixturePaths, accept_friendship, accept_invite, create_friend, create_host,
    prepare_multiplayer_fixture_for_issue, send_friendship, send_invite,
};

#[test]
fn issue1039_dead_simple_join_connections_fsv() {
    let paths = prepare_join_fixture("unit");
    let before = join_summary(&paths);
    println!(
        "issue1039 join before {}",
        serde_json::to_string(&before).unwrap()
    );

    for (connection_id, mode, host, port, invite, ssh) in [
        (
            "join.localhost",
            "localhost",
            "127.0.0.1",
            19700,
            Some("invite.social.001"),
            None,
        ),
        (
            "join.lan",
            "lan",
            "192.168.1.40",
            19700,
            Some("invite.social.001"),
            None,
        ),
        (
            "join.direct",
            "direct_ip",
            "203.0.113.10",
            19700,
            None,
            None,
        ),
        (
            "join.ssh",
            "ssh_tunnel",
            "127.0.0.1",
            22000,
            Some("invite.social.001"),
            Some(("player", "game.example.net", "127.0.0.1", 19700)),
        ),
        (
            "join.invite",
            "invite_code",
            "127.0.0.1",
            19700,
            Some("invite.social.001"),
            None,
        ),
    ] {
        call_mcp_project_tool(
            SOCIAL_GRAPH_TOOL,
            join_connection_args(&paths, connection_id, mode, host, port, invite, ssh),
        )
        .unwrap();
    }

    let social = read_social_graph_state(&paths.social_state_path).unwrap();
    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).unwrap();
    let lobby = &multiplayer.state.lobbies[0];
    let installed_bytes = fs::read(&lobby.installed_package_path).unwrap();
    let modes: Vec<_> = social
        .state
        .join_connections
        .iter()
        .map(|connection| connection.mode)
        .collect();

    assert_eq!(social.state.connection_count, 5);
    assert!(modes.contains(&JoinConnectionMode::Localhost));
    assert!(modes.contains(&JoinConnectionMode::Lan));
    assert!(modes.contains(&JoinConnectionMode::DirectIp));
    assert!(modes.contains(&JoinConnectionMode::SshTunnel));
    assert!(modes.contains(&JoinConnectionMode::InviteCode));
    assert_eq!(sha256_hex(&installed_bytes), lobby.package_sha256);
    assert!(social.state.join_connections.iter().all(|connection| {
        connection.lobby_id == lobby.lobby_id
            && connection.content_address == lobby.content_address
            && connection.path == "/lobby/lobby.social"
            && connection.source_lobby_host == lobby.endpoint.host
            && connection.source_lobby_port == lobby.endpoint.port
            && connection.source_lobby_path == lobby.endpoint.path
            && connection
                .connect_string
                .starts_with("litd://join/lobby.social?")
    }));

    let ssh = social
        .state
        .join_connections
        .iter()
        .find(|connection| connection.connection_id == "join.ssh")
        .unwrap();
    assert!(ssh.requires_existing_ssh_tunnel);
    assert_eq!(ssh.ssh_user.as_deref(), Some("player"));
    assert!(ssh.connect_string.contains("mode=ssh_tunnel"));
    assert!(ssh.connect_string.contains("remote=127.0.0.1:19700"));
    assert!(
        social
            .state
            .join_connections
            .iter()
            .filter(|connection| connection.connection_id != "join.ssh")
            .all(|connection| !connection.requires_existing_ssh_tunnel)
    );

    let after = join_summary(&paths);
    println!(
        "issue1039 join after {}",
        serde_json::to_string(&after).unwrap()
    );
    write_report(
        &paths,
        "happy_path",
        json!({ "before": before, "after": after }),
    );
}

#[test]
fn issue1039_join_connection_edges_fail_closed_without_state_drift() {
    let wildcard_paths = prepare_join_fixture("edge_wildcard_lan");
    let wildcard_before = join_summary(&wildcard_paths);
    let wildcard_error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        join_connection_args(
            &wildcard_paths,
            "join.bad.wildcard",
            "lan",
            "0.0.0.0",
            19700,
            None,
            None,
        ),
    )
    .unwrap_err()
    .to_string();
    let wildcard_after = join_summary(&wildcard_paths);
    assert_eq!(wildcard_before, wildcard_after);
    println!(
        "issue1039 edge wildcard_lan before={} after={} error={}",
        wildcard_before, wildcard_after, wildcard_error
    );

    let ssh_paths = prepare_join_fixture("edge_ssh_metadata");
    let ssh_before = join_summary(&ssh_paths);
    let ssh_error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        join_connection_args(
            &ssh_paths,
            "join.bad.ssh",
            "ssh_tunnel",
            "127.0.0.1",
            22000,
            None,
            None,
        ),
    )
    .unwrap_err()
    .to_string();
    let ssh_after = join_summary(&ssh_paths);
    assert_eq!(ssh_before, ssh_after);
    println!(
        "issue1039 edge ssh_metadata before={} after={} error={}",
        ssh_before, ssh_after, ssh_error
    );

    let mismatch_paths = prepare_join_fixture("edge_invite_mismatch");
    let mismatch_before = join_summary(&mismatch_paths);
    let mismatch_error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        join_connection_args_for_account(JoinConnectionArgsForAccountArgs {
            paths: &mismatch_paths,
            account_id: "account.host",
            connection_id: "join.bad.invite",
            mode: "invite_code",
            host: "127.0.0.1",
            port: 19700,
            invite_id: Some("invite.social.001"),
            ssh: None,
        }),
    )
    .unwrap_err()
    .to_string();
    let mismatch_after = join_summary(&mismatch_paths);
    assert_eq!(mismatch_before, mismatch_after);
    println!(
        "issue1039 edge invite_mismatch before={} after={} error={}",
        mismatch_before, mismatch_after, mismatch_error
    );

    let duplicate_paths = prepare_join_fixture("edge_duplicate");
    call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        join_connection_args(
            &duplicate_paths,
            "join.duplicate",
            "localhost",
            "127.0.0.1",
            19700,
            None,
            None,
        ),
    )
    .unwrap();
    let duplicate_before = join_summary(&duplicate_paths);
    let duplicate_error = call_mcp_project_tool(
        SOCIAL_GRAPH_TOOL,
        join_connection_args(
            &duplicate_paths,
            "join.duplicate",
            "localhost",
            "127.0.0.1",
            19700,
            None,
            None,
        ),
    )
    .unwrap_err()
    .to_string();
    let duplicate_after = join_summary(&duplicate_paths);
    assert_eq!(duplicate_before, duplicate_after);
    println!(
        "issue1039 edge duplicate before={} after={} error={}",
        duplicate_before, duplicate_after, duplicate_error
    );
    write_report(
        &duplicate_paths,
        "edge_cases",
        json!({
            "wildcard_lan": { "before": wildcard_before, "after": wildcard_after, "error": wildcard_error },
            "ssh_metadata": { "before": ssh_before, "after": ssh_after, "error": ssh_error },
            "invite_mismatch": { "before": mismatch_before, "after": mismatch_after, "error": mismatch_error },
            "duplicate": { "before": duplicate_before, "after": duplicate_after, "error": duplicate_error }
        }),
    );
}

fn prepare_join_fixture(label: &str) -> FixturePaths {
    let paths = prepare_multiplayer_fixture_for_issue("issue1039_join_connections", label);
    create_host(&paths);
    create_friend(&paths);
    send_friendship(&paths);
    accept_friendship(&paths);
    send_invite(&paths);
    accept_invite(&paths);
    paths
}

fn join_connection_args(
    paths: &FixturePaths,
    connection_id: &str,
    mode: &str,
    host: &str,
    port: u16,
    invite_id: Option<&str>,
    ssh: Option<(&str, &str, &str, u16)>,
) -> Value {
    join_connection_args_for_account(JoinConnectionArgsForAccountArgs {
        paths,
        account_id: "account.friend",
        connection_id,
        mode,
        host,
        port,
        invite_id,
        ssh,
    })
}

struct JoinConnectionArgsForAccountArgs<'a> {
    paths: &'a FixturePaths,
    account_id: &'a str,
    connection_id: &'a str,
    mode: &'a str,
    host: &'a str,
    port: u16,
    invite_id: Option<&'a str>,
    ssh: Option<(&'a str, &'a str, &'a str, u16)>,
}

fn join_connection_args_for_account(args: JoinConnectionArgsForAccountArgs<'_>) -> Value {
    let JoinConnectionArgsForAccountArgs {
        paths,
        account_id,
        connection_id,
        mode,
        host,
        port,
        invite_id,
        ssh,
    } = args;
    let mut value = json!({
        "request_id": format!("issue1039-join-{connection_id}"),
        "requested_by": "litd-editor-issue1039-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("social-ledger.json"),
        // Shared social ledger established by the fixture uses this stable id; the
        // project tool ledger now appends, so the id must match to accumulate (#1490).
        "ledger_id": "issue1160-social-ledger",
        "social_state_path": paths.social_state_path,
        "social_state_id": "issue1160-social-state",
        "multiplayer_state_path": paths.multiplayer_state_path,
        "action": "create_join_connection",
        "connection_id": connection_id,
        "account_id": account_id,
        "lobby_id": "lobby.social",
        "join_connection_mode": mode,
        "connect_host": host,
        "connect_port": port,
        "connect_path": "/lobby/lobby.social",
        "invite_id": invite_id
    });
    if let Some((user, ssh_host, remote_host, remote_port)) = ssh {
        value["ssh_user"] = json!(user);
        value["ssh_host"] = json!(ssh_host);
        value["ssh_remote_host"] = json!(remote_host);
        value["ssh_remote_port"] = json!(remote_port);
    }
    value
}

fn join_summary(paths: &FixturePaths) -> Value {
    let social = read_social_graph_state(&paths.social_state_path).ok();
    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).ok();
    json!({
        "social_state_exists": paths.social_state_path.exists(),
        "social_state_bytes": paths.social_state_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "social_state_sha256": fs::read(&paths.social_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "schema": social.as_ref().map(|readback| readback.state.schema.clone()).unwrap_or_default(),
        "issue": social.as_ref().map(|readback| readback.state.issue).unwrap_or(0),
        "account_count": social.as_ref().map(|readback| readback.state.account_count).unwrap_or(0),
        "invite_count": social.as_ref().map(|readback| readback.state.invite_count).unwrap_or(0),
        "connection_count": social.as_ref().map(|readback| readback.state.connection_count).unwrap_or(0),
        "connection_ids": social.as_ref().map(|readback| readback.state.join_connections.iter().map(|connection| connection.connection_id.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "connect_strings": social.as_ref().map(|readback| readback.state.join_connections.iter().map(|connection| connection.connect_string.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "lobby_count": multiplayer.as_ref().map(|readback| readback.state.lobby_count).unwrap_or(0),
        "lobby_endpoint": multiplayer.as_ref().and_then(|readback| readback.state.lobbies.first()).map(|lobby| format!("{}:{}{}", lobby.endpoint.host, lobby.endpoint.port, lobby.endpoint.path)).unwrap_or_default()
    })
}

fn write_report(paths: &FixturePaths, label: &str, details: Value) {
    let social = read_social_graph_state(&paths.social_state_path).unwrap();
    let report = json!({
        "issue": 1039,
        "label": label,
        "source_of_truth": "social-graph-state.json join_connections persisted readback",
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
