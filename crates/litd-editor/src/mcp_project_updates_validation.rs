use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::super::mcp_project_tools_validation::{
    validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_updates_types::{
    CompatibilityRecord, GameUpdateAction, GameUpdateReleaseRecord, GameUpdatesArgs,
    GameUpdatesState, UpdateChannelRecord, UpdateNotificationRecord,
};

pub fn validate_update_args(args: &GameUpdatesArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path("updates_state_path", Path::new(&args.updates_state_path))?;
    validate_path("catalog_path", Path::new(&args.catalog_path))?;
    validate_path(
        "install_manifest_path",
        Path::new(&args.install_manifest_path),
    )?;
    validate_path("social_state_path", Path::new(&args.social_state_path))?;
    validate_id("updates_state_id", &args.updates_state_id)?;
    match args.action {
        GameUpdateAction::RegisterRelease => {
            require_id("release_id", args.release_id.as_deref())?;
            require_id("channel_id", args.channel_id.as_deref())?;
            require_line("from_content_address", args.from_content_address.as_deref())?;
            require_line("to_content_address", args.to_content_address.as_deref())?;
            require_protocol("min_client_protocol", args.min_client_protocol)?;
            require_protocol("min_server_protocol", args.min_server_protocol)?;
            require_save_range(args.min_save_schema, args.max_save_schema)?;
            require_line("release_notes", args.release_notes.as_deref())?;
        }
        GameUpdateAction::EvaluateCompatibility => {
            require_id("release_id", args.release_id.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
            require_protocol("client_protocol", args.client_protocol)?;
            require_protocol("server_protocol", args.server_protocol)?;
            require_protocol("save_schema_version", args.save_schema_version)?;
        }
        GameUpdateAction::AcknowledgeNotification => {
            require_id("account_id", args.account_id.as_deref())?;
            require_id("notification_id", args.notification_id.as_deref())?;
        }
    }
    Ok(())
}

pub fn validate_update_state(state: &GameUpdatesState) -> Result<(), EngineError> {
    if state.schema != super::mcp_project_updates_types::GAME_UPDATES_STATE_SCHEMA
        || state.issue != 1162
        || state.source_of_truth != "Calyx Aster game updates state"
        || state.state_id.trim().is_empty()
        || state.catalog_path.trim().is_empty()
        || state.install_manifest_path.trim().is_empty()
        || state.social_state_path.trim().is_empty()
        || state.channel_count != state.channels.len()
        || state.release_count != state.releases.len()
        || state.compatibility_count != state.compatibilities.len()
        || state.notification_count != state.notifications.len()
    {
        return Err(update_error(
            "state",
            "schema, identity, source, paths, and counts are invalid",
        ));
    }
    let mut channels = BTreeSet::new();
    for channel in &state.channels {
        validate_channel(channel)?;
        if !channels.insert(channel.channel_id.as_str()) {
            return Err(update_error("channels", "duplicate channel_id"));
        }
    }
    let mut releases = BTreeSet::new();
    for release in &state.releases {
        validate_release(release)?;
        if !channels.contains(release.channel_id.as_str()) {
            return Err(update_error(
                "release.channel_id",
                "release must reference an existing channel",
            ));
        }
        if !releases.insert(release.release_id.as_str()) {
            return Err(update_error("releases", "duplicate release_id"));
        }
    }
    let mut compatibilities = BTreeSet::new();
    for compatibility in &state.compatibilities {
        validate_compatibility(compatibility)?;
        if !releases.contains(compatibility.release_id.as_str()) {
            return Err(update_error(
                "compatibility.release_id",
                "compatibility must reference an existing release",
            ));
        }
        let key = format!("{}\n{}", compatibility.release_id, compatibility.account_id);
        if !compatibilities.insert(key) {
            return Err(update_error(
                "compatibilities",
                "one compatibility record per account per release is allowed",
            ));
        }
    }
    let mut notifications = BTreeSet::new();
    for notification in &state.notifications {
        validate_notification(notification)?;
        if !releases.contains(notification.release_id.as_str()) {
            return Err(update_error(
                "notification.release_id",
                "notification must reference an existing release",
            ));
        }
        if !notifications.insert(notification.notification_id.as_str()) {
            return Err(update_error("notifications", "duplicate notification_id"));
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
        return Err(update_error(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

pub fn update_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    let field = format!("editor.mcp_project_tools.game_updates.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::game_updates",
        field = %field,
        reason = %reason,
        code = "LITD-GAME-UPDATES-VALIDATION",
        "game update operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn validate_channel(record: &UpdateChannelRecord) -> Result<(), EngineError> {
    validate_id("channel.channel_id", &record.channel_id)?;
    validate_id("channel.project_id", &record.project_id)?;
    validate_id("channel.latest_release_id", &record.latest_release_id)?;
    validate_single_line(
        "channel.latest_content_address",
        &record.latest_content_address,
    )?;
    if record.release_count == 0 {
        return Err(update_error(
            "channel.release_count",
            "release_count must be non-zero",
        ));
    }
    Ok(())
}

fn validate_release(record: &GameUpdateReleaseRecord) -> Result<(), EngineError> {
    validate_id("release.release_id", &record.release_id)?;
    validate_id("release.channel_id", &record.channel_id)?;
    validate_id("release.project_id", &record.project_id)?;
    validate_id("release.package_id", &record.package_id)?;
    validate_single_line("release.version", &record.version)?;
    validate_single_line("release.from_content_address", &record.from_content_address)?;
    validate_single_line("release.to_content_address", &record.to_content_address)?;
    validate_sha256("release.package_sha256", &record.package_sha256)?;
    validate_single_line("release.runtime_id", &record.runtime_id)?;
    validate_single_line("release.release_notes", &record.release_notes)?;
    validate_id("release.created_by", &record.created_by)?;
    if record.from_content_address == record.to_content_address {
        return Err(update_error(
            "release.content_address",
            "from and to content addresses must differ",
        ));
    }
    if record.package_bytes == 0 || record.min_save_schema > record.max_save_schema {
        return Err(update_error(
            "release.compatibility",
            "package bytes and save schema range are invalid",
        ));
    }
    Ok(())
}

fn validate_compatibility(record: &CompatibilityRecord) -> Result<(), EngineError> {
    validate_id("compatibility.compatibility_id", &record.compatibility_id)?;
    validate_id("compatibility.release_id", &record.release_id)?;
    validate_id("compatibility.account_id", &record.account_id)?;
    if record.reasons.is_empty() {
        return Err(update_error(
            "compatibility.reasons",
            "compatibility record needs at least one reason",
        ));
    }
    for reason in &record.reasons {
        validate_single_line("compatibility.reason", reason)?;
    }
    Ok(())
}

fn validate_notification(record: &UpdateNotificationRecord) -> Result<(), EngineError> {
    validate_id("notification.notification_id", &record.notification_id)?;
    validate_id("notification.release_id", &record.release_id)?;
    validate_id("notification.account_id", &record.account_id)?;
    validate_single_line(
        "notification.from_content_address",
        &record.from_content_address,
    )?;
    validate_single_line(
        "notification.to_content_address",
        &record.to_content_address,
    )?;
    validate_single_line("notification.message", &record.message)
}

fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !litd_hash::is_sha256_hex(value) {
        return Err(update_error(
            field,
            "value must be a 64-character lowercase SHA-256 hex",
        ));
    }
    Ok(())
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_id(
        field,
        value.ok_or_else(|| update_error(field, "required field missing"))?,
    )
}

fn require_line(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_single_line(
        field,
        value.ok_or_else(|| update_error(field, "required field missing"))?,
    )
}

fn require_protocol(field: &str, value: Option<u32>) -> Result<(), EngineError> {
    if value.unwrap_or(0) == 0 {
        return Err(update_error(field, "value must be non-zero"));
    }
    Ok(())
}

fn require_save_range(min: Option<u32>, max: Option<u32>) -> Result<(), EngineError> {
    let min = min.ok_or_else(|| update_error("min_save_schema", "min_save_schema is required"))?;
    let max = max.ok_or_else(|| update_error("max_save_schema", "max_save_schema is required"))?;
    if min == 0 || max == 0 || min > max {
        return Err(update_error(
            "save_schema_range",
            "save schema range must be non-zero and ordered",
        ));
    }
    Ok(())
}
