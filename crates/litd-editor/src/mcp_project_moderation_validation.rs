use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::super::mcp_project_tools_validation::{
    validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_moderation_types::{
    AccessRuleKind, AdminActionKind, AppealStatus, HostControlKind, ModerationSafetyAction,
    ModerationSafetyArgs, ModerationSafetyState, ReportReason, ReportStatus, ServerRole,
};
use super::mcp_project_moderation_validation_access::{
    require_access_rule, require_host_control, validate_access_rule, validate_connection_decision,
    validate_host_control,
};
use super::mcp_project_moderation_validation_roleplay::{
    count_roleplay_verdicts, validate_evaluate_roleplay_policy_args, validate_roleplay_evaluation,
    validate_roleplay_policy, validate_set_roleplay_policy_args,
};
#[path = "mcp_project_moderation_validation_records.rs"]
mod records;
use records::{validate_admin_action, validate_appeal, validate_assignment, validate_report};

pub fn validate_moderation_args(args: &ModerationSafetyArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path(
        "moderation_state_path",
        Path::new(&args.moderation_state_path),
    )?;
    validate_path("social_state_path", Path::new(&args.social_state_path))?;
    validate_path("catalog_path", Path::new(&args.catalog_path))?;
    validate_path(
        "multiplayer_state_path",
        Path::new(&args.multiplayer_state_path),
    )?;
    validate_id("moderation_state_id", &args.moderation_state_id)?;
    match args.action {
        ModerationSafetyAction::SubmitReport => {
            require_id("report_id", args.report_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("reporter_account_id", args.reporter_account_id.as_deref())?;
            if let Some(value) = args.reported_account_id.as_deref() {
                validate_id("reported_account_id", value)?;
            }
            if let Some(value) = args.content_address.as_deref() {
                validate_single_line("content_address", value)?;
            }
            if args.reported_account_id.is_none() && args.content_address.is_none() {
                return Err(moderation_error(
                    "report.target",
                    "reported_account_id or content_address is required",
                ));
            }
            require_report_reason(args.reason)?;
            require_severity(args.severity)?;
            require_line("description", args.description.as_deref())?;
            require_sha256("evidence_sha256", args.evidence_sha256.as_deref())?;
        }
        ModerationSafetyAction::AssignServerRole => {
            require_id("assignment_id", args.assignment_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
            require_role(args.role)?;
        }
        ModerationSafetyAction::ApplyAdminAction => {
            require_id("action_id", args.action_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            if let Some(value) = args.report_id.as_deref() {
                validate_id("report_id", value)?;
            }
            if let Some(value) = args.target_account_id.as_deref() {
                validate_id("target_account_id", value)?;
            }
            if let Some(value) = args.content_address.as_deref() {
                validate_single_line("content_address", value)?;
            }
            if args.target_account_id.is_none() && args.content_address.is_none() {
                return Err(moderation_error(
                    "admin_action.target",
                    "target_account_id or content_address is required",
                ));
            }
            require_admin_action_kind(args.admin_action_kind)?;
            require_line("resolution_note", args.resolution_note.as_deref())?;
        }
        ModerationSafetyAction::ResolveReport => {
            require_id("report_id", args.report_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            require_resolution_status(args.resolution_status)?;
            require_line("resolution_note", args.resolution_note.as_deref())?;
            if let Some(value) = args.action_id.as_deref() {
                validate_id("action_id", value)?;
            }
        }
        ModerationSafetyAction::SubmitAppeal => {
            require_id("appeal_id", args.appeal_id.as_deref())?;
            require_id("action_id", args.action_id.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
            require_line("appeal_text", args.appeal_text.as_deref())?;
        }
        ModerationSafetyAction::DecideAppeal => {
            require_id("appeal_id", args.appeal_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            require_appeal_status(args.appeal_status)?;
            require_line("decision_note", args.decision_note.as_deref())?;
        }
        ModerationSafetyAction::SetAccessRule => {
            require_id("rule_id", args.rule_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
            require_access_rule(args.access_rule_kind)?;
            require_line("resolution_note", args.resolution_note.as_deref())?;
        }
        ModerationSafetyAction::EvaluateConnection => {
            require_id("decision_id", args.decision_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
        }
        ModerationSafetyAction::RecordHostControl => {
            require_id("control_id", args.control_id.as_deref())?;
            require_id("lobby_id", args.lobby_id.as_deref())?;
            require_id("actor_account_id", args.actor_account_id.as_deref())?;
            require_host_control(args.host_control_kind)?;
            if matches!(
                args.host_control_kind,
                Some(HostControlKind::Kick | HostControlKind::Mute | HostControlKind::Ban)
            ) {
                require_id("target_account_id", args.target_account_id.as_deref())?;
            }
            if let Some(value) = args.target_account_id.as_deref() {
                validate_id("target_account_id", value)?;
            }
            require_line("resolution_note", args.resolution_note.as_deref())?;
            if let Some(value) = args.action_id.as_deref() {
                validate_id("action_id", value)?;
            }
        }
        ModerationSafetyAction::SetRoleplayPolicy => validate_set_roleplay_policy_args(args)?,
        ModerationSafetyAction::EvaluateRoleplayPolicy => {
            validate_evaluate_roleplay_policy_args(args)?
        }
    }
    Ok(())
}

pub fn validate_moderation_state(state: &ModerationSafetyState) -> Result<(), EngineError> {
    let (reward_verdict_count, punishment_verdict_count) =
        count_roleplay_verdicts(&state.roleplay_evaluations);
    if state.schema != super::mcp_project_moderation_types::MODERATION_SAFETY_STATE_SCHEMA
        || state.issue != 1163
        || state.source_of_truth != "Calyx Aster moderation safety state"
        || state.state_id.trim().is_empty()
        || state.social_state_path.trim().is_empty()
        || state.catalog_path.trim().is_empty()
        || state.multiplayer_state_path.trim().is_empty()
        || state.report_count != state.reports.len()
        || state.role_assignment_count != state.role_assignments.len()
        || state.admin_action_count != state.admin_actions.len()
        || state.appeal_count != state.appeals.len()
        || state.access_rule_count != state.access_rules.len()
        || state.connection_decision_count != state.connection_decisions.len()
        || state.host_control_count != state.host_controls.len()
        || state.roleplay_policy_count != state.roleplay_policies.len()
        || state.roleplay_evaluation_count != state.roleplay_evaluations.len()
        || state.open_report_count
            != state
                .reports
                .iter()
                .filter(|report| report.status == ReportStatus::Open)
                .count()
        || state.active_restriction_count
            != state
                .admin_actions
                .iter()
                .filter(|action| action.active)
                .count()
        || state.allow_rule_count
            != state
                .access_rules
                .iter()
                .filter(|rule| rule.active && rule.rule_kind == AccessRuleKind::Allow)
                .count()
        || state.ban_rule_count
            != state
                .access_rules
                .iter()
                .filter(|rule| rule.active && rule.rule_kind == AccessRuleKind::Ban)
                .count()
        || state.active_roleplay_policy_count
            != state
                .roleplay_policies
                .iter()
                .filter(|policy| policy.active)
                .count()
        || state.reward_verdict_count != reward_verdict_count
        || state.punishment_verdict_count != punishment_verdict_count
    {
        return Err(moderation_error(
            "state",
            "schema, identity, source, paths, and counts are invalid",
        ));
    }
    let mut reports = BTreeSet::new();
    for report in &state.reports {
        validate_report(report)?;
        if !reports.insert(report.report_id.as_str()) {
            return Err(moderation_error("reports", "duplicate report_id"));
        }
    }
    let mut assignments = BTreeSet::new();
    for assignment in &state.role_assignments {
        validate_assignment(assignment)?;
        if !assignments.insert(assignment.assignment_id.as_str()) {
            return Err(moderation_error(
                "role_assignments",
                "duplicate assignment_id",
            ));
        }
    }
    let mut actions = BTreeSet::new();
    for action in &state.admin_actions {
        validate_admin_action(action)?;
        if let Some(report_id) = action.report_id.as_deref()
            && !reports.contains(report_id)
        {
            return Err(moderation_error(
                "admin_action.report_id",
                "admin action report_id must reference an existing report",
            ));
        }
        if !actions.insert(action.action_id.as_str()) {
            return Err(moderation_error("admin_actions", "duplicate action_id"));
        }
    }
    let mut appeals = BTreeSet::new();
    for appeal in &state.appeals {
        validate_appeal(appeal)?;
        if !actions.contains(appeal.action_id.as_str()) {
            return Err(moderation_error(
                "appeal.action_id",
                "appeal must reference an existing admin action",
            ));
        }
        if !appeals.insert(appeal.appeal_id.as_str()) {
            return Err(moderation_error("appeals", "duplicate appeal_id"));
        }
    }
    let mut access_rules = BTreeSet::new();
    for rule in &state.access_rules {
        validate_access_rule(rule)?;
        if !access_rules.insert(rule.rule_id.as_str()) {
            return Err(moderation_error("access_rules", "duplicate rule_id"));
        }
    }
    let mut decisions = BTreeSet::new();
    for decision in &state.connection_decisions {
        validate_connection_decision(decision)?;
        if !decisions.insert(decision.decision_id.as_str()) {
            return Err(moderation_error(
                "connection_decisions",
                "duplicate decision_id",
            ));
        }
    }
    let mut controls = BTreeSet::new();
    for control in &state.host_controls {
        validate_host_control(control)?;
        if !controls.insert(control.control_id.as_str()) {
            return Err(moderation_error("host_controls", "duplicate control_id"));
        }
        if let Some(action_id) = control.action_id.as_deref()
            && !actions.contains(action_id)
        {
            return Err(moderation_error(
                "host_control.action_id",
                "host control action_id must reference an existing admin action",
            ));
        }
    }
    let mut roleplay_policies = BTreeSet::new();
    let mut active_roleplay_lobbies = BTreeSet::new();
    for policy in &state.roleplay_policies {
        validate_roleplay_policy(policy)?;
        if !roleplay_policies.insert(policy.policy_id.as_str()) {
            return Err(moderation_error("roleplay_policies", "duplicate policy_id"));
        }
        if let Some(supersedes) = policy.supersedes_policy_id.as_deref()
            && !roleplay_policies.contains(supersedes)
        {
            return Err(moderation_error(
                "roleplay_policy.supersedes_policy_id",
                "superseded policy must appear earlier in state history",
            ));
        }
        if policy.active && !active_roleplay_lobbies.insert(policy.lobby_id.as_str()) {
            return Err(moderation_error(
                "roleplay_policies",
                "only one active roleplay policy is allowed per lobby",
            ));
        }
    }
    let mut roleplay_evaluations = BTreeSet::new();
    for evaluation in &state.roleplay_evaluations {
        validate_roleplay_evaluation(evaluation)?;
        if !roleplay_policies.contains(evaluation.policy_id.as_str()) {
            return Err(moderation_error(
                "roleplay_evaluation.policy_id",
                "evaluation must reference an existing roleplay policy",
            ));
        }
        if !roleplay_evaluations.insert(evaluation.evaluation_id.as_str()) {
            return Err(moderation_error(
                "roleplay_evaluations",
                "duplicate evaluation_id",
            ));
        }
    }
    Ok(())
}

pub fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(moderation_error(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

pub fn moderation_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    let field = format!(
        "editor.mcp_project_tools.moderation_safety.{}",
        field.into()
    );
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::moderation_safety",
        field = %field,
        reason = %reason,
        code = "LITD-MODERATION-SAFETY-VALIDATION",
        "moderation safety operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_id(
        field,
        value.ok_or_else(|| moderation_error(field, "required field missing"))?,
    )
}

fn require_line(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_single_line(
        field,
        value.ok_or_else(|| moderation_error(field, "required field missing"))?,
    )
}

fn require_sha256(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_sha256(
        field,
        value.ok_or_else(|| moderation_error(field, "required field missing"))?,
    )
}

pub fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !litd_hash::is_sha256_hex(value) {
        return Err(moderation_error(
            field,
            "value must be a 64-character lowercase SHA-256 hex",
        ));
    }
    Ok(())
}

fn require_report_reason(value: Option<ReportReason>) -> Result<(), EngineError> {
    value
        .map(|_| ())
        .ok_or_else(|| moderation_error("reason", "required field missing"))
}

fn require_role(value: Option<ServerRole>) -> Result<(), EngineError> {
    value
        .map(|_| ())
        .ok_or_else(|| moderation_error("role", "required field missing"))
}

fn require_admin_action_kind(value: Option<AdminActionKind>) -> Result<(), EngineError> {
    value
        .map(|_| ())
        .ok_or_else(|| moderation_error("admin_action_kind", "required field missing"))
}

fn require_appeal_status(value: Option<AppealStatus>) -> Result<(), EngineError> {
    match value {
        Some(AppealStatus::Submitted) | None => Err(moderation_error(
            "appeal_status",
            "decision must be upheld, lifted, or modified",
        )),
        Some(_) => Ok(()),
    }
}

fn require_resolution_status(value: Option<ReportStatus>) -> Result<(), EngineError> {
    match value {
        Some(ReportStatus::Actioned | ReportStatus::Dismissed) => Ok(()),
        Some(ReportStatus::Open | ReportStatus::Appealed) | None => Err(moderation_error(
            "resolution_status",
            "resolution must be actioned or dismissed",
        )),
    }
}

fn require_severity(value: Option<u8>) -> Result<(), EngineError> {
    match value {
        Some(1..=5) => Ok(()),
        _ => Err(moderation_error("severity", "severity must be in 1..=5")),
    }
}
