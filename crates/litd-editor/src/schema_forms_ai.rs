use std::{collections::BTreeMap, path::PathBuf};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    authoring::{
        AssayBalanceProfile, AuthoringOutcome, AuthoringStatus, GeneratedAbilityProposal,
        GroundedAbilityRequest, GroundingAnchor, WardPolicy, WorldCorpus,
        generate_grounded_ability, verify_lowered_ability_toml,
    },
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, ObjectBaseTemplate, ObjectDataKind, call_mcp_object_tool,
    },
};

use super::{
    ObjectDataLedgerSummary, ObjectDataReadbackSummary, SchemaFormSubmitArgs, invalid,
    ledger_summary, object_summary, schema_error, validate_single_line, validate_submit_args,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiAssistPanel {
    pub panel_id: String,
    pub connection: AiAssistConnection,
    pub enabled: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AiAssistConnection {
    Online { helper: String },
    Offline { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiAssistAbilityDraftRequest {
    pub request_id: String,
    pub prompt: String,
    pub seed: u64,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiAssistApplyArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request: AiAssistAbilityDraftRequest,
    pub initial_document: Option<EditorDocument>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiAssistAbilityApplyReadback {
    pub panel: AiAssistPanel,
    pub outcome: AuthoringOutcome,
    pub object_id: String,
    pub lowered_fingerprint: String,
    pub abilitycheck_passed: bool,
    pub document: ObjectDataReadbackSummary,
    pub ledger: ObjectDataLedgerSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiAssistAuthoringContext {
    pub corpus: WorldCorpus,
    pub assay: AssayBalanceProfile,
    pub ward: WardPolicy,
}

pub fn build_ai_assist_panel(connection: AiAssistConnection) -> Result<AiAssistPanel, EngineError> {
    let (enabled, status) = match &connection {
        AiAssistConnection::Online { helper } => {
            validate_single_line("ai.helper", helper)?;
            (true, format!("online:{helper}"))
        }
        AiAssistConnection::Offline { reason } => {
            validate_single_line("ai.offline", reason)?;
            (false, format!("offline:{reason}"))
        }
    };
    Ok(AiAssistPanel {
        panel_id: "editor.schema_forms.ai_assist".to_string(),
        connection,
        enabled,
        status,
    })
}

pub fn apply_ai_assist_ability_draft(
    panel: &AiAssistPanel,
    args: AiAssistApplyArgs,
    context: &AiAssistAuthoringContext,
) -> Result<AiAssistAbilityApplyReadback, EngineError> {
    if !panel.enabled {
        return invalid(
            "ai_assist.offline",
            format!("AI assist disabled: {}", panel.status),
        );
    }
    validate_submit_args(&SchemaFormSubmitArgs {
        document_path: args.document_path.clone(),
        ledger_path: args.ledger_path.clone(),
        ledger_id: args.ledger_id.clone(),
        request_id: args.request.request_id.clone(),
        requested_by: args.request.requested_by.clone(),
        initial_document: args.initial_document.clone(),
    })?;
    let request = GroundedAbilityRequest {
        request_id: args.request.request_id.clone(),
        prompt: args.request.prompt.clone(),
        seed: args.request.seed,
        requested_by: args.request.requested_by.clone(),
    };
    let outcome =
        generate_grounded_ability(&request, &context.corpus, &context.assay, &context.ward)?;
    let (proposal, lowered_fingerprint) = abilitycheck(&outcome)?;
    let object_id = format!("ability.issue742.{}", slug(&proposal.ability_id));
    let base = ability_base_template(&object_id, proposal, &outcome, &lowered_fingerprint);
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": args.document_path,
            "ledger_path": args.ledger_path,
            "ledger_id": args.ledger_id,
            "request_id": args.request.request_id,
            "requested_by": args.request.requested_by,
            "initial_document": args.initial_document,
            "new_object_id": object_id,
            "rawcode": ability_rawcode(args.request.seed),
            "base": base,
            "overrides": {}
        }),
    )?;
    Ok(AiAssistAbilityApplyReadback {
        panel: panel.clone(),
        outcome,
        object_id: object_id.clone(),
        lowered_fingerprint,
        abilitycheck_passed: true,
        document: object_summary(&args.document_path, &object_id)?,
        ledger: ledger_summary(&args.ledger_path)?,
    })
}

pub fn issue742_authoring_context() -> AiAssistAuthoringContext {
    AiAssistAuthoringContext {
        corpus: WorldCorpus {
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
                anchor(
                    "wa1-anchor-region",
                    "Aster://wa1/world/ashen-reach#region",
                    "Ashen Reach is a dusk forest scarred by ember fog.",
                    &["Ashen Reach", "dusk forest"],
                ),
                anchor(
                    "wa1-anchor-faction",
                    "Aster://wa1/factions/dawnward",
                    "The Dawnward bind ward flame into defensive rites.",
                    &["Dawnward", "ward flame"],
                ),
                anchor(
                    "wa1-anchor-reagent",
                    "Aster://wa1/reagents/cinder-bloom",
                    "Cinder bloom stores heat without open flame.",
                    &["cinder bloom"],
                ),
            ],
        },
        assay: AssayBalanceProfile {
            assay_id: "assay-742".to_string(),
            profile_id: "ability-tier-1-control".to_string(),
            min_damage: 24,
            max_damage: 68,
            min_cooldown_seconds: 8,
            max_cooldown_seconds: 18,
            max_mana_cost: 60,
            allowed_damage_types: vec!["ward flame".to_string(), "ember".to_string()],
        },
        ward: WardPolicy {
            policy_id: "ward-grounding-issue742".to_string(),
            min_source_anchors: 3,
            min_grounded_terms: 4,
            blocked_terms: vec![
                "spaceship".to_string(),
                "laser".to_string(),
                "stock market".to_string(),
            ],
        },
    }
}

fn abilitycheck(
    outcome: &AuthoringOutcome,
) -> Result<(&GeneratedAbilityProposal, String), EngineError> {
    if outcome.status != AuthoringStatus::Admitted {
        return invalid(
            "ai_assist.abilitycheck",
            format!(
                "draft rejected before object edit: {}",
                outcome.admission_code
            ),
        );
    }
    let proposal = outcome
        .proposal
        .as_ref()
        .ok_or_else(|| schema_error("ai_assist.abilitycheck", "admitted draft missing proposal"))?;
    let lowered = outcome.lowered.as_ref().ok_or_else(|| {
        schema_error(
            "ai_assist.abilitycheck",
            "admitted draft missing lowered TOML",
        )
    })?;
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint)?;
    Ok((proposal, lowered.fingerprint.clone()))
}

fn ability_base_template(
    object_id: &str,
    proposal: &GeneratedAbilityProposal,
    outcome: &AuthoringOutcome,
    fingerprint: &str,
) -> ObjectBaseTemplate {
    let mut properties = BTreeMap::new();
    properties.insert(
        "ability.generated_id".to_string(),
        proposal.ability_id.clone(),
    );
    properties.insert(
        "ability.damage_type".to_string(),
        proposal.damage_type.clone(),
    );
    properties.insert("stats.damage".to_string(), proposal.damage.to_string());
    properties.insert(
        "stats.cooldown_seconds".to_string(),
        proposal.cooldown_seconds.to_string(),
    );
    properties.insert(
        "stats.mana_cost".to_string(),
        proposal.mana_cost.to_string(),
    );
    properties.insert("ability.tags".to_string(), proposal.tags.join("|"));
    properties.insert("ability.summary".to_string(), proposal.summary.clone());
    properties.insert(
        "ai.lowering_fingerprint".to_string(),
        fingerprint.to_string(),
    );
    properties.insert(
        "ai.admission_code".to_string(),
        outcome.admission_code.clone(),
    );
    properties.insert(
        "ai.source_refs".to_string(),
        outcome.grounding.source_refs.join("|"),
    );
    ObjectBaseTemplate {
        base_object_id: object_id.to_string(),
        kind: ObjectDataKind::Ability,
        display_name: proposal.display_name.clone(),
        properties,
    }
}

fn ability_rawcode(seed: u64) -> String {
    format!("A{:03}", seed % 1000)
}

fn anchor(id: &str, source: &str, text: &str, terms: &[&str]) -> GroundingAnchor {
    GroundingAnchor {
        anchor_id: id.to_string(),
        source_ref: source.to_string(),
        text: text.to_string(),
        terms: terms.iter().map(|term| term.to_string()).collect(),
    }
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
