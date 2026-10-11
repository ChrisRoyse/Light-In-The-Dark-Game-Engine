use litd_engine::EngineError;
use serde_json::{Value, json};

use super::{
    mcp_project_moderation_access::{
        ensure_lobby, ensure_social_account, require_role_for_action, required,
    },
    mcp_project_moderation_types::{
        AccessRuleKind, AccessRuleRecord, AdminActionKind, ConnectionDecisionRecord,
        HostControlKind, HostControlRecord, ModerationSafetyArgs, ModerationSafetyState,
        ServerRole,
    },
    mcp_project_moderation_validation::moderation_error,
};

pub fn set_access_rule(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let rule_id = required(args.rule_id.as_deref(), "rule_id")?;
    if state
        .access_rules
        .iter()
        .any(|rule| rule.rule_id == rule_id)
    {
        return Err(moderation_error("rule_id", "access rule already exists"));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    let role = require_role_for_action(state, lobby_id, actor_id)?;
    if role == ServerRole::Moderator {
        return Err(moderation_error(
            "access_rule.role",
            "moderators cannot edit access rules",
        ));
    }
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    ensure_social_account(args, account_id)?;
    let rule_kind = args.access_rule_kind.unwrap();
    if state.access_rules.iter().any(|rule| {
        rule.lobby_id == lobby_id
            && rule.account_id == account_id
            && rule.active
            && rule.rule_kind != rule_kind
    }) {
        return Err(moderation_error(
            "access_rule.conflict",
            "conflicting active allow/ban rule exists for account",
        ));
    }
    let rule = AccessRuleRecord {
        rule_id: rule_id.to_string(),
        lobby_id: lobby_id.to_string(),
        account_id: account_id.to_string(),
        rule_kind,
        reason: required(args.resolution_note.as_deref(), "resolution_note")?.to_string(),
        set_by_account_id: actor_id.to_string(),
        active: true,
    };
    state.access_rules.push(rule.clone());
    Ok(json!({ "access_rule": rule }))
}

pub fn evaluate_connection(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let decision_id = required(args.decision_id.as_deref(), "decision_id")?;
    if state
        .connection_decisions
        .iter()
        .any(|decision| decision.decision_id == decision_id)
    {
        return Err(moderation_error(
            "decision_id",
            "connection decision already exists",
        ));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    let lobby = ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    require_role_for_action(state, lobby_id, actor_id)?;
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    let account = ensure_social_account(args, account_id)?;

    let mut reasons = Vec::new();
    let active_rules = state
        .access_rules
        .iter()
        .filter(|rule| rule.lobby_id == lobby_id && rule.active)
        .collect::<Vec<_>>();
    if active_rules
        .iter()
        .any(|rule| rule.account_id == account_id && rule.rule_kind == AccessRuleKind::Ban)
        || state.admin_actions.iter().any(|action| {
            action.lobby_id == lobby_id
                && action.target_account_id.as_deref() == Some(account_id)
                && action.active
                && action.action_kind == AdminActionKind::Ban
        })
    {
        reasons.push("denied: active ban rule or sanction".to_string());
    }
    let allowlist_enabled = active_rules
        .iter()
        .any(|rule| rule.rule_kind == AccessRuleKind::Allow);
    let explicitly_allowed = active_rules
        .iter()
        .any(|rule| rule.account_id == account_id && rule.rule_kind == AccessRuleKind::Allow);
    let has_server_role = state.role_assignments.iter().any(|assignment| {
        assignment.lobby_id == lobby_id && assignment.account_id == account_id && assignment.active
    });
    if allowlist_enabled
        && !explicitly_allowed
        && !has_server_role
        && account.player_id != lobby.host_player_id
    {
        reasons.push("denied: allowlist requires explicit allow rule".to_string());
    }
    if state.admin_actions.iter().any(|action| {
        action.lobby_id == lobby_id
            && action.active
            && action.action_kind == AdminActionKind::LockWorld
    }) {
        reasons.push("denied: world is locked by host control".to_string());
    }
    let allowed = reasons.is_empty();
    if allowed {
        reasons.push("allowed: no active access rule or sanction blocks account".to_string());
    }
    let decision = ConnectionDecisionRecord {
        decision_id: decision_id.to_string(),
        lobby_id: lobby_id.to_string(),
        account_id: account_id.to_string(),
        evaluated_by_account_id: actor_id.to_string(),
        allowed,
        reasons,
    };
    state.connection_decisions.push(decision.clone());
    Ok(json!({ "connection_decision": decision }))
}

pub fn record_host_control(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let control_id = required(args.control_id.as_deref(), "control_id")?;
    if state
        .host_controls
        .iter()
        .any(|control| control.control_id == control_id)
    {
        return Err(moderation_error(
            "control_id",
            "host control already exists",
        ));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    let role = require_role_for_action(state, lobby_id, actor_id)?;
    let control_kind = args.host_control_kind.unwrap();
    if role == ServerRole::Moderator && matches!(control_kind, HostControlKind::Ban) {
        return Err(moderation_error(
            "host_control.role",
            "moderators cannot record ban host controls",
        ));
    }
    if let Some(account_id) = args.target_account_id.as_deref() {
        ensure_social_account(args, account_id)?;
    }
    if let Some(action_id) = args.action_id.as_deref()
        && !state
            .admin_actions
            .iter()
            .any(|action| action.action_id == action_id)
    {
        return Err(moderation_error(
            "host_control.action_id",
            "linked admin action does not exist",
        ));
    }
    let control = HostControlRecord {
        control_id: control_id.to_string(),
        lobby_id: lobby_id.to_string(),
        actor_account_id: actor_id.to_string(),
        target_account_id: args.target_account_id.clone(),
        control_kind,
        reason: required(args.resolution_note.as_deref(), "resolution_note")?.to_string(),
        action_id: args.action_id.clone(),
    };
    state.host_controls.push(control.clone());
    Ok(json!({ "host_control": control }))
}
