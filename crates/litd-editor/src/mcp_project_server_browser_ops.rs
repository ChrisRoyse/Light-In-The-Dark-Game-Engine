use litd_engine::EngineError;
use serde_json::{Value, json};

use super::super::{
    AccessRuleKind, GameInviteStatus, JoinConnectionRecord, MultiplayerLobbyRecord,
    RoleplayPolicyRecord, ServerRole, read_moderation_safety_state, read_multiplayer_session_state,
    read_social_graph_state,
};
use super::{
    mcp_project_server_browser_types::{
        ServerBrowserArgs, ServerBrowserListingRecord, ServerBrowserQueryRecord,
        ServerBrowserResultRecord, ServerBrowserSort, ServerBrowserState, ServerVisibility,
    },
    mcp_project_server_browser_validation::{server_browser_error, validate_id},
};

pub(super) fn publish_listing(
    args: &ServerBrowserArgs,
    state: &mut ServerBrowserState,
) -> Result<Value, EngineError> {
    let listing_id = required(args.listing_id.as_deref(), "listing_id")?;
    if state
        .listings
        .iter()
        .any(|listing| listing.listing_id == listing_id)
    {
        return Err(server_browser_error(
            "listing_id",
            "server browser listing already exists",
        ));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    if state
        .listings
        .iter()
        .any(|listing| listing.lobby_id == lobby_id)
    {
        return Err(server_browser_error(
            "lobby_id",
            "lobby already has a server browser listing",
        ));
    }
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    let visibility = args
        .visibility
        .ok_or_else(|| server_browser_error("visibility", "server visibility is required"))?;
    let multiplayer = read_multiplayer_session_state(&args.multiplayer_state_path)?;
    let moderation = read_moderation_safety_state(&args.moderation_state_path)?;
    let social = read_social_graph_state(&args.social_state_path)?;
    let lobby = multiplayer
        .state
        .lobbies
        .iter()
        .find(|lobby| lobby.lobby_id == lobby_id)
        .ok_or_else(|| server_browser_error("lobby_id", "lobby is not in multiplayer state"))?;
    let server = multiplayer
        .state
        .servers
        .iter()
        .find(|server| server.lobby_id == lobby_id)
        .ok_or_else(|| {
            server_browser_error("server_id", "running authoritative server is required")
        })?;
    if server.status != super::super::AuthoritativeServerStatus::Running {
        return Err(server_browser_error(
            "server.status",
            "server browser listings require a running server",
        ));
    }
    require_admin_role(&moderation.state.role_assignments, lobby_id, actor_id)?;
    let policy = active_policy(&moderation.state.roleplay_policies, lobby_id)?;
    let active_rules = moderation
        .state
        .access_rules
        .iter()
        .filter(|rule| rule.lobby_id == lobby_id && rule.active)
        .collect::<Vec<_>>();
    let allowlist_enabled = active_rules
        .iter()
        .any(|rule| rule.rule_kind == AccessRuleKind::Allow);
    let ban_rule_count = active_rules
        .iter()
        .filter(|rule| rule.rule_kind == AccessRuleKind::Ban)
        .count();
    let private_invite_count = social
        .state
        .invites
        .iter()
        .filter(|invite| {
            invite.lobby_id == lobby_id
                && matches!(
                    invite.status,
                    GameInviteStatus::Sent | GameInviteStatus::Accepted
                )
        })
        .count();
    if visibility == ServerVisibility::Public && allowlist_enabled {
        return Err(server_browser_error(
            "visibility",
            "public listings cannot be backed by an active allowlist",
        ));
    }
    if visibility == ServerVisibility::PrivateInviteOnly && private_invite_count == 0 {
        return Err(server_browser_error(
            "visibility",
            "private invite-only listings require at least one active invite",
        ));
    }

    let listing = listing_from_sources(ListingFromSourcesArgs {
        listing_id,
        args,
        visibility,
        lobby,
        server_id: &server.server_id,
        policy,
        private_invite_count,
        allowlist_enabled,
        ban_rule_count,
        multiplayer_sha256: &multiplayer.sha256,
        moderation_sha256: &moderation.sha256,
        social_sha256: &social.sha256,
    })?;
    state.listings.push(listing.clone());
    Ok(json!({ "server_listing": listing }))
}

pub(super) fn query_listings(
    args: &ServerBrowserArgs,
    state: &mut ServerBrowserState,
) -> Result<Value, EngineError> {
    let query_id = required(args.query_id.as_deref(), "query_id")?;
    if state.queries.iter().any(|query| query.query_id == query_id) {
        return Err(server_browser_error(
            "query_id",
            "server browser query already exists",
        ));
    }
    let viewer = args.viewer_account_id.as_deref();
    if args.include_private && viewer.is_none() {
        return Err(server_browser_error(
            "viewer_account_id",
            "include_private queries require viewer_account_id",
        ));
    }
    let multiplayer = read_multiplayer_session_state(&args.multiplayer_state_path)?;
    let moderation = read_moderation_safety_state(&args.moderation_state_path)?;
    let social = read_social_graph_state(&args.social_state_path)?;
    if let Some(account_id) = viewer
        && !social
            .state
            .accounts
            .iter()
            .any(|account| account.account_id == account_id)
    {
        return Err(server_browser_error(
            "viewer_account_id",
            "viewer account is not in social graph state",
        ));
    }
    for listing in &mut state.listings {
        let lobby = multiplayer
            .state
            .lobbies
            .iter()
            .find(|lobby| lobby.lobby_id == listing.lobby_id)
            .ok_or_else(|| server_browser_error("listing.lobby_id", "listed lobby is missing"))?;
        let server = multiplayer
            .state
            .servers
            .iter()
            .find(|server| {
                server.server_id == listing.server_id && server.lobby_id == lobby.lobby_id
            })
            .ok_or_else(|| server_browser_error("listing.server_id", "listed server is missing"))?;
        if server.status != super::super::AuthoritativeServerStatus::Running {
            return Err(server_browser_error(
                "listing.server_id",
                "listed server is not running",
            ));
        }
        let policy = active_policy(&moderation.state.roleplay_policies, &listing.lobby_id)?;
        if policy.policy_id != listing.rp_policy_id {
            return Err(server_browser_error(
                "listing.rp_policy_id",
                "listed roleplay policy is stale; republish the listing",
            ));
        }
        listing.current_players = lobby.member_ids.len();
        listing.max_players = lobby.max_players;
        listing.open_slots = lobby.max_players.saturating_sub(lobby.member_ids.len());
        listing.source_multiplayer_sha256 = multiplayer.sha256.clone();
        listing.source_moderation_sha256 = moderation.sha256.clone();
        listing.source_social_sha256 = social.sha256.clone();
    }
    let min_open_slots = args.min_open_slots.unwrap_or(0);
    let sort = args
        .sort
        .ok_or_else(|| server_browser_error("sort", "server browser sort is required"))?;
    let mut hidden_private_count = 0usize;
    let mut results = Vec::new();
    for listing in &state.listings {
        if !listing_matches(listing, &args.required_tags, min_open_slots) {
            continue;
        }
        match listing.visibility {
            ServerVisibility::Public => {
                let connect = listing.public_connect_string.clone().ok_or_else(|| {
                    server_browser_error(
                        "listing.public_connect_string",
                        "public listing is missing connect string",
                    )
                })?;
                results.push(result_from_listing(listing, connect, None));
            }
            ServerVisibility::PrivateInviteOnly => {
                let Some(viewer_account_id) = viewer.filter(|_| args.include_private) else {
                    hidden_private_count += 1;
                    continue;
                };
                let Some((connect, invite_id)) = private_join_connection(
                    &social.state.join_connections,
                    viewer_account_id,
                    listing,
                ) else {
                    hidden_private_count += 1;
                    continue;
                };
                results.push(result_from_listing(listing, connect, Some(invite_id)));
            }
        }
    }
    sort_results(&mut results, sort);
    let query = ServerBrowserQueryRecord {
        query_id: query_id.to_string(),
        viewer_account_id: viewer.map(str::to_string),
        include_private: args.include_private,
        required_tags: args.required_tags.clone(),
        min_open_slots,
        sort,
        result_count: results.len(),
        hidden_private_count,
        results,
    };
    state.queries.push(query.clone());
    Ok(json!({ "server_browser_query": query }))
}

struct ListingFromSourcesArgs<'a> {
    listing_id: &'a str,
    args: &'a ServerBrowserArgs,
    visibility: ServerVisibility,
    lobby: &'a MultiplayerLobbyRecord,
    server_id: &'a str,
    policy: &'a RoleplayPolicyRecord,
    private_invite_count: usize,
    allowlist_enabled: bool,
    ban_rule_count: usize,
    multiplayer_sha256: &'a str,
    moderation_sha256: &'a str,
    social_sha256: &'a str,
}

fn listing_from_sources(
    args: ListingFromSourcesArgs<'_>,
) -> Result<ServerBrowserListingRecord, EngineError> {
    let ListingFromSourcesArgs {
        listing_id,
        args,
        visibility,
        lobby,
        server_id,
        policy,
        private_invite_count,
        allowlist_enabled,
        ban_rule_count,
        multiplayer_sha256,
        moderation_sha256,
        social_sha256,
    } = args;
    let current_players = lobby.member_ids.len();
    let open_slots = lobby.max_players.saturating_sub(current_players);
    let public_connect_string = match visibility {
        ServerVisibility::Public => Some(format!(
            "litd://join/{}?mode=server_browser&host={}&port={}&path={}",
            lobby.lobby_id, lobby.endpoint.host, lobby.endpoint.port, lobby.endpoint.path
        )),
        ServerVisibility::PrivateInviteOnly => None,
    };
    Ok(ServerBrowserListingRecord {
        listing_id: listing_id.to_string(),
        lobby_id: lobby.lobby_id.clone(),
        server_id: server_id.to_string(),
        visibility,
        server_name: required_line(args.server_name.as_deref(), "server_name")?.to_string(),
        title: lobby.title.clone(),
        version: lobby.version.clone(),
        content_address: lobby.content_address.clone(),
        project_id: lobby.project_id.clone(),
        package_id: lobby.package_id.clone(),
        tags: lobby.tags.clone(),
        ruleset_summary: required_line(args.ruleset_summary.as_deref(), "ruleset_summary")?
            .to_string(),
        rp_policy_id: policy.policy_id.clone(),
        rp_policy_mode: format!("{:?}", policy.mode),
        rp_strictness_milli: policy.strictness_milli,
        current_players,
        max_players: lobby.max_players,
        open_slots,
        host_player_id: lobby.host_player_id.clone(),
        endpoint_host: lobby.endpoint.host.clone(),
        endpoint_port: lobby.endpoint.port,
        endpoint_path: lobby.endpoint.path.clone(),
        public_connect_string,
        private_invite_count,
        allowlist_enabled,
        ban_rule_count,
        listed_by_account_id: required(args.actor_account_id.as_deref(), "actor_account_id")?
            .to_string(),
        source_multiplayer_sha256: multiplayer_sha256.to_string(),
        source_moderation_sha256: moderation_sha256.to_string(),
        source_social_sha256: social_sha256.to_string(),
    })
}

fn require_admin_role(
    roles: &[super::super::ServerRoleAssignmentRecord],
    lobby_id: &str,
    account_id: &str,
) -> Result<(), EngineError> {
    let role = roles
        .iter()
        .filter(|role| role.lobby_id == lobby_id && role.account_id == account_id && role.active)
        .map(|role| role.role)
        .max()
        .ok_or_else(|| {
            server_browser_error("actor_account_id", "active admin or owner role required")
        })?;
    if role == ServerRole::Moderator {
        return Err(server_browser_error(
            "actor_account_id",
            "moderators cannot publish server browser listings",
        ));
    }
    Ok(())
}

fn active_policy<'a>(
    policies: &'a [RoleplayPolicyRecord],
    lobby_id: &str,
) -> Result<&'a RoleplayPolicyRecord, EngineError> {
    let policies = policies
        .iter()
        .filter(|policy| policy.lobby_id == lobby_id && policy.active)
        .collect::<Vec<_>>();
    match policies.as_slice() {
        [policy] => Ok(*policy),
        [] => Err(server_browser_error(
            "roleplay_policy",
            "active roleplay policy is required before listing a server",
        )),
        _ => Err(server_browser_error(
            "roleplay_policy",
            "only one active roleplay policy may drive a server listing",
        )),
    }
}

fn private_join_connection(
    connections: &[JoinConnectionRecord],
    account_id: &str,
    listing: &ServerBrowserListingRecord,
) -> Option<(String, String)> {
    connections
        .iter()
        .find(|connection| {
            connection.account_id == account_id
                && connection.lobby_id == listing.lobby_id
                && connection.content_address == listing.content_address
                && connection.source_lobby_host == listing.endpoint_host
                && connection.source_lobby_port == listing.endpoint_port
                && connection.source_lobby_path == listing.endpoint_path
                && connection.invite_id.is_some()
        })
        .map(|connection| {
            (
                connection.connect_string.clone(),
                connection.invite_id.clone().unwrap(),
            )
        })
}

fn listing_matches(
    listing: &ServerBrowserListingRecord,
    required_tags: &[String],
    min_open_slots: usize,
) -> bool {
    listing.open_slots >= min_open_slots
        && required_tags
            .iter()
            .all(|tag| listing.tags.iter().any(|listing_tag| listing_tag == tag))
}

fn result_from_listing(
    listing: &ServerBrowserListingRecord,
    connect_string: String,
    invite_id: Option<String>,
) -> ServerBrowserResultRecord {
    ServerBrowserResultRecord {
        listing_id: listing.listing_id.clone(),
        lobby_id: listing.lobby_id.clone(),
        server_id: listing.server_id.clone(),
        visibility: listing.visibility,
        server_name: listing.server_name.clone(),
        title: listing.title.clone(),
        current_players: listing.current_players,
        max_players: listing.max_players,
        open_slots: listing.open_slots,
        ruleset_summary: listing.ruleset_summary.clone(),
        rp_policy_mode: listing.rp_policy_mode.clone(),
        rp_strictness_milli: listing.rp_strictness_milli,
        connect_string,
        invite_id,
    }
}

fn sort_results(results: &mut [ServerBrowserResultRecord], sort: ServerBrowserSort) {
    match sort {
        ServerBrowserSort::NameAsc => results.sort_by(|left, right| {
            left.server_name
                .cmp(&right.server_name)
                .then_with(|| left.listing_id.cmp(&right.listing_id))
        }),
        ServerBrowserSort::PlayersDesc => results.sort_by(|left, right| {
            right
                .current_players
                .cmp(&left.current_players)
                .then_with(|| left.server_name.cmp(&right.server_name))
        }),
        ServerBrowserSort::RpStrictnessDesc => results.sort_by(|left, right| {
            right
                .rp_strictness_milli
                .cmp(&left.rp_strictness_milli)
                .then_with(|| left.server_name.cmp(&right.server_name))
        }),
        ServerBrowserSort::OpenSlotsDesc => results.sort_by(|left, right| {
            right
                .open_slots
                .cmp(&left.open_slots)
                .then_with(|| left.server_name.cmp(&right.server_name))
        }),
    }
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    let value = value.ok_or_else(|| server_browser_error(field, "required field missing"))?;
    validate_id(field, value)?;
    Ok(value)
}

fn required_line<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    let value = value.ok_or_else(|| server_browser_error(field, "required field missing"))?;
    super::super::mcp_project_tools_validation::validate_single_line(field, value)?;
    Ok(value)
}
