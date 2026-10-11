use std::{fs, path::Path};

use litd_engine::{
    EngineError,
    game_package::{build_runtime_game_run_report, load_runtime_locked_package},
    net_authority::build_net_authority_proof,
    sim::SimBodyId,
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    agent_latency_budget::{
        AgentLatencyBudgetRequest, AgentLatencyCacheMode, AgentLatencyTierKind,
        AgentLatencyTierPlan, FOCUSED_SCENARIO_BUDGET_MS, FULL_WORLD_SMOKE_BUDGET_MS,
        SINGLE_CHANGE_REVALIDATE_BUDGET_MS, build_agent_latency_budget_ledger,
    },
    history::stable_fingerprint,
    play::{PlayCameraRig, PlaySession},
};

use super::super::{read_litdworld_project, read_mcp_project_state};
use super::{
    mcp_project_multiplayer_types::{
        AuthoritativeServerRecord, AuthoritativeServerStatus, MultiplayerSessionArgs,
        MultiplayerSessionState, ServerAuthorityMode,
    },
    mcp_project_multiplayer_validation::multiplayer_error,
};

pub fn start_authoritative_server(
    args: &MultiplayerSessionArgs,
    state: &mut MultiplayerSessionState,
) -> Result<Value, EngineError> {
    let server_id = required(args.server_id.as_deref(), "server_id")?;
    if state
        .servers
        .iter()
        .any(|server| server.server_id == server_id)
    {
        return Err(multiplayer_error("server_id", "server already exists"));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    if state.servers.iter().any(|server| {
        server.lobby_id == lobby_id && server.status == AuthoritativeServerStatus::Running
    }) {
        return Err(multiplayer_error(
            "server.lobby_id",
            "lobby already has a running authoritative server",
        ));
    }

    let content_address = required(args.content_address.as_deref(), "content_address")?;
    let host_player_id = required(args.host_player_id.as_deref(), "host_player_id")?;
    let endpoint = args.endpoint.clone().ok_or_else(|| {
        multiplayer_error(
            "server.endpoint",
            "endpoint is required for start_authoritative_server",
        )
    })?;
    let max_players = required_usize(args.max_players, "max_players")?;
    let tick_rate = required_u16(
        args.deterministic_tick_rate_hz,
        "deterministic_tick_rate_hz",
    )?;
    let probe_ticks = required_u64(args.simulation_probe_ticks, "simulation_probe_ticks")?;
    let agent_count = required_usize(args.calyx_agent_count, "calyx_agent_count")?;

    let lobby = state
        .lobbies
        .iter()
        .find(|lobby| lobby.lobby_id == lobby_id)
        .ok_or_else(|| multiplayer_error("server.lobby_id", "lobby does not exist"))?;
    if lobby.content_address != content_address
        || lobby.host_player_id != host_player_id
        || lobby.endpoint != endpoint
        || lobby.max_players != max_players
    {
        return Err(multiplayer_error(
            "server.lobby",
            "server request must exactly match lobby content, host, endpoint, and capacity",
        ));
    }

    let installed_bytes = fs::read(&lobby.installed_package_path).map_err(|error| {
        multiplayer_error(
            "server.installed_package.read",
            format!("read {}: {error}", lobby.installed_package_path),
        )
    })?;
    let installed_sha = sha256_hex(&installed_bytes);
    if installed_sha != lobby.package_sha256 || installed_bytes.len() != lobby.package_bytes {
        return Err(multiplayer_error(
            "server.installed_package",
            "installed package bytes do not match lobby package readback",
        ));
    }

    let runtime = load_runtime_locked_package(
        Path::new(&lobby.installed_package_path),
        "mcp.project.multiplayer.server",
    )?;
    let runtime_report = build_runtime_game_run_report(&runtime);
    if runtime_report.package_id != lobby.package_id
        || runtime_report.package_sha256 != lobby.package_sha256
        || !runtime_report.runtime_lock_verified
        || !runtime_report.playable_on_litd_runtime
    {
        return Err(multiplayer_error(
            "server.runtime_report",
            "runtime package readback does not match lobby package",
        ));
    }

    let project_state = read_mcp_project_state(&args.state_path)?;
    let project_record = project_state
        .state
        .projects
        .iter()
        .find(|project| project.project_id == lobby.project_id)
        .ok_or_else(|| multiplayer_error("server.project_id", "project record does not exist"))?;
    let project = read_litdworld_project(&project_record.project_path)?;
    if project.project_id != lobby.project_id {
        return Err(multiplayer_error(
            "server.project",
            "project file readback does not match lobby project id",
        ));
    }

    let session = PlaySession::start(&project.document, PlayCameraRig::TopDown)?;
    if session.build.snapshot.config.ticks_per_second != u32::from(tick_rate) {
        return Err(multiplayer_error(
            "server.tick_rate",
            "engine play snapshot tick rate does not match requested server tick rate",
        ));
    }
    let controlled_body = session.build.spawned_bodies.first().ok_or_else(|| {
        multiplayer_error(
            "server.net_authority.body",
            "authoritative server requires a real Unit body for input ownership proof",
        )
    })?;
    let net_authority_proof = build_net_authority_proof::<1024, 16>(
        &session.build.snapshot,
        1,
        SimBodyId::from_raw(controlled_body.body_id),
    )?;
    let initial_state_hash = session.build.snapshot.state_hash;
    let mut sim = session.build.snapshot.restore::<1024>()?;
    let sim_report = sim.run_ticks(probe_ticks)?;
    let budget_ledger = build_agent_latency_budget_ledger(
        &project.document,
        agent_budget_request(
            server_id,
            &lobby.project_id,
            lobby_id,
            host_player_id,
            agent_count,
            probe_ticks,
        ),
    )?;
    let budget_readback = &budget_ledger.readback;
    let play_build_fingerprint = stable_fingerprint(&session.build)?;
    let max_elapsed_micros = (budget_readback.max_elapsed_ms * 1000.0).ceil().max(1.0) as u64;

    let server = AuthoritativeServerRecord {
        server_id: server_id.to_string(),
        lobby_id: lobby_id.to_string(),
        content_address: content_address.to_string(),
        project_id: lobby.project_id.clone(),
        package_id: lobby.package_id.clone(),
        package_sha256: lobby.package_sha256.clone(),
        package_bytes: lobby.package_bytes,
        installed_package_path: lobby.installed_package_path.clone(),
        installed_package_sha256: installed_sha,
        installed_package_bytes: installed_bytes.len(),
        endpoint,
        host_player_id: host_player_id.to_string(),
        max_players,
        status: AuthoritativeServerStatus::Running,
        authority_mode: ServerAuthorityMode::HostAuthoritative,
        deterministic_tick_rate_hz: tick_rate,
        fixed_step_nanos: 1_000_000_000_u64 / u64::from(tick_rate),
        simulation_probe_ticks: probe_ticks,
        simulation_start_tick: sim_report.start_tick,
        simulation_final_tick: sim_report.final_tick,
        simulation_initial_state_hash: initial_state_hash,
        simulation_final_state_hash: sim_report.state_hash,
        simulation_bodies_updated: sim_report.bodies_updated,
        runtime_lock_verified: runtime_report.runtime_lock_verified,
        playable_on_litd_runtime: runtime_report.playable_on_litd_runtime,
        calyx_agent_fleet_enabled: true,
        calyx_agent_count: agent_count,
        calyx_agent_budget_ledger_id: budget_ledger.ledger_id,
        calyx_agent_budget_tier_count: budget_readback.tier_count,
        calyx_agent_budget_all_tiers_within_budget: budget_readback.all_tiers_within_budget,
        calyx_agent_budget_max_elapsed_micros: max_elapsed_micros,
        net_authority_proof: Some(net_authority_proof),
        source_document_fingerprint: budget_readback.source_document_fingerprint.clone(),
        play_build_fingerprint,
        started_by: args.requested_by.clone(),
    };
    tracing::info!(
        server_id = %server.server_id,
        lobby_id = %server.lobby_id,
        max_players = server.max_players,
        calyx_agent_count = server.calyx_agent_count,
        simulation_final_state_hash = server.simulation_final_state_hash,
        code = "LITD-MULTIPLAYER-AUTHORITATIVE-SERVER-STARTED",
        "authoritative multiplayer server state verified"
    );
    state.servers.push(server.clone());
    Ok(json!({ "authoritative_server": server }))
}

fn agent_budget_request(
    server_id: &str,
    project_id: &str,
    lobby_id: &str,
    host_player_id: &str,
    agent_count: usize,
    probe_ticks: u64,
) -> AgentLatencyBudgetRequest {
    AgentLatencyBudgetRequest {
        ledger_id: format!("agent-budget.{server_id}"),
        project_id: project_id.to_string(),
        actor_id: host_player_id.to_string(),
        scenario_id: lobby_id.to_string(),
        camera_rig: PlayCameraRig::TopDown,
        tiers: vec![
            tier(
                "server.single_change.cold",
                AgentLatencyTierKind::SingleChangeRevalidate,
                AgentLatencyCacheMode::Cold,
                SINGLE_CHANGE_REVALIDATE_BUDGET_MS,
                probe_ticks,
                agent_count,
            ),
            tier(
                "server.focused.warm",
                AgentLatencyTierKind::FocusedScenario,
                AgentLatencyCacheMode::Warm,
                FOCUSED_SCENARIO_BUDGET_MS,
                probe_ticks.saturating_add(1),
                agent_count,
            ),
            tier(
                "server.full_world.cold",
                AgentLatencyTierKind::FullWorldSmoke,
                AgentLatencyCacheMode::Cold,
                FULL_WORLD_SMOKE_BUDGET_MS,
                probe_ticks.saturating_add(2),
                agent_count,
            ),
        ],
    }
}

fn tier(
    tier_id: &str,
    kind: AgentLatencyTierKind,
    cache_mode: AgentLatencyCacheMode,
    budget_ms: f64,
    sim_ticks: u64,
    body_count: usize,
) -> AgentLatencyTierPlan {
    AgentLatencyTierPlan {
        tier_id: tier_id.to_string(),
        kind,
        cache_mode,
        budget_ms,
        sim_ticks,
        body_count,
    }
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    value.ok_or_else(|| multiplayer_error(field, "required field missing"))
}

fn required_usize(value: Option<usize>, field: &str) -> Result<usize, EngineError> {
    value.ok_or_else(|| multiplayer_error(field, "required field missing"))
}

fn required_u16(value: Option<u16>, field: &str) -> Result<u16, EngineError> {
    value.ok_or_else(|| multiplayer_error(field, "required field missing"))
}

fn required_u64(value: Option<u64>, field: &str) -> Result<u64, EngineError> {
    value.ok_or_else(|| multiplayer_error(field, "required field missing"))
}
