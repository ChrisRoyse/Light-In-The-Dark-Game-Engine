use std::{fs, path::Path};

use litd_engine::{
    EngineError,
    game_package::{build_runtime_game_run_report, load_runtime_locked_package},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

#[path = "mcp_project_multiplayer_join.rs"]
mod mcp_project_multiplayer_join;
#[path = "mcp_project_multiplayer_server.rs"]
mod mcp_project_multiplayer_server;
#[path = "mcp_project_multiplayer_server_validation.rs"]
mod mcp_project_multiplayer_server_validation;
#[path = "mcp_project_multiplayer_store.rs"]
mod mcp_project_multiplayer_store;
#[path = "mcp_project_multiplayer_types.rs"]
mod mcp_project_multiplayer_types;
#[path = "mcp_project_multiplayer_validation.rs"]
mod mcp_project_multiplayer_validation;

pub use self::{
    mcp_project_multiplayer_store::read_multiplayer_session_state,
    mcp_project_multiplayer_types::{
        AuthoritativeServerRecord, AuthoritativeServerStatus, MULTIPLAYER_SESSION_STATE_SCHEMA,
        MULTIPLAYER_SESSION_TOOL, MatchTicketRecord, MatchTicketStatus, MultiplayerAction,
        MultiplayerEndpoint, MultiplayerLobbyRecord, MultiplayerLobbySlotRecord,
        MultiplayerLobbyStatus, MultiplayerSessionState, MultiplayerSessionStateReadback,
        MultiplayerTransport, PartyRecord, ServerAuthorityMode,
    },
};

use self::{
    mcp_project_multiplayer_join::join_lobby,
    mcp_project_multiplayer_server::start_authoritative_server,
    mcp_project_multiplayer_store::{
        load_or_create_multiplayer_session_state, write_multiplayer_session_state,
    },
    mcp_project_multiplayer_types::MultiplayerSessionArgs,
    mcp_project_multiplayer_validation::{
        multiplayer_error, validate_member_ids, validate_multiplayer_args,
    },
};
use super::{
    McpProjectToolCallReadback, READ_PROJECT_TOOL,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, write_mcp_project_state,
        write_mcp_project_tool_ledger,
    },
    parse_args, read_hub_launcher_state, read_litdgame_install_manifest, validate_mutating_route,
};

pub(super) fn manage_multiplayer_session(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: MultiplayerSessionArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        ("hub_state_path", std::path::Path::new(&args.hub_state_path)),
        (
            "install_manifest_path",
            std::path::Path::new(&args.install_manifest_path),
        ),
        (
            "multiplayer_state_path",
            std::path::Path::new(&args.multiplayer_state_path),
        ),
    ])?;
    if let Some(joining_package_path) = args.joining_package_path.as_deref() {
        crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[(
            "joining_package_path",
            Path::new(joining_package_path),
        )])?;
    }
    validate_multiplayer_args(&args)?;

    let mut multiplayer_state = load_or_create_multiplayer_session_state(
        &args.multiplayer_state_path,
        &args.multiplayer_state_id,
        &args.hub_state_path,
        &args.install_manifest_path,
    )?;
    let result = match args.action {
        MultiplayerAction::HostLobby => host_lobby(&args, &mut multiplayer_state)?,
        MultiplayerAction::StartAuthoritativeServer => {
            start_authoritative_server(&args, &mut multiplayer_state)?
        }
        MultiplayerAction::JoinLobby => join_lobby(&args, &mut multiplayer_state)?,
        MultiplayerAction::CreateParty => create_party(&args, &mut multiplayer_state)?,
        MultiplayerAction::Matchmake => matchmake(&args, &mut multiplayer_state)?,
    };
    refresh_counts(&mut multiplayer_state);
    let multiplayer_readback =
        write_multiplayer_session_state(&args.multiplayer_state_path, &multiplayer_state)?;

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
                "managed local multiplayer lobby, authoritative server, party, join, or matchmaking state"
                    .to_string(),
            affected_ids: affected_ids(&multiplayer_readback.state),
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
            "Multiplayer state JSON + hub state JSON + install manifest JSON + installed package bytes + deterministic sim and Calyx agent-budget readbacks"
                .to_string(),
        result: json!({
            "multiplayer_state_readback": multiplayer_readback,
            "operation": result
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

fn host_lobby(
    args: &MultiplayerSessionArgs,
    state: &mut MultiplayerSessionState,
) -> Result<Value, EngineError> {
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    if state.lobbies.iter().any(|lobby| lobby.lobby_id == lobby_id) {
        return Err(multiplayer_error("lobby_id", "lobby already exists"));
    }
    let content_address = required(args.content_address.as_deref(), "content_address")?;
    let host_player_id = required(args.host_player_id.as_deref(), "host_player_id")?;
    let card = read_hub_launcher_state(&args.hub_state_path)?
        .state
        .cards
        .into_iter()
        .find(|card| card.content_address == content_address)
        .ok_or_else(|| {
            multiplayer_error(
                "host.content_address",
                "content address is not present in hub state",
            )
        })?;
    if !card.installed || !card.launchable {
        return Err(multiplayer_error(
            "host.card",
            "hosted games must be installed and launchable",
        ));
    }
    let installed_record = read_litdgame_install_manifest(&args.install_manifest_path)?
        .manifest
        .records
        .into_iter()
        .find(|record| record.content_address == content_address)
        .ok_or_else(|| {
            multiplayer_error(
                "host.install_manifest",
                "content address is not installed in install manifest",
            )
        })?;
    let installed_path = card.installed_package_path.as_deref().ok_or_else(|| {
        multiplayer_error(
            "host.installed_package_path",
            "hub card is missing installed package path",
        )
    })?;
    if installed_record.installed_package_path != installed_path {
        return Err(multiplayer_error(
            "host.install_manifest",
            "install manifest path does not match hub card",
        ));
    }
    let installed_bytes = fs::read(installed_path).map_err(|error| {
        multiplayer_error(
            "host.installed_package.read",
            format!("read {installed_path}: {error}"),
        )
    })?;
    let actual_sha = sha256_hex(&installed_bytes);
    if actual_sha != card.package_sha256 || installed_bytes.len() != card.package_bytes {
        return Err(multiplayer_error(
            "host.installed_package",
            "installed package bytes do not match hub card",
        ));
    }
    let runtime =
        load_runtime_locked_package(Path::new(installed_path), "mcp.project.multiplayer")?;
    let runtime_report = build_runtime_game_run_report(&runtime);
    if runtime_report.package_id != card.package_id
        || runtime_report.package_sha256 != card.package_sha256
        || !runtime_report.runtime_lock_verified
        || !runtime_report.playable_on_litd_runtime
    {
        return Err(multiplayer_error(
            "host.runtime_report",
            "runtime package readback does not match hub card",
        ));
    }

    let endpoint = args
        .endpoint
        .clone()
        .ok_or_else(|| multiplayer_error("host.endpoint", "endpoint is required for host_lobby"))?;
    let max_players = args.max_players.unwrap_or_default();
    let host_team_id = team_id(args, "team.host");
    let lobby = MultiplayerLobbyRecord {
        lobby_id: lobby_id.to_string(),
        content_address: content_address.to_string(),
        project_id: card.project_id,
        package_id: card.package_id,
        package_sha256: card.package_sha256,
        package_bytes: card.package_bytes,
        title: card.title,
        version: card.version,
        tags: card.tags,
        min_spec_tags: card.min_spec_tags,
        endpoint,
        host_player_id: host_player_id.to_string(),
        max_players,
        member_ids: vec![host_player_id.to_string()],
        slots: vec![slot_record(0, host_player_id, host_team_id)],
        status: if max_players == 1 {
            MultiplayerLobbyStatus::Full
        } else {
            MultiplayerLobbyStatus::Open
        },
        installed_package_path: installed_path.to_string(),
        installed_package_sha256: actual_sha,
        installed_package_bytes: installed_bytes.len(),
        runtime_lock_verified: runtime_report.runtime_lock_verified,
        playable_on_litd_runtime: runtime_report.playable_on_litd_runtime,
        hosted_by: args.requested_by.clone(),
    };
    state.lobbies.push(lobby.clone());
    Ok(json!({ "hosted_lobby": lobby }))
}

fn create_party(
    args: &MultiplayerSessionArgs,
    state: &mut MultiplayerSessionState,
) -> Result<Value, EngineError> {
    let party_id = required(args.party_id.as_deref(), "party_id")?;
    if state.parties.iter().any(|party| party.party_id == party_id) {
        return Err(multiplayer_error("party_id", "party already exists"));
    }
    let leader_player_id = required(args.leader_player_id.as_deref(), "leader_player_id")?;
    validate_member_ids(&args.member_ids)?;
    let party = PartyRecord {
        party_id: party_id.to_string(),
        leader_player_id: leader_player_id.to_string(),
        member_ids: args.member_ids.clone(),
        max_players: args.max_players.unwrap_or_default(),
        invite_id: format!("invite.{party_id}"),
    };
    state.parties.push(party.clone());
    Ok(json!({ "party": party }))
}

fn matchmake(
    args: &MultiplayerSessionArgs,
    state: &mut MultiplayerSessionState,
) -> Result<Value, EngineError> {
    let ticket_id = required(args.ticket_id.as_deref(), "ticket_id")?;
    if state
        .tickets
        .iter()
        .any(|ticket| ticket.ticket_id == ticket_id)
    {
        return Err(multiplayer_error("ticket_id", "ticket already exists"));
    }
    let queue_id = required(args.queue_id.as_deref(), "queue_id")?;
    let (player_id, party_id, members) = match (args.player_id.as_deref(), args.party_id.as_deref())
    {
        (Some(player_id), None) => (
            Some(player_id.to_string()),
            None,
            vec![player_id.to_string()],
        ),
        (None, Some(party_id)) => {
            let party = state
                .parties
                .iter()
                .find(|party| party.party_id == party_id)
                .ok_or_else(|| multiplayer_error("matchmake.party_id", "party does not exist"))?;
            (None, Some(party_id.to_string()), party.member_ids.clone())
        }
        _ => {
            return Err(multiplayer_error(
                "matchmake.participant",
                "matchmake requires exactly one of party_id or player_id",
            ));
        }
    };
    if state.tickets.iter().any(|ticket| {
        ticket.queue_id == queue_id && ticket.player_id == player_id && ticket.party_id == party_id
    }) {
        return Err(multiplayer_error(
            "matchmake.active_ticket",
            "participant already has a ticket record in this queue",
        ));
    }
    let lobby_index = state
        .lobbies
        .iter()
        .position(|lobby| {
            lobby.status == MultiplayerLobbyStatus::Open
                && lobby_has_tags(lobby, &args.required_tags)
                && !members
                    .iter()
                    .any(|member| lobby.member_ids.iter().any(|existing| existing == member))
                && lobby.member_ids.len() + members.len() <= lobby.max_players
        })
        .ok_or_else(|| {
            multiplayer_error(
                "matchmake.lobby",
                "no open lobby satisfies capacity and required tags",
            )
        })?;
    let lobby = &mut state.lobbies[lobby_index];
    add_members_to_lobby(lobby, &members, team_id(args, "team.matchmade"))?;
    let ticket = MatchTicketRecord {
        ticket_id: ticket_id.to_string(),
        queue_id: queue_id.to_string(),
        player_id,
        party_id,
        required_tags: args.required_tags.clone(),
        member_ids: members,
        matched_lobby_id: lobby.lobby_id.clone(),
        status: MatchTicketStatus::Matched,
        rule_summary: "local-first exact-tag capacity match".to_string(),
    };
    state.tickets.push(ticket.clone());
    Ok(json!({ "ticket": ticket, "lobby": lobby }))
}

fn add_members_to_lobby(
    lobby: &mut MultiplayerLobbyRecord,
    members: &[String],
    team_id: &str,
) -> Result<(), EngineError> {
    if lobby.status == MultiplayerLobbyStatus::Full {
        return Err(multiplayer_error("lobby.status", "lobby is full"));
    }
    if members
        .iter()
        .any(|member| lobby.member_ids.iter().any(|existing| existing == member))
    {
        return Err(multiplayer_error(
            "lobby.members",
            "one or more members already joined the lobby",
        ));
    }
    if lobby.member_ids.len() + members.len() > lobby.max_players {
        return Err(multiplayer_error(
            "lobby.capacity",
            "joining members would exceed lobby capacity",
        ));
    }
    let first_slot = lobby.slots.len();
    lobby.member_ids.extend_from_slice(members);
    lobby.slots.extend(
        members
            .iter()
            .enumerate()
            .map(|(offset, member)| slot_record(first_slot + offset, member, team_id)),
    );
    lobby.status = if lobby.member_ids.len() == lobby.max_players {
        MultiplayerLobbyStatus::Full
    } else {
        MultiplayerLobbyStatus::Open
    };
    Ok(())
}

fn slot_record(slot_index: usize, player_id: &str, team_id: &str) -> MultiplayerLobbySlotRecord {
    MultiplayerLobbySlotRecord {
        slot_index,
        player_id: player_id.to_string(),
        team_id: team_id.to_string(),
    }
}

fn team_id<'a>(args: &'a MultiplayerSessionArgs, default_team: &'static str) -> &'a str {
    args.team_id.as_deref().unwrap_or(default_team)
}

fn lobby_has_tags(lobby: &MultiplayerLobbyRecord, required_tags: &[String]) -> bool {
    required_tags
        .iter()
        .all(|tag| lobby.tags.contains(tag) || lobby.min_spec_tags.contains(tag))
}

fn refresh_counts(state: &mut MultiplayerSessionState) {
    state.lobby_count = state.lobbies.len();
    state.party_count = state.parties.len();
    state.join_count = state.joins.len();
    state.ticket_count = state.tickets.len();
    state.server_count = state.servers.len();
    state.running_server_count = state
        .servers
        .iter()
        .filter(|server| server.status == AuthoritativeServerStatus::Running)
        .count();
}

fn affected_ids(state: &MultiplayerSessionState) -> Vec<String> {
    let mut ids: Vec<String> = state
        .lobbies
        .iter()
        .map(|lobby| lobby.lobby_id.clone())
        .chain(state.servers.iter().map(|server| server.server_id.clone()))
        .chain(state.parties.iter().map(|party| party.party_id.clone()))
        .chain(state.tickets.iter().map(|ticket| ticket.ticket_id.clone()))
        .collect();
    if ids.is_empty() {
        ids.push(state.state_id.clone());
    }
    ids
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    value.ok_or_else(|| multiplayer_error(field, "required field missing"))
}
