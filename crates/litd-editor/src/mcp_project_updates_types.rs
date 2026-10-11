use serde::{Deserialize, Serialize};

pub const GAME_UPDATES_TOOL: &str = "litd.editor.project.manage_game_updates";
pub const GAME_UPDATES_STATE_SCHEMA: &str = "litd.ecosystem.game_updates.state.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameUpdateAction {
    RegisterRelease,
    EvaluateCompatibility,
    AcknowledgeNotification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateNotificationStatus {
    Pending,
    Acknowledged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameUpdatesArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub updates_state_path: String,
    pub updates_state_id: String,
    pub catalog_path: String,
    pub install_manifest_path: String,
    pub social_state_path: String,
    pub action: GameUpdateAction,
    #[serde(default)]
    pub release_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub from_content_address: Option<String>,
    #[serde(default)]
    pub to_content_address: Option<String>,
    #[serde(default)]
    pub min_client_protocol: Option<u32>,
    #[serde(default)]
    pub min_server_protocol: Option<u32>,
    #[serde(default)]
    pub min_save_schema: Option<u32>,
    #[serde(default)]
    pub max_save_schema: Option<u32>,
    #[serde(default)]
    pub release_notes: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub client_protocol: Option<u32>,
    #[serde(default)]
    pub server_protocol: Option<u32>,
    #[serde(default)]
    pub save_schema_version: Option<u32>,
    #[serde(default)]
    pub notification_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateChannelRecord {
    pub channel_id: String,
    pub project_id: String,
    pub latest_release_id: String,
    pub latest_content_address: String,
    pub release_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameUpdateReleaseRecord {
    pub release_id: String,
    pub channel_id: String,
    pub project_id: String,
    pub package_id: String,
    pub version: String,
    pub from_content_address: String,
    pub to_content_address: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub runtime_id: String,
    pub min_client_protocol: u32,
    pub min_server_protocol: u32,
    pub min_save_schema: u32,
    pub max_save_schema: u32,
    pub release_notes: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityRecord {
    pub compatibility_id: String,
    pub release_id: String,
    pub account_id: String,
    pub client_protocol: u32,
    pub server_protocol: u32,
    pub save_schema_version: u32,
    pub compatible: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateNotificationRecord {
    pub notification_id: String,
    pub release_id: String,
    pub account_id: String,
    pub from_content_address: String,
    pub to_content_address: String,
    pub status: UpdateNotificationStatus,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameUpdatesState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub catalog_path: String,
    pub install_manifest_path: String,
    pub social_state_path: String,
    pub channel_count: usize,
    pub release_count: usize,
    pub compatibility_count: usize,
    pub notification_count: usize,
    pub channels: Vec<UpdateChannelRecord>,
    pub releases: Vec<GameUpdateReleaseRecord>,
    pub compatibilities: Vec<CompatibilityRecord>,
    pub notifications: Vec<UpdateNotificationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameUpdatesStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: GameUpdatesState,
}
