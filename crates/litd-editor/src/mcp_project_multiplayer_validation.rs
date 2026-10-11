use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::super::mcp_project_tools_validation::{
    validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_multiplayer_join::{validate_join_args, validate_join_record};
use super::mcp_project_multiplayer_server_validation::validate_server_record;
use super::mcp_project_multiplayer_types::{
    AuthoritativeServerStatus, MatchTicketRecord, MultiplayerAction, MultiplayerEndpoint,
    MultiplayerLobbyRecord, MultiplayerLobbyStatus, MultiplayerSessionArgs,
    MultiplayerSessionState, MultiplayerTransport, PartyRecord,
};

pub fn validate_multiplayer_args(args: &MultiplayerSessionArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path("hub_state_path", Path::new(&args.hub_state_path))?;
    validate_path(
        "install_manifest_path",
        Path::new(&args.install_manifest_path),
    )?;
    validate_path(
        "multiplayer_state_path",
        Path::new(&args.multiplayer_state_path),
    )?;
    validate_id("multiplayer_state_id", &args.multiplayer_state_id)?;
    for tag in &args.required_tags {
        validate_id("required_tags", tag)?;
    }
    if let Some(team_id) = args.team_id.as_deref() {
        validate_id("team_id", team_id)?;
    }
    match args.action {
        MultiplayerAction::HostLobby => {
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("content_address", args.content_address.as_deref())?;
            require_id("host_player_id", args.host_player_id.as_deref())?;
            let max_players = args.max_players.ok_or_else(|| {
                multiplayer_error("max_players", "max_players is required for host_lobby")
            })?;
            validate_capacity(max_players)?;
            validate_endpoint(args.endpoint.as_ref().ok_or_else(|| {
                multiplayer_error("endpoint", "endpoint is required for host_lobby")
            })?)?;
        }
        MultiplayerAction::StartAuthoritativeServer => {
            require_id("server_id", args.server_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("content_address", args.content_address.as_deref())?;
            require_id("host_player_id", args.host_player_id.as_deref())?;
            let max_players = args.max_players.ok_or_else(|| {
                multiplayer_error(
                    "max_players",
                    "max_players is required for start_authoritative_server",
                )
            })?;
            validate_capacity(max_players)?;
            validate_endpoint(args.endpoint.as_ref().ok_or_else(|| {
                multiplayer_error(
                    "endpoint",
                    "endpoint is required for start_authoritative_server",
                )
            })?)?;
            validate_tick_rate(args.deterministic_tick_rate_hz.ok_or_else(|| {
                multiplayer_error(
                    "deterministic_tick_rate_hz",
                    "deterministic tick rate is required for start_authoritative_server",
                )
            })?)?;
            validate_probe_ticks(args.simulation_probe_ticks.ok_or_else(|| {
                multiplayer_error(
                    "simulation_probe_ticks",
                    "simulation probe ticks is required for start_authoritative_server",
                )
            })?)?;
            validate_agent_count(
                args.calyx_agent_count.ok_or_else(|| {
                    multiplayer_error(
                        "calyx_agent_count",
                        "Calyx agent count is required for start_authoritative_server",
                    )
                })?,
                max_players,
            )?;
        }
        MultiplayerAction::JoinLobby => {
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("player_id", args.player_id.as_deref())?;
            validate_join_args(args)?;
        }
        MultiplayerAction::CreateParty => {
            require_id("party_id", args.party_id.as_deref())?;
            require_id("leader_player_id", args.leader_player_id.as_deref())?;
            let max_players = args.max_players.ok_or_else(|| {
                multiplayer_error("max_players", "max_players is required for create_party")
            })?;
            validate_capacity(max_players)?;
            validate_member_ids(&args.member_ids)?;
            let leader = args.leader_player_id.as_deref().unwrap_or_default();
            if !args.member_ids.iter().any(|member| member == leader) {
                return Err(multiplayer_error(
                    "party.member_ids",
                    "party members must include the leader",
                ));
            }
            if args.member_ids.len() > max_players {
                return Err(multiplayer_error(
                    "party.capacity",
                    "party member count exceeds max_players",
                ));
            }
        }
        MultiplayerAction::Matchmake => {
            require_id("ticket_id", args.ticket_id.as_deref())?;
            require_id("queue_id", args.queue_id.as_deref())?;
            let has_party = args.party_id.as_deref().is_some();
            let has_player = args.player_id.as_deref().is_some();
            if has_party == has_player {
                return Err(multiplayer_error(
                    "matchmake.participant",
                    "matchmake requires exactly one of party_id or player_id",
                ));
            }
            if let Some(party_id) = args.party_id.as_deref() {
                validate_id("party_id", party_id)?;
            }
            if let Some(player_id) = args.player_id.as_deref() {
                validate_id("player_id", player_id)?;
            }
        }
    }
    Ok(())
}

pub fn validate_multiplayer_state(state: &MultiplayerSessionState) -> Result<(), EngineError> {
    if state.schema != super::mcp_project_multiplayer_types::MULTIPLAYER_SESSION_STATE_SCHEMA
        || state.issue != 1038
        || state.state_id.trim().is_empty()
        || state.source_of_truth != "Calyx Aster multiplayer session state"
        || state.hub_state_path.trim().is_empty()
        || state.install_manifest_path.trim().is_empty()
        || state.lobby_count != state.lobbies.len()
        || state.party_count != state.parties.len()
        || state.join_count != state.joins.len()
        || state.ticket_count != state.tickets.len()
        || state.server_count != state.servers.len()
        || state.running_server_count != running_server_count(state)
    {
        return Err(multiplayer_error(
            "state",
            "schema, identity, source, paths, and counts are invalid",
        ));
    }
    let mut lobby_ids = BTreeSet::new();
    for lobby in &state.lobbies {
        validate_lobby_record(lobby)?;
        if !lobby_ids.insert(lobby.lobby_id.as_str()) {
            return Err(multiplayer_error("lobbies", "duplicate lobby_id"));
        }
    }
    let mut server_ids = BTreeSet::new();
    let mut running_lobby_ids = BTreeSet::new();
    for server in &state.servers {
        validate_server_record(server)?;
        if !server_ids.insert(server.server_id.as_str()) {
            return Err(multiplayer_error("servers", "duplicate server_id"));
        }
        let lobby = state
            .lobbies
            .iter()
            .find(|lobby| lobby.lobby_id == server.lobby_id)
            .ok_or_else(|| multiplayer_error("server.lobby_id", "server lobby must exist"))?;
        if lobby.content_address != server.content_address
            || lobby.project_id != server.project_id
            || lobby.package_id != server.package_id
            || lobby.package_sha256 != server.package_sha256
            || lobby.endpoint != server.endpoint
            || lobby.host_player_id != server.host_player_id
            || lobby.max_players != server.max_players
        {
            return Err(multiplayer_error(
                "server.lobby",
                "server readback must match its lobby package, endpoint, host, and capacity",
            ));
        }
        if server.status == AuthoritativeServerStatus::Running
            && !running_lobby_ids.insert(server.lobby_id.as_str())
        {
            return Err(multiplayer_error(
                "server.running",
                "only one running authoritative server is allowed per lobby",
            ));
        }
    }
    let mut party_ids = BTreeSet::new();
    for party in &state.parties {
        validate_party_record(party)?;
        if !party_ids.insert(party.party_id.as_str()) {
            return Err(multiplayer_error("parties", "duplicate party_id"));
        }
    }
    let mut join_ids = BTreeSet::new();
    for join in &state.joins {
        validate_join_record(join)?;
        if !join_ids.insert(join.join_id.as_str()) {
            return Err(multiplayer_error("joins", "duplicate join_id"));
        }
        let lobby_matches = state.lobbies.iter().any(|lobby| {
            lobby.lobby_id == join.lobby_id
                && lobby.content_address == join.content_address
                && lobby.package_sha256 == join.package_sha256
                && lobby.package_bytes == join.package_bytes
                && join.runtime_lock_verified
                && lobby
                    .member_ids
                    .iter()
                    .any(|member| member == &join.player_id)
        });
        if !lobby_matches {
            return Err(multiplayer_error(
                "join.lobby",
                "join must reference an existing lobby member and content address",
            ));
        }
    }
    let mut ticket_ids = BTreeSet::new();
    for ticket in &state.tickets {
        validate_ticket_record(ticket)?;
        if !ticket_ids.insert(ticket.ticket_id.as_str()) {
            return Err(multiplayer_error("tickets", "duplicate ticket_id"));
        }
        if !lobby_ids.contains(ticket.matched_lobby_id.as_str()) {
            return Err(multiplayer_error(
                "ticket.matched_lobby_id",
                "ticket must reference an existing matched lobby",
            ));
        }
    }
    Ok(())
}

pub fn validate_endpoint(endpoint: &MultiplayerEndpoint) -> Result<(), EngineError> {
    validate_single_line("endpoint.host", &endpoint.host)?;
    validate_single_line("endpoint.path", &endpoint.path)?;
    if endpoint.port == 0 {
        return Err(multiplayer_error("endpoint.port", "port must be non-zero"));
    }
    if endpoint.host == "0.0.0.0" || endpoint.host == "::" {
        return Err(multiplayer_error(
            "endpoint.host",
            "host must be a concrete routable address, not a wildcard bind",
        ));
    }
    if !endpoint.path.starts_with("/lobby/") {
        return Err(multiplayer_error(
            "endpoint.path",
            "path must start with /lobby/",
        ));
    }
    if endpoint.transport == MultiplayerTransport::Loopback
        && !matches!(endpoint.host.as_str(), "127.0.0.1" | "localhost" | "::1")
    {
        return Err(multiplayer_error(
            "endpoint.loopback",
            "loopback transport requires 127.0.0.1, localhost, or ::1",
        ));
    }
    Ok(())
}

pub fn validate_member_ids(values: &[String]) -> Result<(), EngineError> {
    if values.is_empty() {
        return Err(multiplayer_error(
            "member_ids",
            "at least one member is required",
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_id("member_ids", value)?;
        if !seen.insert(value.as_str()) {
            return Err(multiplayer_error("member_ids", "duplicate member id"));
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
        return Err(multiplayer_error(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

pub fn multiplayer_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    let field = format!("editor.mcp_project_tools.multiplayer.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::multiplayer",
        field = %field,
        reason = %reason,
        code = "LITD-MULTIPLAYER-VALIDATION",
        "multiplayer session operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn validate_lobby_record(record: &MultiplayerLobbyRecord) -> Result<(), EngineError> {
    validate_id("lobby.lobby_id", &record.lobby_id)?;
    validate_id("lobby.project_id", &record.project_id)?;
    validate_id("lobby.package_id", &record.package_id)?;
    validate_id("lobby.host_player_id", &record.host_player_id)?;
    validate_endpoint(&record.endpoint)?;
    validate_capacity(record.max_players)?;
    validate_member_ids(&record.member_ids)?;
    validate_lobby_slots(record)?;
    if record.member_ids.len() > record.max_players {
        return Err(multiplayer_error(
            "lobby.capacity",
            "lobby is over capacity",
        ));
    }
    if record.member_ids.first() != Some(&record.host_player_id) {
        return Err(multiplayer_error(
            "lobby.host_player_id",
            "host must be the first lobby member",
        ));
    }
    if record.status == MultiplayerLobbyStatus::Full
        && record.member_ids.len() != record.max_players
    {
        return Err(multiplayer_error(
            "lobby.status",
            "full lobby must have exactly max_players members",
        ));
    }
    if record.package_sha256.len() != 64
        || record.installed_package_sha256 != record.package_sha256
        || record.package_bytes == 0
        || record.installed_package_bytes != record.package_bytes
        || !record.runtime_lock_verified
        || !record.playable_on_litd_runtime
    {
        return Err(multiplayer_error(
            "lobby.package",
            "package and runtime readback fields are invalid",
        ));
    }
    Ok(())
}

fn validate_lobby_slots(record: &MultiplayerLobbyRecord) -> Result<(), EngineError> {
    if record.slots.len() != record.member_ids.len() {
        return Err(multiplayer_error(
            "lobby.slots",
            "slot count must match lobby member count",
        ));
    }
    let mut slot_indexes = BTreeSet::new();
    let mut slot_players = BTreeSet::new();
    for (expected_index, slot) in record.slots.iter().enumerate() {
        validate_id("lobby.slot.player_id", &slot.player_id)?;
        validate_id("lobby.slot.team_id", &slot.team_id)?;
        if slot.slot_index != expected_index {
            return Err(multiplayer_error(
                "lobby.slot.slot_index",
                "slot indexes must be dense and zero-based",
            ));
        }
        if !slot_indexes.insert(slot.slot_index) || !slot_players.insert(slot.player_id.as_str()) {
            return Err(multiplayer_error(
                "lobby.slots",
                "slot indexes and players must be unique",
            ));
        }
        if record.member_ids.get(expected_index) != Some(&slot.player_id) {
            return Err(multiplayer_error(
                "lobby.slots",
                "slot player order must match lobby member order",
            ));
        }
    }
    Ok(())
}

fn validate_party_record(record: &PartyRecord) -> Result<(), EngineError> {
    validate_id("party.party_id", &record.party_id)?;
    validate_id("party.leader_player_id", &record.leader_player_id)?;
    validate_id("party.invite_id", &record.invite_id)?;
    validate_capacity(record.max_players)?;
    validate_member_ids(&record.member_ids)?;
    if record.member_ids.len() > record.max_players {
        return Err(multiplayer_error(
            "party.capacity",
            "party is over capacity",
        ));
    }
    if !record
        .member_ids
        .iter()
        .any(|member| member == &record.leader_player_id)
    {
        return Err(multiplayer_error(
            "party.leader_player_id",
            "party members must include the leader",
        ));
    }
    Ok(())
}

fn validate_ticket_record(record: &MatchTicketRecord) -> Result<(), EngineError> {
    validate_id("ticket.ticket_id", &record.ticket_id)?;
    validate_id("ticket.queue_id", &record.queue_id)?;
    validate_id("ticket.matched_lobby_id", &record.matched_lobby_id)?;
    validate_member_ids(&record.member_ids)?;
    if record.player_id.is_some() == record.party_id.is_some() {
        return Err(multiplayer_error(
            "ticket.participant",
            "ticket requires exactly one participant source",
        ));
    }
    if let Some(player_id) = record.player_id.as_deref() {
        validate_id("ticket.player_id", player_id)?;
    }
    if let Some(party_id) = record.party_id.as_deref() {
        validate_id("ticket.party_id", party_id)?;
    }
    for tag in &record.required_tags {
        validate_id("ticket.required_tags", tag)?;
    }
    Ok(())
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_id(
        field,
        value.ok_or_else(|| multiplayer_error(field, "required field missing"))?,
    )
}

pub(super) fn validate_capacity(value: usize) -> Result<(), EngineError> {
    if !(1..=500).contains(&value) {
        return Err(multiplayer_error("capacity", "capacity must be in 1..=500"));
    }
    Ok(())
}

pub(super) fn validate_tick_rate(value: u16) -> Result<(), EngineError> {
    if value != 64 {
        return Err(multiplayer_error(
            "deterministic_tick_rate_hz",
            "authoritative server tick rate must match the engine fixed-tick runtime at 64Hz",
        ));
    }
    Ok(())
}

pub(super) fn validate_probe_ticks(value: u64) -> Result<(), EngineError> {
    if !(1..=1200).contains(&value) {
        return Err(multiplayer_error(
            "simulation_probe_ticks",
            "simulation probe ticks must be in 1..=1200",
        ));
    }
    Ok(())
}

pub(super) fn validate_agent_count(value: usize, max_players: usize) -> Result<(), EngineError> {
    if value == 0 || value > max_players {
        return Err(multiplayer_error(
            "calyx_agent_count",
            "Calyx agent count must be 1..=max_players",
        ));
    }
    Ok(())
}

fn running_server_count(state: &MultiplayerSessionState) -> usize {
    state
        .servers
        .iter()
        .filter(|server| server.status == AuthoritativeServerStatus::Running)
        .count()
}
