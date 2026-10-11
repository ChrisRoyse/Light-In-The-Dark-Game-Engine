use serde::{Deserialize, Serialize};

pub const SERVER_BROWSER_TOOL: &str = "litd.editor.project.manage_server_browser";
pub const SERVER_BROWSER_STATE_SCHEMA: &str = "litd.ecosystem.server_browser.state.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerBrowserAction {
    PublishListing,
    QueryListings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerVisibility {
    Public,
    PrivateInviteOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerBrowserSort {
    NameAsc,
    PlayersDesc,
    RpStrictnessDesc,
    OpenSlotsDesc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBrowserArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub server_browser_state_path: String,
    pub server_browser_state_id: String,
    pub multiplayer_state_path: String,
    pub moderation_state_path: String,
    pub social_state_path: String,
    pub action: ServerBrowserAction,
    #[serde(default)]
    pub listing_id: Option<String>,
    #[serde(default)]
    pub query_id: Option<String>,
    #[serde(default)]
    pub lobby_id: Option<String>,
    #[serde(default)]
    pub actor_account_id: Option<String>,
    #[serde(default)]
    pub viewer_account_id: Option<String>,
    #[serde(default)]
    pub visibility: Option<ServerVisibility>,
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub ruleset_summary: Option<String>,
    #[serde(default)]
    pub required_tags: Vec<String>,
    #[serde(default)]
    pub min_open_slots: Option<usize>,
    #[serde(default)]
    pub sort: Option<ServerBrowserSort>,
    #[serde(default)]
    pub include_private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBrowserListingRecord {
    pub listing_id: String,
    pub lobby_id: String,
    pub server_id: String,
    pub visibility: ServerVisibility,
    pub server_name: String,
    pub title: String,
    pub version: String,
    pub content_address: String,
    pub project_id: String,
    pub package_id: String,
    pub tags: Vec<String>,
    pub ruleset_summary: String,
    pub rp_policy_id: String,
    pub rp_policy_mode: String,
    pub rp_strictness_milli: u16,
    pub current_players: usize,
    pub max_players: usize,
    pub open_slots: usize,
    pub host_player_id: String,
    pub endpoint_host: String,
    pub endpoint_port: u16,
    pub endpoint_path: String,
    pub public_connect_string: Option<String>,
    pub private_invite_count: usize,
    pub allowlist_enabled: bool,
    pub ban_rule_count: usize,
    pub listed_by_account_id: String,
    pub source_multiplayer_sha256: String,
    pub source_moderation_sha256: String,
    pub source_social_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBrowserResultRecord {
    pub listing_id: String,
    pub lobby_id: String,
    pub server_id: String,
    pub visibility: ServerVisibility,
    pub server_name: String,
    pub title: String,
    pub current_players: usize,
    pub max_players: usize,
    pub open_slots: usize,
    pub ruleset_summary: String,
    pub rp_policy_mode: String,
    pub rp_strictness_milli: u16,
    pub connect_string: String,
    pub invite_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBrowserQueryRecord {
    pub query_id: String,
    pub viewer_account_id: Option<String>,
    pub include_private: bool,
    pub required_tags: Vec<String>,
    pub min_open_slots: usize,
    pub sort: ServerBrowserSort,
    pub result_count: usize,
    pub hidden_private_count: usize,
    pub results: Vec<ServerBrowserResultRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBrowserState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub multiplayer_state_path: String,
    pub moderation_state_path: String,
    pub social_state_path: String,
    pub listing_count: usize,
    pub public_listing_count: usize,
    pub private_listing_count: usize,
    pub query_count: usize,
    pub listings: Vec<ServerBrowserListingRecord>,
    pub queries: Vec<ServerBrowserQueryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerBrowserStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: ServerBrowserState,
}
