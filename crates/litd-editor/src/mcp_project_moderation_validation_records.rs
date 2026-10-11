use litd_engine::EngineError;

use super::super::super::mcp_project_tools_validation::validate_single_line;
use super::super::mcp_project_moderation_types::{
    AdminActionKind, AdminActionRecord, AppealStatus, ContentReportRecord, ModerationAppealRecord,
    ServerRoleAssignmentRecord,
};
use super::{moderation_error, validate_id, validate_sha256};

pub(super) fn validate_report(record: &ContentReportRecord) -> Result<(), EngineError> {
    validate_id("report.report_id", &record.report_id)?;
    validate_id("report.lobby_id", &record.lobby_id)?;
    validate_id("report.reporter_account_id", &record.reporter_account_id)?;
    if let Some(value) = record.reported_account_id.as_deref() {
        validate_id("report.reported_account_id", value)?;
    }
    if let Some(value) = record.content_address.as_deref() {
        validate_single_line("report.content_address", value)?;
    }
    validate_single_line("report.description", &record.description)?;
    validate_sha256("report.evidence_sha256", &record.evidence_sha256)?;
    if record.severity == 0 || record.severity > 5 {
        return Err(moderation_error(
            "report.severity",
            "severity must be in 1..=5",
        ));
    }
    if let Some(value) = record.resolution_note.as_deref() {
        validate_single_line("report.resolution_note", value)?;
    }
    Ok(())
}

pub(super) fn validate_assignment(record: &ServerRoleAssignmentRecord) -> Result<(), EngineError> {
    validate_id("role.assignment_id", &record.assignment_id)?;
    validate_id("role.lobby_id", &record.lobby_id)?;
    validate_id("role.account_id", &record.account_id)?;
    validate_id("role.granted_by_account_id", &record.granted_by_account_id)
}

pub(super) fn validate_admin_action(record: &AdminActionRecord) -> Result<(), EngineError> {
    validate_id("admin_action.action_id", &record.action_id)?;
    validate_id("admin_action.lobby_id", &record.lobby_id)?;
    validate_id("admin_action.actor_account_id", &record.actor_account_id)?;
    if let Some(value) = record.report_id.as_deref() {
        validate_id("admin_action.report_id", value)?;
    }
    if let Some(value) = record.target_account_id.as_deref() {
        validate_id("admin_action.target_account_id", value)?;
    }
    if let Some(value) = record.content_address.as_deref() {
        validate_single_line("admin_action.content_address", value)?;
    }
    validate_single_line("admin_action.reason", &record.reason)?;
    if matches!(
        record.action_kind,
        AdminActionKind::Mute | AdminActionKind::Ban
    ) && record.duration_minutes.unwrap_or(0) == 0
    {
        return Err(moderation_error(
            "admin_action.duration_minutes",
            "mute and ban actions require a non-zero duration",
        ));
    }
    Ok(())
}

pub(super) fn validate_appeal(record: &ModerationAppealRecord) -> Result<(), EngineError> {
    validate_id("appeal.appeal_id", &record.appeal_id)?;
    validate_id("appeal.action_id", &record.action_id)?;
    validate_id(
        "appeal.submitted_by_account_id",
        &record.submitted_by_account_id,
    )?;
    validate_single_line("appeal.appeal_text", &record.appeal_text)?;
    if let Some(value) = record.decided_by_account_id.as_deref() {
        validate_id("appeal.decided_by_account_id", value)?;
    }
    if let Some(value) = record.decision_note.as_deref() {
        validate_single_line("appeal.decision_note", value)?;
    }
    if record.status != AppealStatus::Submitted && record.decided_by_account_id.is_none() {
        return Err(moderation_error(
            "appeal.decided_by_account_id",
            "decided appeals require decided_by_account_id",
        ));
    }
    Ok(())
}
