use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    calyx_content::EngineContentKind,
    calyx_persistence::{EngineCalyxDatabaseReadback, EngineCalyxPersistenceError},
    calyx_semantic::EngineCalyxSemanticError,
};

pub const ENGINE_CALYX_EDITOR_INTELLIGENCE_SCHEMA: &str =
    "litd.engine.calyx_editor_intelligence.v1";
pub const ENGINE_CALYX_EDITOR_INTELLIGENCE_READBACK_SCHEMA: &str =
    "litd.engine.calyx_editor_intelligence_readback.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineEditorDraftKind {
    Ability,
    Unit,
}

impl EngineEditorDraftKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ability => "ability",
            Self::Unit => "unit",
        }
    }

    pub const fn content_kind(self) -> EngineContentKind {
        match self {
            Self::Ability => EngineContentKind::Ability,
            Self::Unit => EngineContentKind::Unit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineEditorAuthoringRequest {
    pub action_id: String,
    pub semantic_index_id: String,
    pub prompt: String,
    pub draft_kind: EngineEditorDraftKind,
    pub draft_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorDraft {
    pub draft_id: String,
    pub draft_kind: EngineEditorDraftKind,
    pub base_content_id: String,
    pub base_display_name: String,
    pub payload: Value,
    pub provenance_refs: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorBalanceSample {
    pub sample_id: String,
    pub stats: BTreeMap<String, f32>,
    pub outcome_win: bool,
    pub provenance_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorBalanceStat {
    pub stat_name: String,
    pub slot_id: u16,
    pub marginal_bits: f32,
    pub sole_carrier: bool,
    pub min_value: f32,
    pub max_value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorBalanceReport {
    pub sample_count: usize,
    pub target_entropy_bits: f32,
    pub total_bits: f32,
    pub sufficient: bool,
    pub top_stat: String,
    pub stats: Vec<EngineEditorBalanceStat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineEditorKernelAnswerRequest {
    pub action_id: String,
    pub semantic_index_id: String,
    pub query: String,
    pub max_kernel_hits: usize,
    pub min_grounding_hits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineEditorKernelAnswer {
    pub refused: bool,
    pub refusal_code: Option<String>,
    pub answer: String,
    pub kernel_hit_count: usize,
    pub kernel_content_ids: Vec<String>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorOracleRequest {
    pub action_id: String,
    pub panel_bits: f32,
    pub anchor_entropy_bits: f32,
    pub requested_confidence: f32,
    pub desired_outcome: String,
    pub cause_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorOracleForecast {
    pub status: String,
    pub confidence: f32,
    pub deficit_bits: f32,
    pub desired_outcome: String,
    pub reverse_causes: Vec<String>,
    pub refusal_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorIntelligenceLedger {
    pub schema: String,
    pub action_id: String,
    pub operation: String,
    pub payload: Value,
    pub result_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineEditorIntelligenceStoredReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub ledger: EngineEditorIntelligenceLedger,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineEditorIntelligenceError {
    #[error("engine Calyx editor intelligence field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx editor intelligence persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx editor intelligence semantic search failed: {0}")]
    Semantic(#[from] EngineCalyxSemanticError),
    #[error("engine Calyx editor intelligence Assay failed {code}: {reason}")]
    Assay { code: String, reason: String },
    #[error("engine Calyx editor intelligence codec failed: {0}")]
    Codec(String),
}
