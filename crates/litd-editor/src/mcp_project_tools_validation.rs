use std::path::Path;

use litd_engine::EngineError;

use super::{
    HeadlessPlaytestRecord, LitdGamePackageRecord, LitdWorldProject, MCP_LITDWORLD_SOURCE_OF_TRUTH,
    ProjectLifecycleRecord, PublishedGameRecord, invalid_mcp_project,
    mcp_project_tools_store::McpProjectState,
};
use crate::quick_share::{LITD_LOCK_IN_POLICY, LITD_RUNTIME_ID};

// #1466: path confinement lives in its own module to keep this file under the <500-line
// doctrine (#800 §6). Re-exported here so `mcp_project_tools_validation::{confine_tool_paths,
// confine_path, resolve_project_root, reverify_confined_before_write, PROJECT_ROOT_ENV}` and the
// test helper resolve exactly as before — no external call site changes.
#[path = "mcp_project_tools_path_confinement.rs"]
mod mcp_project_tools_path_confinement;
#[cfg(test)]
pub(crate) use mcp_project_tools_path_confinement::ensure_project_root_env_for_tests;
pub use mcp_project_tools_path_confinement::{
    PROJECT_ROOT_ENV, confine_path, confine_tool_paths, resolve_project_root,
    reverify_confined_before_write,
};

pub fn validate_request_identity(request_id: &str, requested_by: &str) -> Result<(), EngineError> {
    validate_id("request_id", request_id)?;
    validate_id("requested_by", requested_by)
}

pub fn validate_project_file(project: &LitdWorldProject) -> Result<(), EngineError> {
    if project.schema != super::MCP_LITDWORLD_SCHEMA
        || project.issue != 926
        || project.source_of_truth != MCP_LITDWORLD_SOURCE_OF_TRUTH
        || project.document_fingerprint.trim().is_empty()
    {
        return Err(invalid_mcp_project(
            "project",
            "schema, issue, source, and fingerprint are required",
        ));
    }
    validate_project_metadata(
        &project.project_id,
        &project.title,
        &project.version,
        &project.tags,
        &project.snapshot_label,
    )?;
    project.document.validate()?;
    Ok(())
}

pub fn validate_project_metadata(
    project_id: &str,
    title: &str,
    version: &str,
    tags: &[String],
    snapshot_label: &str,
) -> Result<(), EngineError> {
    validate_id("project_id", project_id)?;
    validate_single_line("title", title)?;
    validate_single_line("version", version)?;
    validate_single_line("snapshot_label", snapshot_label)?;
    validate_id_list("tags", tags)?;
    Ok(())
}

pub fn validate_project_record(record: &ProjectLifecycleRecord) -> Result<(), EngineError> {
    validate_project_metadata(
        &record.project_id,
        &record.title,
        &record.version,
        &record.tags,
        &record.snapshot_label,
    )?;
    validate_single_line("project_path", &record.project_path)?;
    validate_single_line("document_fingerprint", &record.document_fingerprint)?;
    validate_single_line("revision_id", &record.revision_id)?;
    if record.object_count == 0 || record.unit_count == 0 {
        return Err(invalid_mcp_project(
            "project.record",
            "project must contain at least one object and one Unit for playtest",
        ));
    }
    Ok(())
}

pub fn validate_playtest_ticks(ticks: u64) -> Result<(), EngineError> {
    if !(1..=36_000).contains(&ticks) {
        return Err(invalid_mcp_project(
            "playtest.ticks",
            "ticks must be in 1..=36000",
        ));
    }
    Ok(())
}

pub fn validate_playtest_record(record: &HeadlessPlaytestRecord) -> Result<(), EngineError> {
    validate_id("playtest.project_id", &record.project_id)?;
    validate_single_line("playtest.project_path", &record.project_path)?;
    validate_id("playtest.player_id", &record.player_id)?;
    validate_single_line(
        "playtest.oracle_run_id",
        &record.oracle_verdict.oracle_run_id,
    )?;
    validate_single_line("playtest.run_id", &record.oracle_verdict.run_id)?;
    validate_single_line("playtest.verdict_reason", &record.verdict_reason)?;
    if record.steps == 0 || record.final_tick <= record.start_tick {
        return Err(invalid_mcp_project(
            "playtest.progress",
            "playtest must advance at least one tick",
        ));
    }
    if record.oracle_verdict.check_count != record.oracle_verdict.checks.len()
        || record.oracle_verdict.check_count == 0
        || record.oracle_verdict.passed_count + record.oracle_verdict.failed_count
            != record.oracle_verdict.check_count
    {
        return Err(invalid_mcp_project(
            "playtest.oracle_counts",
            "oracle verdict counts must match persisted checks",
        ));
    }
    let actual_failures = record
        .oracle_verdict
        .checks
        .iter()
        .filter(|check| !check.passed)
        .count();
    if actual_failures != record.oracle_verdict.failed_count {
        return Err(invalid_mcp_project(
            "playtest.oracle_failures",
            "oracle failed_count must match failing checks",
        ));
    }
    let expected_verdict = if record.oracle_verdict.failed_count == 0 {
        "passed"
    } else {
        "failed"
    };
    if record.verdict != expected_verdict || record.oracle_verdict.verdict != expected_verdict {
        return Err(invalid_mcp_project(
            "playtest.verdict",
            "playtest verdict must match oracle failed_count",
        ));
    }
    for check in &record.oracle_verdict.checks {
        validate_single_line("playtest.oracle.actual", &check.actual)?;
        validate_single_line("playtest.oracle.reason", &check.reason)?;
    }
    if record.verdict == "passed"
        && (record.spawned_bodies == 0
            || record.visible_instance_count == 0
            || record.draw_batches == 0
            || record.bodies_updated == 0
            || record.initial_state_hash == record.final_state_hash)
    {
        return Err(invalid_mcp_project(
            "playtest.passed_evidence",
            "passed playtests require spawned bodies, visible output, body updates, and state change",
        ));
    }
    Ok(())
}

pub fn validate_package_record(record: &LitdGamePackageRecord) -> Result<(), EngineError> {
    validate_id("package.project_id", &record.project_id)?;
    validate_id("package.package_id", &record.package_id)?;
    validate_single_line("package.path", &record.package_path)?;
    validate_sha256("package.sha256", &record.package_sha256)?;
    validate_single_line("package.runtime_id", &record.runtime_id)?;
    validate_single_line("package.lock_in_policy", &record.lock_in_policy)?;
    validate_single_line("package.host_uri", &record.host_uri)?;
    validate_single_line("package.document_fingerprint", &record.document_fingerprint)?;
    validate_single_line(
        "package.play_build_fingerprint",
        &record.play_build_fingerprint,
    )?;
    if record.package_bytes == 0 || record.asset_count == 0 {
        return Err(invalid_mcp_project(
            "package",
            "package bytes and asset_count must be non-zero",
        ));
    }
    if !record.world_hash_matches || !record.runtime_lock_verified {
        return Err(invalid_mcp_project(
            "package.readback",
            "world hash and runtime lock must be verified",
        ));
    }
    Ok(())
}

pub fn validate_publish_metadata(
    title: &str,
    version: &str,
    summary: &str,
    tags: &[String],
    min_spec_tags: &[String],
) -> Result<(), EngineError> {
    validate_single_line("publish.title", title)?;
    validate_single_line("publish.version", version)?;
    validate_single_line("publish.summary", summary)?;
    validate_id_list("publish.tags", tags)?;
    validate_id_list("publish.min_spec_tags", min_spec_tags)?;
    Ok(())
}

pub fn validate_published_record(record: &PublishedGameRecord) -> Result<(), EngineError> {
    validate_id("publish.project_id", &record.project_id)?;
    validate_id("publish.package_id", &record.package_id)?;
    validate_id("publish.publish_id", &record.publish_id)?;
    validate_single_line("publish.content_address", &record.content_address)?;
    validate_publish_metadata(
        &record.title,
        &record.version,
        &record.summary,
        &record.tags,
        &record.min_spec_tags,
    )?;
    validate_single_line("publish.package_path", &record.package_path)?;
    validate_sha256("publish.package_sha256", &record.package_sha256)?;
    validate_single_line("publish.thumbnail_path", &record.thumbnail_path)?;
    validate_sha256("publish.thumbnail_sha256", &record.thumbnail_sha256)?;
    validate_single_line("publish.thumbnail_media_type", &record.thumbnail_media_type)?;
    validate_id("publish.published_by", &record.published_by)?;
    if record.runtime_id != LITD_RUNTIME_ID || record.lock_in_policy != LITD_LOCK_IN_POLICY {
        return Err(invalid_mcp_project(
            "publish.runtime_lock",
            "published games must target the LitD runtime-only package contract",
        ));
    }
    let expected_content_address = format!("litdgame.sha256.{}", record.package_sha256);
    if record.content_address != expected_content_address {
        return Err(invalid_mcp_project(
            "publish.content_address",
            "content address must be derived from package_sha256",
        ));
    }
    if record.package_bytes == 0 || record.thumbnail_bytes == 0 || record.asset_count == 0 {
        return Err(invalid_mcp_project(
            "publish.bytes",
            "package, thumbnail, and asset counts must be non-zero",
        ));
    }
    Ok(())
}

pub fn validate_project_state(state: &McpProjectState) -> Result<(), EngineError> {
    for record in &state.projects {
        validate_project_record(record)?;
    }
    for record in &state.playtests {
        validate_playtest_record(record)?;
    }
    for record in &state.packages {
        validate_package_record(record)?;
    }
    for record in &state.published_games {
        validate_published_record(record)?;
        let package_matches = state.packages.iter().any(|package| {
            package.package_id == record.package_id
                && package.project_id == record.project_id
                && package.package_sha256 == record.package_sha256
                && package.runtime_lock_verified
        });
        if !package_matches {
            return Err(invalid_mcp_project(
                "publish.package",
                "published game must reference a verified package in project state",
            ));
        }
    }
    ensure_unique(
        "projects",
        state
            .projects
            .iter()
            .map(|record| record.project_id.as_str()),
    )?;
    ensure_unique(
        "playtests",
        state
            .playtests
            .iter()
            .map(|record| record.project_id.as_str()),
    )?;
    ensure_unique(
        "packages",
        state
            .packages
            .iter()
            .map(|record| record.package_id.as_str()),
    )?;
    ensure_unique(
        "published_games",
        state
            .published_games
            .iter()
            .map(|record| record.publish_id.as_str()),
    )?;
    Ok(())
}

pub fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    let value = path.display().to_string();
    validate_single_line(field, &value)
}

pub fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(|ch| ch == '\r' || ch == '\n') {
        return Err(invalid_mcp_project(
            field,
            "value must be non-empty and single-line",
        ));
    }
    Ok(())
}

pub fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(invalid_mcp_project(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

fn validate_id_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.is_empty() {
        return Err(invalid_mcp_project(field, "at least one value is required"));
    }
    for value in values {
        validate_id(field, value)?;
    }
    ensure_unique(field, values.iter().map(String::as_str))
}

fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !litd_hash::is_sha256_hex(value) {
        return Err(invalid_mcp_project(
            field,
            "value must be a 64-character lowercase SHA-256 hex",
        ));
    }
    Ok(())
}

fn ensure_unique<'a>(
    field: &str,
    values: impl Iterator<Item = &'a str>,
) -> Result<(), EngineError> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(invalid_mcp_project(
                field,
                format!("duplicate value {value}"),
            ));
        }
    }
    Ok(())
}
