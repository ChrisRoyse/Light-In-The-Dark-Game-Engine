use litd_engine::game_package::RuntimeGameRunReport;
use serde::{Deserialize, Serialize};

pub const HUB_LAUNCHER_TOOL: &str = "litd.editor.project.open_hub_launcher";
pub const HUB_LAUNCHER_STATE_SCHEMA: &str = "litd.ecosystem.hub_launcher.state.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubSortMode {
    // Explicit serde renames pin the persisted `*_asc` wire strings (this is a
    // versioned state schema, HUB_LAUNCHER_STATE_SCHEMA .v1) so dropping the
    // redundant Rust-side `Asc` postfix (clippy::enum_variant_names) does not
    // change the serialized form.
    #[serde(rename = "title_asc")]
    Title,
    #[serde(rename = "version_asc")]
    Version,
    #[serde(rename = "content_address_asc")]
    ContentAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubLauncherArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub catalog_path: String,
    pub install_manifest_path: String,
    pub hub_state_path: String,
    pub hub_id: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub required_tags: Vec<String>,
    #[serde(default)]
    pub installed_only: bool,
    pub sort: HubSortMode,
    #[serde(default)]
    pub launch_content_address: Option<String>,
    #[serde(default)]
    pub launched_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubGameCard {
    pub project_id: String,
    pub package_id: String,
    pub publish_id: String,
    pub content_address: String,
    pub title: String,
    pub version: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub min_spec_tags: Vec<String>,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub thumbnail_path: String,
    pub installed: bool,
    pub install_tool_name: String,
    pub installed_package_path: Option<String>,
    pub installed_package_sha256: Option<String>,
    pub installed_package_bytes: Option<usize>,
    pub launchable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubLaunchRecord {
    pub launch_id: String,
    pub content_address: String,
    pub package_id: String,
    pub installed_package_path: String,
    pub launched_by: String,
    pub runtime_report: RuntimeGameRunReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubLauncherState {
    pub schema: String,
    pub issue: u32,
    pub hub_id: String,
    pub source_of_truth: String,
    pub catalog_path: String,
    pub install_manifest_path: String,
    pub query: Option<String>,
    pub required_tags: Vec<String>,
    pub installed_only: bool,
    pub sort: HubSortMode,
    pub result_count: usize,
    pub cards: Vec<HubGameCard>,
    pub launch_count: usize,
    pub launches: Vec<HubLaunchRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubLauncherStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: HubLauncherState,
}
