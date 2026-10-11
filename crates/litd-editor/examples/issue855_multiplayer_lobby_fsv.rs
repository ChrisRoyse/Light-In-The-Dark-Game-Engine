use std::{fs, path::Path};

use litd_editor::mcp_project_tools::{
    MULTIPLAYER_SESSION_TOOL, call_mcp_project_tool, read_hub_launcher_state,
    read_litdgame_install_manifest, read_multiplayer_session_state,
};
use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

#[path = "issue855_multiplayer_lobby_fsv/support.rs"]
mod support;
use support::{
    Paths, content_address, file_readback, host_args, join_args, prepare_fixture, run_root,
    start_server_args, summary,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run_root = run_root()?;
    fs::create_dir_all(&run_root)?;
    let paths = prepare_fixture(&run_root)?;
    let content_address = content_address(&paths.catalog_path)?;

    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(
            &paths,
            &content_address,
            "lobby.issue855",
            2,
            "127.0.0.1",
            "team.dawnward",
        ),
    )?;
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        join_args(&paths, "lobby.issue855", "player.guest", "team.vesper"),
    )?;

    let mut edges = vec![
        edge(
            &paths,
            "wildcard_endpoint_rejected",
            "LITD-RENDER-UI-INVALID-CONFIG",
            || {
                call_mcp_project_tool(
                    MULTIPLAYER_SESSION_TOOL,
                    host_args(
                        &paths,
                        &content_address,
                        "lobby.issue855.bad",
                        2,
                        "0.0.0.0",
                        "team.dawnward",
                    ),
                )
                .map(|_| ())
            },
        )?,
        edge(
            &paths,
            "invalid_team_rejected",
            "LITD-RENDER-UI-INVALID-CONFIG",
            || {
                call_mcp_project_tool(
                    MULTIPLAYER_SESSION_TOOL,
                    join_args(&paths, "lobby.issue855", "player.badteam", "bad team"),
                )
                .map(|_| ())
            },
        )?,
        edge(
            &paths,
            "full_lobby_extra_join_rejected",
            "LITD-RENDER-UI-INVALID-CONFIG",
            || {
                call_mcp_project_tool(
                    MULTIPLAYER_SESSION_TOOL,
                    join_args(&paths, "lobby.issue855", "player.extra", "team.vesper"),
                )
                .map(|_| ())
            },
        )?,
        edge(
            &paths,
            "mismatched_server_endpoint_rejected",
            "LITD-RENDER-UI-INVALID-CONFIG",
            || {
                call_mcp_project_tool(
                    MULTIPLAYER_SESSION_TOOL,
                    start_server_args(
                        &paths,
                        &content_address,
                        "server.issue855.bad_endpoint",
                        "lobby.issue855",
                        "localhost",
                    ),
                )
                .map(|_| ())
            },
        )?,
    ];

    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        start_server_args(
            &paths,
            &content_address,
            "server.issue855.authority",
            "lobby.issue855",
            "127.0.0.1",
        ),
    )?;
    edges.push(edge(
        &paths,
        "duplicate_server_rejected",
        "LITD-RENDER-UI-INVALID-CONFIG",
        || {
            call_mcp_project_tool(
                MULTIPLAYER_SESSION_TOOL,
                start_server_args(
                    &paths,
                    &content_address,
                    "server.issue855.authority",
                    "lobby.issue855",
                    "127.0.0.1",
                ),
            )
            .map(|_| ())
        },
    )?);

    let multiplayer = read_multiplayer_session_state(&paths.multiplayer_state_path)?;
    let hub = read_hub_launcher_state(&paths.hub_state_path)?;
    let install = read_litdgame_install_manifest(&paths.install_manifest_path)?;
    let lobby = multiplayer.state.lobbies.first().ok_or("missing lobby")?;
    let server = multiplayer.state.servers.first().ok_or("missing server")?;
    let proof = server
        .net_authority_proof
        .as_ref()
        .ok_or("missing net authority proof")?;
    let installed = file_readback(Path::new(&lobby.installed_package_path))?;
    let matched_edges = edges
        .iter()
        .filter(|edge| edge["matched"].as_bool() == Some(true))
        .count();

    let acceptance = json!({
        "lobby_count": multiplayer.state.lobby_count,
        "join_count": multiplayer.state.join_count,
        "server_count": multiplayer.state.server_count,
        "running_server_count": multiplayer.state.running_server_count,
        "lobby_id": lobby.lobby_id,
        "lobby_status": format!("{:?}", lobby.status),
        "max_players": lobby.max_players,
        "member_ids": lobby.member_ids,
        "slots": lobby.slots,
        "slot_count": lobby.slots.len(),
        "host_slot_team": lobby.slots[0].team_id,
        "guest_slot_team": lobby.slots[1].team_id,
        "package_transfer_sha_matches": installed["sha256"] == lobby.package_sha256,
        "hub_content_address_matches": hub.state.cards[0].content_address == lobby.content_address,
        "install_manifest_path_matches": install.manifest.records[0].installed_package_path == lobby.installed_package_path,
        "runtime_lock_verified": lobby.runtime_lock_verified && server.runtime_lock_verified,
        "playable_on_litd_runtime": lobby.playable_on_litd_runtime && server.playable_on_litd_runtime,
        "server_id": server.server_id,
        "authority_mode": format!("{:?}", server.authority_mode),
        "fixed_step_nanos": server.fixed_step_nanos,
        "simulation_final_tick": server.simulation_final_tick,
        "simulation_final_state_hash": server.simulation_final_state_hash,
        "calyx_agent_count": server.calyx_agent_count,
        "net_authority_proves_server": proof.proves_server_authority(),
        "matched_edge_count": matched_edges,
        "all_edges_matched": matched_edges == edges.len()
    });
    ensure_acceptance(&acceptance)?;

    let report = json!({
        "schema": "litd.editor.issue855.multiplayer_lobby_fsv.v1",
        "issue": 855,
        "source_of_truth": {
            "project_state": paths.state_path,
            "multiplayer_state": paths.multiplayer_state_path,
            "multiplayer_ledger": paths.multiplayer_ledger_path,
            "hub_state": paths.hub_state_path,
            "install_manifest": paths.install_manifest_path,
            "installed_package": lobby.installed_package_path,
            "manual_gate": "Manual Synapse/browser readback must inspect this report, persisted multiplayer state, ledger, and package hashes before close"
        },
        "acceptance": acceptance,
        "artifacts": {
            "project_state": file_readback(&paths.state_path)?,
            "multiplayer_state": file_readback(&paths.multiplayer_state_path)?,
            "multiplayer_ledger": file_readback(&paths.multiplayer_ledger_path)?,
            "hub_state": file_readback(&paths.hub_state_path)?,
            "install_manifest": file_readback(&paths.install_manifest_path)?,
            "installed_package": installed
        },
        "edges": edges,
        "sufficiency_argument": "The smallest #855 proof uses one installed package, one host lobby, one joined client, two explicit team slots, one full-lobby boundary, one authoritative server, and five fail-closed edges. This covers host/join slots, team assignment, map package transfer, server-authoritative ownership, duplicate protection, invalid endpoint/team input, full capacity, and server/lobby endpoint mismatch without adding volume that exercises no new state transition."
    });
    let report_path = run_root.join("manual-fsv-report.json");
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    println!(
        "ISSUE855_MULTIPLAYER_FSV report={} sha256={} multiplayer_sha={} package_sha={} server_hash={} edges={}/{}",
        report_path.display(),
        sha256_hex(&report_bytes),
        multiplayer.sha256,
        lobby.package_sha256,
        server.simulation_final_state_hash,
        matched_edges,
        report["edges"]
            .as_array()
            .map(|edges| edges.len())
            .unwrap_or_default()
    );
    Ok(())
}

fn ensure_acceptance(value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let required = [
        value["lobby_count"] == 1,
        value["join_count"] == 1,
        value["server_count"] == 1,
        value["running_server_count"] == 1,
        value["lobby_status"] == "Full",
        value["member_ids"] == json!(["player.host", "player.guest"]),
        value["slot_count"] == 2,
        value["host_slot_team"] == "team.dawnward",
        value["guest_slot_team"] == "team.vesper",
        value["package_transfer_sha_matches"] == true,
        value["runtime_lock_verified"] == true,
        value["playable_on_litd_runtime"] == true,
        value["authority_mode"] == "HostAuthoritative",
        value["fixed_step_nanos"] == 15_625_000,
        value["simulation_final_state_hash"]
            .as_u64()
            .unwrap_or_default()
            != 0,
        value["calyx_agent_count"] == 2,
        value["net_authority_proves_server"] == true,
        value["all_edges_matched"] == true,
    ];
    if required.iter().all(|ok| *ok) {
        Ok(())
    } else {
        Err(format!("issue855 acceptance failed: {value}").into())
    }
}

fn edge<F>(
    paths: &Paths,
    label: &str,
    expected_code: &str,
    operation: F,
) -> Result<Value, Box<dyn std::error::Error>>
where
    F: FnOnce() -> Result<(), EngineError>,
{
    let before = summary(paths);
    let result = operation();
    let after = summary(paths);
    let drift_free = before == after;
    let (actual_code, reason) = match result {
        Ok(()) => ("ok".to_string(), String::new()),
        Err(error) => (error.code().to_string(), error.to_string()),
    };
    Ok(json!({
        "label": label,
        "expected_code": expected_code,
        "actual_code": actual_code,
        "reason": reason,
        "drift_free": drift_free,
        "matched": actual_code == expected_code && drift_free,
        "before": before,
        "after": after
    }))
}
