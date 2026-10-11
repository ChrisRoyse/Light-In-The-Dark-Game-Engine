use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPipelineRecord {
    pub asset_id: String,
    pub display_name: String,
    pub asset_type: String,
    pub source_kind: String,
    pub prompt: String,
    pub source_ref: String,
    pub content_sha256: String,
    pub license: String,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPreparation {
    pub asset_id: String,
    pub mesh_ref: String,
    pub rig_ref: String,
    pub texture_refs: Vec<String>,
    pub animation_refs: Vec<String>,
    pub ready_for_runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetBudgetConformance {
    pub asset_id: String,
    pub triangle_count: u32,
    pub material_count: u32,
    pub texture_bytes: u64,
    pub draw_call_estimate: u32,
    pub max_triangle_count: u32,
    pub max_material_count: u32,
    pub max_texture_bytes: u64,
    pub max_draw_calls: u32,
    pub calyx_ingest_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateImportAssetArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub asset: AssetPipelineRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigTextureAnimateArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub preparation: AssetPreparation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformIngestBudgetArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub budget: AssetBudgetConformance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadAssetsArgs {
    pub state_path: PathBuf,
}
