use std::{fs, path::Path};

use litd_engine::{
    EngineError,
    game_package::{build_runtime_game_run_report, load_runtime_locked_package},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

#[path = "mcp_project_hub_store.rs"]
mod mcp_project_hub_store;
#[path = "mcp_project_hub_types.rs"]
mod mcp_project_hub_types;
#[path = "mcp_project_hub_validation.rs"]
mod mcp_project_hub_validation;

pub use self::{
    mcp_project_hub_store::read_hub_launcher_state,
    mcp_project_hub_types::{
        HUB_LAUNCHER_STATE_SCHEMA, HUB_LAUNCHER_TOOL, HubGameCard, HubLaunchRecord,
        HubLauncherState, HubLauncherStateReadback,
    },
};

use self::{
    mcp_project_hub_store::{new_hub_launcher_state, write_hub_launcher_state},
    mcp_project_hub_types::{HubLauncherArgs, HubSortMode},
    mcp_project_hub_validation::{hub_error, validate_hub_args, validate_hub_card},
};
use super::{
    DELIVERY_LITDGAME_TOOL, InstalledGameRecord, McpProjectToolCallReadback, READ_PROJECT_TOOL,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, read_publish_catalog,
        write_mcp_project_state, write_mcp_project_tool_ledger,
    },
    parse_args, read_litdgame_install_manifest, validate_mutating_route,
};

pub(super) fn open_hub_launcher(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: HubLauncherArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        ("catalog_path", std::path::Path::new(&args.catalog_path)),
        (
            "install_manifest_path",
            std::path::Path::new(&args.install_manifest_path),
        ),
        ("hub_state_path", std::path::Path::new(&args.hub_state_path)),
    ])?;
    validate_hub_args(&args)?;

    let catalog = read_publish_catalog(&args.catalog_path)?.catalog;
    let install_manifest = read_litdgame_install_manifest(&args.install_manifest_path)?.manifest;
    let mut cards = Vec::new();
    for entry in &catalog.entries {
        if !matches_query(&args, entry) {
            continue;
        }
        let installed = install_manifest
            .records
            .iter()
            .find(|record| record.content_address == entry.content_address);
        if args.installed_only && installed.is_none() {
            continue;
        }
        let card = hub_card_from_record(entry, installed)?;
        validate_hub_card(&card)?;
        cards.push(card);
    }
    sort_cards(&mut cards, args.sort);

    let mut launches = Vec::new();
    if let Some(content_address) = args.launch_content_address.as_deref() {
        let card = cards
            .iter()
            .find(|card| card.content_address == content_address)
            .ok_or_else(|| {
                hub_error(
                    "launch.content_address",
                    "requested launch content address is not in the current hub result set",
                )
            })?;
        let installed_path = card.installed_package_path.as_deref().ok_or_else(|| {
            hub_error(
                "launch.installed_package_path",
                "game must be installed before hub launch",
            )
        })?;
        let readback =
            load_runtime_locked_package(Path::new(installed_path), "mcp.project.hub.launch")?;
        if readback.package_sha256 != card.package_sha256 || readback.package_id != card.package_id
        {
            return Err(hub_error(
                "launch.runtime_readback",
                "runtime package readback does not match hub card",
            ));
        }
        let runtime_report = build_runtime_game_run_report(&readback);
        launches.push(HubLaunchRecord {
            launch_id: format!("litd.launch.{}", &card.package_sha256[..16]),
            content_address: card.content_address.clone(),
            package_id: card.package_id.clone(),
            installed_package_path: installed_path.to_string(),
            launched_by: args.launched_by.clone().unwrap_or_default(),
            runtime_report,
        });
    }

    let mut hub_state = new_hub_launcher_state(
        args.hub_id.clone(),
        args.catalog_path.clone(),
        args.install_manifest_path.clone(),
    );
    hub_state.query = args.query.clone();
    hub_state.required_tags = args.required_tags.clone();
    hub_state.installed_only = args.installed_only;
    hub_state.sort = args.sort;
    hub_state.result_count = cards.len();
    hub_state.cards = cards;
    hub_state.launch_count = launches.len();
    hub_state.launches = launches;
    let hub_readback = write_hub_launcher_state(&args.hub_state_path, &hub_state)?;

    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    state.update_count = state.update_count.saturating_add(1);
    let state_readback = write_mcp_project_state(&args.state_path, &state)?;
    let mut affected_ids: Vec<String> = hub_state
        .cards
        .iter()
        .map(|card| card.content_address.clone())
        .collect();
    if affected_ids.is_empty() {
        affected_ids.push(args.hub_id.clone());
    }
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
            action_summary: "opened hub launcher, searched catalog, and verified launch state"
                .to_string(),
            affected_ids,
            project_count: state_readback.project_count,
            playtest_count: state_readback.playtest_count,
            package_count: state_readback.package_count,
            published_count: state_readback.published_count,
        },
    )?;

    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: "Calyx Aster hub launcher state + publish catalog + install manifest"
            .to_string(),
        result: json!({ "hub_state_readback": hub_readback }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

fn hub_card_from_record(
    entry: &super::PublishedGameRecord,
    installed: Option<&InstalledGameRecord>,
) -> Result<HubGameCard, EngineError> {
    let Some(installed) = installed else {
        return Ok(HubGameCard {
            project_id: entry.project_id.clone(),
            package_id: entry.package_id.clone(),
            publish_id: entry.publish_id.clone(),
            content_address: entry.content_address.clone(),
            title: entry.title.clone(),
            version: entry.version.clone(),
            summary: entry.summary.clone(),
            tags: entry.tags.clone(),
            min_spec_tags: entry.min_spec_tags.clone(),
            package_sha256: entry.package_sha256.clone(),
            package_bytes: entry.package_bytes,
            thumbnail_path: entry.thumbnail_path.clone(),
            installed: false,
            install_tool_name: DELIVERY_LITDGAME_TOOL.to_string(),
            installed_package_path: None,
            installed_package_sha256: None,
            installed_package_bytes: None,
            launchable: false,
        });
    };
    if installed.package_id != entry.package_id
        || installed.project_id != entry.project_id
        || installed.installed_package_sha256 != entry.package_sha256
        || installed.installed_package_bytes != entry.package_bytes
    {
        return Err(hub_error(
            "install_manifest.record",
            "installed record does not match published catalog entry",
        ));
    }
    let bytes = fs::read(&installed.installed_package_path).map_err(|error| {
        hub_error(
            "installed_package.read",
            format!("read {}: {error}", installed.installed_package_path),
        )
    })?;
    let actual_sha = sha256_hex(&bytes);
    if actual_sha != entry.package_sha256 || bytes.len() != entry.package_bytes {
        return Err(hub_error(
            "installed_package.bytes",
            "installed package bytes do not match published catalog entry",
        ));
    }
    let runtime = load_runtime_locked_package(
        Path::new(&installed.installed_package_path),
        "mcp.project.hub.card",
    )?;
    Ok(HubGameCard {
        project_id: entry.project_id.clone(),
        package_id: entry.package_id.clone(),
        publish_id: entry.publish_id.clone(),
        content_address: entry.content_address.clone(),
        title: entry.title.clone(),
        version: entry.version.clone(),
        summary: entry.summary.clone(),
        tags: entry.tags.clone(),
        min_spec_tags: entry.min_spec_tags.clone(),
        package_sha256: entry.package_sha256.clone(),
        package_bytes: entry.package_bytes,
        thumbnail_path: entry.thumbnail_path.clone(),
        installed: true,
        install_tool_name: DELIVERY_LITDGAME_TOOL.to_string(),
        installed_package_path: Some(installed.installed_package_path.clone()),
        installed_package_sha256: Some(actual_sha),
        installed_package_bytes: Some(bytes.len()),
        launchable: runtime.runtime_lock_verified && runtime.playable_on_litd_runtime,
    })
}

fn matches_query(args: &HubLauncherArgs, entry: &super::PublishedGameRecord) -> bool {
    if !args
        .required_tags
        .iter()
        .all(|tag| entry.tags.contains(tag) || entry.min_spec_tags.contains(tag))
    {
        return false;
    }
    let Some(query) = args.query.as_deref() else {
        return true;
    };
    let query = query.to_ascii_lowercase();
    entry.title.to_ascii_lowercase().contains(&query)
        || entry.summary.to_ascii_lowercase().contains(&query)
        || entry.content_address.to_ascii_lowercase().contains(&query)
        || entry
            .tags
            .iter()
            .chain(entry.min_spec_tags.iter())
            .any(|tag| tag.to_ascii_lowercase().contains(&query))
}

fn sort_cards(cards: &mut [HubGameCard], sort: HubSortMode) {
    match sort {
        HubSortMode::Title => cards.sort_by(|left, right| {
            left.title
                .cmp(&right.title)
                .then(left.content_address.cmp(&right.content_address))
        }),
        HubSortMode::Version => cards.sort_by(|left, right| {
            left.version
                .cmp(&right.version)
                .then(left.content_address.cmp(&right.content_address))
        }),
        HubSortMode::ContentAddress => {
            cards.sort_by(|left, right| left.content_address.cmp(&right.content_address));
        }
    }
}
