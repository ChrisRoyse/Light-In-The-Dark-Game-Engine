use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsvArtifactKind {
    WorldDocument,
    WorldLedger,
    ObjectDocument,
    ObjectLedger,
    TriggerState,
    SoundRecord,
    SoundRuntime,
    TriggerLedger,
    StoryState,
    StoryLedger,
    LoreCanonQueryLedger,
    LoreCowriterLedger,
    LoreTemplateLedger,
    RenderState,
    RenderLedger,
    VfxLedger,
    AgentState,
    AgentLedger,
    AssetState,
    AssetLedger,
    ProjectState,
    ProjectLedger,
    LitdWorld,
    LitdGame,
    FsvReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsvArtifactRequest {
    pub label: String,
    pub kind: FsvArtifactKind,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsvReadArtifactsArgs {
    pub artifacts: Vec<FsvArtifactRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FsvArtifactReadback {
    pub label: String,
    pub kind: FsvArtifactKind,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub summary: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FsvToolReadback {
    pub schema: String,
    pub issue: u32,
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub artifact_count: usize,
    pub aggregate_sha256: String,
    pub artifacts: Vec<FsvArtifactReadback>,
}
