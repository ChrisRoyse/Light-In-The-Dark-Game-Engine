use std::{fs, path::Path};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

#[path = "mcp_project_updates_store.rs"]
mod mcp_project_updates_store;
#[path = "mcp_project_updates_types.rs"]
mod mcp_project_updates_types;
#[path = "mcp_project_updates_validation.rs"]
mod mcp_project_updates_validation;

pub use self::{
    mcp_project_updates_store::read_game_updates_state,
    mcp_project_updates_types::{
        CompatibilityRecord, GAME_UPDATES_STATE_SCHEMA, GAME_UPDATES_TOOL, GameUpdateAction,
        GameUpdateReleaseRecord, GameUpdatesState, GameUpdatesStateReadback, UpdateChannelRecord,
        UpdateNotificationRecord, UpdateNotificationStatus,
    },
};

use self::{
    mcp_project_updates_store::{load_or_create_game_updates_state, write_game_updates_state},
    mcp_project_updates_types::GameUpdatesArgs,
    mcp_project_updates_validation::{update_error, validate_update_args},
};
use super::{
    McpProjectToolCallReadback, READ_PROJECT_TOOL,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, write_mcp_project_state,
        write_mcp_project_tool_ledger,
    },
    parse_args, read_litdgame_install_manifest, read_publish_catalog, read_social_graph_state,
    validate_mutating_route,
};

pub(super) fn manage_game_updates(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: GameUpdatesArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        (
            "updates_state_path",
            std::path::Path::new(&args.updates_state_path),
        ),
        ("catalog_path", std::path::Path::new(&args.catalog_path)),
        (
            "install_manifest_path",
            std::path::Path::new(&args.install_manifest_path),
        ),
        (
            "social_state_path",
            std::path::Path::new(&args.social_state_path),
        ),
    ])?;
    validate_update_args(&args)?;
    let mut updates_state = load_or_create_game_updates_state(
        &args.updates_state_path,
        &args.updates_state_id,
        &args.catalog_path,
        &args.install_manifest_path,
        &args.social_state_path,
    )?;
    let result = match args.action {
        GameUpdateAction::RegisterRelease => register_release(&args, &mut updates_state)?,
        GameUpdateAction::EvaluateCompatibility => {
            evaluate_compatibility(&args, &mut updates_state)?
        }
        GameUpdateAction::AcknowledgeNotification => {
            acknowledge_notification(&args, &mut updates_state)?
        }
    };
    refresh_counts(&mut updates_state);
    let updates_readback = write_game_updates_state(&args.updates_state_path, &updates_state)?;

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
            action_summary:
                "managed game update releases, compatibility checks, or update notifications"
                    .to_string(),
            affected_ids: affected_ids(&updates_readback.state),
            project_count: state_readback.project_count,
            playtest_count: state_readback.playtest_count,
            package_count: state_readback.package_count,
            published_count: state_readback.published_count,
        },
    )?;

    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth:
            "Game updates JSON + publish catalog + install manifest + social graph JSON".to_string(),
        result: json!({
            "updates_state_readback": updates_readback,
            "operation": result
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

fn register_release(
    args: &GameUpdatesArgs,
    state: &mut GameUpdatesState,
) -> Result<Value, EngineError> {
    let release_id = required(args.release_id.as_deref(), "release_id")?;
    let channel_id = required(args.channel_id.as_deref(), "channel_id")?;
    let from_content = required(args.from_content_address.as_deref(), "from_content_address")?;
    let to_content = required(args.to_content_address.as_deref(), "to_content_address")?;
    if state
        .releases
        .iter()
        .any(|release| release.release_id == release_id)
    {
        return Err(update_error("release_id", "release already exists"));
    }
    let (source, target) = verify_release_sources(args, from_content, to_content)?;
    let release = GameUpdateReleaseRecord {
        release_id: release_id.to_string(),
        channel_id: channel_id.to_string(),
        project_id: target.project_id.clone(),
        package_id: target.package_id.clone(),
        version: target.version.clone(),
        from_content_address: from_content.to_string(),
        to_content_address: to_content.to_string(),
        package_sha256: target.package_sha256.clone(),
        package_bytes: target.package_bytes,
        runtime_id: target.runtime_id.clone(),
        min_client_protocol: args.min_client_protocol.unwrap_or_default(),
        min_server_protocol: args.min_server_protocol.unwrap_or_default(),
        min_save_schema: args.min_save_schema.unwrap_or_default(),
        max_save_schema: args.max_save_schema.unwrap_or_default(),
        release_notes: required(args.release_notes.as_deref(), "release_notes")?.to_string(),
        created_by: args.requested_by.clone(),
    };
    if source.project_id != release.project_id {
        return Err(update_error(
            "release.project_id",
            "update source and target must belong to the same project",
        ));
    }
    state.releases.push(release.clone());
    upsert_channel(state, &release);
    create_notifications(args, state, &release)?;
    Ok(json!({ "release": release }))
}

fn evaluate_compatibility(
    args: &GameUpdatesArgs,
    state: &mut GameUpdatesState,
) -> Result<Value, EngineError> {
    let release_id = required(args.release_id.as_deref(), "release_id")?;
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    ensure_social_account(args, account_id)?;
    let release = state
        .releases
        .iter()
        .find(|release| release.release_id == release_id)
        .ok_or_else(|| update_error("release_id", "release does not exist"))?;
    let client = args.client_protocol.unwrap_or_default();
    let server = args.server_protocol.unwrap_or_default();
    let save = args.save_schema_version.unwrap_or_default();
    let mut reasons = Vec::new();
    if client < release.min_client_protocol {
        reasons.push(format!(
            "client_protocol {client} below {}",
            release.min_client_protocol
        ));
    }
    if server < release.min_server_protocol {
        reasons.push(format!(
            "server_protocol {server} below {}",
            release.min_server_protocol
        ));
    }
    if save < release.min_save_schema || save > release.max_save_schema {
        reasons.push(format!(
            "save_schema_version {save} outside {}..={}",
            release.min_save_schema, release.max_save_schema
        ));
    }
    if reasons.is_empty() {
        reasons.push("compatible".to_string());
    }
    let record = CompatibilityRecord {
        compatibility_id: format!("compat.{release_id}.{account_id}"),
        release_id: release_id.to_string(),
        account_id: account_id.to_string(),
        client_protocol: client,
        server_protocol: server,
        save_schema_version: save,
        compatible: reasons.len() == 1 && reasons[0] == "compatible",
        reasons,
    };
    if let Some(existing) = state.compatibilities.iter_mut().find(|existing| {
        existing.release_id == record.release_id && existing.account_id == record.account_id
    }) {
        *existing = record.clone();
    } else {
        state.compatibilities.push(record.clone());
    }
    Ok(json!({ "compatibility": record }))
}

fn acknowledge_notification(
    args: &GameUpdatesArgs,
    state: &mut GameUpdatesState,
) -> Result<Value, EngineError> {
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    let notification_id = required(args.notification_id.as_deref(), "notification_id")?;
    ensure_social_account(args, account_id)?;
    let notification = state
        .notifications
        .iter_mut()
        .find(|notification| notification.notification_id == notification_id)
        .ok_or_else(|| update_error("notification_id", "notification does not exist"))?;
    if notification.account_id != account_id {
        return Err(update_error(
            "notification.account_id",
            "notification belongs to another account",
        ));
    }
    if notification.status != UpdateNotificationStatus::Pending {
        return Err(update_error(
            "notification.status",
            "notification is already acknowledged",
        ));
    }
    let compatible = state.compatibilities.iter().any(|compatibility| {
        compatibility.release_id == notification.release_id
            && compatibility.account_id == account_id
            && compatibility.compatible
    });
    if !compatible {
        return Err(update_error(
            "notification.compatibility",
            "compatible evaluation is required before acknowledgement",
        ));
    }
    notification.status = UpdateNotificationStatus::Acknowledged;
    Ok(json!({ "notification": notification }))
}

fn verify_release_sources(
    args: &GameUpdatesArgs,
    from_content: &str,
    to_content: &str,
) -> Result<(super::PublishedGameRecord, super::PublishedGameRecord), EngineError> {
    if from_content == to_content {
        return Err(update_error(
            "content_address",
            "update source and target content addresses must differ",
        ));
    }
    let catalog = read_publish_catalog(&args.catalog_path)?.catalog;
    let source = catalog
        .entries
        .iter()
        .find(|entry| entry.content_address == from_content)
        .cloned()
        .ok_or_else(|| update_error("catalog.from_content_address", "source is not published"))?;
    let target = catalog
        .entries
        .iter()
        .find(|entry| entry.content_address == to_content)
        .cloned()
        .ok_or_else(|| update_error("catalog.to_content_address", "target is not published"))?;
    let install = read_litdgame_install_manifest(&args.install_manifest_path)?.manifest;
    let installed = install
        .records
        .iter()
        .find(|record| record.content_address == from_content)
        .ok_or_else(|| update_error("install.from_content_address", "source is not installed"))?;
    if installed.installed_package_sha256 != source.package_sha256
        || !installed.runtime_lock_verified
        || !installed.world_hash_matches
    {
        return Err(update_error(
            "install.source_readback",
            "installed source package does not match publish catalog",
        ));
    }
    let package_path = Path::new(&target.package_path);
    let package_bytes = fs::read(package_path).map_err(|error| {
        update_error(
            "target_package.read",
            format!("read {}: {error}", package_path.display()),
        )
    })?;
    if sha256_hex(&package_bytes) != target.package_sha256
        || package_bytes.len() != target.package_bytes
    {
        return Err(update_error(
            "target_package.readback",
            "target package bytes do not match publish catalog",
        ));
    }
    if source.runtime_id != target.runtime_id || source.lock_in_policy != target.lock_in_policy {
        return Err(update_error(
            "release.runtime",
            "source and target runtime contracts must match",
        ));
    }
    Ok((source, target))
}

fn create_notifications(
    args: &GameUpdatesArgs,
    state: &mut GameUpdatesState,
    release: &GameUpdateReleaseRecord,
) -> Result<(), EngineError> {
    let social = read_social_graph_state(&args.social_state_path)?.state;
    if social.accounts.is_empty() {
        return Err(update_error(
            "social.accounts",
            "at least one account is required for update notifications",
        ));
    }
    for account in social.accounts {
        let notification_id = format!("update.{}.{}", release.release_id, account.account_id);
        if state
            .notifications
            .iter()
            .any(|notification| notification.notification_id == notification_id)
        {
            return Err(update_error(
                "notification.duplicate",
                "release notification already exists",
            ));
        }
        state.notifications.push(UpdateNotificationRecord {
            notification_id,
            release_id: release.release_id.clone(),
            account_id: account.account_id,
            from_content_address: release.from_content_address.clone(),
            to_content_address: release.to_content_address.clone(),
            status: UpdateNotificationStatus::Pending,
            message: format!("{} is available on {}", release.version, release.channel_id),
        });
    }
    Ok(())
}

fn upsert_channel(state: &mut GameUpdatesState, release: &GameUpdateReleaseRecord) {
    let release_count = state
        .releases
        .iter()
        .filter(|existing| existing.channel_id == release.channel_id)
        .count();
    let channel = UpdateChannelRecord {
        channel_id: release.channel_id.clone(),
        project_id: release.project_id.clone(),
        latest_release_id: release.release_id.clone(),
        latest_content_address: release.to_content_address.clone(),
        release_count,
    };
    if let Some(existing) = state
        .channels
        .iter_mut()
        .find(|existing| existing.channel_id == channel.channel_id)
    {
        *existing = channel;
    } else {
        state.channels.push(channel);
    }
}

fn ensure_social_account(
    args: &GameUpdatesArgs,
    account_id: &str,
) -> Result<super::AccountRecord, EngineError> {
    read_social_graph_state(&args.social_state_path)?
        .state
        .accounts
        .into_iter()
        .find(|account| account.account_id == account_id)
        .ok_or_else(|| update_error("account_id", "account is not in social graph state"))
}

fn refresh_counts(state: &mut GameUpdatesState) {
    state.channel_count = state.channels.len();
    state.release_count = state.releases.len();
    state.compatibility_count = state.compatibilities.len();
    state.notification_count = state.notifications.len();
}

fn affected_ids(state: &GameUpdatesState) -> Vec<String> {
    let mut ids = state
        .releases
        .iter()
        .map(|release| release.release_id.clone())
        .chain(
            state
                .notifications
                .iter()
                .map(|notification| notification.notification_id.clone()),
        )
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.push(state.state_id.clone());
    }
    ids
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    value.ok_or_else(|| update_error(field, "required field missing"))
}
