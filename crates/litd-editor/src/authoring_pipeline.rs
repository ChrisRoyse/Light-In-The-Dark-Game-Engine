use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    AssayBalanceProfile, AuthoringOutcome, AuthoringStatus, GeneratedAbilityProposal,
    GroundedAbilityRequest, GroundingReport, WardDecision, WardPolicy, WorldCorpus,
    authoring_lowering::lower_ability_to_toml, invalid_authoring, invalid_authoring_error,
    verify_lowered_ability_toml,
};

pub fn generate_grounded_ability(
    request: &GroundedAbilityRequest,
    corpus: &WorldCorpus,
    assay: &AssayBalanceProfile,
    ward: &WardPolicy,
) -> Result<AuthoringOutcome, EngineError> {
    validate_inputs(request, corpus, assay, ward)?;
    let grounding = evaluate_grounding(&request.prompt, corpus, ward);
    let request_id = request.request_id.clone();

    if request.prompt.trim().is_empty() {
        return Ok(refused(
            "LITD-AUTHORING-EMPTY-REQUEST",
            "request prompt must be non-empty",
            request_id,
            grounding,
        ));
    }
    if !grounding.blocked_terms.is_empty() {
        return Ok(refused(
            "LITD-AUTHORING-REFUSED-OOD",
            format!(
                "request contained out-of-domain terms: {}",
                grounding.blocked_terms.join("|")
            ),
            request_id,
            grounding,
        ));
    }
    if grounding.matched_anchor_ids.len() < ward.min_source_anchors
        || grounding.matched_terms.len() < ward.min_grounded_terms
    {
        return Ok(AuthoringOutcome {
            status: AuthoringStatus::Provisional,
            admission_code: "LITD-AUTHORING-PROVISIONAL-UNGROUNDED".to_string(),
            request_id,
            grounding,
            ward_decision: WardDecision {
                passed: false,
                code: "LITD-WARD-INSUFFICIENT-GROUNDING".to_string(),
                reasons: vec![
                    "request needs more grounded corpus evidence before lowering".to_string(),
                ],
            },
            proposal: None,
            lowered: None,
        });
    }

    let proposal = build_ability_proposal(request, corpus, assay, &grounding)?;
    let ward_decision = validate_ward(&proposal, &grounding, assay, ward);
    if !ward_decision.passed {
        return Ok(AuthoringOutcome {
            status: AuthoringStatus::Refused,
            admission_code: ward_decision.code.clone(),
            request_id,
            grounding,
            ward_decision,
            proposal: Some(proposal),
            lowered: None,
        });
    }

    let lowered = lower_ability_to_toml(request, corpus, assay, ward, &grounding, &proposal)?;
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint)?;
    Ok(AuthoringOutcome {
        status: AuthoringStatus::Admitted,
        admission_code: "LITD-AUTHORING-ADMITTED".to_string(),
        request_id,
        grounding,
        ward_decision,
        proposal: Some(proposal),
        lowered: Some(lowered),
    })
}

fn build_ability_proposal(
    request: &GroundedAbilityRequest,
    corpus: &WorldCorpus,
    assay: &AssayBalanceProfile,
    grounding: &GroundingReport,
) -> Result<GeneratedAbilityProposal, EngineError> {
    let faction = first_matching(&request.prompt, &corpus.factions).unwrap_or(&corpus.factions[0]);
    let damage_type = first_matching(&request.prompt, &assay.allowed_damage_types)
        .unwrap_or(&assay.allowed_damage_types[0]);
    let core_term = grounding
        .matched_terms
        .iter()
        .filter(|term| term.split_whitespace().count() > 1)
        .max_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)))
        .or_else(|| grounding.matched_terms.first())
        .ok_or_else(|| invalid_authoring_error("grounding", "missing grounded term"))?;
    let entropy = fingerprint_seed(request, corpus, grounding);
    let damage = bounded_value(entropy, assay.min_damage, assay.max_damage);
    let cooldown_seconds = bounded_value(
        entropy.rotate_left(7),
        assay.min_cooldown_seconds,
        assay.max_cooldown_seconds,
    );
    let mana_cost = bounded_value(entropy.rotate_left(13), 1, assay.max_mana_cost);
    let display_name = title_case(core_term);
    let tags = sorted_unique([
        slug(&corpus.region),
        slug(faction),
        slug(damage_type),
        slug(core_term),
    ]);

    Ok(GeneratedAbilityProposal {
        ability_id: format!("ability.{}.{}", slug(faction), slug(core_term)),
        display_name: format!("{display_name} Ward"),
        world_id: corpus.world_id.clone(),
        region: corpus.region.clone(),
        faction: faction.clone(),
        damage_type: damage_type.clone(),
        damage,
        cooldown_seconds,
        mana_cost,
        tags,
        summary: format!(
            "{} channels {} through {} in {}.",
            faction, core_term, damage_type, corpus.region
        ),
    })
}

fn validate_ward(
    proposal: &GeneratedAbilityProposal,
    grounding: &GroundingReport,
    assay: &AssayBalanceProfile,
    ward: &WardPolicy,
) -> WardDecision {
    let mut reasons = Vec::new();
    if grounding.matched_anchor_ids.len() < ward.min_source_anchors {
        reasons.push(format!(
            "matched {} anchors, need {}",
            grounding.matched_anchor_ids.len(),
            ward.min_source_anchors
        ));
    }
    if grounding.matched_terms.len() < ward.min_grounded_terms {
        reasons.push(format!(
            "matched {} terms, need {}",
            grounding.matched_terms.len(),
            ward.min_grounded_terms
        ));
    }
    if proposal.damage < assay.min_damage || proposal.damage > assay.max_damage {
        reasons.push(format!(
            "damage {} outside assay range {}..{}",
            proposal.damage, assay.min_damage, assay.max_damage
        ));
    }
    if proposal.cooldown_seconds < assay.min_cooldown_seconds
        || proposal.cooldown_seconds > assay.max_cooldown_seconds
    {
        reasons.push(format!(
            "cooldown {} outside assay range {}..{}",
            proposal.cooldown_seconds, assay.min_cooldown_seconds, assay.max_cooldown_seconds
        ));
    }
    if proposal.mana_cost > assay.max_mana_cost {
        reasons.push(format!(
            "mana cost {} exceeds assay max {}",
            proposal.mana_cost, assay.max_mana_cost
        ));
    }
    if !assay
        .allowed_damage_types
        .iter()
        .any(|kind| kind == &proposal.damage_type)
    {
        reasons.push(format!(
            "damage type {} is not assay-approved",
            proposal.damage_type
        ));
    }
    if reasons.is_empty() {
        WardDecision {
            passed: true,
            code: "LITD-WARD-PASS".to_string(),
            reasons,
        }
    } else {
        WardDecision {
            passed: false,
            code: "LITD-WARD-REJECTED".to_string(),
            reasons,
        }
    }
}

fn evaluate_grounding(prompt: &str, corpus: &WorldCorpus, ward: &WardPolicy) -> GroundingReport {
    let haystack = prompt.to_ascii_lowercase();
    let mut matched_anchor_ids = BTreeSet::new();
    let mut matched_terms = BTreeSet::new();
    let mut source_refs = BTreeSet::new();
    for term in corpus
        .lore_terms
        .iter()
        .chain(corpus.factions.iter())
        .chain(std::iter::once(&corpus.region))
    {
        if contains_term(&haystack, term) {
            matched_terms.insert(term.to_ascii_lowercase());
        }
    }
    for anchor in &corpus.anchors {
        let anchor_hit = anchor
            .terms
            .iter()
            .any(|term| contains_term(&haystack, term));
        if anchor_hit {
            matched_anchor_ids.insert(anchor.anchor_id.clone());
            source_refs.insert(anchor.source_ref.clone());
            for term in &anchor.terms {
                if contains_term(&haystack, term) {
                    matched_terms.insert(term.to_ascii_lowercase());
                }
            }
        }
    }
    let blocked_terms = ward
        .blocked_terms
        .iter()
        .filter(|term| contains_term(&haystack, term))
        .map(|term| term.to_ascii_lowercase())
        .collect();

    GroundingReport {
        matched_anchor_ids: matched_anchor_ids.into_iter().collect(),
        matched_terms: matched_terms.into_iter().collect(),
        blocked_terms,
        source_refs: source_refs.into_iter().collect(),
    }
}

fn validate_inputs(
    request: &GroundedAbilityRequest,
    corpus: &WorldCorpus,
    assay: &AssayBalanceProfile,
    ward: &WardPolicy,
) -> Result<(), EngineError> {
    if request.request_id.trim().is_empty() || request.requested_by.trim().is_empty() {
        return invalid_authoring("request", "request_id and requested_by must be non-empty");
    }
    if corpus.world_id.trim().is_empty()
        || corpus.revision_id.trim().is_empty()
        || corpus.region.trim().is_empty()
        || corpus.factions.is_empty()
        || corpus.lore_terms.is_empty()
        || corpus.anchors.is_empty()
    {
        return invalid_authoring(
            "corpus",
            "world id, revision, region, factions, terms, and anchors are required",
        );
    }
    if assay.assay_id.trim().is_empty()
        || assay.profile_id.trim().is_empty()
        || assay.min_damage > assay.max_damage
        || assay.min_cooldown_seconds > assay.max_cooldown_seconds
        || assay.max_mana_cost == 0
        || assay.allowed_damage_types.is_empty()
    {
        return invalid_authoring("assay", "balance bounds and damage types must be valid");
    }
    if ward.policy_id.trim().is_empty()
        || ward.min_source_anchors == 0
        || ward.min_grounded_terms == 0
    {
        return invalid_authoring("ward", "policy id and grounding thresholds must be valid");
    }
    Ok(())
}

fn refused(
    code: &str,
    reason: impl Into<String>,
    request_id: String,
    grounding: GroundingReport,
) -> AuthoringOutcome {
    AuthoringOutcome {
        status: AuthoringStatus::Refused,
        admission_code: code.to_string(),
        request_id,
        grounding,
        ward_decision: WardDecision {
            passed: false,
            code: code.to_string(),
            reasons: vec![reason.into()],
        },
        proposal: None,
        lowered: None,
    }
}

fn first_matching<'a>(prompt: &str, values: &'a [String]) -> Option<&'a String> {
    let haystack = prompt.to_ascii_lowercase();
    values.iter().find(|value| contains_term(&haystack, value))
}

fn contains_term(haystack: &str, term: &str) -> bool {
    let needle = term.trim().to_ascii_lowercase();
    !needle.is_empty() && haystack.contains(&needle)
}

fn bounded_value(seed: u64, min: u32, max: u32) -> u32 {
    let span = u64::from(max - min + 1);
    min + (seed % span) as u32
}

fn fingerprint_seed(
    request: &GroundedAbilityRequest,
    corpus: &WorldCorpus,
    grounding: &GroundingReport,
) -> u64 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&request.seed.to_le_bytes());
    bytes.extend_from_slice(request.prompt.as_bytes());
    bytes.extend_from_slice(corpus.world_id.as_bytes());
    for term in &grounding.matched_terms {
        bytes.extend_from_slice(term.as_bytes());
    }
    litd_hash::fnv1a64(&bytes)
}

fn sorted_unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_sep = false;
        } else if !last_was_sep && !out.is_empty() {
            out.push('_');
            last_was_sep = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn title_case(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
