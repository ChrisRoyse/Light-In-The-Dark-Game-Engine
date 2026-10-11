use std::{fs, path::Path};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    mcp_tools::McpDomainTool,
    quick_share::{
        LITD_LOCK_IN_POLICY, LITD_RUNTIME_ID, QuickShareBuildRequest, QuickShareEndpoint,
        build_quick_share_package, load_quick_share_package, write_quick_share_package,
    },
};

use super::{
    LitdGamePackageRecord, LitdWorldProject, McpProjectToolCallReadback, PublishLitdGameArgs,
    PublishedGameRecord, READ_PROJECT_TOOL, invalid_mcp_project,
    mcp_project_tools_store::{
        load_or_create_mcp_project_state, load_or_create_publish_catalog, write_publish_catalog,
    },
    mcp_project_tools_validation::{
        reverify_confined_before_write, validate_package_record, validate_path,
        validate_publish_metadata, validate_published_record, validate_request_identity,
    },
    parse_args, upsert_by, validate_mutating_route,
};

const MAX_PUBLISH_THUMBNAIL_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn publish_litdgame(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: PublishLitdGameArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        ("project_path", std::path::Path::new(&args.project_path)),
        ("package_path", std::path::Path::new(&args.package_path)),
        ("catalog_path", std::path::Path::new(&args.catalog_path)),
        ("thumbnail_path", std::path::Path::new(&args.thumbnail_path)),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_publish_metadata(
        &args.title,
        &args.version,
        &args.summary,
        &args.tags,
        &args.min_spec_tags,
    )?;
    let project = super::read_litdworld_project(&args.project_path)?;
    let thumbnail = read_publish_thumbnail(&args.thumbnail_path)?;
    let package_record = package_project(
        &project,
        &args.package_path,
        &args.share_id,
        &args.created_by,
        args.host_port,
        args.max_package_bytes,
    )?;
    let publish_record = PublishedGameRecord {
        project_id: project.project_id.clone(),
        package_id: package_record.package_id.clone(),
        publish_id: publish_id_from_sha256(&package_record.package_sha256),
        content_address: content_address_for_package(&package_record.package_sha256),
        title: args.title.clone(),
        version: args.version.clone(),
        summary: args.summary.clone(),
        tags: args.tags.clone(),
        min_spec_tags: args.min_spec_tags.clone(),
        package_path: package_record.package_path.clone(),
        package_sha256: package_record.package_sha256.clone(),
        package_bytes: package_record.package_bytes,
        thumbnail_path: thumbnail.path,
        thumbnail_sha256: thumbnail.sha256,
        thumbnail_bytes: thumbnail.bytes,
        thumbnail_media_type: thumbnail.media_type,
        runtime_id: LITD_RUNTIME_ID.to_string(),
        lock_in_policy: LITD_LOCK_IN_POLICY.to_string(),
        asset_count: package_record.asset_count,
        published_by: args.created_by.clone(),
    };
    validate_published_record(&publish_record)?;

    // Cross-store write ordering (#1490): these are four independent Calyx Aster
    // vaults with no shared transaction, so a mid-publish crash can leave them out
    // of step. Commit the outward-facing ecosystem CATALOG *last*, after the local
    // authoritative project state and its audit ledger. That way a failure can only
    // ever leave a game published locally-but-not-yet-discoverable (a re-publish
    // idempotently completes it via the `upsert_by` keyed on stable publish_id) —
    // never the reverse, where the catalog advertises a game absent from state with
    // nothing to reconcile it. (Best practice: commit the derived/visible index last
    // when a true distributed transaction is unavailable.)
    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    upsert_by(
        &mut state.packages,
        |existing| existing.package_id == package_record.package_id,
        package_record.clone(),
    );
    upsert_by(
        &mut state.published_games,
        |existing| existing.publish_id == publish_record.publish_id,
        publish_record.clone(),
    );
    state.update_count = state.update_count.saturating_add(1);
    // 1 + 2: local authoritative state, then its audit ledger.
    let (state_readback, ledger_readback) = super::write_state_and_ledger(
        &route,
        &args.state_path,
        &args.ledger_path,
        &args.ledger_id,
        &args.request_id,
        &args.requested_by,
        "packaged and published runtime-locked .litdgame to local ecosystem catalog",
        vec![
            publish_record.project_id.clone(),
            publish_record.package_id.clone(),
            publish_record.publish_id.clone(),
        ],
        state,
    )?;
    // 3: outward-facing discoverable catalog, committed LAST.
    let mut catalog = load_or_create_publish_catalog(&args.catalog_path, "local-litd-ecosystem")?;
    upsert_by(
        &mut catalog.entries,
        |existing| existing.publish_id == publish_record.publish_id,
        publish_record.clone(),
    );
    catalog.entry_count = catalog.entries.len();
    let catalog_readback = write_publish_catalog(&args.catalog_path, &catalog)?;
    Ok(super::call_readback(
        &route,
        "Calyx Aster MCP project state + tool ledger + ecosystem publish catalog",
        json!({
            "package": package_record,
            "published_game": publish_record,
            "publish_catalog_readback": catalog_readback
        }),
        state_readback,
        Some(ledger_readback),
    ))
}

pub(super) fn package_project(
    project: &LitdWorldProject,
    package_path: &str,
    share_id: &str,
    created_by: &str,
    host_port: u16,
    max_package_bytes: usize,
) -> Result<LitdGamePackageRecord, EngineError> {
    let request = QuickShareBuildRequest::new(
        share_id.to_string(),
        created_by.to_string(),
        QuickShareEndpoint::loopback(share_id, host_port),
        max_package_bytes,
    );
    let package = build_quick_share_package(&project.document, &request)?;
    // #1518: last confinement check before the quick-share package write closes the
    // symlink/junction TOCTOU window between the handler's confine_tool_paths and here.
    reverify_confined_before_write("package_path", Path::new(package_path))?;
    let writeback =
        write_quick_share_package(Path::new(package_path), &package, max_package_bytes)?;
    let client = load_quick_share_package(
        Path::new(package_path),
        LITD_RUNTIME_ID,
        "mcp.project.package.readback",
    )?;
    let record = LitdGamePackageRecord {
        project_id: project.project_id.clone(),
        package_id: package.package_id.clone(),
        package_path: writeback.package_path.clone(),
        package_sha256: writeback.sha256.clone(),
        package_bytes: writeback.bytes,
        runtime_id: LITD_RUNTIME_ID.to_string(),
        lock_in_policy: LITD_LOCK_IN_POLICY.to_string(),
        host_uri: writeback.host_uri.clone(),
        document_fingerprint: package.document_fingerprint.clone(),
        play_build_fingerprint: package.play_build_fingerprint.clone(),
        world_hash_matches: client.world_hash_matches,
        runtime_lock_verified: client.runtime_lock_verified,
        asset_count: package.asset_count,
    };
    validate_package_record(&record)?;
    Ok(record)
}

struct PublishThumbnailReadback {
    path: String,
    sha256: String,
    bytes: usize,
    media_type: String,
}

fn read_publish_thumbnail(path: &str) -> Result<PublishThumbnailReadback, EngineError> {
    let path_ref = Path::new(path);
    validate_path("publish.thumbnail_path", path_ref)?;
    let bytes = fs::read(path_ref).map_err(|error| {
        invalid_mcp_project(
            "publish.thumbnail.read",
            format!("read {}: {error}", path_ref.display()),
        )
    })?;
    if bytes.is_empty() || bytes.len() > MAX_PUBLISH_THUMBNAIL_BYTES {
        return Err(invalid_mcp_project(
            "publish.thumbnail.size",
            format!(
                "thumbnail bytes must be in 1..={MAX_PUBLISH_THUMBNAIL_BYTES}, got {}",
                bytes.len()
            ),
        ));
    }
    let media_type = detect_thumbnail_media_type(path_ref, &bytes)?;
    Ok(PublishThumbnailReadback {
        path: path_ref.display().to_string(),
        sha256: sha256_hex(&bytes),
        bytes: bytes.len(),
        media_type,
    })
}

fn detect_thumbnail_media_type(path: &Path, bytes: &[u8]) -> Result<String, EngineError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let media_type = match extension.as_str() {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
        "jpg" | "jpeg" if bytes.starts_with(&[0xff, 0xd8, 0xff]) => "image/jpeg",
        "webp" if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => {
            "image/webp"
        }
        _ => {
            return Err(invalid_mcp_project(
                "publish.thumbnail.format",
                "thumbnail must be png, jpg/jpeg, or webp with matching magic bytes",
            ));
        }
    };
    Ok(media_type.to_string())
}

fn publish_id_from_sha256(package_sha256: &str) -> String {
    format!("litd.publish.{}", &package_sha256[..16])
}

fn content_address_for_package(package_sha256: &str) -> String {
    format!("litdgame.sha256.{package_sha256}")
}
