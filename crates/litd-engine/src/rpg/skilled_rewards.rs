use std::collections::BTreeSet;

use super::serialize_sha256;
use serde_json::json;

use crate::EngineError;

use super::{
    RPG_SKILLED_REWARD_SCHEMA, SkilledPlayEvidence, SkilledRewardCandidate,
    SkilledRewardLoweredEffect, SkilledRewardPolicy, SkilledRewardReadback,
};

pub fn evaluate_skilled_reward(
    policy: &SkilledRewardPolicy,
    candidate: &SkilledRewardCandidate,
) -> Result<SkilledRewardReadback, EngineError> {
    validate_skilled_reward_policy(policy)?;
    validate_candidate(policy, candidate)?;
    let skill_score_milli = skill_score(&candidate.evidence)?;
    let combined_score_milli =
        combined_score(policy, skill_score_milli, candidate.rp_quality_milli)?;
    let distinct_evidence_kind_count = distinct_kind_count(&candidate.evidence);
    let acceptance = acceptance(policy, candidate, skill_score_milli, combined_score_milli);
    let lowered_effects = if acceptance.accepted {
        vec![lower_effect(
            policy,
            candidate,
            combined_score_milli,
            skill_score_milli,
        )?]
    } else {
        Vec::new()
    };
    let mut readback = SkilledRewardReadback {
        schema: RPG_SKILLED_REWARD_SCHEMA.to_string(),
        server_policy_id: policy.server_policy_id.clone(),
        source_issue: policy.source_issue,
        candidate_id: candidate.candidate_id.clone(),
        character_id: candidate.character_id.clone(),
        requested_reward_kind: candidate.requested_reward_kind,
        accepted: acceptance.accepted,
        grade: acceptance.grade,
        reason: acceptance.reason,
        skill_score_milli,
        rp_quality_milli: candidate.rp_quality_milli,
        combined_score_milli,
        evidence_count: candidate.evidence.len(),
        distinct_evidence_kind_count,
        effect_count: lowered_effects.len(),
        lowered_effects,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        candidate_sha256: serialize_sha256(candidate)?,
        policy_sha256: skilled_reward_policy_hash(policy)?,
        decision_ledger_sha256: String::new(),
        result_sha256: String::new(),
    };
    readback.decision_ledger_sha256 = serialize_sha256(&ledger_payload(&readback))?;
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    Ok(readback)
}

pub fn validate_skilled_reward_policy(policy: &SkilledRewardPolicy) -> Result<(), EngineError> {
    if policy.schema != RPG_SKILLED_REWARD_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_SKILLED_REWARD_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1047 {
        return invalid("policy.source_issue", "must point at issue 1047");
    }
    if policy.skill_weight_milli + policy.rp_weight_milli != 1000 {
        return invalid("policy.weights", "skill and RP weights must sum to 1000");
    }
    if policy.min_skill_score_milli > 1000
        || policy.min_rp_quality_milli > 1000
        || policy.min_combined_score_milli > 1000
    {
        return invalid("policy.thresholds", "thresholds must be <= 1000");
    }
    if policy.max_reward_tier == 0 || policy.max_reward_tier > 10 {
        return invalid("policy.max_reward_tier", "must be inside 1..=10");
    }
    if policy.evidence_rows_required == 0 || policy.evidence_rows_required > 16 {
        return invalid("policy.evidence_rows_required", "must be inside 1..=16");
    }
    if policy.distinct_evidence_kinds_required == 0
        || policy.distinct_evidence_kinds_required > policy.evidence_rows_required
    {
        return invalid(
            "policy.distinct_evidence_kinds_required",
            "must be non-zero and <= evidence_rows_required",
        );
    }
    if !policy.rewards_revocable || !policy.immortal_or_system_approval_required {
        return invalid(
            "policy.reward_flags",
            "skilled rewards must be approved and revocable",
        );
    }
    if policy.built_in_map_required || policy.grants_map_or_minimap || policy.grants_gps_markers {
        return invalid("policy.map_flags", "skilled rewards cannot grant map/GPS");
    }
    Ok(())
}

pub fn skilled_reward_policy_hash(policy: &SkilledRewardPolicy) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

pub fn validate_skilled_reward_readback(
    policy: &SkilledRewardPolicy,
    readback: &SkilledRewardReadback,
) -> Result<(), EngineError> {
    validate_skilled_reward_policy(policy)?;
    if readback.schema != RPG_SKILLED_REWARD_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {RPG_SKILLED_REWARD_SCHEMA}"),
        );
    }
    if readback.server_policy_id != policy.server_policy_id {
        return invalid(
            "readback.server_policy_id",
            "must match the active reward policy",
        );
    }
    if readback.source_issue != 1047 {
        return invalid("readback.source_issue", "must point at issue 1047");
    }
    if readback.policy_sha256 != skilled_reward_policy_hash(policy)? {
        return invalid("readback.policy_sha256", "does not match active policy");
    }
    if readback.skill_score_milli > 1000
        || readback.rp_quality_milli > 1000
        || readback.combined_score_milli > 1000
    {
        return invalid("readback.scores", "scores must be <= 1000");
    }
    if readback.effect_count != readback.lowered_effects.len() {
        return invalid("readback.effect_count", "must equal lowered effects");
    }
    if readback.accepted && readback.lowered_effects.is_empty() {
        return invalid(
            "readback.lowered_effects",
            "accepted reward needs an effect",
        );
    }
    if !readback.accepted && !readback.lowered_effects.is_empty() {
        return invalid(
            "readback.lowered_effects",
            "rejected reward cannot lower effects",
        );
    }
    if readback.built_in_map_required
        || readback.grants_map_or_minimap
        || readback.grants_gps_markers
    {
        return invalid("readback.map_flags", "skilled rewards cannot grant map/GPS");
    }
    Ok(())
}

fn validate_candidate(
    policy: &SkilledRewardPolicy,
    candidate: &SkilledRewardCandidate,
) -> Result<(), EngineError> {
    validate_token("candidate.candidate_id", &candidate.candidate_id)?;
    validate_token("candidate.character_id", &candidate.character_id)?;
    validate_text("candidate.character_name", &candidate.character_name, 128)?;
    validate_token("candidate.class_id", &candidate.class_id)?;
    if candidate.rp_quality_milli > 1000 {
        return invalid("candidate.rp_quality_milli", "must be <= 1000");
    }
    if let Some(immortal_id) = &candidate.immortal_id {
        validate_token("candidate.immortal_id", immortal_id)?;
    }
    if candidate.evidence.len() < policy.evidence_rows_required || candidate.evidence.len() > 64 {
        return invalid("candidate.evidence", "wrong evidence row count");
    }
    if distinct_kind_count(&candidate.evidence) < policy.distinct_evidence_kinds_required {
        return invalid(
            "candidate.evidence.kind",
            "not enough distinct evidence kinds",
        );
    }
    validate_evidence_rows(&candidate.evidence)
}

fn validate_evidence_rows(evidence: &[SkilledPlayEvidence]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for row in evidence {
        validate_token("evidence.evidence_id", &row.evidence_id)?;
        validate_token("evidence.source_ref", &row.source_ref)?;
        validate_text("evidence.summary", &row.summary, 180)?;
        if row.metric_milli == 0 || row.metric_milli > 1000 {
            return invalid("evidence.metric_milli", "must be inside 1..=1000");
        }
        if row.difficulty_milli == 0 || row.difficulty_milli > 1000 {
            return invalid("evidence.difficulty_milli", "must be inside 1..=1000");
        }
        if !row.witnessed_by_system {
            return invalid("evidence.witnessed_by_system", "evidence must be measured");
        }
        if !seen.insert(row.evidence_id.as_str()) {
            return invalid("evidence.evidence_id", "duplicate evidence row");
        }
    }
    Ok(())
}

fn skill_score(evidence: &[SkilledPlayEvidence]) -> Result<u16, EngineError> {
    let total = evidence
        .iter()
        .map(|row| u32::from(row.metric_milli) * u32::from(row.difficulty_milli) / 1000)
        .sum::<u32>();
    Ok((total / u32::try_from(evidence.len()).unwrap_or(1))
        .try_into()
        .unwrap_or(u16::MAX))
}

fn combined_score(policy: &SkilledRewardPolicy, skill: u16, rp: u16) -> Result<u16, EngineError> {
    let weighted = u32::from(skill) * u32::from(policy.skill_weight_milli)
        + u32::from(rp) * u32::from(policy.rp_weight_milli);
    Ok((weighted / 1000).try_into().unwrap_or(u16::MAX))
}

fn acceptance(
    policy: &SkilledRewardPolicy,
    candidate: &SkilledRewardCandidate,
    skill_score_milli: u16,
    combined_score_milli: u16,
) -> Acceptance {
    if skill_score_milli < policy.min_skill_score_milli {
        return reject(
            "needs_gameplay_skill",
            "skill evidence is below reward threshold",
        );
    }
    if candidate.rp_quality_milli < policy.min_rp_quality_milli {
        return reject("needs_roleplay", "RP quality is below reward threshold");
    }
    if combined_score_milli < policy.min_combined_score_milli {
        return reject(
            "needs_combined_excellence",
            "combined score is below threshold",
        );
    }
    accept(
        "accepted",
        "skill evidence and RP quality both satisfy reward policy",
    )
}

fn lower_effect(
    policy: &SkilledRewardPolicy,
    candidate: &SkilledRewardCandidate,
    combined_score_milli: u16,
    skill_score_milli: u16,
) -> Result<SkilledRewardLoweredEffect, EngineError> {
    let tier = reward_tier(policy, combined_score_milli);
    let reward_id = format!(
        "reward.skill.{}.{}",
        candidate.requested_reward_kind.as_str(),
        candidate.candidate_id
    );
    let approval_actor_id = candidate
        .immortal_id
        .clone()
        .unwrap_or_else(|| "system.skilled_reward_arbiter".to_string());
    let mut effect = SkilledRewardLoweredEffect {
        reward_id,
        reward_kind: candidate.requested_reward_kind,
        subject_character_id: candidate.character_id.clone(),
        tier,
        ledger_entry: format!(
            "skill_reward:{}:{}:{}:{}",
            candidate.character_id, candidate.candidate_id, skill_score_milli, combined_score_milli
        ),
        revocable: policy.rewards_revocable,
        approval_actor_id,
        effect_sha256: String::new(),
    };
    effect.effect_sha256 = serialize_sha256(&effect_payload(&effect))?;
    Ok(effect)
}

fn reward_tier(policy: &SkilledRewardPolicy, combined_score_milli: u16) -> u16 {
    let raw = (u32::from(combined_score_milli) * u32::from(policy.max_reward_tier)) / 1000;
    raw.max(1).min(u32::from(policy.max_reward_tier)) as u16
}

fn distinct_kind_count(evidence: &[SkilledPlayEvidence]) -> usize {
    evidence
        .iter()
        .map(|row| row.kind)
        .collect::<BTreeSet<_>>()
        .len()
}

struct Acceptance {
    accepted: bool,
    grade: String,
    reason: String,
}

fn accept(grade: &str, reason: &str) -> Acceptance {
    Acceptance {
        accepted: true,
        grade: grade.to_string(),
        reason: reason.to_string(),
    }
}

fn reject(grade: &str, reason: &str) -> Acceptance {
    Acceptance {
        accepted: false,
        grade: grade.to_string(),
        reason: reason.to_string(),
    }
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > max_len {
        return invalid(field, format!("must be <= {max_len} bytes"));
    }
    Ok(())
}

fn ledger_payload(readback: &SkilledRewardReadback) -> serde_json::Value {
    json!({
        "candidate_id": readback.candidate_id,
        "character_id": readback.character_id,
        "accepted": readback.accepted,
        "grade": readback.grade,
        "skill_score_milli": readback.skill_score_milli,
        "rp_quality_milli": readback.rp_quality_milli,
        "combined_score_milli": readback.combined_score_milli,
        "lowered_effects": readback.lowered_effects,
    })
}

fn readback_fingerprint(readback: &SkilledRewardReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "server_policy_id": readback.server_policy_id,
        "source_issue": readback.source_issue,
        "candidate_id": readback.candidate_id,
        "accepted": readback.accepted,
        "grade": readback.grade,
        "skill_score_milli": readback.skill_score_milli,
        "rp_quality_milli": readback.rp_quality_milli,
        "combined_score_milli": readback.combined_score_milli,
        "effect_count": readback.effect_count,
        "built_in_map_required": readback.built_in_map_required,
        "candidate_sha256": readback.candidate_sha256,
        "policy_sha256": readback.policy_sha256,
        "decision_ledger_sha256": readback.decision_ledger_sha256,
    })
}

fn effect_payload(effect: &SkilledRewardLoweredEffect) -> serde_json::Value {
    json!({
        "reward_id": effect.reward_id,
        "reward_kind": effect.reward_kind,
        "subject_character_id": effect.subject_character_id,
        "tier": effect.tier,
        "ledger_entry": effect.ledger_entry,
        "revocable": effect.revocable,
        "approval_actor_id": effect.approval_actor_id,
    })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: format!("skilled_rewards.{}", field.into()),
        reason: reason.to_string(),
    })
}
