//! Grounded world-authoring proposals lowered to deterministic TOML.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundedAbilityRequest {
    pub request_id: String,
    pub prompt: String,
    pub seed: u64,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCorpus {
    pub world_id: String,
    pub revision_id: String,
    pub region: String,
    pub factions: Vec<String>,
    pub lore_terms: Vec<String>,
    pub anchors: Vec<GroundingAnchor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingAnchor {
    pub anchor_id: String,
    pub source_ref: String,
    pub text: String,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssayBalanceProfile {
    pub assay_id: String,
    pub profile_id: String,
    pub min_damage: u32,
    pub max_damage: u32,
    pub min_cooldown_seconds: u32,
    pub max_cooldown_seconds: u32,
    pub max_mana_cost: u32,
    pub allowed_damage_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardPolicy {
    pub policy_id: String,
    pub min_source_anchors: usize,
    pub min_grounded_terms: usize,
    pub blocked_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingReport {
    pub matched_anchor_ids: Vec<String>,
    pub matched_terms: Vec<String>,
    pub blocked_terms: Vec<String>,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringStatus {
    Admitted,
    Provisional,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardDecision {
    pub passed: bool,
    pub code: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedAbilityProposal {
    pub ability_id: String,
    pub display_name: String,
    pub world_id: String,
    pub region: String,
    pub faction: String,
    pub damage_type: String,
    pub damage: u32,
    pub cooldown_seconds: u32,
    pub mana_cost: u32,
    pub tags: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTomlArtifact {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringOutcome {
    pub status: AuthoringStatus,
    pub admission_code: String,
    pub request_id: String,
    pub grounding: GroundingReport,
    pub ward_decision: WardDecision,
    pub proposal: Option<GeneratedAbilityProposal>,
    pub lowered: Option<LoweredTomlArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredAbilityDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub ability: LoweredAbility,
    pub balance: LoweredBalance,
    pub ward: LoweredWard,
    pub provenance: LoweredProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredAbility {
    pub id: String,
    pub name: String,
    pub world_id: String,
    pub region: String,
    pub faction: String,
    pub damage_type: String,
    pub damage: u32,
    pub cooldown_seconds: u32,
    pub mana_cost: u32,
    pub tags: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredBalance {
    pub assay_id: String,
    pub profile_id: String,
    pub damage_min: u32,
    pub damage_max: u32,
    pub cooldown_min_seconds: u32,
    pub cooldown_max_seconds: u32,
    pub mana_cost_max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredWard {
    pub policy_id: String,
    pub passed: bool,
    pub code: String,
    pub matched_anchor_count: usize,
    pub matched_term_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredProvenance {
    pub request_id: String,
    pub requested_by: String,
    pub corpus_revision_id: String,
    pub source_refs: Vec<String>,
    pub matched_terms: Vec<String>,
    pub seed: u64,
    pub lowering_version: String,
}

#[path = "authoring_lowering.rs"]
mod authoring_lowering;
#[path = "authoring_pipeline.rs"]
mod authoring_pipeline;

pub use authoring_lowering::verify_lowered_ability_toml;
pub use authoring_pipeline::generate_grounded_ability;

pub(crate) fn invalid_authoring<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_authoring_error(field, reason))
}

pub(crate) fn invalid_authoring_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.authoring.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "authoring_tests.rs"]
mod tests;
