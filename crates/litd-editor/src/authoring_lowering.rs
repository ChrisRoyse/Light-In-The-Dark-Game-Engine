use litd_engine::EngineError;

use super::{
    AssayBalanceProfile, GeneratedAbilityProposal, GroundedAbilityRequest, GroundingReport,
    LoweredAbility, LoweredAbilityDocument, LoweredBalance, LoweredProvenance, LoweredTomlArtifact,
    LoweredWard, WardPolicy, WorldCorpus, invalid_authoring,
};

pub(super) fn lower_ability_to_toml(
    request: &GroundedAbilityRequest,
    corpus: &WorldCorpus,
    assay: &AssayBalanceProfile,
    ward: &WardPolicy,
    grounding: &GroundingReport,
    proposal: &GeneratedAbilityProposal,
) -> Result<LoweredTomlArtifact, EngineError> {
    let mut document = LoweredAbilityDocument {
        schema_version: 1,
        lowering_fingerprint: String::new(),
        ability: LoweredAbility {
            id: proposal.ability_id.clone(),
            name: proposal.display_name.clone(),
            world_id: proposal.world_id.clone(),
            region: proposal.region.clone(),
            faction: proposal.faction.clone(),
            damage_type: proposal.damage_type.clone(),
            damage: proposal.damage,
            cooldown_seconds: proposal.cooldown_seconds,
            mana_cost: proposal.mana_cost,
            tags: proposal.tags.clone(),
            summary: proposal.summary.clone(),
        },
        balance: LoweredBalance {
            assay_id: assay.assay_id.clone(),
            profile_id: assay.profile_id.clone(),
            damage_min: assay.min_damage,
            damage_max: assay.max_damage,
            cooldown_min_seconds: assay.min_cooldown_seconds,
            cooldown_max_seconds: assay.max_cooldown_seconds,
            mana_cost_max: assay.max_mana_cost,
        },
        ward: LoweredWard {
            policy_id: ward.policy_id.clone(),
            passed: true,
            code: "LITD-WARD-PASS".to_string(),
            matched_anchor_count: grounding.matched_anchor_ids.len(),
            matched_term_count: grounding.matched_terms.len(),
        },
        provenance: LoweredProvenance {
            request_id: request.request_id.clone(),
            requested_by: request.requested_by.clone(),
            corpus_revision_id: corpus.revision_id.clone(),
            source_refs: grounding.source_refs.clone(),
            matched_terms: grounding.matched_terms.clone(),
            seed: request.seed,
            lowering_version: "litd-editor.authoring.v1".to_string(),
        },
    };
    document.lowering_fingerprint = lowered_document_fingerprint(&document)?;
    let canonical_toml =
        toml::to_string(&document).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered ability TOML failed: {error}"),
        })?;
    Ok(LoweredTomlArtifact {
        canonical_toml,
        fingerprint: document.lowering_fingerprint,
    })
}

pub fn verify_lowered_ability_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<LoweredAbilityDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return invalid_authoring("fingerprint", "expected fingerprint must be non-empty");
    }
    let document: LoweredAbilityDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered ability TOML failed: {error}"),
        })?;
    validate_lowered_document(&document)?;
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = lowered_document_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn validate_lowered_document(document: &LoweredAbilityDocument) -> Result<(), EngineError> {
    if document.schema_version != 1 {
        return invalid_authoring("toml.schema_version", "expected schema version 1");
    }
    if document.lowering_fingerprint.trim().is_empty()
        || document.ability.id.trim().is_empty()
        || document.ability.name.trim().is_empty()
        || document.ability.world_id.trim().is_empty()
        || document.ability.tags.is_empty()
        || document.provenance.source_refs.is_empty()
        || document.provenance.matched_terms.is_empty()
    {
        return invalid_authoring(
            "toml.required",
            "lowered ability TOML is missing required fields",
        );
    }
    if !document.ward.passed || document.ward.code != "LITD-WARD-PASS" {
        return invalid_authoring(
            "toml.ward",
            "lowered TOML must carry a passing Ward decision",
        );
    }
    if document.ability.damage < document.balance.damage_min
        || document.ability.damage > document.balance.damage_max
        || document.ability.cooldown_seconds < document.balance.cooldown_min_seconds
        || document.ability.cooldown_seconds > document.balance.cooldown_max_seconds
        || document.ability.mana_cost > document.balance.mana_cost_max
    {
        return invalid_authoring(
            "toml.balance",
            "ability stats violate lowered balance bounds",
        );
    }
    Ok(())
}

fn lowered_document_fingerprint(document: &LoweredAbilityDocument) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered ability fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}
