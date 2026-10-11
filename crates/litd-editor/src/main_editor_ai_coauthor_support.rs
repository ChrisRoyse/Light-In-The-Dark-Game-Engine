use litd_editor::{
    authoring::{
        AssayBalanceProfile, AuthoringOutcome, GeneratedAbilityProposal, GroundingAnchor,
        WardPolicy, WorldCorpus,
    },
    capabilities::default_capability_layer_manifest,
    history::EditorObject,
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};

use super::{AI_CAPABILITY_OPERATION_ID, ai_error, invalid_ai};

pub(super) fn object_for_proposal(
    proposal: &GeneratedAbilityProposal,
    outcome: &AuthoringOutcome,
    fingerprint: &str,
) -> Result<EditorObject, EngineError> {
    let object_id = format!("ai_coauthor.{}", proposal.ability_id.replace('.', "_"));
    let mut object = EditorObject::new(
        &object_id,
        "GeneratedAbility",
        None,
        Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
    );
    let source_refs = outcome.grounding.source_refs.join("|");
    for (key, value) in [
        ("ability_id", proposal.ability_id.as_str()),
        ("display_name", proposal.display_name.as_str()),
        ("world_id", proposal.world_id.as_str()),
        ("region", proposal.region.as_str()),
        ("faction", proposal.faction.as_str()),
        ("damage_type", proposal.damage_type.as_str()),
        ("summary", proposal.summary.as_str()),
        ("lowering_fingerprint", fingerprint),
        ("source_refs", source_refs.as_str()),
        ("capability_operation_id", AI_CAPABILITY_OPERATION_ID),
    ] {
        object = object.with_property(key, value);
    }
    Ok(object
        .with_property("damage", proposal.damage.to_string())
        .with_property("cooldown_seconds", proposal.cooldown_seconds.to_string())
        .with_property("mana_cost", proposal.mana_cost.to_string())
        .with_property("admission_code", outcome.admission_code.clone()))
}

pub(super) fn default_prompt() -> String {
    "Create a Dawnward cinder bloom ward flame ability for Ashen Reach dusk forest.".to_string()
}

pub(super) fn authoring_corpus() -> WorldCorpus {
    WorldCorpus {
        world_id: "wa1-ashen-reach".to_string(),
        revision_id: "wa1-rev-20260627".to_string(),
        region: "Ashen Reach".to_string(),
        factions: vec!["Dawnward".to_string(), "Cinder Choir".to_string()],
        lore_terms: vec![
            "cinder bloom".to_string(),
            "ward flame".to_string(),
            "dusk forest".to_string(),
        ],
        anchors: vec![
            GroundingAnchor {
                anchor_id: "wa1-anchor-region".to_string(),
                source_ref: "Aster://wa1/world/ashen-reach#region".to_string(),
                text: "Ashen Reach is a dusk forest scarred by ember fog.".to_string(),
                terms: vec!["Ashen Reach".to_string(), "dusk forest".to_string()],
            },
            GroundingAnchor {
                anchor_id: "wa1-anchor-faction".to_string(),
                source_ref: "Aster://wa1/factions/dawnward".to_string(),
                text: "The Dawnward bind ward flame into defensive rites.".to_string(),
                terms: vec!["Dawnward".to_string(), "ward flame".to_string()],
            },
            GroundingAnchor {
                anchor_id: "wa1-anchor-reagent".to_string(),
                source_ref: "Aster://wa1/reagents/cinder-bloom".to_string(),
                text: "Cinder bloom stores heat without open flame.".to_string(),
                terms: vec!["cinder bloom".to_string()],
            },
        ],
    }
}

pub(super) fn authoring_assay() -> AssayBalanceProfile {
    AssayBalanceProfile {
        assay_id: "assay-724".to_string(),
        profile_id: "ability-tier-1-control".to_string(),
        min_damage: 24,
        max_damage: 68,
        min_cooldown_seconds: 8,
        max_cooldown_seconds: 18,
        max_mana_cost: 60,
        allowed_damage_types: vec!["ward flame".to_string(), "ember".to_string()],
    }
}

pub(super) fn ward() -> WardPolicy {
    WardPolicy {
        policy_id: "ward-grounding-issue1230".to_string(),
        min_source_anchors: 3,
        min_grounded_terms: 4,
        blocked_terms: vec![
            "spaceship".to_string(),
            "laser".to_string(),
            "stock market".to_string(),
        ],
    }
}

pub(super) fn capability_route() -> Result<(String, String), EngineError> {
    let manifest = default_capability_layer_manifest()?;
    let operation = manifest
        .registry
        .operations
        .iter()
        .find(|operation| operation.operation_id == AI_CAPABILITY_OPERATION_ID)
        .ok_or_else(|| {
            ai_error(
                "capability.operation",
                format!("missing operation {AI_CAPABILITY_OPERATION_ID}"),
            )
        })?;
    if !operation.calyx_backed || !operation.lowering_boundary || !operation.fsv_readback_required {
        return invalid_ai(
            "capability.operation",
            format!(
                "{} must be Calyx-backed, lowering-boundary, and FSV-readback-required",
                operation.operation_id
            ),
        );
    }
    Ok((
        operation.operation_id.clone(),
        operation.readback_tool_name.clone(),
    ))
}
