use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRenderToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpRenderToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRenderToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub state_path: String,
    pub state_sha256: String,
    pub state_bytes: u64,
    pub state_update_count: u64,
    pub action_summary: String,
    pub affected_material_ids: Vec<String>,
    pub light_count: usize,
    pub animation_clip_count: usize,
    pub scene_id: Option<String>,
    pub scene_placement_count: usize,
    pub post_pass_count: Option<usize>,
    #[serde(default)]
    pub shader_gallery_selected_preset: Option<String>,
    #[serde(default)]
    pub shader_gallery_material_count: usize,
    #[serde(default)]
    pub custom_wgsl_shader_id: Option<String>,
    #[serde(default)]
    pub primitive_gallery_count: usize,
    #[serde(default)]
    pub primitive_gallery_selected_id: Option<String>,
    #[serde(default)]
    pub primitive_gallery_topology_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRenderToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpRenderToolLedger,
}
