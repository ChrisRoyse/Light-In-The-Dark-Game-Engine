use std::{env, path::PathBuf};

use litd_editor::mcp_project_tools::{
    MODERATION_SAFETY_TOOL, call_mcp_project_tool, read_moderation_safety_state,
    read_publish_catalog,
};
use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde_json::json;

use super::*;

pub(super) struct GuiModerationReportConfig {
    state_path: PathBuf,
    state_id: String,
    ledger_path: PathBuf,
    ledger_id: String,
    moderation_state_path: PathBuf,
    moderation_state_id: String,
    social_state_path: PathBuf,
    catalog_path: PathBuf,
    multiplayer_state_path: PathBuf,
    request_id: String,
    requested_by: String,
    lobby_id: String,
    report_id: String,
    reporter_account_id: String,
    reported_account_id: String,
    content_address: String,
    reason: String,
    severity: u8,
    description: String,
    evidence_sha256: String,
}

impl EditorShellRuntime {
    pub(super) fn submit_moderation_report_command(&mut self) -> Result<(), EngineError> {
        let config = GuiModerationReportConfig::from_env()?;
        call_mcp_project_tool(
            MODERATION_SAFETY_TOOL,
            json!({
                "request_id": config.request_id,
                "requested_by": config.requested_by,
                "state_path": config.state_path,
                "state_id": config.state_id,
                "ledger_path": config.ledger_path,
                "ledger_id": config.ledger_id,
                "moderation_state_path": config.moderation_state_path,
                "moderation_state_id": config.moderation_state_id,
                "social_state_path": config.social_state_path,
                "catalog_path": config.catalog_path,
                "multiplayer_state_path": config.multiplayer_state_path,
                "action": "submit_report",
                "lobby_id": config.lobby_id,
                "report_id": config.report_id,
                "reporter_account_id": config.reporter_account_id,
                "reported_account_id": config.reported_account_id,
                "content_address": config.content_address,
                "reason": config.reason,
                "severity": config.severity,
                "description": config.description,
                "evidence_sha256": config.evidence_sha256
            }),
        )?;
        let readback = read_moderation_safety_state(&config.moderation_state_path)?;
        let report = readback
            .state
            .reports
            .iter()
            .find(|report| report.report_id == config.report_id)
            .ok_or_else(|| {
                moderation_shell_error(
                    "moderation.readback",
                    format!("report {} was not persisted", config.report_id),
                )
            })?;
        self.moderation_last_action = format!(
            "submitted_report:{}:{}",
            report.report_id,
            format!("{:?}", report.status).to_ascii_lowercase()
        );
        self.moderation_report_count = readback.state.report_count;
        self.moderation_state_path = readback.path;
        self.moderation_state_sha256 = readback.sha256;
        Ok(())
    }
}

impl GuiModerationReportConfig {
    fn from_env() -> Result<Self, EngineError> {
        let catalog_path = required_path("LITD_GUI_MODERATION_CATALOG_PATH")?;
        let content_address = match env::var("LITD_GUI_MODERATION_CONTENT_ADDRESS") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => first_catalog_content_address(&catalog_path)?,
        };
        let evidence_text = env::var("LITD_GUI_MODERATION_EVIDENCE_TEXT")
            .unwrap_or_else(|_| "gui moderation report issue1467 evidence".to_string());
        Ok(Self {
            state_path: required_path("LITD_GUI_MODERATION_STATE_PATH")?,
            state_id: optional_text("LITD_GUI_MODERATION_STATE_ID", "issue1160-project-state")?,
            ledger_path: required_path("LITD_GUI_MODERATION_LEDGER_PATH")?,
            ledger_id: optional_text(
                "LITD_GUI_MODERATION_LEDGER_ID",
                "issue1467-gui-moderation-ledger",
            )?,
            moderation_state_path: required_path("LITD_GUI_MODERATION_MODERATION_STATE_PATH")?,
            moderation_state_id: optional_text(
                "LITD_GUI_MODERATION_MODERATION_STATE_ID",
                "issue1467-gui-moderation-state",
            )?,
            social_state_path: required_path("LITD_GUI_MODERATION_SOCIAL_STATE_PATH")?,
            catalog_path,
            multiplayer_state_path: required_path("LITD_GUI_MODERATION_MULTIPLAYER_STATE_PATH")?,
            request_id: optional_text("LITD_GUI_MODERATION_REQUEST_ID", "issue1467-gui-report")?,
            requested_by: optional_text("LITD_GUI_MODERATION_REQUESTED_BY", "litd-editor-gui")?,
            lobby_id: optional_text("LITD_GUI_MODERATION_LOBBY_ID", "lobby.social")?,
            report_id: optional_text("LITD_GUI_MODERATION_REPORT_ID", "report.gui.issue1467")?,
            reporter_account_id: optional_text(
                "LITD_GUI_MODERATION_REPORTER_ACCOUNT_ID",
                "account.host",
            )?,
            reported_account_id: optional_text(
                "LITD_GUI_MODERATION_REPORTED_ACCOUNT_ID",
                "account.friend",
            )?,
            content_address,
            reason: optional_text("LITD_GUI_MODERATION_REASON", "harassment")?,
            severity: optional_severity()?,
            description: optional_text(
                "LITD_GUI_MODERATION_DESCRIPTION",
                "Known deterministic GUI moderation report for issue1467",
            )?,
            evidence_sha256: sha256_hex(evidence_text.as_bytes()),
        })
    }
}

fn first_catalog_content_address(path: &PathBuf) -> Result<String, EngineError> {
    let catalog = read_publish_catalog(path)?;
    catalog
        .catalog
        .entries
        .first()
        .map(|entry| entry.content_address.clone())
        .ok_or_else(|| moderation_shell_error("moderation.catalog", "publish catalog is empty"))
}

fn required_path(name: &str) -> Result<PathBuf, EngineError> {
    let value = env::var(name)
        .map_err(|_| moderation_shell_error("moderation.env", format!("{name} is required")))?;
    if value.trim().is_empty() {
        return Err(moderation_shell_error(
            "moderation.env",
            format!("{name} must be non-empty"),
        ));
    }
    Ok(PathBuf::from(value))
}

fn optional_text(name: &str, default: &str) -> Result<String, EngineError> {
    let value = env::var(name).unwrap_or_else(|_| default.to_string());
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(moderation_shell_error(
            "moderation.env",
            format!("{name} must be non-empty single-line text"),
        ));
    }
    Ok(value)
}

fn optional_severity() -> Result<u8, EngineError> {
    let raw = env::var("LITD_GUI_MODERATION_SEVERITY").unwrap_or_else(|_| "4".to_string());
    let value = raw.parse::<u8>().map_err(|error| {
        moderation_shell_error(
            "moderation.env",
            format!("LITD_GUI_MODERATION_SEVERITY parse failed: {error}"),
        )
    })?;
    if !(1..=5).contains(&value) {
        return Err(moderation_shell_error(
            "moderation.env",
            "LITD_GUI_MODERATION_SEVERITY must be inside 1..=5",
        ));
    }
    Ok(value)
}

fn moderation_shell_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.shell.runtime.{}", field.into()),
        reason: reason.into(),
    }
}
