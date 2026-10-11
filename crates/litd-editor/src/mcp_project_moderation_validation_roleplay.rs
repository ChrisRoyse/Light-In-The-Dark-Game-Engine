use litd_engine::EngineError;

use super::mcp_project_moderation_types::{
    ModerationSafetyArgs, RoleplayPolicyEvaluationRecord, RoleplayPolicyMode, RoleplayPolicyRecord,
    RoleplayPolicyVerdict,
};
use super::mcp_project_moderation_validation::{moderation_error, validate_id, validate_sha256};

pub fn validate_set_roleplay_policy_args(args: &ModerationSafetyArgs) -> Result<(), EngineError> {
    require_id("policy_id", args.policy_id.as_deref())?;
    require_id("lobby_id", args.lobby_id.as_deref())?;
    require_id("actor_account_id", args.actor_account_id.as_deref())?;
    require_roleplay_mode(args.roleplay_policy_mode)?;
    require_milli("strictness_milli", args.strictness_milli)?;
    require_multiplier("reward_multiplier_milli", args.reward_multiplier_milli)?;
    require_multiplier(
        "punishment_multiplier_milli",
        args.punishment_multiplier_milli,
    )?;
    require_evidence_ref_count(args.min_evidence_refs)?;
    if let Some(value) = args.supersedes_policy_id.as_deref() {
        validate_id("supersedes_policy_id", value)?;
    }
    Ok(())
}

pub fn validate_evaluate_roleplay_policy_args(
    args: &ModerationSafetyArgs,
) -> Result<(), EngineError> {
    require_id("evaluation_id", args.evaluation_id.as_deref())?;
    require_id("lobby_id", args.lobby_id.as_deref())?;
    require_id("actor_account_id", args.actor_account_id.as_deref())?;
    require_id("account_id", args.account_id.as_deref())?;
    if let Some(value) = args.policy_id.as_deref() {
        validate_id("policy_id", value)?;
    }
    require_milli("rp_fit_score_milli", args.rp_fit_score_milli)?;
    validate_sha256(
        "evidence_sha256",
        args.evidence_sha256
            .as_deref()
            .ok_or_else(|| moderation_error("evidence_sha256", "required field missing"))?,
    )?;
    if args.evidence_refs.is_empty() {
        return Err(moderation_error(
            "evidence_refs",
            "roleplay policy evaluation requires evidence refs",
        ));
    }
    for value in &args.evidence_refs {
        validate_id("evidence_refs", value)?;
    }
    Ok(())
}

pub fn validate_roleplay_policy(record: &RoleplayPolicyRecord) -> Result<(), EngineError> {
    validate_id("roleplay_policy.policy_id", &record.policy_id)?;
    validate_id("roleplay_policy.lobby_id", &record.lobby_id)?;
    validate_id(
        "roleplay_policy.set_by_account_id",
        &record.set_by_account_id,
    )?;
    validate_thresholds(
        record.strictness_milli,
        record.reward_threshold_milli,
        record.punishment_threshold_milli,
    )?;
    validate_multiplier(
        "roleplay_policy.reward_multiplier_milli",
        record.reward_multiplier_milli,
    )?;
    validate_multiplier(
        "roleplay_policy.punishment_multiplier_milli",
        record.punishment_multiplier_milli,
    )?;
    validate_evidence_ref_count_value(record.min_evidence_refs)?;
    if let Some(value) = record.supersedes_policy_id.as_deref() {
        validate_id("roleplay_policy.supersedes_policy_id", value)?;
        if value == record.policy_id {
            return Err(moderation_error(
                "roleplay_policy.supersedes_policy_id",
                "policy cannot supersede itself",
            ));
        }
    }
    Ok(())
}

pub fn validate_roleplay_evaluation(
    record: &RoleplayPolicyEvaluationRecord,
) -> Result<(), EngineError> {
    validate_id("roleplay_evaluation.evaluation_id", &record.evaluation_id)?;
    validate_id("roleplay_evaluation.policy_id", &record.policy_id)?;
    validate_id("roleplay_evaluation.lobby_id", &record.lobby_id)?;
    validate_id("roleplay_evaluation.account_id", &record.account_id)?;
    validate_id(
        "roleplay_evaluation.evaluated_by_account_id",
        &record.evaluated_by_account_id,
    )?;
    validate_milli_value(
        "roleplay_evaluation.rp_fit_score_milli",
        record.rp_fit_score_milli,
    )?;
    validate_milli_value(
        "roleplay_evaluation.reward_delta_milli",
        record.reward_delta_milli,
    )?;
    validate_milli_value(
        "roleplay_evaluation.punishment_delta_milli",
        record.punishment_delta_milli,
    )?;
    if record.reward_delta_milli > 0 && record.punishment_delta_milli > 0 {
        return Err(moderation_error(
            "roleplay_evaluation.delta",
            "reward and punishment deltas cannot both be positive",
        ));
    }
    validate_sha256(
        "roleplay_evaluation.evidence_sha256",
        &record.evidence_sha256,
    )?;
    if record.evidence_refs.is_empty() {
        return Err(moderation_error(
            "roleplay_evaluation.evidence_refs",
            "roleplay evaluation requires evidence refs",
        ));
    }
    for value in &record.evidence_refs {
        validate_id("roleplay_evaluation.evidence_refs", value)?;
    }
    Ok(())
}

pub fn thresholds_for_strictness(strictness_milli: u16) -> (u16, u16) {
    let reward_threshold = 600 + ((strictness_milli as u32 * 300) / 1000) as u16;
    let punishment_threshold = 200 + ((strictness_milli as u32 * 400) / 1000) as u16;
    (reward_threshold, punishment_threshold)
}

pub fn count_roleplay_verdicts(evaluations: &[RoleplayPolicyEvaluationRecord]) -> (usize, usize) {
    let reward_count = evaluations
        .iter()
        .filter(|evaluation| evaluation.verdict == RoleplayPolicyVerdict::Reward)
        .count();
    let punishment_count = evaluations
        .iter()
        .filter(|evaluation| evaluation.verdict == RoleplayPolicyVerdict::Punish)
        .count();
    (reward_count, punishment_count)
}

fn validate_thresholds(
    strictness_milli: u16,
    reward_threshold_milli: u16,
    punishment_threshold_milli: u16,
) -> Result<(), EngineError> {
    validate_milli_value("roleplay_policy.strictness_milli", strictness_milli)?;
    let expected = thresholds_for_strictness(strictness_milli);
    if (reward_threshold_milli, punishment_threshold_milli) != expected
        || punishment_threshold_milli >= reward_threshold_milli
    {
        return Err(moderation_error(
            "roleplay_policy.thresholds",
            "thresholds must match strictness-derived reward/punishment bounds",
        ));
    }
    Ok(())
}

fn require_roleplay_mode(value: Option<RoleplayPolicyMode>) -> Result<(), EngineError> {
    value
        .map(|_| ())
        .ok_or_else(|| moderation_error("roleplay_policy_mode", "required field missing"))
}

fn require_milli(field: &str, value: Option<u16>) -> Result<(), EngineError> {
    validate_milli_value(
        field,
        value.ok_or_else(|| moderation_error(field, "required field missing"))?,
    )
}

fn validate_milli_value(field: &str, value: u16) -> Result<(), EngineError> {
    if value > 1000 {
        return Err(moderation_error(field, "milli value must be in 0..=1000"));
    }
    Ok(())
}

fn validate_multiplier(field: &str, value: u16) -> Result<(), EngineError> {
    if !(1..=3000).contains(&value) {
        return Err(moderation_error(
            field,
            "multiplier milli value must be in 1..=3000",
        ));
    }
    Ok(())
}

fn require_multiplier(field: &str, value: Option<u16>) -> Result<(), EngineError> {
    validate_multiplier(
        field,
        value.ok_or_else(|| moderation_error(field, "required field missing"))?,
    )
}

fn require_evidence_ref_count(value: Option<u8>) -> Result<(), EngineError> {
    validate_evidence_ref_count_value(
        value.ok_or_else(|| moderation_error("min_evidence_refs", "required field missing"))?,
    )
}

fn validate_evidence_ref_count_value(value: u8) -> Result<(), EngineError> {
    if !(1..=16).contains(&value) {
        return Err(moderation_error(
            "min_evidence_refs",
            "evidence ref count must be in 1..=16",
        ));
    }
    Ok(())
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_id(
        field,
        value.ok_or_else(|| moderation_error(field, "required field missing"))?,
    )
}
