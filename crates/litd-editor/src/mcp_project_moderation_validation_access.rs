use litd_engine::EngineError;

use super::super::mcp_project_tools_validation::validate_single_line;
use super::mcp_project_moderation_types::{
    AccessRuleKind, AccessRuleRecord, ConnectionDecisionRecord, HostControlKind, HostControlRecord,
};
use super::mcp_project_moderation_validation::{moderation_error, validate_id};

pub fn validate_access_rule(record: &AccessRuleRecord) -> Result<(), EngineError> {
    validate_id("access_rule.rule_id", &record.rule_id)?;
    validate_id("access_rule.lobby_id", &record.lobby_id)?;
    validate_id("access_rule.account_id", &record.account_id)?;
    validate_id("access_rule.set_by_account_id", &record.set_by_account_id)?;
    validate_single_line("access_rule.reason", &record.reason)
}

pub fn validate_connection_decision(record: &ConnectionDecisionRecord) -> Result<(), EngineError> {
    validate_id("connection.decision_id", &record.decision_id)?;
    validate_id("connection.lobby_id", &record.lobby_id)?;
    validate_id("connection.account_id", &record.account_id)?;
    validate_id(
        "connection.evaluated_by_account_id",
        &record.evaluated_by_account_id,
    )?;
    if record.reasons.is_empty() {
        return Err(moderation_error(
            "connection.reasons",
            "connection decision requires at least one reason",
        ));
    }
    for reason in &record.reasons {
        validate_single_line("connection.reason", reason)?;
    }
    Ok(())
}

pub fn validate_host_control(record: &HostControlRecord) -> Result<(), EngineError> {
    validate_id("host_control.control_id", &record.control_id)?;
    validate_id("host_control.lobby_id", &record.lobby_id)?;
    validate_id("host_control.actor_account_id", &record.actor_account_id)?;
    if let Some(value) = record.target_account_id.as_deref() {
        validate_id("host_control.target_account_id", value)?;
    }
    if matches!(
        record.control_kind,
        HostControlKind::Kick | HostControlKind::Mute | HostControlKind::Ban
    ) && record.target_account_id.is_none()
    {
        return Err(moderation_error(
            "host_control.target_account_id",
            "kick, mute, and ban controls require a target account",
        ));
    }
    if let Some(value) = record.action_id.as_deref() {
        validate_id("host_control.action_id", value)?;
    }
    validate_single_line("host_control.reason", &record.reason)
}

pub fn require_access_rule(value: Option<AccessRuleKind>) -> Result<(), EngineError> {
    value
        .map(|_| ())
        .ok_or_else(|| moderation_error("access_rule_kind", "required field missing"))
}

pub fn require_host_control(value: Option<HostControlKind>) -> Result<(), EngineError> {
    value
        .map(|_| ())
        .ok_or_else(|| moderation_error("host_control_kind", "required field missing"))
}
