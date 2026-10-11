//! Read-only FSV/introspection MCP tools over persisted source-of-truth files.

use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{
    mcp_agent_tools::{read_mcp_agent_state, read_mcp_agent_tool_ledger},
    mcp_asset_tools::{read_mcp_asset_state, read_mcp_asset_tool_ledger},
    mcp_object_tools::{read_mcp_object_document, read_mcp_object_tool_ledger},
    mcp_project_tools::{
        read_litdworld_project_readback, read_mcp_project_state, read_mcp_project_tool_ledger,
    },
    mcp_render_tools::{read_mcp_render_state, read_mcp_render_tool_ledger},
    mcp_story_tools::{
        read_lore_canon_query_ledger, read_lore_cowriter_ledger, read_lore_template_ledger,
        read_mcp_story_state, read_mcp_story_tool_ledger,
    },
    mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest},
    mcp_trigger_tools::{
        read_mcp_sound_record, read_mcp_sound_runtime, read_mcp_trigger_state,
        read_mcp_trigger_tool_ledger,
    },
    mcp_vfx_tools::read_mcp_vfx_tool_ledger,
    mcp_world_tools::{read_mcp_world_document, read_mcp_world_tool_ledger},
    quick_share::parse_quick_share_package,
};

#[path = "mcp_fsv_tools_types.rs"]
mod mcp_fsv_tools_types;

pub use mcp_fsv_tools_types::{
    FsvArtifactKind, FsvArtifactReadback, FsvArtifactRequest, FsvReadArtifactsArgs, FsvToolReadback,
};

pub const FSV_READBACK_SCHEMA: &str = "litd.mcp.fsv_tools.readback.v1";

/// FSV artifacts are read via a size-capped `fs::read`: the path is tool-supplied, and an
/// unbounded read of a multi-GB file is a process-OOM/abort primitive — the same hole
/// #1487 closed on the quick-share load path. Check the size via `metadata` before reading
/// a single byte (#1518). 64 MiB matches the quick-share package load limit; a `.litdgame`
/// package or an FSV JSON report far larger than that is malformed and rejected loud.
const MAX_FSV_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
pub const READ_WORLD_STATE_TOOL: &str = "litd.editor.fsv.read_world_state";
pub const READ_OBJECT_STATE_TOOL: &str = "litd.editor.fsv.read_object_state";
pub const READ_TRIGGER_RUNTIME_STATE_TOOL: &str = "litd.editor.fsv.read_trigger_runtime_state";
pub const READ_STORY_RENDER_STATE_TOOL: &str = "litd.editor.fsv.read_story_render_state";
pub const READ_PROJECT_ARTIFACTS_TOOL: &str = "litd.editor.fsv.read_project_artifacts";

pub fn call_mcp_fsv_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<FsvToolReadback, EngineError> {
    let route = fsv_route(tool_name)?;
    let args: FsvReadArtifactsArgs = parse_args(&route.tool_name, arguments)?;
    match tool_name {
        READ_WORLD_STATE_TOOL => read_artifacts(
            route,
            args,
            &[FsvArtifactKind::WorldDocument, FsvArtifactKind::WorldLedger],
        ),
        READ_OBJECT_STATE_TOOL => read_artifacts(
            route,
            args,
            &[
                FsvArtifactKind::ObjectDocument,
                FsvArtifactKind::ObjectLedger,
            ],
        ),
        READ_TRIGGER_RUNTIME_STATE_TOOL => read_artifacts(
            route,
            args,
            &[
                FsvArtifactKind::TriggerState,
                FsvArtifactKind::SoundRecord,
                FsvArtifactKind::SoundRuntime,
                FsvArtifactKind::TriggerLedger,
            ],
        ),
        READ_STORY_RENDER_STATE_TOOL => read_artifacts(
            route,
            args,
            &[
                FsvArtifactKind::StoryState,
                FsvArtifactKind::StoryLedger,
                FsvArtifactKind::LoreCanonQueryLedger,
                FsvArtifactKind::LoreCowriterLedger,
                FsvArtifactKind::LoreTemplateLedger,
                FsvArtifactKind::RenderState,
                FsvArtifactKind::RenderLedger,
                FsvArtifactKind::VfxLedger,
                FsvArtifactKind::AgentState,
                FsvArtifactKind::AgentLedger,
                FsvArtifactKind::AssetState,
                FsvArtifactKind::AssetLedger,
            ],
        ),
        READ_PROJECT_ARTIFACTS_TOOL => read_artifacts(
            route,
            args,
            &[
                FsvArtifactKind::ProjectState,
                FsvArtifactKind::ProjectLedger,
                FsvArtifactKind::LitdWorld,
                FsvArtifactKind::LitdGame,
                FsvArtifactKind::FsvReport,
            ],
        ),
        other => Err(invalid_mcp_fsv(
            "tool_name",
            format!("{other} is not implemented by #927 FSV tools"),
        )),
    }
}

fn read_artifacts(
    route: McpDomainTool,
    args: FsvReadArtifactsArgs,
    allowed: &[FsvArtifactKind],
) -> Result<FsvToolReadback, EngineError> {
    validate_read_route(&route)?;
    if args.artifacts.is_empty() {
        return Err(invalid_mcp_fsv(
            "artifacts",
            "at least one artifact is required",
        ));
    }
    let mut readbacks = Vec::with_capacity(args.artifacts.len());
    for artifact in args.artifacts {
        validate_artifact_request(&artifact, allowed)?;
        readbacks.push(read_artifact(&artifact)?);
    }
    let aggregate_sha256 = aggregate_sha256(&readbacks)?;
    Ok(FsvToolReadback {
        schema: FSV_READBACK_SCHEMA.to_string(),
        issue: 927,
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth:
            "typed domain readers over physical artifact files and Calyx Aster vault rows"
                .to_string(),
        artifact_count: readbacks.len(),
        aggregate_sha256,
        artifacts: readbacks,
    })
}

fn read_artifact(artifact: &FsvArtifactRequest) -> Result<FsvArtifactReadback, EngineError> {
    let path = Path::new(&artifact.path);
    let (bytes, sha256, summary) = match artifact.kind {
        FsvArtifactKind::WorldDocument => {
            let readback = read_mcp_world_document(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::WorldLedger => {
            let readback = read_mcp_world_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::ObjectDocument => {
            let readback = read_mcp_object_document(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::ObjectLedger => {
            let readback = read_mcp_object_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::LitdWorld => {
            let readback = read_litdworld_project_readback(path)?;
            let project = readback.project;
            (
                readback.bytes,
                readback.sha256,
                json!({
                    "schema": project.schema,
                    "issue": project.issue,
                    "project_id": project.project_id,
                    "title": project.title,
                    "version": project.version,
                    "snapshot_label": project.snapshot_label,
                    "source_of_truth": project.source_of_truth,
                    "document_fingerprint": project.document_fingerprint,
                    "aster_seq": readback.seq,
                    "aster_graph_rows": readback.graph_rows,
                    "revision_id": project.document.revision_id,
                    "object_count": project.document.objects.len(),
                    "unit_count": project
                        .document
                        .objects
                        .values()
                        .filter(|object| object.kind == "Unit")
                        .count(),
                    "asset_product_count": project.document.asset_products.len(),
                    "terrain_cell_count": project.document.terrain_cells.len(),
                    "region_count": project.document.regions.len()
                }),
            )
        }
        FsvArtifactKind::LitdGame => {
            let raw = read_fsv_artifact_capped(path)?;
            let sha256 = sha256_hex(&raw);
            let package = parse_quick_share_package(&raw)?;
            let runtime =
                litd_engine::game_package::load_runtime_locked_package(path, "mcp.fsv.readback")?;
            (
                raw.len() as u64,
                sha256,
                json!({
                    "schema": package.schema,
                    "share_id": package.share_id,
                    "package_id": package.package_id,
                    "created_by": package.created_by,
                    "runtime_id": package.runtime_id,
                    "lock_in_policy": package.lock_in_policy,
                    "host_uri": package.host_uri,
                    "document_fingerprint": package.document_fingerprint,
                    "play_build_fingerprint": package.play_build_fingerprint,
                    "source_document_fingerprint": package.source_provenance.source_document_fingerprint,
                    "manifest_fingerprint": package.manifest_fingerprint,
                    "document_schema": runtime.document_summary.document_schema,
                    "object_count": runtime.document_summary.object_count,
                    "unit_count": runtime.document_summary.unit_count,
                    "asset_count": package.asset_count
                }),
            )
        }
        FsvArtifactKind::FsvReport => {
            let raw = read_fsv_artifact_capped(path)?;
            let sha256 = sha256_hex(&raw);
            let value = serde_json::from_slice(&raw)
                .map_err(|error| invalid_mcp_fsv("report.parse", error))?;
            (raw.len() as u64, sha256, value)
        }
        FsvArtifactKind::TriggerState => {
            let readback = read_mcp_trigger_state(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::SoundRecord => {
            let readback = read_mcp_sound_record(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::SoundRuntime => {
            let readback = read_mcp_sound_runtime(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::TriggerLedger => {
            let readback = read_mcp_trigger_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::StoryState => {
            let readback = read_mcp_story_state(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::StoryLedger => {
            let readback = read_mcp_story_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::LoreCanonQueryLedger => {
            let readback = read_lore_canon_query_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::LoreCowriterLedger => {
            let readback = read_lore_cowriter_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::LoreTemplateLedger => {
            let readback = read_lore_template_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::RenderState => {
            let readback = read_mcp_render_state(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::RenderLedger => {
            let readback = read_mcp_render_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::VfxLedger => {
            let readback = read_mcp_vfx_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::AgentState => {
            let readback = read_mcp_agent_state(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::AgentLedger => {
            let readback = read_mcp_agent_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::AssetState => {
            let readback = read_mcp_asset_state(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::AssetLedger => {
            let readback = read_mcp_asset_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::ProjectState => {
            let readback = read_mcp_project_state(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
        FsvArtifactKind::ProjectLedger => {
            let readback = read_mcp_project_tool_ledger(path)?;
            (readback.bytes, readback.sha256.clone(), summary(readback)?)
        }
    };
    Ok(FsvArtifactReadback {
        label: artifact.label.clone(),
        kind: artifact.kind,
        path: artifact.path.clone(),
        bytes,
        sha256,
        summary,
    })
}

fn validate_artifact_request(
    artifact: &FsvArtifactRequest,
    allowed: &[FsvArtifactKind],
) -> Result<(), EngineError> {
    validate_token("artifact.label", &artifact.label)?;
    validate_token("artifact.path", &artifact.path)?;
    if !allowed.contains(&artifact.kind) {
        return Err(invalid_mcp_fsv(
            "artifact.kind",
            format!("{:?} is not allowed for this FSV route", artifact.kind),
        ));
    }
    Ok(())
}

fn validate_read_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if route.mutates_state || !route.local_only || !route.returns_structured_content {
        return Err(invalid_mcp_fsv(
            "route",
            format!("{} is not a read-only local FSV route", route.tool_name),
        ));
    }
    Ok(())
}

fn fsv_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "fsv_introspection")
        .ok_or_else(|| invalid_mcp_fsv("domain", "missing fsv_introspection domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_fsv(
                "tool_name",
                format!("{tool_name} is not registered in fsv_introspection"),
            )
        })
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, arguments: Value) -> Result<T, EngineError> {
    serde_json::from_value(arguments).map_err(|error| invalid_mcp_fsv(tool_name, error))
}

fn summary(value: impl serde::Serialize) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|error| EngineError::SerializationFailed {
        format: "fsv-summary-json",
        reason: error.to_string(),
    })
}

fn aggregate_sha256(readbacks: &[FsvArtifactReadback]) -> Result<String, EngineError> {
    let bytes =
        serde_json::to_vec(readbacks).map_err(|error| EngineError::SerializationFailed {
            format: "fsv-artifact-readbacks-json",
            reason: error.to_string(),
        })?;
    Ok(sha256_hex(&bytes))
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.chars().any(|ch| ch == '\r' || ch == '\n') {
        return Err(invalid_mcp_fsv(
            field,
            "value must be non-empty and single-line",
        ));
    }
    Ok(())
}

/// Read an FSV artifact into memory with a size cap (#1518). Fails loud if the file is
/// larger than [`MAX_FSV_ARTIFACT_BYTES`] BEFORE reading any bytes, so a tool-supplied
/// path to a huge file cannot OOM/abort the process.
fn read_fsv_artifact_capped(path: &Path) -> Result<Vec<u8>, EngineError> {
    let metadata = fs::metadata(path).map_err(|error| {
        invalid_mcp_fsv(
            "artifact.metadata",
            format!("stat {}: {error}", path.display()),
        )
    })?;
    if metadata.len() > MAX_FSV_ARTIFACT_BYTES {
        tracing::error!(
            path = %path.display(),
            bytes = metadata.len(),
            max_fsv_artifact_bytes = MAX_FSV_ARTIFACT_BYTES,
            code = "LITD-MCP-FSV-READ-LIMIT",
            "FSV artifact read rejected by size limit"
        );
        return Err(invalid_mcp_fsv(
            "artifact.read.size",
            format!(
                "artifact {} is {} bytes, exceeding the read limit {MAX_FSV_ARTIFACT_BYTES}",
                path.display(),
                metadata.len()
            ),
        ));
    }
    fs::read(path).map_err(|error| invalid_mcp_fsv("artifact.read", error))
}

pub(super) fn invalid_mcp_fsv(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> EngineError {
    let field = format!("editor.mcp_fsv_tools.{}", field.into());
    tracing::error!(
        target: "litd_editor::mcp_fsv_tools",
        field,
        reason = %reason,
        "invalid FSV/introspection MCP readback"
    );
    EngineError::UiRenderInvalidConfig {
        field,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
#[path = "mcp_fsv_tools_tests.rs"]
mod tests;
