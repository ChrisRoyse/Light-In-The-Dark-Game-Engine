use litd_engine::net_authority::NetAuthorityProof;
use serde::{Deserialize, Serialize};

pub const MULTIPLAYER_SESSION_TOOL: &str = "litd.editor.project.manage_multiplayer_session";
pub const MULTIPLAYER_SESSION_STATE_SCHEMA: &str = "litd.ecosystem.multiplayer.state.v4";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiplayerAction {
    HostLobby,
    StartAuthoritativeServer,
    JoinLobby,
    CreateParty,
    Matchmake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiplayerTransport {
    Loopback,
    Lan,
    DirectIp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiplayerLobbyStatus {
    Open,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritativeServerStatus {
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerAuthorityMode {
    HostAuthoritative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchTicketStatus {
    Matched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiplayerEndpoint {
    pub transport: MultiplayerTransport,
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiplayerSessionArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub hub_state_path: String,
    pub install_manifest_path: String,
    pub multiplayer_state_path: String,
    pub multiplayer_state_id: String,
    pub action: MultiplayerAction,
    #[serde(default)]
    pub lobby_id: Option<String>,
    #[serde(default)]
    pub party_id: Option<String>,
    #[serde(default)]
    pub ticket_id: Option<String>,
    #[serde(default)]
    pub queue_id: Option<String>,
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub content_address: Option<String>,
    #[serde(default)]
    pub player_id: Option<String>,
    #[serde(default)]
    pub joining_package_path: Option<String>,
    #[serde(default)]
    pub host_player_id: Option<String>,
    #[serde(default)]
    pub leader_player_id: Option<String>,
    #[serde(default)]
    pub member_ids: Vec<String>,
    #[serde(default)]
    pub required_tags: Vec<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub max_players: Option<usize>,
    #[serde(default)]
    pub endpoint: Option<MultiplayerEndpoint>,
    #[serde(default)]
    pub deterministic_tick_rate_hz: Option<u16>,
    #[serde(default)]
    pub simulation_probe_ticks: Option<u64>,
    #[serde(default)]
    pub calyx_agent_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiplayerLobbyRecord {
    pub lobby_id: String,
    pub content_address: String,
    pub project_id: String,
    pub package_id: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub title: String,
    pub version: String,
    pub tags: Vec<String>,
    pub min_spec_tags: Vec<String>,
    pub endpoint: MultiplayerEndpoint,
    pub host_player_id: String,
    pub max_players: usize,
    pub member_ids: Vec<String>,
    pub slots: Vec<MultiplayerLobbySlotRecord>,
    pub status: MultiplayerLobbyStatus,
    pub installed_package_path: String,
    pub installed_package_sha256: String,
    pub installed_package_bytes: usize,
    pub runtime_lock_verified: bool,
    pub playable_on_litd_runtime: bool,
    pub hosted_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiplayerLobbySlotRecord {
    pub slot_index: usize,
    pub player_id: String,
    pub team_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeServerRecord {
    pub server_id: String,
    pub lobby_id: String,
    pub content_address: String,
    pub project_id: String,
    pub package_id: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub installed_package_path: String,
    pub installed_package_sha256: String,
    pub installed_package_bytes: usize,
    pub endpoint: MultiplayerEndpoint,
    pub host_player_id: String,
    pub max_players: usize,
    pub status: AuthoritativeServerStatus,
    pub authority_mode: ServerAuthorityMode,
    pub deterministic_tick_rate_hz: u16,
    pub fixed_step_nanos: u64,
    pub simulation_probe_ticks: u64,
    pub simulation_start_tick: u64,
    pub simulation_final_tick: u64,
    pub simulation_initial_state_hash: u64,
    pub simulation_final_state_hash: u64,
    pub simulation_bodies_updated: u64,
    pub runtime_lock_verified: bool,
    pub playable_on_litd_runtime: bool,
    pub calyx_agent_fleet_enabled: bool,
    pub calyx_agent_count: usize,
    pub calyx_agent_budget_ledger_id: String,
    pub calyx_agent_budget_tier_count: usize,
    pub calyx_agent_budget_all_tiers_within_budget: bool,
    pub calyx_agent_budget_max_elapsed_micros: u64,
    #[serde(default)]
    pub net_authority_proof: Option<NetAuthorityProof>,
    pub source_document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub started_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyRecord {
    pub party_id: String,
    pub leader_player_id: String,
    pub member_ids: Vec<String>,
    pub max_players: usize,
    pub invite_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRecord {
    pub join_id: String,
    pub lobby_id: String,
    pub player_id: String,
    pub content_address: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub runtime_lock_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchTicketRecord {
    pub ticket_id: String,
    pub queue_id: String,
    pub player_id: Option<String>,
    pub party_id: Option<String>,
    pub required_tags: Vec<String>,
    pub member_ids: Vec<String>,
    pub matched_lobby_id: String,
    pub status: MatchTicketStatus,
    pub rule_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiplayerSessionState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub hub_state_path: String,
    pub install_manifest_path: String,
    pub lobby_count: usize,
    pub party_count: usize,
    pub join_count: usize,
    pub ticket_count: usize,
    pub server_count: usize,
    pub running_server_count: usize,
    pub lobbies: Vec<MultiplayerLobbyRecord>,
    pub servers: Vec<AuthoritativeServerRecord>,
    pub parties: Vec<PartyRecord>,
    pub joins: Vec<JoinRecord>,
    pub tickets: Vec<MatchTicketRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiplayerSessionStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: MultiplayerSessionState,
}
