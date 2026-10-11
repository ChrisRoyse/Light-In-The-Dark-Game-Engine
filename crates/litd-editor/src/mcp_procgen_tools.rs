//! MCP `generate_procedural_mesh` tool over the engine procedural-geometry
//! capability layer (#1482, MCP parity #915).
//!
//! This is a self-contained tool surface: it validates its inputs, calls the
//! shared `litd_engine::procgen` capability (the SAME code the GUI uses), lowers
//! to a content-addressed frozen mesh, persists the opaque geometry bytes
//! through the content-addressed store, appends a tool-call provenance record to
//! the Calyx Aster ledger row (single source of truth, #1348), and returns the
//! store address + a verifying readback. It deliberately does not go through the
//! strict
//! `render_materials` manifest route (a sibling agent is reworking that
//! McpServer registration) — the tool lives in its own module and is wired via
//! [`call_mcp_procgen_tool`].

use std::path::PathBuf;

use litd_engine::{
    EngineError,
    procgen::{
        FrozenMesh, HeightfieldSpec, PrimitiveSpec, ProcGenStore, generate_heightfield,
        generate_primitive,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[path = "mcp_procgen_tools_ledger.rs"]
mod mcp_procgen_tools_ledger;

use mcp_procgen_tools_ledger::append_procgen_ledger;
pub use mcp_procgen_tools_ledger::{
    ProcGenLedger, ProcGenLedgerReadback, ProcGenToolRecord, read_procgen_ledger,
    read_procgen_ledger_wal,
};

pub const GENERATE_MESH_TOOL: &str = "litd.engine.render.generate_procedural_mesh";
pub const GENERATE_MESH_OPERATION: &str = "engine.render.generate_procedural_mesh";

/// What geometry to generate. Both variants lower to a single [`FrozenMesh`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum ProcMeshRequest {
    Primitive { spec: PrimitiveSpec },
    Heightfield { spec: HeightfieldSpec },
}

impl ProcMeshRequest {
    fn kind_id(&self) -> String {
        match self {
            Self::Primitive { spec } => format!("primitive.{}", spec.kind_id()),
            Self::Heightfield { .. } => "heightfield".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateMeshArgs {
    pub store_root: PathBuf,
    pub ledger_path: PathBuf,
    pub request_id: String,
    pub requested_by: String,
    pub mesh_id: String,
    pub request: ProcMeshRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpProcGenToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub ledger_readback: ProcGenLedgerReadback,
}

/// Dispatch entry point for procedural-geometry MCP tools.
pub fn call_mcp_procgen_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpProcGenToolCallOutput, EngineError> {
    match tool_name {
        GENERATE_MESH_TOOL => generate_procedural_mesh(arguments),
        other => Err(invalid(
            "tool_name",
            format!("{other} is not a registered procedural-geometry MCP tool"),
        )),
    }
}

fn generate_procedural_mesh(arguments: Value) -> Result<McpProcGenToolCallOutput, EngineError> {
    let args: GenerateMeshArgs = serde_json::from_value(arguments)
        .map_err(|error| invalid("arguments", format!("decode arguments: {error}")))?;
    validate_single_line("request_id", &args.request_id)?;
    validate_single_line("requested_by", &args.requested_by)?;
    validate_single_line("mesh_id", &args.mesh_id)?;
    validate_non_empty_path("store_root", &args.store_root)?;
    validate_non_empty_path("ledger_path", &args.ledger_path)?;

    // Shared capability call: identical to what the GUI path invokes.
    let frozen = build_frozen(&args)?;
    // Lower to content-addressed store, then read it straight back by hash.
    let store = ProcGenStore::open(&args.store_root)?;
    let stored = store.store_mesh(&frozen)?;
    let loaded = store.load_mesh(&frozen.content_hash)?;
    loaded.verify()?;
    if loaded.content_hash != frozen.content_hash {
        return Err(EngineError::ProcGenReadbackMismatch {
            field: "mcp.store_roundtrip".to_string(),
            expected: frozen.content_hash.clone(),
            actual: loaded.content_hash,
        });
    }

    let record = ProcGenToolRecord {
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        tool_name: GENERATE_MESH_TOOL.to_string(),
        operation_id: GENERATE_MESH_OPERATION.to_string(),
        mesh_id: args.mesh_id.clone(),
        request_kind: args.request.kind_id(),
        content_hash: frozen.content_hash.clone(),
        vertex_count: frozen.vertex_count,
        index_count: frozen.index_count,
        triangle_count: frozen.triangle_count,
        manifest_path: stored.manifest_path.display().to_string(),
    };
    let ledger_readback = append_procgen_ledger(&args.ledger_path, record)?;

    let result = json!({
        "mesh_id": args.mesh_id,
        "request_kind": args.request.kind_id(),
        "content_hash": frozen.content_hash,
        "vertex_count": frozen.vertex_count,
        "index_count": frozen.index_count,
        "triangle_count": frozen.triangle_count,
        "vertex_stride": frozen.vertex_stride,
        "vertex_sha256": frozen.vertex_sha256,
        "index_sha256": frozen.index_sha256,
        "bounds_min": frozen.bounds_min,
        "bounds_max": frozen.bounds_max,
        "sample_positions": frozen.sample_positions(3),
        "store_manifest_path": stored.manifest_path.display().to_string(),
        "store_vertex_path": stored.vertex_path.display().to_string(),
        "store_index_path": stored.index_path.display().to_string(),
        "store_vertex_bytes": stored.vertex_bytes_len,
        "store_index_bytes": stored.index_bytes_len,
        "loaded_vertex_count": loaded.vertices().len(),
        "loaded_index_count": loaded.indices().len(),
        "ledger_path": ledger_readback.path,
        "ledger_sha256": ledger_readback.sha256,
        "ledger_record_count": ledger_readback.record_count,
    });

    Ok(McpProcGenToolCallOutput {
        tool_name: GENERATE_MESH_TOOL.to_string(),
        operation_id: GENERATE_MESH_OPERATION.to_string(),
        source_of_truth:
            "Calyx Aster procgen MCP ledger row + content-addressed procgen mesh blob store"
                .to_string(),
        result,
        ledger_readback,
    })
}

fn build_frozen(args: &GenerateMeshArgs) -> Result<FrozenMesh, EngineError> {
    match &args.request {
        ProcMeshRequest::Primitive { spec } => generate_primitive(spec, &args.mesh_id)?.freeze(),
        ProcMeshRequest::Heightfield { spec } => {
            let product = generate_heightfield(spec)?;
            product.mesh.freeze()
        }
    }
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid(field, "value must be a non-empty single line"));
    }
    Ok(())
}

fn validate_non_empty_path(field: &str, path: &std::path::Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid(field, "path must be non-empty"));
    }
    Ok(())
}

fn invalid(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.mcp_procgen_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_procgen_tools",
        field = %field,
        reason = %reason,
        "invalid procgen MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "mcp_procgen_tools_tests.rs"]
mod tests;
