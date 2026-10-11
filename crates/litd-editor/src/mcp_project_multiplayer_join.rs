use std::path::Path;

use litd_engine::{EngineError, game_package::verify_runtime_package_admission};
use serde_json::{Value, json};

use super::{
    super::mcp_project_tools_validation::{confine_path, resolve_project_root, validate_path},
    add_members_to_lobby,
    mcp_project_multiplayer_types::{JoinRecord, MultiplayerSessionArgs, MultiplayerSessionState},
    mcp_project_multiplayer_validation::{multiplayer_error, validate_id},
    required, team_id,
};

pub(super) fn validate_join_args(args: &MultiplayerSessionArgs) -> Result<(), EngineError> {
    let package_path = args.joining_package_path.as_deref().ok_or_else(|| {
        multiplayer_error(
            "joining_package_path",
            "joining package path is required for join_lobby",
        )
    })?;
    validate_path("joining_package_path", Path::new(package_path))
}

pub(super) fn join_lobby(
    args: &MultiplayerSessionArgs,
    state: &mut MultiplayerSessionState,
) -> Result<Value, EngineError> {
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    let player_id = required(args.player_id.as_deref(), "player_id")?;
    let joining_package_path =
        required(args.joining_package_path.as_deref(), "joining_package_path")?;
    // Resolve the path again at the admission boundary and use the returned
    // canonical path for every package read. The dispatcher performs an early
    // confinement check, but retaining the caller's raw string here would
    // reopen a path-resolution window while lobby state is inspected.
    let project_root = resolve_project_root()?;
    let confined_joining_package_path = confine_path(
        "joining_package_path",
        Path::new(joining_package_path),
        &project_root,
    )?;
    let lobby_index = state
        .lobbies
        .iter()
        .position(|lobby| lobby.lobby_id == lobby_id)
        .ok_or_else(|| multiplayer_error("join.lobby_id", "lobby does not exist"))?;
    let join_id = format!("join.{lobby_id}.{player_id}");
    if state
        .joins
        .iter()
        .any(|existing| existing.join_id == join_id)
    {
        return Err(multiplayer_error(
            "join.join_id",
            "join record already exists",
        ));
    }

    let lobby = &state.lobbies[lobby_index];
    let runtime_report = verify_runtime_package_admission(
        &confined_joining_package_path,
        &lobby.package_id,
        &lobby.package_sha256,
        lobby.package_bytes,
        "mcp.project.multiplayer.join",
    )?;

    let join = JoinRecord {
        join_id,
        lobby_id: lobby_id.to_string(),
        player_id: player_id.to_string(),
        content_address: lobby.content_address.clone(),
        package_sha256: runtime_report.package_sha256.clone(),
        package_bytes: runtime_report.package_bytes,
        runtime_lock_verified: runtime_report.runtime_lock_verified,
    };
    let lobby = &mut state.lobbies[lobby_index];
    add_members_to_lobby(lobby, &[player_id.to_string()], team_id(args, "team.guest"))?;
    state.joins.push(join.clone());
    Ok(json!({ "join": join, "lobby": lobby }))
}

pub(super) fn validate_join_record(record: &JoinRecord) -> Result<(), EngineError> {
    validate_id("join.join_id", &record.join_id)?;
    validate_id("join.lobby_id", &record.lobby_id)?;
    validate_id("join.player_id", &record.player_id)?;
    validate_id("join.content_address", &record.content_address)?;
    if record.package_sha256.len() != 64
        || record.package_bytes == 0
        || !record.runtime_lock_verified
    {
        return Err(multiplayer_error(
            "join.package",
            "package hash, byte length, and runtime lock proof are required",
        ));
    }
    Ok(())
}
