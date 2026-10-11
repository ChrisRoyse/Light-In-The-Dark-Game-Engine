use litd_engine::EngineError;
use serde_json::{Value, json};

use super::{
    mcp_project_moderation_access::{
        ensure_lobby, ensure_social_account, require_role_for_action, required,
    },
    mcp_project_moderation_types::{
        ModerationSafetyArgs, ModerationSafetyState, RoleplayPolicyEvaluationRecord,
        RoleplayPolicyRecord, RoleplayPolicyVerdict, ServerRole,
    },
    mcp_project_moderation_validation::moderation_error,
    mcp_project_moderation_validation_roleplay::thresholds_for_strictness,
};

pub fn set_roleplay_policy(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let policy_id = required(args.policy_id.as_deref(), "policy_id")?;
    if state
        .roleplay_policies
        .iter()
        .any(|policy| policy.policy_id == policy_id)
    {
        return Err(moderation_error(
            "policy_id",
            "roleplay policy already exists",
        ));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    let role = require_role_for_action(state, lobby_id, actor_id)?;
    if role == ServerRole::Moderator {
        return Err(moderation_error(
            "roleplay_policy.role",
            "moderators cannot set roleplay policy",
        ));
    }

    let active_policy_index = state
        .roleplay_policies
        .iter()
        .position(|policy| policy.lobby_id == lobby_id && policy.active);
    match (active_policy_index, args.supersedes_policy_id.as_deref()) {
        (Some(index), Some(supersedes))
            if state.roleplay_policies[index].policy_id == supersedes =>
        {
            state.roleplay_policies[index].active = false;
        }
        (Some(_), Some(_)) => {
            return Err(moderation_error(
                "supersedes_policy_id",
                "supersedes_policy_id does not match active roleplay policy",
            ));
        }
        (Some(_), None) => {
            return Err(moderation_error(
                "supersedes_policy_id",
                "new active policy must supersede the current active policy",
            ));
        }
        (None, Some(_)) => {
            return Err(moderation_error(
                "supersedes_policy_id",
                "cannot supersede when no active roleplay policy exists",
            ));
        }
        (None, None) => {}
    }

    let strictness_milli = args.strictness_milli.unwrap();
    let (reward_threshold_milli, punishment_threshold_milli) =
        thresholds_for_strictness(strictness_milli);
    let policy = RoleplayPolicyRecord {
        policy_id: policy_id.to_string(),
        lobby_id: lobby_id.to_string(),
        set_by_account_id: actor_id.to_string(),
        mode: args.roleplay_policy_mode.unwrap(),
        strictness_milli,
        reward_threshold_milli,
        punishment_threshold_milli,
        reward_multiplier_milli: args.reward_multiplier_milli.unwrap(),
        punishment_multiplier_milli: args.punishment_multiplier_milli.unwrap(),
        min_evidence_refs: args.min_evidence_refs.unwrap(),
        supersedes_policy_id: args.supersedes_policy_id.clone(),
        active: true,
    };
    state.roleplay_policies.push(policy.clone());
    Ok(json!({ "roleplay_policy": policy }))
}

pub fn evaluate_roleplay_policy(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let evaluation_id = required(args.evaluation_id.as_deref(), "evaluation_id")?;
    if state
        .roleplay_evaluations
        .iter()
        .any(|evaluation| evaluation.evaluation_id == evaluation_id)
    {
        return Err(moderation_error(
            "evaluation_id",
            "roleplay policy evaluation already exists",
        ));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    require_role_for_action(state, lobby_id, actor_id)?;
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    ensure_social_account(args, account_id)?;

    let policy = select_active_policy(state, lobby_id, args.policy_id.as_deref())?.clone();
    if args.evidence_refs.len() < policy.min_evidence_refs as usize {
        return Err(moderation_error(
            "evidence_refs",
            "not enough evidence refs for active roleplay policy",
        ));
    }

    let score = args.rp_fit_score_milli.unwrap();
    let (verdict, reward_delta_milli, punishment_delta_milli) =
        score_roleplay_policy(&policy, score);
    let evaluation = RoleplayPolicyEvaluationRecord {
        evaluation_id: evaluation_id.to_string(),
        policy_id: policy.policy_id,
        lobby_id: lobby_id.to_string(),
        account_id: account_id.to_string(),
        evaluated_by_account_id: actor_id.to_string(),
        rp_fit_score_milli: score,
        verdict,
        reward_delta_milli,
        punishment_delta_milli,
        evidence_sha256: required(args.evidence_sha256.as_deref(), "evidence_sha256")?.to_string(),
        evidence_refs: args.evidence_refs.clone(),
    };
    state.roleplay_evaluations.push(evaluation.clone());
    Ok(json!({ "roleplay_evaluation": evaluation }))
}

fn select_active_policy<'a>(
    state: &'a ModerationSafetyState,
    lobby_id: &str,
    requested_policy_id: Option<&str>,
) -> Result<&'a RoleplayPolicyRecord, EngineError> {
    let policies = state
        .roleplay_policies
        .iter()
        .filter(|policy| policy.lobby_id == lobby_id && policy.active)
        .collect::<Vec<_>>();
    if let Some(policy_id) = requested_policy_id {
        return policies
            .into_iter()
            .find(|policy| policy.policy_id == policy_id)
            .ok_or_else(|| moderation_error("policy_id", "active roleplay policy not found"));
    }
    match policies.as_slice() {
        [policy] => Ok(*policy),
        [] => Err(moderation_error(
            "policy_id",
            "active roleplay policy is required",
        )),
        _ => Err(moderation_error(
            "policy_id",
            "multiple active roleplay policies require explicit policy_id",
        )),
    }
}

fn score_roleplay_policy(
    policy: &RoleplayPolicyRecord,
    score: u16,
) -> (RoleplayPolicyVerdict, u16, u16) {
    if score >= policy.reward_threshold_milli {
        let delta = ((score - policy.reward_threshold_milli) as u32
            * policy.reward_multiplier_milli as u32
            / 1000) as u16;
        (RoleplayPolicyVerdict::Reward, delta, 0)
    } else if score <= policy.punishment_threshold_milli {
        let delta = ((policy.punishment_threshold_milli - score) as u32
            * policy.punishment_multiplier_milli as u32
            / 1000) as u16;
        (RoleplayPolicyVerdict::Punish, 0, delta)
    } else {
        (RoleplayPolicyVerdict::Neutral, 0, 0)
    }
}
