use serde::{Deserialize, Serialize};

pub const DELIVERY_LITDGAME_TOOL: &str = "litd.editor.project.deliver_litdgame";
pub const DELIVERY_STORE_SCHEMA: &str = "litd.ecosystem.delivery_store.v1";
pub const LITDGAME_INSTALL_MANIFEST_SCHEMA: &str = "litd.ecosystem.install_manifest.v1";
pub const LITDGAME_PACKAGE_MEDIA_TYPE: &str = "application/vnd.litd.game.package.v1+json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverLitdGameArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub catalog_path: String,
    pub delivery_manifest_path: String,
    pub delivery_store_id: String,
    pub blob_root_path: String,
    pub install_manifest_path: String,
    pub install_id: String,
    pub install_root_path: String,
    pub content_address: String,
    pub delivered_by: String,
    pub installed_by: String,
    #[serde(default)]
    pub source_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveredGameRecord {
    pub project_id: String,
    pub package_id: String,
    pub publish_id: String,
    pub content_address: String,
    pub media_type: String,
    pub digest: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub source_package_path: String,
    pub stored_blob_path: String,
    pub store_relative_path: String,
    pub stored_blob_sha256: String,
    pub stored_blob_bytes: usize,
    pub runtime_id: String,
    pub lock_in_policy: String,
    pub delivered_by: String,
    /// #1513: number of declared assets whose bytes were bundled (content-addressed by
    /// `source_sha256`) into the delivery store alongside the package JSON, so an
    /// installed package can be made self-contained and byte-verifiable.
    pub bundled_asset_count: usize,
    /// #1513: total declared bytes of the bundled asset payloads.
    pub bundled_asset_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledGameRecord {
    pub project_id: String,
    pub package_id: String,
    pub publish_id: String,
    pub content_address: String,
    pub source_blob_path: String,
    pub installed_package_path: String,
    pub installed_package_sha256: String,
    pub installed_package_bytes: usize,
    pub runtime_id: String,
    pub lock_in_policy: String,
    pub world_hash_matches: bool,
    pub runtime_lock_verified: bool,
    pub installed_by: String,
    /// #1513: every declared asset's bytes were materialized co-located with the
    /// installed package and verified against its `source_sha256` at load. When true the
    /// installed package is self-contained and byte-integrity-verified.
    pub asset_bytes_verified: bool,
    /// #1513: number of declared asset byte payloads materialized next to the package.
    pub materialized_asset_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryStoreManifest {
    pub schema: String,
    pub issue: u32,
    pub store_id: String,
    pub source_of_truth: String,
    pub blob_root_path: String,
    pub entry_count: usize,
    pub entries: Vec<DeliveredGameRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitdGameInstallManifest {
    pub schema: String,
    pub issue: u32,
    pub install_id: String,
    pub source_of_truth: String,
    pub install_root_path: String,
    pub record_count: usize,
    pub records: Vec<InstalledGameRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryStoreManifestReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub manifest: DeliveryStoreManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitdGameInstallManifestReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub manifest: LitdGameInstallManifest,
}

pub struct BlobWriteback {
    pub path: String,
    pub relative_path: String,
    pub bytes: usize,
    pub sha256: String,
    pub deduplicated: bool,
}
