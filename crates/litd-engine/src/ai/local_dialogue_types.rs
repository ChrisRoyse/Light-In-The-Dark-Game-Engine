use serde::{Deserialize, Serialize};

use super::model_tiering_types::AgentModelSelectionReadback;

pub const AI_LOCAL_NPC_DIALOGUE_SCHEMA: &str = "litd.ai.local_npc_dialogue.v1";
pub const AI_LOCAL_NPC_DIALOGUE_HOT_PATH_POLICY: &str =
    "out_of_band_local_llm_lowered_dialogue_only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNpcDialogueIntent {
    ShareGroundedFact,
    AskClarifyingQuestion,
    RefuseOutOfDistribution,
}

impl LocalNpcDialogueIntent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ShareGroundedFact => "share_grounded_fact",
            Self::AskClarifyingQuestion => "ask_clarifying_question",
            Self::RefuseOutOfDistribution => "refuse_out_of_distribution",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxDialogueFact {
    pub fact_id: String,
    pub text: String,
    pub ward_tags: Vec<String>,
    pub provenance_ref: String,
    pub required_dialogue_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNpcDialogueInput {
    pub run_id: String,
    pub prompt_request_id: String,
    pub npc_id: String,
    pub display_name: String,
    pub world_id: String,
    pub prompt_profile_id: String,
    pub role_profile: String,
    pub player_utterance: String,
    pub signal_summary: String,
    pub ward_tags: Vec<String>,
    pub allowed_intents: Vec<LocalNpcDialogueIntent>,
    pub grounding_facts: Vec<CalyxDialogueFact>,
    pub blocked_terms: Vec<String>,
    pub model_selection: AgentModelSelectionReadback,
    pub serving_model: String,
    pub serving_endpoint: String,
    pub max_dialogue_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNpcDialogueModelResponse {
    pub dialogue: String,
    pub intent: LocalNpcDialogueIntent,
    pub voice: String,
    pub grounded_fact_ids: Vec<String>,
    pub ward_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNpcDialogueTelemetry {
    pub model_name: String,
    pub model_digest: String,
    pub done_reason: String,
    pub total_duration_ms: u64,
    pub load_duration_ms: u64,
    pub prompt_eval_count: u64,
    pub eval_count: u64,
    pub warm_pool_loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNpcDialogueLoweredOrder {
    pub order_id: String,
    pub source: String,
    pub npc_id: String,
    pub prompt_request_id: String,
    pub text: String,
    pub intent: LocalNpcDialogueIntent,
    pub grounded_fact_ids: Vec<String>,
    pub provenance_refs: Vec<String>,
    pub deterministic: bool,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNpcDialogueReadback {
    pub schema: String,
    pub run_id: String,
    pub prompt_request_id: String,
    pub npc_id: String,
    pub display_name: String,
    pub world_id: String,
    pub selected_model_id: String,
    pub serving_model: String,
    pub serving_endpoint: String,
    pub model_digest: String,
    pub accepted: bool,
    pub dialogue: String,
    pub intent: LocalNpcDialogueIntent,
    pub voice: String,
    pub grounded_fact_ids: Vec<String>,
    pub ward_tags: Vec<String>,
    pub provenance_refs: Vec<String>,
    pub calyx_fact_count: usize,
    pub prompt_sha256: String,
    pub raw_response_sha256: String,
    pub total_duration_ms: u64,
    pub load_duration_ms: u64,
    pub prompt_eval_count: u64,
    pub eval_count: u64,
    pub warm_pool_loaded: bool,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub lowering_boundary: String,
    pub lowered_order: LocalNpcDialogueLoweredOrder,
    pub readback_fingerprint: String,
}
