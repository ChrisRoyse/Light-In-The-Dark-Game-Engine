use litd_engine::EngineError;

use super::super::{
    AccountRecord, MultiplayerLobbyRecord, read_multiplayer_session_state, read_publish_catalog,
    read_social_graph_state,
};
use super::mcp_project_moderation_types::{
    AccessRuleKind, AdminActionKind, ContentReportRecord, ModerationSafetyArgs,
    ModerationSafetyState, ReportStatus, RoleplayPolicyVerdict, ServerRole,
};
use super::mcp_project_moderation_validation::{moderation_error, validate_id};

pub fn authorize_role_assignment(
    state: &ModerationSafetyState,
    lobby: &MultiplayerLobbyRecord,
    actor: &AccountRecord,
    account_id: &str,
    role: ServerRole,
) -> Result<(), EngineError> {
    let active_roles = state
        .role_assignments
        .iter()
        .filter(|assignment| assignment.lobby_id == lobby.lobby_id && assignment.active)
        .collect::<Vec<_>>();
    if active_roles.is_empty() {
        if actor.player_id != lobby.host_player_id || actor.account_id != account_id {
            return Err(moderation_error(
                "role.bootstrap",
                "first owner role must be self-granted by the lobby host",
            ));
        }
        if role != ServerRole::Owner {
            return Err(moderation_error(
                "role.bootstrap",
                "first role must be owner",
            ));
        }
        return Ok(());
    }
    let actor_role = require_role_for_action(state, &lobby.lobby_id, &actor.account_id)?;
    if actor_role == ServerRole::Moderator {
        return Err(moderation_error(
            "role.actor",
            "moderators cannot grant server roles",
        ));
    }
    if actor_role == ServerRole::Admin && role != ServerRole::Moderator {
        return Err(moderation_error(
            "role.actor",
            "admins can only grant moderator roles",
        ));
    }
    Ok(())
}

pub fn require_role_for_action(
    state: &ModerationSafetyState,
    lobby_id: &str,
    account_id: &str,
) -> Result<ServerRole, EngineError> {
    state
        .role_assignments
        .iter()
        .filter(|assignment| {
            assignment.lobby_id == lobby_id
                && assignment.account_id == account_id
                && assignment.active
        })
        .map(|assignment| assignment.role)
        .max()
        .ok_or_else(|| moderation_error("role", "active moderator/admin/owner role required"))
}

pub fn ensure_social_account(
    args: &ModerationSafetyArgs,
    account_id: &str,
) -> Result<AccountRecord, EngineError> {
    read_social_graph_state(&args.social_state_path)?
        .state
        .accounts
        .into_iter()
        .find(|account| account.account_id == account_id)
        .ok_or_else(|| moderation_error("account_id", "account is not in social graph state"))
}

pub fn ensure_lobby(
    args: &ModerationSafetyArgs,
    lobby_id: &str,
) -> Result<MultiplayerLobbyRecord, EngineError> {
    read_multiplayer_session_state(&args.multiplayer_state_path)?
        .state
        .lobbies
        .into_iter()
        .find(|lobby| lobby.lobby_id == lobby_id)
        .ok_or_else(|| moderation_error("lobby_id", "lobby is not in multiplayer state"))
}

pub fn ensure_published_content(
    args: &ModerationSafetyArgs,
    content_address: &str,
) -> Result<(), EngineError> {
    read_publish_catalog(&args.catalog_path)?
        .catalog
        .entries
        .iter()
        .any(|entry| entry.content_address == content_address)
        .then_some(())
        .ok_or_else(|| moderation_error("content_address", "content is not published"))
}

pub fn find_report<'a>(
    state: &'a ModerationSafetyState,
    report_id: &str,
) -> Result<&'a ContentReportRecord, EngineError> {
    validate_id("report_id", report_id)?;
    state
        .reports
        .iter()
        .find(|report| report.report_id == report_id)
        .ok_or_else(|| moderation_error("report_id", "report does not exist"))
}

pub fn find_report_mut<'a>(
    state: &'a mut ModerationSafetyState,
    report_id: &str,
) -> Result<&'a mut ContentReportRecord, EngineError> {
    validate_id("report_id", report_id)?;
    state
        .reports
        .iter_mut()
        .find(|report| report.report_id == report_id)
        .ok_or_else(|| moderation_error("report_id", "report does not exist"))
}

pub fn refresh_counts(state: &mut ModerationSafetyState) {
    state.report_count = state.reports.len();
    state.role_assignment_count = state.role_assignments.len();
    state.admin_action_count = state.admin_actions.len();
    state.appeal_count = state.appeals.len();
    state.access_rule_count = state.access_rules.len();
    state.connection_decision_count = state.connection_decisions.len();
    state.host_control_count = state.host_controls.len();
    state.roleplay_policy_count = state.roleplay_policies.len();
    state.roleplay_evaluation_count = state.roleplay_evaluations.len();
    state.open_report_count = state
        .reports
        .iter()
        .filter(|report| report.status == ReportStatus::Open)
        .count();
    state.active_restriction_count = state
        .admin_actions
        .iter()
        .filter(|action| action.active)
        .count();
    state.allow_rule_count = state
        .access_rules
        .iter()
        .filter(|rule| rule.active && rule.rule_kind == AccessRuleKind::Allow)
        .count();
    state.ban_rule_count = state
        .access_rules
        .iter()
        .filter(|rule| rule.active && rule.rule_kind == AccessRuleKind::Ban)
        .count();
    state.active_roleplay_policy_count = state
        .roleplay_policies
        .iter()
        .filter(|policy| policy.active)
        .count();
    state.reward_verdict_count = state
        .roleplay_evaluations
        .iter()
        .filter(|evaluation| evaluation.verdict == RoleplayPolicyVerdict::Reward)
        .count();
    state.punishment_verdict_count = state
        .roleplay_evaluations
        .iter()
        .filter(|evaluation| evaluation.verdict == RoleplayPolicyVerdict::Punish)
        .count();
}

pub fn affected_ids(state: &ModerationSafetyState) -> Vec<String> {
    let mut ids = state
        .reports
        .iter()
        .map(|report| report.report_id.clone())
        .chain(
            state
                .admin_actions
                .iter()
                .map(|action| action.action_id.clone()),
        )
        .chain(state.appeals.iter().map(|appeal| appeal.appeal_id.clone()))
        .chain(state.access_rules.iter().map(|rule| rule.rule_id.clone()))
        .chain(
            state
                .connection_decisions
                .iter()
                .map(|decision| decision.decision_id.clone()),
        )
        .chain(
            state
                .host_controls
                .iter()
                .map(|control| control.control_id.clone()),
        )
        .chain(
            state
                .roleplay_policies
                .iter()
                .map(|policy| policy.policy_id.clone()),
        )
        .chain(
            state
                .roleplay_evaluations
                .iter()
                .map(|evaluation| evaluation.evaluation_id.clone()),
        )
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.push(state.state_id.clone());
    }
    ids
}

pub fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    value.ok_or_else(|| moderation_error(field, "required field missing"))
}

pub fn action_is_active_restriction(action_kind: AdminActionKind) -> bool {
    matches!(
        action_kind,
        AdminActionKind::Mute | AdminActionKind::Ban | AdminActionKind::LockWorld
    )
}
