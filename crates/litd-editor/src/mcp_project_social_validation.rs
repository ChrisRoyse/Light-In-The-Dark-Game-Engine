use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::super::mcp_project_tools_validation::{
    validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_social_types::{
    AccountRecord, FriendshipRecord, FriendshipStatus, GameInviteRecord, JoinConnectionMode,
    JoinConnectionRecord, PresenceRecord, PresenceStatus, ProfileRecord, SocialGraphAction,
    SocialGraphArgs, SocialGraphState,
};

pub fn validate_social_args(args: &SocialGraphArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path("social_state_path", Path::new(&args.social_state_path))?;
    validate_path(
        "multiplayer_state_path",
        Path::new(&args.multiplayer_state_path),
    )?;
    validate_id("social_state_id", &args.social_state_id)?;
    match args.action {
        SocialGraphAction::CreateAccountProfile => {
            require_id("account_id", args.account_id.as_deref())?;
            require_id("player_id", args.player_id.as_deref())?;
            require_id("profile_id", args.profile_id.as_deref())?;
            require_id("handle", args.handle.as_deref())?;
            require_line("display_name", args.display_name.as_deref())?;
            validate_tags(&args.tags)?;
        }
        SocialGraphAction::SendFriendRequest => {
            require_id("account_id", args.account_id.as_deref())?;
            require_id("target_account_id", args.target_account_id.as_deref())?;
        }
        SocialGraphAction::AcceptFriendRequest => {
            require_id("account_id", args.account_id.as_deref())?;
            require_id("target_account_id", args.target_account_id.as_deref())?;
        }
        SocialGraphAction::SetPresence => {
            require_id("account_id", args.account_id.as_deref())?;
            let status = args
                .presence_status
                .ok_or_else(|| social_error("presence_status", "presence_status is required"))?;
            if matches!(status, PresenceStatus::InLobby | PresenceStatus::InGame) {
                require_id("lobby_id", args.lobby_id.as_deref())?;
            } else if args.lobby_id.is_some() {
                return Err(social_error(
                    "presence.lobby_id",
                    "offline, online, and away presence cannot carry lobby_id",
                ));
            }
        }
        SocialGraphAction::SendLobbyInvite => {
            require_id("account_id", args.account_id.as_deref())?;
            require_id("target_account_id", args.target_account_id.as_deref())?;
            require_id("invite_id", args.invite_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
        }
        SocialGraphAction::AcceptLobbyInvite => {
            require_id("account_id", args.account_id.as_deref())?;
            require_id("invite_id", args.invite_id.as_deref())?;
        }
        SocialGraphAction::CreateJoinConnection => {
            require_id("connection_id", args.connection_id.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            let mode = args.join_connection_mode.ok_or_else(|| {
                social_error("join_connection_mode", "join connection mode is required")
            })?;
            if mode == JoinConnectionMode::InviteCode {
                require_id("invite_id", args.invite_id.as_deref())?;
            }
            require_line("connect_host", args.connect_host.as_deref())?;
            let port = args
                .connect_port
                .ok_or_else(|| social_error("connect_port", "connect port is required"))?;
            if port == 0 {
                return Err(social_error(
                    "connect_port",
                    "connect port must be non-zero",
                ));
            }
            require_line("connect_path", args.connect_path.as_deref())?;
            if mode == JoinConnectionMode::SshTunnel {
                require_line("ssh_user", args.ssh_user.as_deref())?;
                require_line("ssh_host", args.ssh_host.as_deref())?;
                require_line("ssh_remote_host", args.ssh_remote_host.as_deref())?;
                if args.ssh_remote_port.unwrap_or_default() == 0 {
                    return Err(social_error(
                        "ssh_remote_port",
                        "ssh remote port must be non-zero",
                    ));
                }
            }
        }
    }
    if let Some(path) = args.avatar_asset_path.as_deref() {
        validate_single_line("avatar_asset_path", path)?;
    }
    if let Some(bio) = args.bio.as_deref() {
        validate_single_line("bio", bio)?;
    }
    if let Some(text) = args.rich_presence.as_deref() {
        validate_single_line("rich_presence", text)?;
    }
    Ok(())
}

pub fn validate_social_state(state: &SocialGraphState) -> Result<(), EngineError> {
    if state.schema != super::mcp_project_social_types::SOCIAL_GRAPH_STATE_SCHEMA
        || state.issue != 1039
        || state.state_id.trim().is_empty()
        || state.source_of_truth != "Calyx Aster social graph state"
        || state.multiplayer_state_path.trim().is_empty()
        || state.account_count != state.accounts.len()
        || state.profile_count != state.profiles.len()
        || state.friendship_count != state.friendships.len()
        || state.presence_count != state.presences.len()
        || state.invite_count != state.invites.len()
        || state.connection_count != state.join_connections.len()
    {
        return Err(social_error(
            "state",
            "schema, identity, source, and counts are invalid",
        ));
    }
    let mut account_ids = BTreeSet::new();
    let mut player_ids = BTreeSet::new();
    let mut handles = BTreeSet::new();
    for account in &state.accounts {
        validate_account(account)?;
        if !account_ids.insert(account.account_id.as_str()) {
            return Err(social_error("accounts", "duplicate account_id"));
        }
        if !player_ids.insert(account.player_id.as_str()) {
            return Err(social_error("accounts", "duplicate player_id"));
        }
        if !handles.insert(account.handle.as_str()) {
            return Err(social_error("accounts", "duplicate handle"));
        }
    }
    let mut profile_ids = BTreeSet::new();
    let mut profile_accounts = BTreeSet::new();
    for profile in &state.profiles {
        validate_profile(profile)?;
        if !profile_ids.insert(profile.profile_id.as_str()) {
            return Err(social_error("profiles", "duplicate profile_id"));
        }
        if !profile_accounts.insert(profile.account_id.as_str()) {
            return Err(social_error("profiles", "duplicate account profile"));
        }
        if !account_ids.contains(profile.account_id.as_str()) {
            return Err(social_error(
                "profile.account_id",
                "profile must reference an account",
            ));
        }
    }
    let mut friendships = BTreeSet::new();
    for friendship in &state.friendships {
        validate_friendship(friendship)?;
        if !account_ids.contains(friendship.requester_account_id.as_str())
            || !account_ids.contains(friendship.addressee_account_id.as_str())
        {
            return Err(social_error(
                "friendship.accounts",
                "friendship must reference existing accounts",
            ));
        }
        if !friendships.insert(friendship_pair(
            &friendship.requester_account_id,
            &friendship.addressee_account_id,
        )) {
            return Err(social_error("friendships", "duplicate friendship pair"));
        }
    }
    let mut presences = BTreeSet::new();
    for presence in &state.presences {
        validate_presence(presence)?;
        let account = state
            .accounts
            .iter()
            .find(|account| account.account_id == presence.account_id)
            .ok_or_else(|| social_error("presence.account_id", "presence account is missing"))?;
        if account.player_id != presence.player_id {
            return Err(social_error(
                "presence.player_id",
                "presence player_id must match account",
            ));
        }
        if !presences.insert(presence.account_id.as_str()) {
            return Err(social_error("presences", "duplicate account presence"));
        }
    }
    let mut invite_ids = BTreeSet::new();
    for invite in &state.invites {
        validate_invite(invite)?;
        if !account_ids.contains(invite.from_account_id.as_str())
            || !account_ids.contains(invite.to_account_id.as_str())
        {
            return Err(social_error(
                "invite.accounts",
                "invite must reference existing accounts",
            ));
        }
        if !has_accepted_friendship(
            &state.friendships,
            &invite.from_account_id,
            &invite.to_account_id,
        ) {
            return Err(social_error(
                "invite.friendship",
                "invite requires accepted friendship",
            ));
        }
        if !invite_ids.insert(invite.invite_id.as_str()) {
            return Err(social_error("invites", "duplicate invite_id"));
        }
    }
    let mut connection_ids = BTreeSet::new();
    for connection in &state.join_connections {
        validate_join_connection(connection)?;
        if !account_ids.contains(connection.account_id.as_str()) {
            return Err(social_error(
                "connection.account_id",
                "join connection must reference an account",
            ));
        }
        if let Some(invite_id) = connection.invite_id.as_deref()
            && !invite_ids.contains(invite_id)
        {
            return Err(social_error(
                "connection.invite_id",
                "join connection invite must exist",
            ));
        }
        if !connection_ids.insert(connection.connection_id.as_str()) {
            return Err(social_error("join_connections", "duplicate connection_id"));
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
        return Err(social_error(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

pub fn has_accepted_friendship(friendships: &[FriendshipRecord], left: &str, right: &str) -> bool {
    friendships.iter().any(|friendship| {
        friendship.status == FriendshipStatus::Accepted
            && friendship_pair(
                &friendship.requester_account_id,
                &friendship.addressee_account_id,
            ) == friendship_pair(left, right)
    })
}

pub fn friendship_id(left: &str, right: &str) -> String {
    let (first, second) = ordered_pair(left, right);
    format!("friendship.{first}.{second}")
}

pub fn social_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    let field = format!("editor.mcp_project_tools.social.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::social",
        field = %field,
        reason = %reason,
        code = "LITD-SOCIAL-GRAPH-VALIDATION",
        "social graph operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn validate_account(record: &AccountRecord) -> Result<(), EngineError> {
    validate_id("account.account_id", &record.account_id)?;
    validate_id("account.player_id", &record.player_id)?;
    validate_id("account.handle", &record.handle)?;
    validate_id("account.created_by", &record.created_by)
}

fn validate_profile(record: &ProfileRecord) -> Result<(), EngineError> {
    validate_id("profile.profile_id", &record.profile_id)?;
    validate_id("profile.account_id", &record.account_id)?;
    validate_single_line("profile.display_name", &record.display_name)?;
    if let Some(path) = record.avatar_asset_path.as_deref() {
        validate_single_line("profile.avatar_asset_path", path)?;
    }
    if let Some(bio) = record.bio.as_deref() {
        validate_single_line("profile.bio", bio)?;
    }
    validate_tags(&record.tags)
}

fn validate_friendship(record: &FriendshipRecord) -> Result<(), EngineError> {
    validate_id("friendship.friendship_id", &record.friendship_id)?;
    validate_id(
        "friendship.requester_account_id",
        &record.requester_account_id,
    )?;
    validate_id(
        "friendship.addressee_account_id",
        &record.addressee_account_id,
    )?;
    if record.requester_account_id == record.addressee_account_id {
        return Err(social_error(
            "friendship.self",
            "self-friendship is invalid",
        ));
    }
    if record.friendship_id
        != friendship_id(&record.requester_account_id, &record.addressee_account_id)
    {
        return Err(social_error(
            "friendship.friendship_id",
            "friendship_id must be derived from the ordered account pair",
        ));
    }
    Ok(())
}

fn validate_presence(record: &PresenceRecord) -> Result<(), EngineError> {
    validate_id("presence.account_id", &record.account_id)?;
    validate_id("presence.player_id", &record.player_id)?;
    validate_single_line("presence.rich_presence", &record.rich_presence)?;
    if matches!(
        record.status,
        PresenceStatus::InLobby | PresenceStatus::InGame
    ) && record.lobby_id.is_none()
    {
        return Err(social_error(
            "presence.lobby_id",
            "in_lobby and in_game presence require lobby_id",
        ));
    }
    if !matches!(
        record.status,
        PresenceStatus::InLobby | PresenceStatus::InGame
    ) && record.lobby_id.is_some()
    {
        return Err(social_error(
            "presence.lobby_id",
            "offline, online, and away presence cannot carry lobby_id",
        ));
    }
    if let Some(lobby_id) = record.lobby_id.as_deref() {
        validate_id("presence.lobby_id", lobby_id)?;
    }
    Ok(())
}

fn validate_invite(record: &GameInviteRecord) -> Result<(), EngineError> {
    validate_id("invite.invite_id", &record.invite_id)?;
    validate_id("invite.from_account_id", &record.from_account_id)?;
    validate_id("invite.to_account_id", &record.to_account_id)?;
    validate_id("invite.lobby_id", &record.lobby_id)?;
    validate_id("invite.content_address", &record.content_address)?;
    validate_single_line("invite.connect_string", &record.connect_string)?;
    if record.from_account_id == record.to_account_id {
        return Err(social_error("invite.self", "self-invite is invalid"));
    }
    Ok(())
}

fn validate_join_connection(record: &JoinConnectionRecord) -> Result<(), EngineError> {
    validate_id("connection.connection_id", &record.connection_id)?;
    validate_id("connection.account_id", &record.account_id)?;
    validate_id("connection.player_id", &record.player_id)?;
    validate_id("connection.lobby_id", &record.lobby_id)?;
    validate_id("connection.content_address", &record.content_address)?;
    validate_single_line("connection.host", &record.host)?;
    validate_single_line("connection.path", &record.path)?;
    validate_single_line("connection.connect_string", &record.connect_string)?;
    validate_single_line("connection.source_lobby_host", &record.source_lobby_host)?;
    validate_single_line("connection.source_lobby_path", &record.source_lobby_path)?;
    if record.port == 0 || record.source_lobby_port == 0 || record.created_by.trim().is_empty() {
        return Err(social_error(
            "connection",
            "ports and creator must be valid",
        ));
    }
    if record.mode == JoinConnectionMode::SshTunnel {
        validate_single_line(
            "connection.ssh_user",
            required_option(record.ssh_user.as_deref())?,
        )?;
        validate_single_line(
            "connection.ssh_host",
            required_option(record.ssh_host.as_deref())?,
        )?;
        validate_single_line(
            "connection.ssh_remote_host",
            required_option(record.ssh_remote_host.as_deref())?,
        )?;
        if !record.requires_existing_ssh_tunnel || record.ssh_remote_port.unwrap_or_default() == 0 {
            return Err(social_error(
                "connection.ssh",
                "ssh tunnel connections require ssh metadata and a remote port",
            ));
        }
    } else if record.requires_existing_ssh_tunnel
        || record.ssh_user.is_some()
        || record.ssh_host.is_some()
        || record.ssh_remote_host.is_some()
        || record.ssh_remote_port.is_some()
    {
        return Err(social_error(
            "connection.ssh",
            "non-ssh connections cannot carry ssh metadata",
        ));
    }
    if record.host == "0.0.0.0"
        || record.host == "::"
        || !record.path.starts_with("/lobby/")
        || !record.connect_string.starts_with("litd://join/")
        || record.connect_string.chars().any(char::is_whitespace)
    {
        return Err(social_error(
            "connection.endpoint",
            "join connection endpoint and connect string must be concrete and parseable",
        ));
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> Result<(), EngineError> {
    if tags.is_empty() {
        return Err(social_error("tags", "at least one tag is required"));
    }
    let mut seen = BTreeSet::new();
    for tag in tags {
        validate_id("tags", tag)?;
        if !seen.insert(tag.as_str()) {
            return Err(social_error("tags", "duplicate tag"));
        }
    }
    Ok(())
}

fn required_option(value: Option<&str>) -> Result<&str, EngineError> {
    value.ok_or_else(|| social_error("connection.ssh", "required ssh field missing"))
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_id(
        field,
        value.ok_or_else(|| social_error(field, "required field missing"))?,
    )
}

fn require_line(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_single_line(
        field,
        value.ok_or_else(|| social_error(field, "required field missing"))?,
    )
}

fn friendship_pair(left: &str, right: &str) -> String {
    let (first, second) = ordered_pair(left, right);
    format!("{first}\n{second}")
}

fn ordered_pair<'a>(left: &'a str, right: &'a str) -> (&'a str, &'a str) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}
