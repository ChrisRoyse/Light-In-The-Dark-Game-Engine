use std::{fs, path::Path};

use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::{
    mcp_tools::McpDomainTool,
    quick_share::{LITD_LOCK_IN_POLICY, LITD_RUNTIME_ID, load_quick_share_package},
};

#[path = "mcp_project_delivery_store.rs"]
mod mcp_project_delivery_store;
#[path = "mcp_project_delivery_types.rs"]
pub(crate) mod mcp_project_delivery_types;
#[path = "mcp_project_delivery_validation.rs"]
mod mcp_project_delivery_validation;

use self::{
    mcp_project_delivery_store::{
        bundle_delivery_assets, install_litdgame_from_blob, load_or_create_delivery_store_manifest,
        load_or_create_install_manifest, write_delivery_store_manifest,
        write_litdgame_install_manifest, write_or_verify_blob,
    },
    mcp_project_delivery_types::{
        DeliverLitdGameArgs, DeliveredGameRecord, LITDGAME_PACKAGE_MEDIA_TYPE,
    },
    mcp_project_delivery_validation::{
        confine_delivery_paths, content_address_sha256, delivery_error, validate_delivery_args,
        validate_delivery_record, validate_litdgame_package_path, verify_package_bytes,
    },
};

use super::mcp_project_tools_validation::{confine_path, resolve_project_root};

pub use self::{
    mcp_project_delivery_store::{read_delivery_store_manifest, read_litdgame_install_manifest},
    mcp_project_delivery_types::{
        DELIVERY_LITDGAME_TOOL, DELIVERY_STORE_SCHEMA, DeliveryStoreManifest,
        DeliveryStoreManifestReadback, InstalledGameRecord, LITDGAME_INSTALL_MANIFEST_SCHEMA,
        LitdGameInstallManifest, LitdGameInstallManifestReadback,
    },
};

use super::{
    McpProjectToolCallReadback, READ_PROJECT_TOOL,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, read_publish_catalog,
        write_mcp_project_state, write_mcp_project_tool_ledger,
    },
    parse_args, upsert_by, validate_mutating_route,
};

pub(super) fn deliver_litdgame(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: DeliverLitdGameArgs = parse_args(&route.tool_name, arguments)?;
    validate_delivery_args(&args)?;

    // Confine every tool-supplied path to the trusted project root BEFORE any
    // filesystem read/write/remove. A poisoned catalog cannot escape the project
    // directory (#1487). Fail loud if the root is unconfigured — no arbitrary-path
    // fallback.
    let project_root = resolve_project_root()?;
    confine_delivery_paths(&args, &project_root)?;

    let catalog = read_publish_catalog(&args.catalog_path)?.catalog;
    let published = catalog
        .entries
        .iter()
        .find(|entry| entry.content_address == args.content_address)
        .ok_or_else(|| {
            delivery_error(
                "catalog.content_address",
                format!("{} is not in the publish catalog", args.content_address),
            )
        })?;
    let expected_sha256 = content_address_sha256(&args.content_address)?;
    if expected_sha256 != published.package_sha256 {
        return Err(delivery_error(
            "catalog.package_sha256",
            "catalog package_sha256 does not match requested content address",
        ));
    }

    // The package path comes from the (untrusted) catalog record — confine it to the
    // project root before reading, so a poisoned catalog cannot exfiltrate arbitrary
    // local files through the delivery readback (#1487).
    let package_path = Path::new(&published.package_path);
    validate_litdgame_package_path(package_path)?;
    confine_path("package_path", package_path, &project_root)?;
    let package_bytes = fs::read(package_path).map_err(|error| {
        delivery_error(
            "package.read",
            format!("read {}: {error}", package_path.display()),
        )
    })?;
    verify_package_bytes(
        &package_bytes,
        published.package_bytes,
        &published.package_sha256,
    )?;
    let client = load_quick_share_package(
        package_path,
        LITD_RUNTIME_ID,
        "mcp.project.delivery.package_readback",
    )?;
    if client.package_id != published.package_id
        || client.package_sha256 != published.package_sha256
        || !client.world_hash_matches
        || !client.runtime_lock_verified
    {
        return Err(delivery_error(
            "package.readback",
            "package runtime readback does not match published catalog record",
        ));
    }

    let blob = write_or_verify_blob(
        Path::new(&args.blob_root_path),
        &published.package_sha256,
        &package_bytes,
    )?;
    // #1513: bundle the declared asset bytes (content-addressed) into the delivery store so
    // the subsequent install can materialize a self-contained, byte-verifiable package.
    let (bundled_asset_count, bundled_asset_bytes) =
        bundle_delivery_assets(Path::new(&args.blob_root_path), &package_bytes)?;
    let delivered = DeliveredGameRecord {
        project_id: published.project_id.clone(),
        package_id: published.package_id.clone(),
        publish_id: published.publish_id.clone(),
        content_address: published.content_address.clone(),
        media_type: LITDGAME_PACKAGE_MEDIA_TYPE.to_string(),
        digest: format!("sha256:{}", published.package_sha256),
        package_sha256: published.package_sha256.clone(),
        package_bytes: published.package_bytes,
        source_package_path: published.package_path.clone(),
        stored_blob_path: blob.path.clone(),
        store_relative_path: blob.relative_path.clone(),
        stored_blob_sha256: blob.sha256.clone(),
        stored_blob_bytes: blob.bytes,
        runtime_id: LITD_RUNTIME_ID.to_string(),
        lock_in_policy: LITD_LOCK_IN_POLICY.to_string(),
        delivered_by: args.delivered_by.clone(),
        bundled_asset_count,
        bundled_asset_bytes,
    };
    validate_delivery_record(&delivered)?;
    let installed = install_litdgame_from_blob(&args, &delivered)?;

    let mut delivery_manifest = load_or_create_delivery_store_manifest(
        &args.delivery_manifest_path,
        &args.delivery_store_id,
        &args.blob_root_path,
    )?;
    upsert_by(
        &mut delivery_manifest.entries,
        |entry| entry.content_address == delivered.content_address,
        delivered.clone(),
    );
    delivery_manifest.entry_count = delivery_manifest.entries.len();
    let delivery_readback =
        write_delivery_store_manifest(&args.delivery_manifest_path, &delivery_manifest)?;

    let mut install_manifest = load_or_create_install_manifest(
        &args.install_manifest_path,
        &args.install_id,
        &args.install_root_path,
    )?;
    upsert_by(
        &mut install_manifest.records,
        |record| record.content_address == installed.content_address,
        installed.clone(),
    );
    install_manifest.record_count = install_manifest.records.len();
    let install_readback =
        write_litdgame_install_manifest(&args.install_manifest_path, &install_manifest)?;

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
            action_summary: "delivered and installed .litdgame from verified local content store"
                .to_string(),
            affected_ids: vec![
                delivered.project_id.clone(),
                delivered.package_id.clone(),
                delivered.content_address.clone(),
            ],
            project_count: state_readback.project_count,
            playtest_count: state_readback.playtest_count,
            package_count: state_readback.package_count,
            published_count: state_readback.published_count,
        },
    )?;

    tracing::info!(
        content_address = %delivered.content_address,
        package_sha256 = %delivered.package_sha256,
        package_bytes = delivered.package_bytes,
        stored_blob_path = %delivered.stored_blob_path,
        installed_package_path = %installed.installed_package_path,
        deduplicated = blob.deduplicated,
        code = "LITD-DELIVERY-LITDGAME-VERIFIED",
        "verified .litdgame delivery and install source of truth"
    );

    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth:
            "Delivery store manifest JSON + install manifest JSON + package blob readback"
                .to_string(),
        result: json!({
            "delivered_game": delivered,
            "installed_game": installed,
            "delivery_manifest_readback": delivery_readback,
            "install_manifest_readback": install_readback
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}
