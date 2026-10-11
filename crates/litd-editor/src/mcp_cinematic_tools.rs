//! #892: MCP tools for no-code cinematic authoring and deterministic evaluation.
//!
//! `author_timeline` upserts an authored [`EditorCinematicTimeline`] onto the world
//! document, persists the document through the REAL Calyx Aster world store (the
//! same source of truth `read_world` reads), lowers it once to prove it resolves,
//! and records a ledger row. `evaluate_timeline` reads the stored document back,
//! lowers the named timeline into the frozen engine [`CinematicTimeline`], and
//! samples the PURE engine evaluator at the requested timestamps -- so the numbers
//! the editor previews are exactly the numbers the runtime will play.

use std::path::{Path, PathBuf};

use litd_engine::EngineError;
use litd_engine::cinematic_timeline::{
    CinematicTimeline, EvaluatedCinematic, sample_cinematic_at_times,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::cinematic_authoring::{EditorCinematicTimeline, lower_cinematic_timeline};
use crate::history::{EditorDocument, stable_fingerprint};
use crate::mcp_world_tools::{
    McpWorldDocumentReadback, McpWorldToolLedgerReadback, McpWorldToolRecord,
    read_mcp_world_document, write_mcp_world_document, write_mcp_world_tool_ledger,
};

pub const AUTHOR_TIMELINE_TOOL: &str = "litd.editor.world.author_timeline";
pub const EVALUATE_TIMELINE_TOOL: &str = "litd.editor.world.evaluate_timeline";

const AUTHOR_TIMELINE_OP: &str = "editor.world.author_timeline";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorTimelineArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub timeline: EditorCinematicTimeline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluateTimelineArgs {
    pub document_path: PathBuf,
    pub timeline_id: String,
    pub sample_times: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpCinematicToolOutput {
    pub tool_name: String,
    pub source_of_truth: String,
    pub result: Value,
    pub document_readback: Option<McpWorldDocumentReadback>,
    pub ledger_readback: Option<McpWorldToolLedgerReadback>,
}

pub fn call_mcp_cinematic_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpCinematicToolOutput, EngineError> {
    match tool_name {
        AUTHOR_TIMELINE_TOOL => author_timeline(arguments),
        EVALUATE_TIMELINE_TOOL => evaluate_timeline(arguments),
        other => Err(invalid_cinematic_tool(
            "tool_name",
            format!("{other} is not a registered cinematic MCP tool"),
        )),
    }
}

fn author_timeline(arguments: Value) -> Result<McpCinematicToolOutput, EngineError> {
    let mut args: AuthorTimelineArgs = parse_args(AUTHOR_TIMELINE_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("ledger_path", &args.ledger_path)?;
    validate_single_line("request_id", &args.request_id)?;
    validate_single_line("requested_by", &args.requested_by)?;
    if let Some(initial) = args.initial_document.as_ref() {
        initial.validate()?;
    }
    let mut document =
        load_document_for_mutation(&args.document_path, args.initial_document.take())?;
    document.upsert_cinematic_timeline(args.timeline.clone())?;
    // Lower once now, so an authored timeline that cannot resolve fails BEFORE we persist.
    let lowered = lower_cinematic_timeline(&document, &args.timeline)?;
    let readback = write_mcp_world_document(&args.document_path, &document)?;
    let record = build_record(
        &args,
        &readback,
        &format!("authored cinematic timeline {}", args.timeline.id),
    )?;
    let ledger_readback = write_mcp_world_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(McpCinematicToolOutput {
        tool_name: AUTHOR_TIMELINE_TOOL.to_string(),
        source_of_truth: "Calyx Aster EditorDocument row + world MCP ledger row".to_string(),
        result: serde_json::json!({
            "timeline_id": args.timeline.id,
            "lowered_fingerprint": stable_fingerprint(&lowered)?,
            "camera_keyframe_count": lowered.camera_track.len(),
            "duration_seconds": lowered.duration_seconds,
        }),
        document_readback: Some(readback),
        ledger_readback: Some(ledger_readback),
    })
}

fn evaluate_timeline(arguments: Value) -> Result<McpCinematicToolOutput, EngineError> {
    let args: EvaluateTimelineArgs = parse_args(EVALUATE_TIMELINE_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    if args.sample_times.is_empty() {
        return Err(invalid_cinematic_tool(
            "sample_times",
            "at least one sample time is required",
        ));
    }
    let readback = read_mcp_world_document(&args.document_path)?;
    let timeline = readback
        .document
        .cinematic_timelines
        .get(&args.timeline_id)
        .ok_or_else(|| {
            invalid_cinematic_tool(
                "timeline_id",
                format!("no cinematic timeline {} in document", args.timeline_id),
            )
        })?;
    let lowered = lower_cinematic_timeline(&readback.document, timeline)?;
    let samples: Vec<EvaluatedCinematic> = sample_cinematic_at_times(&lowered, &args.sample_times)
        .map_err(|error| invalid_cinematic_tool("evaluate", error.to_string()))?;
    let lowered_fingerprint = stable_fingerprint(&lowered)?;
    Ok(McpCinematicToolOutput {
        tool_name: EVALUATE_TIMELINE_TOOL.to_string(),
        source_of_truth: "Calyx Aster EditorDocument row -> lowered engine CinematicTimeline"
            .to_string(),
        result: serde_json::json!({
            "timeline_id": args.timeline_id,
            "lowered_fingerprint": lowered_fingerprint,
            "lowered_timeline": lowered,
            "samples": samples,
        }),
        document_readback: Some(readback),
        ledger_readback: None,
    })
}

fn load_document_for_mutation(
    path: &Path,
    initial_document: Option<EditorDocument>,
) -> Result<EditorDocument, EngineError> {
    if path.exists() {
        return Ok(read_mcp_world_document(path)?.document);
    }
    let document = initial_document.ok_or_else(|| {
        invalid_cinematic_tool(
            "initial_document",
            format!(
                "{} does not exist and no initial_document was supplied",
                path.display()
            ),
        )
    })?;
    document.validate()?;
    Ok(document)
}

fn build_record(
    args: &AuthorTimelineArgs,
    readback: &McpWorldDocumentReadback,
    action_summary: &str,
) -> Result<McpWorldToolRecord, EngineError> {
    let start_location_count = readback
        .document
        .objects
        .values()
        .filter(|object| object.kind == "StartLocation")
        .count();
    Ok(McpWorldToolRecord {
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        tool_name: AUTHOR_TIMELINE_TOOL.to_string(),
        operation_id: AUTHOR_TIMELINE_OP.to_string(),
        document_path: readback.path.clone(),
        document_sha256: readback.sha256.clone(),
        document_bytes: readback.bytes,
        document_fingerprint: readback.document_fingerprint.clone(),
        action_summary: action_summary.to_string(),
        terrain_cell_count: readback.terrain_cell_count,
        terrain_stamp_count: readback.terrain_stamp_count,
        object_count: readback.object_count,
        region_count: readback.region_count,
        pathing_override_count: readback.pathing_override_count,
        start_location_count,
    })
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_cinematic_tool(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_cinematic_tool(field, "path must be non-empty"));
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_cinematic_tool(
            field,
            "value must be non-empty single-line",
        ));
    }
    Ok(())
}

pub(crate) fn invalid_cinematic_tool(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.mcp_cinematic_tools.{}", field.into()),
        reason: reason.into(),
    }
}

/// Lower and validate a timeline named in a stored document (shared readback helper).
pub fn lower_stored_timeline(
    document: &EditorDocument,
    timeline_id: &str,
) -> Result<CinematicTimeline, EngineError> {
    let timeline = document
        .cinematic_timelines
        .get(timeline_id)
        .ok_or_else(|| {
            invalid_cinematic_tool(
                "timeline_id",
                format!("no cinematic timeline {timeline_id} in document"),
            )
        })?;
    lower_cinematic_timeline(document, timeline)
}

#[cfg(test)]
#[path = "mcp_cinematic_tools_tests.rs"]
mod tests;
