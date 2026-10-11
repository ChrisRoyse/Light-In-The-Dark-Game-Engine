use std::fs;

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::mcp_project_tools::{
    MULTIPLAYER_SESSION_TOOL, call_mcp_project_tool, read_multiplayer_session_state,
    read_publish_catalog,
};

use crate::mcp_project_social_test_support::{FixturePaths, prepare_multiplayer_fixture_for_issue};

#[test]
fn issue1038_authoritative_server_hosting_fsv() {
    let paths = prepare_server_fixture("unit");
    let before = server_summary(&paths);
    println!(
        "issue1038 server before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        start_server_args(&paths, "server.ashen_reach.500", "lobby.authoritative.500"),
    )
    .unwrap();

    let readback = read_multiplayer_session_state(&paths.multiplayer_state_path).unwrap();
    let state = &readback.state;
    let server = state
        .servers
        .iter()
        .find(|server| server.server_id == "server.ashen_reach.500")
        .unwrap();
    let installed_bytes = fs::read(&server.installed_package_path).unwrap();

    assert_eq!(state.server_count, 1);
    assert_eq!(state.running_server_count, 1);
    assert_eq!(server.lobby_id, "lobby.authoritative.500");
    assert_eq!(server.max_players, 500);
    assert_eq!(server.calyx_agent_count, 500);
    assert_eq!(server.deterministic_tick_rate_hz, 64);
    assert_eq!(server.simulation_probe_ticks, 4);
    assert_eq!(server.simulation_final_tick, 4);
    assert_eq!(
        server.installed_package_sha256,
        sha256_hex(&installed_bytes)
    );
    assert_eq!(server.installed_package_bytes, installed_bytes.len());
    assert!(server.runtime_lock_verified);
    assert!(server.playable_on_litd_runtime);
    assert!(server.calyx_agent_fleet_enabled);
    assert_eq!(server.calyx_agent_budget_tier_count, 3);
    assert!(server.calyx_agent_budget_all_tiers_within_budget);
    assert_ne!(server.simulation_final_state_hash, 0);
    let proof = server.net_authority_proof.as_ref().unwrap();
    assert!(proof.proves_server_authority());
    assert_eq!(proof.authority_mode, "server_authoritative");
    assert_eq!(proof.client_contract, "inputs_only");
    assert_eq!(proof.transport_contract, "snapshot_delta_stream");
    assert_eq!(proof.connected_clients, 2);
    assert_eq!(proof.accepted_inputs, 2);
    assert_eq!(proof.rejected_inputs, 1);
    assert_eq!(proof.corrected_forged_states, 1);
    assert_eq!(proof.rejected_forged_positions, 1);
    assert_eq!(proof.rejected_forged_hit_points, 1);
    assert_eq!(proof.rejected_forged_items, 1);
    assert_eq!(proof.lockstep_divergence_rejections, 1);
    assert_eq!(proof.server_truth.len(), 2);
    assert!(proof.forged_claim_rejections[0].position_rejected);
    assert!(proof.forged_claim_rejections[0].hit_points_rejected);
    assert!(proof.forged_claim_rejections[0].item_rejected);

    let report = json!({
        "issue": 1318,
        "source_of_truth": "Calyx Aster multiplayer session state row with persisted net-authority proof",
        "multiplayer_state_path": &readback.path,
        "multiplayer_state_sha256": &readback.sha256,
        "server_id": &server.server_id,
        "net_authority_proof": proof
    });
    let report_readback = paths
        .fsv_run
        .write_json_file(
            &paths.root.join("issue1318-net-authority-readback.json"),
            &report,
        )
        .unwrap();
    println!(
        "issue1318 net_authority_readback path={} sha256={}",
        report_readback.path.display(),
        report_readback.sha256
    );

    let after = server_summary(&paths);
    println!(
        "issue1038 server after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1038_authoritative_server_edges_fail_closed_without_state_drift() {
    let missing_lobby_paths = prepare_server_fixture("edge_missing_lobby");
    let missing_before = server_summary(&missing_lobby_paths);
    let missing_error = call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        start_server_args(
            &missing_lobby_paths,
            "server.missing_lobby",
            "lobby.missing",
        ),
    )
    .unwrap_err()
    .to_string();
    let missing_after = server_summary(&missing_lobby_paths);
    assert_eq!(missing_before, missing_after);
    println!(
        "issue1038 edge missing_lobby before={} after={} error={}",
        missing_before, missing_after, missing_error
    );

    let capacity_paths = prepare_server_fixture("edge_capacity");
    let capacity_before = server_summary(&capacity_paths);
    let capacity_error = call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        start_server_args_with_capacity(
            &capacity_paths,
            "server.capacity",
            "lobby.authoritative.500",
            501,
        ),
    )
    .unwrap_err()
    .to_string();
    let capacity_after = server_summary(&capacity_paths);
    assert_eq!(capacity_before, capacity_after);
    println!(
        "issue1038 edge capacity_501 before={} after={} error={}",
        capacity_before, capacity_after, capacity_error
    );

    let endpoint_paths = prepare_server_fixture("edge_endpoint");
    let endpoint_before = server_summary(&endpoint_paths);
    let endpoint_error = call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        start_server_args_with_host(
            &endpoint_paths,
            "server.endpoint",
            "lobby.authoritative.500",
            "localhost",
        ),
    )
    .unwrap_err()
    .to_string();
    let endpoint_after = server_summary(&endpoint_paths);
    assert_eq!(endpoint_before, endpoint_after);
    println!(
        "issue1038 edge endpoint_mismatch before={} after={} error={}",
        endpoint_before, endpoint_after, endpoint_error
    );

    let duplicate_paths = prepare_server_fixture("edge_duplicate");
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        start_server_args(
            &duplicate_paths,
            "server.duplicate",
            "lobby.authoritative.500",
        ),
    )
    .unwrap();
    let duplicate_before = server_summary(&duplicate_paths);
    let duplicate_error = call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        start_server_args(
            &duplicate_paths,
            "server.duplicate",
            "lobby.authoritative.500",
        ),
    )
    .unwrap_err()
    .to_string();
    let duplicate_after = server_summary(&duplicate_paths);
    assert_eq!(duplicate_before, duplicate_after);
    println!(
        "issue1038 edge duplicate_server before={} after={} error={}",
        duplicate_before, duplicate_after, duplicate_error
    );
}

fn prepare_server_fixture(label: &str) -> FixturePaths {
    let paths = prepare_multiplayer_fixture_for_issue("issue1038_authoritative_server", label);
    let content_address = content_address(&paths);
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_lobby_args(&paths, &content_address),
    )
    .unwrap();
    paths
}

fn host_lobby_args(paths: &FixturePaths, content_address: &str) -> Value {
    json!({
        "request_id": "issue1038-host-lobby-500",
        "requested_by": "litd-editor-issue1038-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1160-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1160-multiplayer-state",
        "action": "host_lobby",
        "lobby_id": "lobby.authoritative.500",
        "content_address": content_address,
        "host_player_id": "player.host",
        "max_players": 500,
        "endpoint": endpoint("127.0.0.1", "lobby.authoritative.500")
    })
}

fn start_server_args(paths: &FixturePaths, server_id: &str, lobby_id: &str) -> Value {
    start_server_args_with_capacity(paths, server_id, lobby_id, 500)
}

fn start_server_args_with_capacity(
    paths: &FixturePaths,
    server_id: &str,
    lobby_id: &str,
    max_players: usize,
) -> Value {
    start_server_args_body(paths, server_id, lobby_id, max_players, "127.0.0.1")
}

fn start_server_args_with_host(
    paths: &FixturePaths,
    server_id: &str,
    lobby_id: &str,
    host: &str,
) -> Value {
    start_server_args_body(paths, server_id, lobby_id, 500, host)
}

fn start_server_args_body(
    paths: &FixturePaths,
    server_id: &str,
    lobby_id: &str,
    max_players: usize,
    host: &str,
) -> Value {
    json!({
        "request_id": format!("issue1038-start-{server_id}"),
        "requested_by": "litd-editor-issue1038-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.root.join("multiplayer-ledger.json"),
        "ledger_id": "issue1160-multiplayer-ledger",
        "hub_state_path": paths.hub_state_path,
        "install_manifest_path": paths.install_manifest_path,
        "multiplayer_state_path": paths.multiplayer_state_path,
        "multiplayer_state_id": "issue1160-multiplayer-state",
        "action": "start_authoritative_server",
        "server_id": server_id,
        "lobby_id": lobby_id,
        "content_address": content_address(paths),
        "host_player_id": "player.host",
        "max_players": max_players,
        "endpoint": endpoint(host, lobby_id),
        "deterministic_tick_rate_hz": 64,
        "simulation_probe_ticks": 4,
        "calyx_agent_count": 500
    })
}

fn endpoint(host: &str, lobby_id: &str) -> Value {
    json!({
        "transport": "loopback",
        "host": host,
        "port": 19750,
        "path": format!("/lobby/{lobby_id}")
    })
}

fn server_summary(paths: &FixturePaths) -> Value {
    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path).ok();
    let server = multiplayer
        .as_ref()
        .and_then(|readback| readback.state.servers.last());
    json!({
        "multiplayer_state_exists": paths.multiplayer_state_path.exists(),
        "multiplayer_state_bytes": paths.multiplayer_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "multiplayer_state_sha256": fs::read(&paths.multiplayer_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "lobby_count": multiplayer.as_ref().map(|s| s.state.lobby_count).unwrap_or(0),
        "server_count": multiplayer.as_ref().map(|s| s.state.server_count).unwrap_or(0),
        "running_server_count": multiplayer.as_ref().map(|s| s.state.running_server_count).unwrap_or(0),
        "last_server_id": server.map(|s| s.server_id.clone()).unwrap_or_default(),
        "last_server_max_players": server.map(|s| s.max_players).unwrap_or(0),
        "last_server_calyx_agents": server.map(|s| s.calyx_agent_count).unwrap_or(0),
        "last_server_sim_hash": server.map(|s| s.simulation_final_state_hash).unwrap_or(0),
        "last_server_package_sha256": server.map(|s| s.installed_package_sha256.clone()).unwrap_or_default()
    })
}

fn content_address(paths: &FixturePaths) -> String {
    read_publish_catalog(&paths.catalog_path)
        .unwrap()
        .catalog
        .entries[0]
        .content_address
        .clone()
}
