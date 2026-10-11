use litd_engine::EngineError;
use serde_json::{Value, json};

use super::{
    ensure_account, ensure_lobby_exists,
    mcp_project_social_types::{
        GameInviteStatus, JoinConnectionMode, JoinConnectionRecord, SocialGraphArgs,
        SocialGraphState,
    },
    mcp_project_social_validation::social_error,
};

pub fn create_join_connection(
    args: &SocialGraphArgs,
    state: &mut SocialGraphState,
) -> Result<Value, EngineError> {
    let connection_id = required(args.connection_id.as_deref(), "connection_id")?;
    if state
        .join_connections
        .iter()
        .any(|connection| connection.connection_id == connection_id)
    {
        return Err(social_error(
            "connection_id",
            "join connection already exists",
        ));
    }
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    let mode = args
        .join_connection_mode
        .ok_or_else(|| social_error("join_connection_mode", "join connection mode is required"))?;
    let account = ensure_account(state, account_id)?.clone();
    let lobby = ensure_lobby_exists(&args.multiplayer_state_path, lobby_id)?;
    let invite_id = validate_invite(args, state, account_id, lobby_id, &lobby.content_address)?;
    let host = required(args.connect_host.as_deref(), "connect_host")?;
    let port = args
        .connect_port
        .ok_or_else(|| social_error("connect_port", "connect port is required"))?;
    let path = required(args.connect_path.as_deref(), "connect_path")?;
    validate_connection_endpoint(mode, host, port, path)?;

    let connection = JoinConnectionRecord {
        connection_id: connection_id.to_string(),
        mode,
        account_id: account.account_id,
        player_id: account.player_id,
        lobby_id: lobby_id.to_string(),
        invite_id,
        content_address: lobby.content_address,
        host: host.to_string(),
        port,
        path: path.to_string(),
        connect_string: connect_string(args, mode, lobby_id, host, port, path),
        requires_existing_ssh_tunnel: mode == JoinConnectionMode::SshTunnel,
        ssh_user: args.ssh_user.clone(),
        ssh_host: args.ssh_host.clone(),
        ssh_remote_host: args.ssh_remote_host.clone(),
        ssh_remote_port: args.ssh_remote_port,
        source_lobby_host: lobby.endpoint.host,
        source_lobby_port: lobby.endpoint.port,
        source_lobby_path: lobby.endpoint.path,
        created_by: args.requested_by.clone(),
    };
    state.join_connections.push(connection.clone());
    Ok(json!({ "join_connection": connection }))
}

fn validate_invite(
    args: &SocialGraphArgs,
    state: &SocialGraphState,
    account_id: &str,
    lobby_id: &str,
    content_address: &str,
) -> Result<Option<String>, EngineError> {
    let Some(invite_id) = args.invite_id.as_deref() else {
        if args.join_connection_mode == Some(JoinConnectionMode::InviteCode) {
            return Err(social_error(
                "invite_id",
                "invite_code join connections require invite_id",
            ));
        }
        return Ok(None);
    };
    let invite = state
        .invites
        .iter()
        .find(|invite| invite.invite_id == invite_id)
        .ok_or_else(|| social_error("invite_id", "invite does not exist"))?;
    if invite.to_account_id != account_id
        || invite.lobby_id != lobby_id
        || invite.content_address != content_address
        || !matches!(
            invite.status,
            GameInviteStatus::Sent | GameInviteStatus::Accepted
        )
    {
        return Err(social_error(
            "invite",
            "join invite must target this account, lobby, and content address",
        ));
    }
    Ok(Some(invite_id.to_string()))
}

fn validate_connection_endpoint(
    mode: JoinConnectionMode,
    host: &str,
    port: u16,
    path: &str,
) -> Result<(), EngineError> {
    if port == 0 {
        return Err(social_error(
            "connect_port",
            "connect port must be non-zero",
        ));
    }
    if !path.starts_with("/lobby/") {
        return Err(social_error(
            "connect_path",
            "connect path must start with /lobby/",
        ));
    }
    if host == "0.0.0.0" || host == "::" {
        return Err(social_error(
            "connect_host",
            "join connections require a concrete host, not a wildcard bind",
        ));
    }
    if host
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '/' | '?' | '#' | '&' | '='))
        || path
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '?' | '#' | '&'))
    {
        return Err(social_error(
            "connect_endpoint",
            "join endpoints cannot contain URL/query delimiters or whitespace",
        ));
    }
    let loopback = matches!(host, "127.0.0.1" | "localhost" | "::1");
    match mode {
        JoinConnectionMode::Localhost | JoinConnectionMode::SshTunnel if !loopback => {
            Err(social_error(
                "connect_host",
                "localhost and ssh tunnel joins require a loopback host",
            ))
        }
        JoinConnectionMode::Lan | JoinConnectionMode::DirectIp if loopback => Err(social_error(
            "connect_host",
            "lan and direct_ip joins require a non-loopback host",
        )),
        _ => Ok(()),
    }
}

fn connect_string(
    args: &SocialGraphArgs,
    mode: JoinConnectionMode,
    lobby_id: &str,
    host: &str,
    port: u16,
    path: &str,
) -> String {
    let mode = match mode {
        JoinConnectionMode::Localhost => "localhost",
        JoinConnectionMode::Lan => "lan",
        JoinConnectionMode::SshTunnel => "ssh_tunnel",
        JoinConnectionMode::DirectIp => "direct_ip",
        JoinConnectionMode::InviteCode => "invite_code",
    };
    let invite = args.invite_id.as_deref().unwrap_or("");
    let mut value =
        format!("litd://join/{lobby_id}?mode={mode}&host={host}&port={port}&path={path}");
    if !invite.is_empty() {
        value.push_str("&invite=");
        value.push_str(invite);
    }
    if let (Some(user), Some(ssh_host), Some(remote), Some(remote_port)) = (
        args.ssh_user.as_deref(),
        args.ssh_host.as_deref(),
        args.ssh_remote_host.as_deref(),
        args.ssh_remote_port,
    ) {
        value.push_str(&format!(
            "&ssh={user}@{ssh_host}&remote={remote}:{remote_port}"
        ));
    }
    value
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    value.ok_or_else(|| social_error(field, "required field missing"))
}
