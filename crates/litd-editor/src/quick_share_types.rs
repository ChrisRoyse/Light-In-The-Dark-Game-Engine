use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use litd_engine::game_package::{
    LITD_LOCK_IN_POLICY, LITD_LOWERED_DOCUMENT_SCHEMA,
    LITD_QUICK_SHARE_SCHEMA as QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimeCampaignManifest,
};
pub use litd_engine::game_runtime::RuntimeDatasetsManifest;

pub const QUICK_SHARE_LEDGER_SCHEMA: &str = "litd.quick_share.ledger.v1";
pub const QUICK_SHARE_GUI_ROUTE: &str = "egui://editor.project/quick_share";
pub const QUICK_SHARE_TOOL_NAME: &str = "litd.editor.project.quick_share_playtest";
pub const QUICK_SHARE_READBACK_TOOL: &str = "litd.editor.project.read_quick_share";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickShareHostMode {
    Loopback,
    Lan,
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickShareEndpoint {
    pub mode: QuickShareHostMode,
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickShareBuildRequest {
    pub share_id: String,
    pub created_by: String,
    pub endpoint: QuickShareEndpoint,
    pub runtime_id: String,
    pub max_package_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickShareAssetRef {
    pub product_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub source_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickShareSourceProvenance {
    pub calyx_document_id: String,
    pub revision_id: String,
    pub source_document_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickSharePackage {
    pub schema: String,
    pub share_id: String,
    pub package_id: String,
    pub created_by: String,
    pub runtime_id: String,
    pub lock_in_policy: String,
    pub gui_route: String,
    pub tool_name: String,
    pub readback_tool_name: String,
    pub endpoint: QuickShareEndpoint,
    pub host_uri: String,
    pub document: Value,
    pub source_provenance: QuickShareSourceProvenance,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub asset_count: usize,
    pub assets: Vec<QuickShareAssetRef>,
    pub manifest_fingerprint: String,
    /// #853: present only for single-player campaign packages; carries the full
    /// multi-map campaign manifest that `litd-game` validates at package validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign: Option<RuntimeCampaignManifest>,
    /// #1843: present when the package carries the lowered runtime datasets (economy
    /// ruleset + match blueprint + runtime constants) emitted by the editor object-data
    /// lowering valve; `litd-game` validates + instantiates the runtime from these.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeDatasetsManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickSharePackageWriteback {
    pub package_path: String,
    pub bytes: usize,
    pub sha256: String,
    pub host_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickShareHostResolution {
    pub host_uri: String,
    pub mode: QuickShareHostMode,
    pub resolved_host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickShareClientReadback {
    pub package_id: String,
    pub loaded_by: String,
    pub runtime_id: String,
    pub package_sha256: String,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub world_hash_matches: bool,
    pub runtime_lock_verified: bool,
    pub host_resolution: QuickShareHostResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickShareLedger {
    pub schema: String,
    pub ledger_id: String,
    pub share_id: String,
    pub package_id: String,
    pub package_path: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub host_uri: String,
    pub resolved_host: String,
    pub document_fingerprint: String,
    pub client_document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub client_play_build_fingerprint: String,
    pub world_hash_matches: bool,
    pub runtime_lock_verified: bool,
    pub asset_count: usize,
}

impl QuickShareEndpoint {
    pub fn loopback(share_id: &str, port: u16) -> Self {
        Self {
            mode: QuickShareHostMode::Loopback,
            host: "127.0.0.1".to_string(),
            port,
            path: format!("/share/{share_id}"),
        }
    }

    pub fn uri(&self) -> String {
        format!("litd-share://{}:{}{}", self.host, self.port, self.path)
    }
}

impl QuickShareBuildRequest {
    pub fn new(
        share_id: impl Into<String>,
        created_by: impl Into<String>,
        endpoint: QuickShareEndpoint,
        max_package_bytes: usize,
    ) -> Self {
        Self {
            share_id: share_id.into(),
            created_by: created_by.into(),
            endpoint,
            runtime_id: LITD_RUNTIME_ID.to_string(),
            max_package_bytes,
        }
    }
}
