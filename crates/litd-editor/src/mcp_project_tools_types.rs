use serde::{Deserialize, Serialize};

use crate::{
    history::EditorDocument, mcp_harness_tools::HarnessOracleRunRecord, play::PlayCameraRig,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLifecycleAction {
    New,
    Open,
    SaveSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewOpenSaveProjectArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub action: ProjectLifecycleAction,
    pub project_id: String,
    pub title: String,
    pub version: String,
    pub tags: Vec<String>,
    pub project_path: String,
    pub snapshot_label: String,
    pub document: Option<EditorDocument>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunHeadlessPlaytestArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub project_path: String,
    pub camera_rig: PlayCameraRig,
    pub ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageLitdGameArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub project_path: String,
    pub package_path: String,
    pub share_id: String,
    pub created_by: String,
    pub host_port: u16,
    pub max_package_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishLitdGameArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub project_path: String,
    pub package_path: String,
    pub catalog_path: String,
    pub share_id: String,
    pub created_by: String,
    pub host_port: u16,
    pub max_package_bytes: usize,
    pub title: String,
    pub version: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub thumbnail_path: String,
    pub min_spec_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadProjectArgs {
    pub state_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LitdWorldProject {
    pub schema: String,
    pub issue: u32,
    pub project_id: String,
    pub title: String,
    pub version: String,
    pub tags: Vec<String>,
    pub source_of_truth: String,
    pub snapshot_label: String,
    pub document_fingerprint: String,
    pub document: EditorDocument,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectLifecycleRecord {
    pub project_id: String,
    pub title: String,
    pub version: String,
    pub tags: Vec<String>,
    pub project_path: String,
    pub document_fingerprint: String,
    pub revision_id: String,
    pub object_count: usize,
    pub unit_count: usize,
    pub snapshot_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadlessPlaytestRecord {
    pub project_id: String,
    pub project_path: String,
    pub camera_rig: PlayCameraRig,
    pub player_id: String,
    pub start_tick: u64,
    pub final_tick: u64,
    pub steps: u64,
    pub bodies_updated: u64,
    pub initial_state_hash: u64,
    pub final_state_hash: u64,
    pub spawned_bodies: usize,
    pub visible_instance_count: usize,
    pub draw_batches: usize,
    pub oracle_verdict: HarnessOracleRunRecord,
    pub verdict_reason: String,
    pub verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitdGamePackageRecord {
    pub project_id: String,
    pub package_id: String,
    pub package_path: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub runtime_id: String,
    pub lock_in_policy: String,
    pub host_uri: String,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub world_hash_matches: bool,
    pub runtime_lock_verified: bool,
    pub asset_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedGameRecord {
    pub project_id: String,
    pub package_id: String,
    pub publish_id: String,
    pub content_address: String,
    pub title: String,
    pub version: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub min_spec_tags: Vec<String>,
    pub package_path: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub thumbnail_path: String,
    pub thumbnail_sha256: String,
    pub thumbnail_bytes: usize,
    pub thumbnail_media_type: String,
    pub runtime_id: String,
    pub lock_in_policy: String,
    pub asset_count: usize,
    pub published_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcosystemPublishCatalog {
    pub schema: String,
    pub issue: u32,
    pub catalog_id: String,
    pub source_of_truth: String,
    pub entry_count: usize,
    pub entries: Vec<PublishedGameRecord>,
}
