use std::path::Path;

use litd_engine::EngineError;

use super::super::DELIVERY_LITDGAME_TOOL;
use super::super::mcp_project_tools_validation::{
    validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_hub_types::{
    HubGameCard, HubLaunchRecord, HubLauncherArgs, HubLauncherState,
};

pub fn validate_hub_args(args: &HubLauncherArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path("catalog_path", Path::new(&args.catalog_path))?;
    validate_path(
        "install_manifest_path",
        Path::new(&args.install_manifest_path),
    )?;
    validate_path("hub_state_path", Path::new(&args.hub_state_path))?;
    validate_single_line("hub_id", &args.hub_id)?;
    if let Some(query) = args.query.as_deref() {
        validate_single_line("hub.query", query)?;
    }
    for tag in &args.required_tags {
        validate_single_line("hub.required_tags", tag)?;
    }
    if args.launch_content_address.is_some() && args.launched_by.is_none() {
        return Err(hub_error(
            "launch.launched_by",
            "launched_by is required when launch_content_address is set",
        ));
    }
    if let Some(content_address) = args.launch_content_address.as_deref() {
        validate_content_address(content_address)?;
    }
    if let Some(launched_by) = args.launched_by.as_deref() {
        validate_single_line("launch.launched_by", launched_by)?;
    }
    Ok(())
}

pub fn validate_hub_state(state: &HubLauncherState) -> Result<(), EngineError> {
    if state.schema != super::mcp_project_hub_types::HUB_LAUNCHER_STATE_SCHEMA
        || state.issue != 1157
        || state.hub_id.trim().is_empty()
        || state.source_of_truth
            != "Calyx Aster hub launcher state + publish catalog + install manifest"
        || state.catalog_path.trim().is_empty()
        || state.install_manifest_path.trim().is_empty()
        || state.result_count != state.cards.len()
        || state.launch_count != state.launches.len()
    {
        return Err(hub_error(
            "state",
            "schema, source, identity, and counts are invalid",
        ));
    }
    let mut cards = std::collections::BTreeSet::new();
    for card in &state.cards {
        validate_hub_card(card)?;
        if !cards.insert(card.content_address.as_str()) {
            return Err(hub_error("state.cards", "duplicate content address"));
        }
    }
    let mut launches = std::collections::BTreeSet::new();
    for launch in &state.launches {
        validate_launch_record(launch)?;
        if !launches.insert(launch.launch_id.as_str()) {
            return Err(hub_error("state.launches", "duplicate launch id"));
        }
    }
    Ok(())
}

pub fn validate_hub_card(card: &HubGameCard) -> Result<(), EngineError> {
    validate_single_line("card.project_id", &card.project_id)?;
    validate_single_line("card.package_id", &card.package_id)?;
    validate_single_line("card.publish_id", &card.publish_id)?;
    validate_content_address(&card.content_address)?;
    validate_single_line("card.title", &card.title)?;
    validate_single_line("card.version", &card.version)?;
    validate_single_line("card.summary", &card.summary)?;
    validate_sha256(&card.package_sha256)?;
    if card.package_bytes == 0 || card.install_tool_name != DELIVERY_LITDGAME_TOOL {
        return Err(hub_error(
            "card.package",
            "package bytes and install tool must be valid",
        ));
    }
    if card.installed {
        let Some(installed_sha256) = card.installed_package_sha256.as_deref() else {
            return Err(hub_error("card.installed_sha256", "installed sha missing"));
        };
        validate_sha256(installed_sha256)?;
        if card
            .installed_package_path
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
            || card.installed_package_bytes.unwrap_or(0) == 0
            || installed_sha256 != card.package_sha256
            || !card.launchable
        {
            return Err(hub_error(
                "card.installed",
                "installed card must have verified path, sha, bytes, and launchability",
            ));
        }
    } else if card.launchable
        || card.installed_package_path.is_some()
        || card.installed_package_sha256.is_some()
        || card.installed_package_bytes.is_some()
    {
        return Err(hub_error(
            "card.uninstalled",
            "uninstalled card cannot carry install readback",
        ));
    }
    Ok(())
}

pub fn validate_launch_record(record: &HubLaunchRecord) -> Result<(), EngineError> {
    validate_single_line("launch.launch_id", &record.launch_id)?;
    validate_content_address(&record.content_address)?;
    validate_single_line("launch.package_id", &record.package_id)?;
    validate_single_line(
        "launch.installed_package_path",
        &record.installed_package_path,
    )?;
    validate_single_line("launch.launched_by", &record.launched_by)?;
    if record.runtime_report.package_id != record.package_id
        || record.runtime_report.package_sha256.trim().is_empty()
        || !record.runtime_report.runtime_lock_verified
        || !record.runtime_report.manifest_integrity_verified
        || !record.runtime_report.playable_on_litd_runtime
    {
        return Err(hub_error(
            "launch.runtime_report",
            "runtime launch readback is invalid",
        ));
    }
    Ok(())
}

pub fn validate_content_address(value: &str) -> Result<(), EngineError> {
    let Some(sha256) = value.strip_prefix("litdgame.sha256.") else {
        return Err(hub_error(
            "content_address",
            "content address must be litdgame.sha256.<64 lowercase hex>",
        ));
    };
    validate_sha256(sha256)
}

pub fn validate_sha256(value: &str) -> Result<(), EngineError> {
    if !litd_hash::is_sha256_hex(value) {
        return Err(hub_error("sha256", "SHA-256 must be 64 lowercase hex"));
    }
    Ok(())
}

pub fn hub_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    let field = format!("editor.mcp_project_tools.hub.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::hub",
        field = %field,
        reason = %reason,
        code = "LITD-HUB-LAUNCHER-VALIDATION",
        "hub launcher operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
