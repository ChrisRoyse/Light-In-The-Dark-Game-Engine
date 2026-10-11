use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

#[path = "mcp_project_social_connect.rs"]
mod mcp_project_social_connect;
#[path = "mcp_project_social_store.rs"]
mod mcp_project_social_store;
#[path = "mcp_project_social_types.rs"]
mod mcp_project_social_types;
#[path = "mcp_project_social_validation.rs"]
mod mcp_project_social_validation;

pub use self::{
    mcp_project_social_store::read_social_graph_state,
    mcp_project_social_types::{
        AccountRecord, FriendshipRecord, FriendshipStatus, GameInviteRecord, GameInviteStatus,
        JoinConnectionMode, JoinConnectionRecord, PresenceRecord, PresenceStatus, ProfileRecord,
        SOCIAL_GRAPH_STATE_SCHEMA, SOCIAL_GRAPH_TOOL, SocialGraphAction, SocialGraphState,
        SocialGraphStateReadback,
    },
};

use self::{
    mcp_project_social_connect::create_join_connection,
    mcp_project_social_store::{load_or_create_social_graph_state, write_social_graph_state},
    mcp_project_social_types::SocialGraphArgs,
    mcp_project_social_validation::{
        friendship_id, has_accepted_friendship, social_error, validate_social_args,
    },
};
use super::{
    McpProjectToolCallReadback, READ_PROJECT_TOOL,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, write_mcp_project_state,
        write_mcp_project_tool_ledger,
    },
    parse_args, read_multiplayer_session_state, validate_mutating_route,
};

pub(super) fn manage_social_graph(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: SocialGraphArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine the filesystem-sink paths to the project root before any fs effect.
    // NOTE: avatar_asset_path and connect_path are deliberately NOT confined — they are not
    // filesystem sinks in this handler. `connect_path` is a network lobby connect string
    // (e.g. "/lobby/lobby.social") and `avatar_asset_path` is a stored avatar reference; both
    // are persisted as opaque strings, never opened/read/written here, so confining them
    // would wrongly reject legitimate non-filesystem values.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        (
            "social_state_path",
            std::path::Path::new(&args.social_state_path),
        ),
        (
            "multiplayer_state_path",
            std::path::Path::new(&args.multiplayer_state_path),
        ),
    ])?;
    validate_social_args(&args)?;
    let mut social_state = load_or_create_social_graph_state(
        &args.social_state_path,
        &args.social_state_id,
        &args.multiplayer_state_path,
    )?;
    let result = match args.action {
        SocialGraphAction::CreateAccountProfile => {
            create_account_profile(&args, &mut social_state)?
        }
        SocialGraphAction::SendFriendRequest => send_friend_request(&args, &mut social_state)?,
        SocialGraphAction::AcceptFriendRequest => accept_friend_request(&args, &mut social_state)?,
        SocialGraphAction::SetPresence => set_presence(&args, &mut social_state)?,
        SocialGraphAction::SendLobbyInvite => send_lobby_invite(&args, &mut social_state)?,
        SocialGraphAction::AcceptLobbyInvite => accept_lobby_invite(&args, &mut social_state)?,
        SocialGraphAction::CreateJoinConnection => {
            create_join_connection(&args, &mut social_state)?
        }
    };
    refresh_counts(&mut social_state);
    let social_readback = write_social_graph_state(&args.social_state_path, &social_state)?;

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
                "managed local accounts, profiles, friends, presence, game invites, or join connections"
                    .to_string(),
            affected_ids: affected_ids(&social_readback.state),
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
            "Social graph state JSON + multiplayer session state JSON + join connection readbacks + project ledger JSON"
                .to_string(),
        result: json!({
            "social_state_readback": social_readback,
            "operation": result
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

fn create_account_profile(
    args: &SocialGraphArgs,
    state: &mut SocialGraphState,
) -> Result<Value, EngineError> {
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    let player_id = required(args.player_id.as_deref(), "player_id")?;
    let profile_id = required(args.profile_id.as_deref(), "profile_id")?;
    let handle = required(args.handle.as_deref(), "handle")?;
    if state.accounts.iter().any(|account| {
        account.account_id == account_id
            || account.player_id == player_id
            || account.handle == handle
    }) {
        return Err(social_error(
            "account.unique",
            "account_id, player_id, and handle must be unique",
        ));
    }
    if state
        .profiles
        .iter()
        .any(|profile| profile.profile_id == profile_id || profile.account_id == account_id)
    {
        return Err(social_error("profile.unique", "profile already exists"));
    }
    let account = AccountRecord {
        account_id: account_id.to_string(),
        player_id: player_id.to_string(),
        handle: handle.to_string(),
        created_by: args.requested_by.clone(),
    };
    let profile = ProfileRecord {
        profile_id: profile_id.to_string(),
        account_id: account_id.to_string(),
        display_name: required(args.display_name.as_deref(), "display_name")?.to_string(),
        avatar_asset_path: args.avatar_asset_path.clone(),
        bio: args.bio.clone(),
        tags: args.tags.clone(),
    };
    state.accounts.push(account.clone());
    state.profiles.push(profile.clone());
    Ok(json!({ "account": account, "profile": profile }))
}

fn send_friend_request(
    args: &SocialGraphArgs,
    state: &mut SocialGraphState,
) -> Result<Value, EngineError> {
    let requester = required(args.account_id.as_deref(), "account_id")?;
    let addressee = required(args.target_account_id.as_deref(), "target_account_id")?;
    ensure_account(state, requester)?;
    ensure_account(state, addressee)?;
    if requester == addressee {
        return Err(social_error(
            "friendship.self",
            "self-friendship is invalid",
        ));
    }
    let friendship_id = friendship_id(requester, addressee);
    if state
        .friendships
        .iter()
        .any(|friendship| friendship.friendship_id == friendship_id)
    {
        return Err(social_error(
            "friendship.duplicate",
            "friendship already exists for this account pair",
        ));
    }
    let friendship = FriendshipRecord {
        friendship_id,
        requester_account_id: requester.to_string(),
        addressee_account_id: addressee.to_string(),
        status: FriendshipStatus::Pending,
    };
    state.friendships.push(friendship.clone());
    Ok(json!({ "friendship": friendship }))
}

fn accept_friend_request(
    args: &SocialGraphArgs,
    state: &mut SocialGraphState,
) -> Result<Value, EngineError> {
    let acceptor = required(args.account_id.as_deref(), "account_id")?;
    let requester = required(args.target_account_id.as_deref(), "target_account_id")?;
    let friendship_id = friendship_id(acceptor, requester);
    let friendship = state
        .friendships
        .iter_mut()
        .find(|friendship| friendship.friendship_id == friendship_id)
        .ok_or_else(|| social_error("friendship", "pending friendship does not exist"))?;
    if friendship.status != FriendshipStatus::Pending
        || friendship.requester_account_id != requester
        || friendship.addressee_account_id != acceptor
    {
        return Err(social_error(
            "friendship.accept",
            "only the addressee can accept a pending request",
        ));
    }
    friendship.status = FriendshipStatus::Accepted;
    Ok(json!({ "friendship": friendship }))
}

fn set_presence(
    args: &SocialGraphArgs,
    state: &mut SocialGraphState,
) -> Result<Value, EngineError> {
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    let account = ensure_account(state, account_id)?.clone();
    let status = args
        .presence_status
        .ok_or_else(|| social_error("presence_status", "presence_status is required"))?;
    if let Some(lobby_id) = args.lobby_id.as_deref() {
        ensure_lobby_member(&args.multiplayer_state_path, lobby_id, &account.player_id)?;
    }
    if matches!(status, PresenceStatus::InLobby | PresenceStatus::InGame) && args.lobby_id.is_none()
    {
        return Err(social_error(
            "presence.lobby_id",
            "in_lobby and in_game require lobby_id",
        ));
    }
    let presence = PresenceRecord {
        account_id: account.account_id,
        player_id: account.player_id,
        status,
        lobby_id: args.lobby_id.clone(),
        rich_presence: args
            .rich_presence
            .clone()
            .unwrap_or_else(|| format!("{status:?}")),
    };
    if let Some(existing) = state
        .presences
        .iter_mut()
        .find(|existing| existing.account_id == presence.account_id)
    {
        *existing = presence.clone();
    } else {
        state.presences.push(presence.clone());
    }
    Ok(json!({ "presence": presence }))
}

fn send_lobby_invite(
    args: &SocialGraphArgs,
    state: &mut SocialGraphState,
) -> Result<Value, EngineError> {
    let from = required(args.account_id.as_deref(), "account_id")?;
    let to = required(args.target_account_id.as_deref(), "target_account_id")?;
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    let invite_id = required(args.invite_id.as_deref(), "invite_id")?;
    let from_account = ensure_account(state, from)?.clone();
    ensure_account(state, to)?;
    if !has_accepted_friendship(&state.friendships, from, to) {
        return Err(social_error(
            "invite.friendship",
            "game invite requires accepted friendship",
        ));
    }
    if state
        .invites
        .iter()
        .any(|invite| invite.invite_id == invite_id)
    {
        return Err(social_error("invite_id", "invite already exists"));
    }
    let lobby = ensure_lobby_member(
        &args.multiplayer_state_path,
        lobby_id,
        &from_account.player_id,
    )?;
    if lobby.member_ids.iter().any(|player| {
        state
            .accounts
            .iter()
            .any(|account| account.account_id == to && account.player_id == *player)
    }) {
        return Err(social_error(
            "invite.target",
            "target account is already in the lobby",
        ));
    }
    let connect_string = format!("litd://join/{lobby_id}?invite={invite_id}");
    let invite = GameInviteRecord {
        invite_id: invite_id.to_string(),
        from_account_id: from.to_string(),
        to_account_id: to.to_string(),
        lobby_id: lobby_id.to_string(),
        content_address: lobby.content_address,
        connect_string,
        status: GameInviteStatus::Sent,
    };
    state.invites.push(invite.clone());
    Ok(json!({ "invite": invite }))
}

fn accept_lobby_invite(
    args: &SocialGraphArgs,
    state: &mut SocialGraphState,
) -> Result<Value, EngineError> {
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    let invite_id = required(args.invite_id.as_deref(), "invite_id")?;
    let account = ensure_account(state, account_id)?.clone();
    let invite = state
        .invites
        .iter_mut()
        .find(|invite| invite.invite_id == invite_id)
        .ok_or_else(|| social_error("invite_id", "invite does not exist"))?;
    if invite.to_account_id != account_id || invite.status != GameInviteStatus::Sent {
        return Err(social_error(
            "invite.accept",
            "only the sent invite target can accept",
        ));
    }
    ensure_lobby_exists(&args.multiplayer_state_path, &invite.lobby_id)?;
    if invite.from_account_id == account.account_id {
        return Err(social_error("invite.self", "self-invite is invalid"));
    }
    invite.status = GameInviteStatus::Accepted;
    Ok(json!({ "invite": invite }))
}

fn ensure_account<'a>(
    state: &'a SocialGraphState,
    account_id: &str,
) -> Result<&'a AccountRecord, EngineError> {
    state
        .accounts
        .iter()
        .find(|account| account.account_id == account_id)
        .ok_or_else(|| social_error("account_id", format!("{account_id} does not exist")))
}

fn ensure_lobby_member(
    multiplayer_state_path: &str,
    lobby_id: &str,
    player_id: &str,
) -> Result<super::MultiplayerLobbyRecord, EngineError> {
    let lobby = ensure_lobby_exists(multiplayer_state_path, lobby_id)?;
    if !lobby.member_ids.iter().any(|member| member == player_id) {
        return Err(social_error(
            "lobby.member",
            "account player_id is not a member of the lobby",
        ));
    }
    Ok(lobby)
}

fn ensure_lobby_exists(
    multiplayer_state_path: &str,
    lobby_id: &str,
) -> Result<super::MultiplayerLobbyRecord, EngineError> {
    read_multiplayer_session_state(multiplayer_state_path)?
        .state
        .lobbies
        .into_iter()
        .find(|lobby| lobby.lobby_id == lobby_id)
        .ok_or_else(|| social_error("lobby_id", "lobby does not exist in multiplayer state"))
}

fn refresh_counts(state: &mut SocialGraphState) {
    state.account_count = state.accounts.len();
    state.profile_count = state.profiles.len();
    state.friendship_count = state.friendships.len();
    state.presence_count = state.presences.len();
    state.invite_count = state.invites.len();
    state.connection_count = state.join_connections.len();
}

fn affected_ids(state: &SocialGraphState) -> Vec<String> {
    let mut ids: Vec<String> = state
        .accounts
        .iter()
        .map(|account| account.account_id.clone())
        .chain(
            state
                .friendships
                .iter()
                .map(|friendship| friendship.friendship_id.clone()),
        )
        .chain(state.invites.iter().map(|invite| invite.invite_id.clone()))
        .chain(
            state
                .join_connections
                .iter()
                .map(|connection| connection.connection_id.clone()),
        )
        .collect();
    if ids.is_empty() {
        ids.push(state.state_id.clone());
    }
    ids
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    value.ok_or_else(|| social_error(field, "required field missing"))
}
