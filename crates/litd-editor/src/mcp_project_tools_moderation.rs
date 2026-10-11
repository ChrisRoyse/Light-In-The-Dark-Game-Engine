use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

#[path = "mcp_project_moderation_access.rs"]
mod mcp_project_moderation_access;
#[path = "mcp_project_moderation_controls.rs"]
mod mcp_project_moderation_controls;
#[path = "mcp_project_moderation_roleplay.rs"]
mod mcp_project_moderation_roleplay;
#[path = "mcp_project_moderation_store.rs"]
mod mcp_project_moderation_store;
#[path = "mcp_project_moderation_types.rs"]
mod mcp_project_moderation_types;
#[path = "mcp_project_moderation_validation.rs"]
mod mcp_project_moderation_validation;
#[path = "mcp_project_moderation_validation_access.rs"]
mod mcp_project_moderation_validation_access;
#[path = "mcp_project_moderation_validation_roleplay.rs"]
mod mcp_project_moderation_validation_roleplay;

pub use self::{
    mcp_project_moderation_store::read_moderation_safety_state,
    mcp_project_moderation_types::{
        AccessRuleKind, AccessRuleRecord, AdminActionKind, AdminActionRecord, AppealStatus,
        ConnectionDecisionRecord, ContentReportRecord, HostControlKind, HostControlRecord,
        MODERATION_SAFETY_STATE_SCHEMA, MODERATION_SAFETY_TOOL, ModerationAppealRecord,
        ModerationSafetyAction, ModerationSafetyState, ModerationSafetyStateReadback, ReportReason,
        ReportStatus, RoleplayPolicyEvaluationRecord, RoleplayPolicyMode, RoleplayPolicyRecord,
        RoleplayPolicyVerdict, ServerRole, ServerRoleAssignmentRecord,
    },
};

use self::{
    mcp_project_moderation_access::{
        action_is_active_restriction, affected_ids, authorize_role_assignment, ensure_lobby,
        ensure_published_content, ensure_social_account, find_report, find_report_mut,
        refresh_counts, require_role_for_action, required,
    },
    mcp_project_moderation_controls::{evaluate_connection, record_host_control, set_access_rule},
    mcp_project_moderation_roleplay::{evaluate_roleplay_policy, set_roleplay_policy},
    mcp_project_moderation_store::{
        load_or_create_moderation_state, write_moderation_safety_state,
    },
    mcp_project_moderation_types::ModerationSafetyArgs,
    mcp_project_moderation_validation::{moderation_error, validate_moderation_args},
};
use super::{
    McpProjectToolCallReadback, READ_PROJECT_TOOL,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, write_mcp_project_state,
        write_mcp_project_tool_ledger,
    },
    parse_args, validate_mutating_route,
};

pub(super) fn manage_moderation_safety(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: ModerationSafetyArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        (
            "moderation_state_path",
            std::path::Path::new(&args.moderation_state_path),
        ),
        (
            "social_state_path",
            std::path::Path::new(&args.social_state_path),
        ),
        ("catalog_path", std::path::Path::new(&args.catalog_path)),
        (
            "multiplayer_state_path",
            std::path::Path::new(&args.multiplayer_state_path),
        ),
    ])?;
    validate_moderation_args(&args)?;
    let mut moderation_state = load_or_create_moderation_state(
        &args.moderation_state_path,
        &args.moderation_state_id,
        &args.social_state_path,
        &args.catalog_path,
        &args.multiplayer_state_path,
    )?;
    let result = match args.action {
        ModerationSafetyAction::SubmitReport => submit_report(&args, &mut moderation_state)?,
        ModerationSafetyAction::AssignServerRole => {
            assign_server_role(&args, &mut moderation_state)?
        }
        ModerationSafetyAction::ApplyAdminAction => {
            apply_admin_action(&args, &mut moderation_state)?
        }
        ModerationSafetyAction::ResolveReport => resolve_report(&args, &mut moderation_state)?,
        ModerationSafetyAction::SubmitAppeal => submit_appeal(&args, &mut moderation_state)?,
        ModerationSafetyAction::DecideAppeal => decide_appeal(&args, &mut moderation_state)?,
        ModerationSafetyAction::SetAccessRule => set_access_rule(&args, &mut moderation_state)?,
        ModerationSafetyAction::EvaluateConnection => {
            evaluate_connection(&args, &mut moderation_state)?
        }
        ModerationSafetyAction::RecordHostControl => {
            record_host_control(&args, &mut moderation_state)?
        }
        ModerationSafetyAction::SetRoleplayPolicy => {
            set_roleplay_policy(&args, &mut moderation_state)?
        }
        ModerationSafetyAction::EvaluateRoleplayPolicy => {
            evaluate_roleplay_policy(&args, &mut moderation_state)?
        }
    };
    refresh_counts(&mut moderation_state);
    let moderation_readback =
        write_moderation_safety_state(&args.moderation_state_path, &moderation_state)?;

    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    state.update_count = state.update_count.saturating_add(1);
    let state_readback = write_mcp_project_state(&args.state_path, &state)?;
    let ledger_readback = write_mcp_project_tool_ledger(
        &args.ledger_path,
        &args.ledger_id,
        McpProjectToolRecord {
            request_id: args.request_id.clone(),
            requested_by: args.requested_by.clone(),
            tool_name: route.tool_name.clone(),
            operation_id: route.operation_id.clone(),
            state_path: state_readback.path.clone(),
            state_sha256: state_readback.sha256.clone(),
            state_bytes: state_readback.bytes,
            action_summary: "managed reports, sanctions, appeals, server controls, or RP policy"
                .to_string(),
            affected_ids: affected_ids(&moderation_readback.state),
            project_count: state_readback.project_count,
            playtest_count: state_readback.playtest_count,
            package_count: state_readback.package_count,
            published_count: state_readback.published_count,
        },
    )?;

    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: "Calyx Aster moderation safety + social + catalog + multiplayer state"
            .to_string(),
        result: json!({
            "moderation_state_readback": moderation_readback,
            "operation": result
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

fn submit_report(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let report_id = required(args.report_id.as_deref(), "report_id")?;
    if state
        .reports
        .iter()
        .any(|report| report.report_id == report_id)
    {
        return Err(moderation_error("report_id", "report already exists"));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let reporter = required(args.reporter_account_id.as_deref(), "reporter_account_id")?;
    ensure_social_account(args, reporter)?;
    if let Some(account_id) = args.reported_account_id.as_deref() {
        ensure_social_account(args, account_id)?;
    }
    if let Some(content_address) = args.content_address.as_deref() {
        ensure_published_content(args, content_address)?;
    }
    let report = ContentReportRecord {
        report_id: report_id.to_string(),
        lobby_id: lobby_id.to_string(),
        reporter_account_id: reporter.to_string(),
        reported_account_id: args.reported_account_id.clone(),
        content_address: args.content_address.clone(),
        reason: args.reason.unwrap(),
        severity: args.severity.unwrap(),
        description: required(args.description.as_deref(), "description")?.to_string(),
        evidence_sha256: required(args.evidence_sha256.as_deref(), "evidence_sha256")?.to_string(),
        status: ReportStatus::Open,
        resolution_note: None,
        action_id: None,
    };
    state.reports.push(report.clone());
    Ok(json!({ "report": report }))
}

fn assign_server_role(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let assignment_id = required(args.assignment_id.as_deref(), "assignment_id")?;
    if state
        .role_assignments
        .iter()
        .any(|assignment| assignment.assignment_id == assignment_id)
    {
        return Err(moderation_error(
            "assignment_id",
            "role assignment already exists",
        ));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    let lobby = ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    let actor = ensure_social_account(args, actor_id)?;
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    ensure_social_account(args, account_id)?;
    let role = args.role.unwrap();
    authorize_role_assignment(state, &lobby, &actor, account_id, role)?;
    if state.role_assignments.iter().any(|assignment| {
        assignment.lobby_id == lobby_id
            && assignment.account_id == account_id
            && assignment.role == role
            && assignment.active
    }) {
        return Err(moderation_error(
            "role_assignment",
            "active role assignment already exists for account",
        ));
    }
    let assignment = ServerRoleAssignmentRecord {
        assignment_id: assignment_id.to_string(),
        lobby_id: lobby_id.to_string(),
        account_id: account_id.to_string(),
        role,
        granted_by_account_id: actor_id.to_string(),
        active: true,
    };
    state.role_assignments.push(assignment.clone());
    Ok(json!({ "role_assignment": assignment }))
}

fn apply_admin_action(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let action_id = required(args.action_id.as_deref(), "action_id")?;
    if state
        .admin_actions
        .iter()
        .any(|action| action.action_id == action_id)
    {
        return Err(moderation_error("action_id", "admin action already exists"));
    }
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    let actor_role = require_role_for_action(state, lobby_id, actor_id)?;
    let action_kind = args.admin_action_kind.unwrap();
    if actor_role == ServerRole::Moderator
        && matches!(
            action_kind,
            AdminActionKind::Ban | AdminActionKind::LockWorld
        )
    {
        return Err(moderation_error(
            "admin_action.role",
            "moderators cannot ban accounts or lock worlds",
        ));
    }
    if let Some(report_id) = args.report_id.as_deref() {
        let report = find_report(state, report_id)?;
        if report.lobby_id != lobby_id {
            return Err(moderation_error(
                "report.lobby_id",
                "report belongs to another lobby",
            ));
        }
    }
    if let Some(account_id) = args.target_account_id.as_deref() {
        ensure_social_account(args, account_id)?;
    }
    if let Some(content_address) = args.content_address.as_deref() {
        ensure_published_content(args, content_address)?;
    }
    let active = action_is_active_restriction(action_kind);
    let action = AdminActionRecord {
        action_id: action_id.to_string(),
        report_id: args.report_id.clone(),
        lobby_id: lobby_id.to_string(),
        actor_account_id: actor_id.to_string(),
        target_account_id: args.target_account_id.clone(),
        content_address: args.content_address.clone(),
        action_kind,
        reason: required(args.resolution_note.as_deref(), "resolution_note")?.to_string(),
        duration_minutes: args.duration_minutes,
        active,
    };
    state.admin_actions.push(action.clone());
    Ok(json!({ "admin_action": action }))
}

fn resolve_report(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let report_id = required(args.report_id.as_deref(), "report_id")?;
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    require_role_for_action(state, lobby_id, actor_id)?;
    let status = args.resolution_status.unwrap();
    let action_id = args.action_id.clone();
    if status == ReportStatus::Actioned {
        let action_id = required(action_id.as_deref(), "action_id")?;
        if !state.admin_actions.iter().any(|action| {
            action.action_id == action_id && action.report_id.as_deref() == Some(report_id)
        }) {
            return Err(moderation_error(
                "report.action_id",
                "actioned reports require a matching admin action",
            ));
        }
    }
    let report = find_report_mut(state, report_id)?;
    if report.lobby_id != lobby_id {
        return Err(moderation_error(
            "report.lobby_id",
            "report belongs to another lobby",
        ));
    }
    if !matches!(report.status, ReportStatus::Open | ReportStatus::Appealed) {
        return Err(moderation_error(
            "report.status",
            "report is already resolved",
        ));
    }
    report.status = status;
    report.resolution_note =
        Some(required(args.resolution_note.as_deref(), "resolution_note")?.to_string());
    report.action_id = action_id;
    Ok(json!({ "report": report }))
}

fn submit_appeal(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let appeal_id = required(args.appeal_id.as_deref(), "appeal_id")?;
    if state
        .appeals
        .iter()
        .any(|appeal| appeal.appeal_id == appeal_id)
    {
        return Err(moderation_error("appeal_id", "appeal already exists"));
    }
    let action_id = required(args.action_id.as_deref(), "action_id")?;
    if state
        .appeals
        .iter()
        .any(|appeal| appeal.action_id == action_id)
    {
        return Err(moderation_error(
            "appeal.action_id",
            "one appeal is allowed per admin action",
        ));
    }
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    ensure_social_account(args, account_id)?;
    let action = state
        .admin_actions
        .iter()
        .find(|action| action.action_id == action_id)
        .ok_or_else(|| moderation_error("action_id", "admin action does not exist"))?;
    if !action.active {
        return Err(moderation_error(
            "appeal.action",
            "only active restrictions can be appealed",
        ));
    }
    if action.target_account_id.as_deref() != Some(account_id) {
        return Err(moderation_error(
            "appeal.account_id",
            "appeal submitter must be the sanctioned account",
        ));
    }
    let appeal = ModerationAppealRecord {
        appeal_id: appeal_id.to_string(),
        action_id: action_id.to_string(),
        submitted_by_account_id: account_id.to_string(),
        status: AppealStatus::Submitted,
        appeal_text: required(args.appeal_text.as_deref(), "appeal_text")?.to_string(),
        decided_by_account_id: None,
        decision_note: None,
    };
    if let Some(report) = state
        .reports
        .iter_mut()
        .find(|report| report.action_id.as_deref() == Some(action_id))
    {
        report.status = ReportStatus::Appealed;
    }
    state.appeals.push(appeal.clone());
    Ok(json!({ "appeal": appeal }))
}

fn decide_appeal(
    args: &ModerationSafetyArgs,
    state: &mut ModerationSafetyState,
) -> Result<Value, EngineError> {
    let appeal_id = required(args.appeal_id.as_deref(), "appeal_id")?;
    let lobby_id = required(args.lobby_id.as_deref(), "lobby_id")?;
    ensure_lobby(args, lobby_id)?;
    let actor_id = required(args.actor_account_id.as_deref(), "actor_account_id")?;
    ensure_social_account(args, actor_id)?;
    let role = require_role_for_action(state, lobby_id, actor_id)?;
    if role == ServerRole::Moderator {
        return Err(moderation_error(
            "appeal.role",
            "moderators cannot decide appeals",
        ));
    }
    let status = args.appeal_status.unwrap();
    let appeal_index = state
        .appeals
        .iter()
        .position(|appeal| appeal.appeal_id == appeal_id)
        .ok_or_else(|| moderation_error("appeal_id", "appeal does not exist"))?;
    if state.appeals[appeal_index].status != AppealStatus::Submitted {
        return Err(moderation_error(
            "appeal.status",
            "appeal is already decided",
        ));
    }
    let action_id = state.appeals[appeal_index].action_id.clone();
    let action = state
        .admin_actions
        .iter_mut()
        .find(|action| action.action_id == action_id)
        .ok_or_else(|| moderation_error("action_id", "admin action does not exist"))?;
    if action.lobby_id != lobby_id {
        return Err(moderation_error(
            "appeal.lobby_id",
            "appealed action belongs to another lobby",
        ));
    }
    if status == AppealStatus::Lifted {
        action.active = false;
    }
    let appeal = &mut state.appeals[appeal_index];
    appeal.status = status;
    appeal.decided_by_account_id = Some(actor_id.to_string());
    appeal.decision_note =
        Some(required(args.decision_note.as_deref(), "decision_note")?.to_string());
    Ok(json!({ "appeal": appeal, "admin_action": action }))
}
