use serde::{Deserialize, Serialize};

pub const SOCIAL_GRAPH_TOOL: &str = "litd.editor.project.manage_social_graph";
pub const SOCIAL_GRAPH_STATE_SCHEMA: &str = "litd.ecosystem.social_graph.state.v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialGraphAction {
    CreateAccountProfile,
    SendFriendRequest,
    AcceptFriendRequest,
    SetPresence,
    SendLobbyInvite,
    AcceptLobbyInvite,
    CreateJoinConnection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendshipStatus {
    Pending,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    Offline,
    Online,
    Away,
    InLobby,
    InGame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameInviteStatus {
    Sent,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinConnectionMode {
    Localhost,
    Lan,
    SshTunnel,
    DirectIp,
    InviteCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialGraphArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub social_state_path: String,
    pub social_state_id: String,
    pub multiplayer_state_path: String,
    pub action: SocialGraphAction,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub target_account_id: Option<String>,
    #[serde(default)]
    pub player_id: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_asset_path: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub presence_status: Option<PresenceStatus>,
    #[serde(default)]
    pub lobby_id: Option<String>,
    #[serde(default)]
    pub rich_presence: Option<String>,
    #[serde(default)]
    pub invite_id: Option<String>,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub join_connection_mode: Option<JoinConnectionMode>,
    #[serde(default)]
    pub connect_host: Option<String>,
    #[serde(default)]
    pub connect_port: Option<u16>,
    #[serde(default)]
    pub connect_path: Option<String>,
    #[serde(default)]
    pub ssh_user: Option<String>,
    #[serde(default)]
    pub ssh_host: Option<String>,
    #[serde(default)]
    pub ssh_remote_host: Option<String>,
    #[serde(default)]
    pub ssh_remote_port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRecord {
    pub account_id: String,
    pub player_id: String,
    pub handle: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileRecord {
    pub profile_id: String,
    pub account_id: String,
    pub display_name: String,
    pub avatar_asset_path: Option<String>,
    pub bio: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendshipRecord {
    pub friendship_id: String,
    pub requester_account_id: String,
    pub addressee_account_id: String,
    pub status: FriendshipStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceRecord {
    pub account_id: String,
    pub player_id: String,
    pub status: PresenceStatus,
    pub lobby_id: Option<String>,
    pub rich_presence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameInviteRecord {
    pub invite_id: String,
    pub from_account_id: String,
    pub to_account_id: String,
    pub lobby_id: String,
    pub content_address: String,
    pub connect_string: String,
    pub status: GameInviteStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinConnectionRecord {
    pub connection_id: String,
    pub mode: JoinConnectionMode,
    pub account_id: String,
    pub player_id: String,
    pub lobby_id: String,
    pub invite_id: Option<String>,
    pub content_address: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub connect_string: String,
    pub requires_existing_ssh_tunnel: bool,
    pub ssh_user: Option<String>,
    pub ssh_host: Option<String>,
    pub ssh_remote_host: Option<String>,
    pub ssh_remote_port: Option<u16>,
    pub source_lobby_host: String,
    pub source_lobby_port: u16,
    pub source_lobby_path: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialGraphState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub multiplayer_state_path: String,
    pub account_count: usize,
    pub profile_count: usize,
    pub friendship_count: usize,
    pub presence_count: usize,
    pub invite_count: usize,
    pub connection_count: usize,
    pub accounts: Vec<AccountRecord>,
    pub profiles: Vec<ProfileRecord>,
    pub friendships: Vec<FriendshipRecord>,
    pub presences: Vec<PresenceRecord>,
    pub invites: Vec<GameInviteRecord>,
    pub join_connections: Vec<JoinConnectionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialGraphStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: SocialGraphState,
}
