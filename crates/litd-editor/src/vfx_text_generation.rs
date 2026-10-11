use std::{collections::BTreeSet, path::Path};

use litd_engine::{
    EngineError,
    render::vfx::{
        VfxEffectAsset, VfxEffectDescriptor, VfxEffectStoreWriteback, VfxValidationReport,
        validate_effect, write_effect_to_content_store,
    },
};
use serde::{Deserialize, Serialize};

use crate::mcp_tools::default_mcp_domain_toolset_manifest;

#[path = "vfx_text_generation_effect.rs"]
mod vfx_text_generation_effect;
#[path = "vfx_text_generation_store.rs"]
mod vfx_text_generation_store;
#[path = "vfx_text_generation_taxonomy.rs"]
mod vfx_text_generation_taxonomy;

use vfx_text_generation_effect::build_effect;
pub use vfx_text_generation_store::{
    VfxTextGenerationLedgerReadback, parse_vfx_text_generation_ledger,
    read_vfx_text_generation_ledger, write_vfx_text_generation_ledger,
};
use vfx_text_generation_taxonomy::{classify_taxonomy, particle_budget, prompt_tokens};

pub const VFX_TEXT_GENERATION_SCHEMA: &str = "litd.vfx.text_generation.ledger.v1";
const DESCRIBE_EFFECT_OPERATION_ID: &str = "engine.vfx.describe_effect_asset";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxTextGenerationRequest {
    pub request_id: String,
    pub prompt: String,
    pub seed: u64,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxTextElement {
    Fire,
    Lightning,
    Frost,
    Arcane,
    Smoke,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxTextPattern {
    Explosion,
    Bolt,
    Aura,
    Trail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxTextRegion {
    Point,
    Sphere,
    Cone,
    Line,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxTextTaxonomy {
    pub element: VfxTextElement,
    pub pattern: VfxTextPattern,
    pub region: VfxTextRegion,
    pub features: Vec<String>,
    pub source_of_truth: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxParticleBudget {
    pub requested_max_particles: u32,
    pub emitted_max_particles: u32,
    pub max_allowed_particles: u32,
    pub capped: bool,
    pub cap_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxTextGenerationProposal {
    pub operation_id: String,
    pub readback_tool_name: String,
    pub request: VfxTextGenerationRequest,
    pub taxonomy: VfxTextTaxonomy,
    pub budget: VfxParticleBudget,
    pub effect: VfxEffectAsset,
    pub validation: VfxValidationReport,
    pub source_anchors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxTextGenerationRecord {
    pub request_id: String,
    pub prompt: String,
    pub requested_by: String,
    pub operation_id: String,
    pub readback_tool_name: String,
    pub taxonomy: VfxTextTaxonomy,
    pub budget: VfxParticleBudget,
    pub source_anchors: Vec<String>,
    pub effect_id: String,
    pub descriptor: VfxEffectDescriptor,
    pub validation: VfxValidationReport,
    pub content_object_path: String,
    pub content_object_sha256: String,
    pub content_object_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxTextGenerationLedger {
    pub schema: String,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<VfxTextGenerationRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredVfxTextGeneration {
    pub proposal: VfxTextGenerationProposal,
    pub content_writeback: VfxEffectStoreWriteback,
    pub record: VfxTextGenerationRecord,
    pub ledger: VfxTextGenerationLedger,
    pub ledger_readback: VfxTextGenerationLedgerReadback,
}

pub fn generate_vfx_text_proposal(
    request: VfxTextGenerationRequest,
) -> Result<VfxTextGenerationProposal, EngineError> {
    validate_request(&request)?;
    let (operation_id, readback_tool_name) = validate_vfx_mcp_route()?;
    let tokens = prompt_tokens(&request.prompt);
    let taxonomy = classify_taxonomy(&tokens)?;
    let budget = particle_budget(&tokens, &taxonomy);
    let effect = build_effect(&taxonomy, &budget)?;
    let validation = validate_effect(&effect)?;
    Ok(VfxTextGenerationProposal {
        operation_id,
        readback_tool_name,
        request,
        taxonomy,
        budget,
        effect,
        validation,
        source_anchors: vec![
            "schema://render/vfx/effect.v1".to_string(),
            "mcp://litd.engine.vfx.describe_effect_asset".to_string(),
            "taxonomy://effectmaker/element-pattern-region".to_string(),
        ],
    })
}

pub fn generate_and_store_vfx_text_effect(
    state_path: impl AsRef<Path>,
    aster_dir: impl AsRef<Path>,
    store_root: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    request: VfxTextGenerationRequest,
) -> Result<StoredVfxTextGeneration, EngineError> {
    let proposal = generate_vfx_text_proposal(request)?;
    let content_writeback = write_effect_to_content_store(store_root, &proposal.effect)?;
    let record = record_from_writeback(&proposal, &content_writeback);
    let ledger = build_vfx_text_generation_ledger(ledger_id, vec![record.clone()])?;
    let ledger_readback =
        write_vfx_text_generation_ledger(state_path.as_ref(), aster_dir.as_ref(), &ledger)?;
    Ok(StoredVfxTextGeneration {
        proposal,
        content_writeback,
        record,
        ledger,
        ledger_readback,
    })
}

pub fn build_vfx_text_generation_ledger(
    ledger_id: impl Into<String>,
    records: Vec<VfxTextGenerationRecord>,
) -> Result<VfxTextGenerationLedger, EngineError> {
    let ledger = VfxTextGenerationLedger {
        schema: VFX_TEXT_GENERATION_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        source_of_truth:
            "Aster VFX text-generation ledger row + content-addressed VFX effect object".to_string(),
        record_count: records.len(),
        records,
    };
    validate_vfx_text_generation_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_vfx_text_generation_ledger(
    ledger: &VfxTextGenerationLedger,
) -> Result<(), EngineError> {
    validate_single_line("ledger_id", &ledger.ledger_id)?;
    if ledger.schema != VFX_TEXT_GENERATION_SCHEMA
        || ledger.record_count == 0
        || ledger.record_count != ledger.records.len()
        || !ledger.source_of_truth.contains("Aster")
    {
        return invalid_text_generation("ledger", "schema, source, and record count are invalid");
    }
    let mut seen = BTreeSet::new();
    for record in &ledger.records {
        validate_record(record)?;
        if !seen.insert(record.request_id.as_str()) {
            return invalid_text_generation("ledger.duplicate", "duplicate request id");
        }
    }
    Ok(())
}

fn validate_record(record: &VfxTextGenerationRecord) -> Result<(), EngineError> {
    validate_single_line("record.request_id", &record.request_id)?;
    validate_non_empty("record.prompt", &record.prompt)?;
    validate_single_line("record.requested_by", &record.requested_by)?;
    if record.operation_id != DESCRIBE_EFFECT_OPERATION_ID
        || record.readback_tool_name != "litd.engine.vfx.read_vfx"
        || record.effect_id != record.validation.effect_id
        || record.content_object_sha256 != record.descriptor.sha256()?
        || record.content_object_bytes != record.descriptor.size
    {
        return invalid_text_generation(
            "record",
            "operation, descriptor, validation, and content readback must agree",
        );
    }
    validate_effect_descriptor_shape(&record.descriptor)
}

fn validate_effect_descriptor_shape(descriptor: &VfxEffectDescriptor) -> Result<(), EngineError> {
    litd_engine::render::vfx::validate_effect_descriptor(descriptor)?;
    Ok(())
}

fn validate_request(request: &VfxTextGenerationRequest) -> Result<(), EngineError> {
    validate_single_line("request_id", &request.request_id)?;
    validate_single_line("requested_by", &request.requested_by)?;
    validate_non_empty("prompt", &request.prompt)?;
    if request.prompt.chars().count() > 512 {
        return invalid_text_generation("prompt", "prompt must be <= 512 characters");
    }
    Ok(())
}

fn validate_vfx_mcp_route() -> Result<(String, String), EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .iter()
        .find(|domain| domain.domain_id == "vfx_particles")
        .ok_or_else(|| text_generation_error("mcp.domain", "missing vfx_particles domain"))?;
    let tool = domain
        .tools
        .iter()
        .find(|tool| tool.operation_id == DESCRIBE_EFFECT_OPERATION_ID)
        .ok_or_else(|| {
            text_generation_error(
                "mcp.tool",
                format!("missing operation {DESCRIBE_EFFECT_OPERATION_ID}"),
            )
        })?;
    if !tool.mutates_state || !tool.requires_lowering || !tool.records_ledger {
        return invalid_text_generation(
            "mcp.tool",
            "describe_effect_asset must mutate through a lowering ledger",
        );
    }
    Ok((
        tool.operation_id.clone(),
        tool.fsv_readback_tool_name.clone(),
    ))
}

fn record_from_writeback(
    proposal: &VfxTextGenerationProposal,
    writeback: &VfxEffectStoreWriteback,
) -> VfxTextGenerationRecord {
    VfxTextGenerationRecord {
        request_id: proposal.request.request_id.clone(),
        prompt: proposal.request.prompt.clone(),
        requested_by: proposal.request.requested_by.clone(),
        operation_id: proposal.operation_id.clone(),
        readback_tool_name: proposal.readback_tool_name.clone(),
        taxonomy: proposal.taxonomy.clone(),
        budget: proposal.budget.clone(),
        source_anchors: proposal.source_anchors.clone(),
        effect_id: proposal.effect.effect_id.clone(),
        descriptor: writeback.descriptor.clone(),
        validation: proposal.validation.clone(),
        content_object_path: writeback.object_path.display().to_string(),
        content_object_sha256: writeback.readback.sha256.clone(),
        content_object_bytes: writeback.readback.bytes,
    }
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    validate_non_empty(field, value)?;
    if value.contains(['\n', '\r', '\t']) {
        return invalid_text_generation(field, "value must be single-line");
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_text_generation(field, "value must be non-empty");
    }
    Ok(())
}

fn invalid_text_generation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(text_generation_error(field, reason))
}

fn text_generation_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.vfx_text_generation.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "vfx_text_generation_tests.rs"]
mod tests;
