//! Engine-facing bridge for Calyx's own `calyx.*` MCP tools.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

#[cfg(feature = "calyx-live-calls")]
use calyx_core::AuthN;
use calyx_mcp::{JsonRpcId, JsonRpcRequest, McpServer, tools::register_all};
use calyx_mcp::{JsonRpcResponse, ToolDef};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::EditorMcpError;

use crate::{fingerprint::stable_fingerprint, mcp_tools::McpToolKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxMcpSurfaceManifest {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub root_mcp_epic: u32,
    pub source_mcp_epic: u32,
    pub engine_surface: String,
    pub source_server_name: String,
    pub transport: String,
    pub expected_tools: Vec<String>,
    pub tools: Vec<EngineCalyxMcpToolBridge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxMcpToolBridge {
    pub tool_name: String,
    pub source_tool_name: String,
    pub engine_route: String,
    pub kind: McpToolKind,
    pub requires_authn: bool,
    pub input_schema: Value,
    pub source_description: String,
    pub source_use_when: String,
    pub preserves_jsonrpc_error_shape: bool,
    pub local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxMcpSurfaceReport {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub tool_count: usize,
    pub expected_tool_count: usize,
    pub mutating_tool_count: usize,
    pub readback_tool_count: usize,
    pub local_only_count: usize,
    pub jsonrpc_error_preserving_count: usize,
    pub source_tool_fingerprint: String,
    pub surface_fingerprint: String,
}

/// Readback of a single read-only `calyx.*` tool call over the engine's in-process
/// loopback to the local calyx-mcp server.
///
/// This is a SINGLE in-process surface: the "engine loopback" dispatches against the
/// exact same thread-local calyx-mcp [`McpServer`] as any direct call would (see
/// [`source_server`]). It therefore CANNOT, on its own, prove cross-implementation
/// parity — there is no independent second implementation to diverge from in-process.
/// Real cross-implementation parity is the CALLER's job against a genuinely independent
/// source (e.g. the `calyx-cli` binary in `calyx_mcp_surface_fsv`), never a vacuous
/// self-comparison here (#1533, residual false-green from #1458 finding 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxMcpCallReadback {
    pub tool_name: String,
    pub arguments: Value,
    pub surface: String,
    pub engine_response: JsonRpcResponse,
    pub response_fingerprint: String,
}

pub fn expected_calyx_mcp_tool_names() -> Result<Vec<String>, EditorMcpError> {
    expected_tool_vec()
}

pub fn calyx_mcp_tool_requires_authn(tool_name: &str) -> Result<bool, EditorMcpError> {
    requires_authn(tool_name)
}

pub fn calyx_mcp_tool_def(tool_name: &str) -> Result<ToolDef, EditorMcpError> {
    source_tool_defs()?
        .into_iter()
        .find(|tool| tool.name == tool_name)
        .ok_or_else(|| EditorMcpError::InvalidConfig {
            field: "editor.mcp_tools.calyx_surface.tool.descriptor".to_string(),
            reason: format!("{tool_name} is not advertised by calyx-mcp tools/list"),
        })
}

pub fn default_engine_calyx_mcp_surface_manifest()
-> Result<EngineCalyxMcpSurfaceManifest, EditorMcpError> {
    let source_tools = source_tool_defs()?;
    let actual = tool_names(&source_tools);
    validate_tool_name_set(&actual)?;
    let expected_tools = expected_tool_vec()?;
    let tools = source_tools
        .into_iter()
        .map(|def| {
            let requires_authn = requires_authn(def.name.as_str())?;
            Ok(EngineCalyxMcpToolBridge {
                source_tool_name: def.name.clone(),
                engine_route: format!("mcp://litd-engine/calyx/{}", def.name),
                kind: if requires_authn {
                    McpToolKind::Mutation
                } else {
                    McpToolKind::Readback
                },
                requires_authn,
                tool_name: def.name,
                input_schema: def.input_schema,
                source_description: def.description,
                source_use_when: def.use_when,
                preserves_jsonrpc_error_shape: true,
                local_only: true,
            })
        })
        .collect::<Result<Vec<_>, EditorMcpError>>()?;
    let manifest = EngineCalyxMcpSurfaceManifest {
        schema_version: 1,
        parent_issue: 1201,
        root_mcp_epic: 1143,
        source_mcp_epic: 916,
        engine_surface: "litd-engine-calyx-mcp".to_string(),
        source_server_name: "calyx-mcp".to_string(),
        transport: "loopback_jsonrpc_mcp".to_string(),
        expected_tools,
        tools,
    };
    validate_engine_calyx_mcp_surface_manifest(&manifest)?;
    Ok(manifest)
}

pub fn build_engine_calyx_mcp_surface_report(
    manifest: &EngineCalyxMcpSurfaceManifest,
) -> Result<EngineCalyxMcpSurfaceReport, EditorMcpError> {
    validate_engine_calyx_mcp_surface_manifest(manifest)?;
    let source_names = manifest
        .tools
        .iter()
        .map(|tool| tool.source_tool_name.as_str())
        .collect::<Vec<_>>();
    Ok(EngineCalyxMcpSurfaceReport {
        schema_version: manifest.schema_version,
        parent_issue: manifest.parent_issue,
        tool_count: manifest.tools.len(),
        expected_tool_count: manifest.expected_tools.len(),
        mutating_tool_count: manifest
            .tools
            .iter()
            .filter(|tool| tool.requires_authn)
            .count(),
        readback_tool_count: manifest
            .tools
            .iter()
            .filter(|tool| !tool.requires_authn)
            .count(),
        local_only_count: manifest.tools.iter().filter(|tool| tool.local_only).count(),
        jsonrpc_error_preserving_count: manifest
            .tools
            .iter()
            .filter(|tool| tool.preserves_jsonrpc_error_shape)
            .count(),
        source_tool_fingerprint: stable_fingerprint(&source_names)?,
        surface_fingerprint: stable_fingerprint(manifest)?,
    })
}

pub fn validate_engine_calyx_mcp_surface_manifest(
    manifest: &EngineCalyxMcpSurfaceManifest,
) -> Result<(), EditorMcpError> {
    if manifest.schema_version != 1 {
        return invalid_calyx_surface("schema_version", "expected schema version 1");
    }
    if manifest.parent_issue != 1201
        || manifest.root_mcp_epic != 1143
        || manifest.source_mcp_epic != 916
    {
        return invalid_calyx_surface("issue_binding", "manifest must bind #1201 under #1143/#916");
    }
    if manifest.engine_surface != "litd-engine-calyx-mcp"
        || manifest.source_server_name != "calyx-mcp"
        || manifest.transport != "loopback_jsonrpc_mcp"
    {
        return invalid_calyx_surface(
            "transport",
            "surface must bridge litd-engine to the local calyx-mcp server",
        );
    }
    validate_tool_name_set(
        &manifest
            .tools
            .iter()
            .map(|tool| tool.tool_name.clone())
            .collect::<Vec<_>>(),
    )?;
    if manifest.expected_tools != expected_tool_vec()? {
        return invalid_calyx_surface("expected_tools", "calyx-mcp tools/list drifted");
    }
    for tool in &manifest.tools {
        validate_bridge_tool(tool)?;
    }
    Ok(())
}

#[cfg(feature = "calyx-live-calls")]
pub fn call_engine_calyx_mcp_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<JsonRpcResponse, EditorMcpError> {
    let server = source_server()?;
    Ok(dispatch_call(&server, 1201, tool_name, arguments))
}

#[cfg(feature = "calyx-live-calls")]
pub fn readback_engine_calyx_mcp_tool_call(
    tool_name: &str,
    arguments: Value,
) -> Result<EngineCalyxMcpCallReadback, EditorMcpError> {
    if requires_authn(tool_name)? {
        return invalid_calyx_surface(
            "replay",
            format!("{tool_name} mutates or authenticates state; use persisted readback instead"),
        );
    }
    let engine_response = call_engine_calyx_mcp_tool(tool_name, arguments.clone())?;
    let readback = EngineCalyxMcpCallReadback {
        tool_name: tool_name.to_string(),
        arguments,
        surface: "calyx-mcp-loopback".to_string(),
        engine_response,
        response_fingerprint: String::new(),
    };
    let response_fingerprint = stable_fingerprint(&readback)?;
    Ok(EngineCalyxMcpCallReadback {
        response_fingerprint,
        ..readback
    })
}

fn source_tool_defs() -> Result<Vec<ToolDef>, EditorMcpError> {
    let server = source_server()?;
    let response = server.dispatch(request(1201, "tools/list", None));
    if let Some(error) = response.error {
        return invalid_calyx_surface(
            "tools_list",
            format!(
                "calyx-mcp tools/list failed: {} {}",
                error.code, error.message
            ),
        );
    }
    let result = response
        .result
        .ok_or_else(|| EditorMcpError::InvalidConfig {
            field: "editor.mcp_tools.calyx_surface.tools_list".to_string(),
            reason: "calyx-mcp tools/list returned no result".to_string(),
        })?;
    serde_json::from_value(result["tools"].clone()).map_err(|error| EditorMcpError::InvalidConfig {
        field: "editor.mcp_tools.calyx_surface.tools_list".to_string(),
        reason: format!("decode calyx-mcp tools/list descriptors: {error}"),
    })
}

thread_local! {
    /// Per-thread cache of the fully-registered calyx-mcp source server (#1461 item 5).
    /// `register_all` builds a fixed, deterministic tool set, and `McpServer::dispatch` is
    /// `&self`, so the server is safe to build once and reuse for every subsequent call on
    /// the thread. It holds `Box<dyn Tool>` (not `Send + Sync`), so a process-global
    /// `OnceLock` is not possible — a thread_local `Rc` gives effectively-once construction
    /// on the loopback handler thread instead of a fresh `register_all` per call (which ran
    /// twice per parity readback).
    static SOURCE_SERVER: RefCell<Option<Rc<McpServer>>> = const { RefCell::new(None) };
}

fn source_server() -> Result<Rc<McpServer>, EditorMcpError> {
    SOURCE_SERVER.with(|cell| {
        if let Some(server) = cell.borrow().as_ref() {
            return Ok(Rc::clone(server));
        }
        let mut server = McpServer::new();
        register_all(&mut server).map_err(|error| {
            tracing::error!(
                target: "litd_editor::mcp_tools::calyx_surface",
                calyx_code = error.code,
                remediation = error.remediation,
                message = error.message,
                "register calyx-mcp tools"
            );
            EditorMcpError::InvalidConfig {
                field: "editor.mcp_tools.calyx_surface.register_all".to_string(),
                reason: format!("{}: {}", error.code, error.message),
            }
        })?;
        let server = Rc::new(server);
        *cell.borrow_mut() = Some(Rc::clone(&server));
        Ok(server)
    })
}

#[cfg(feature = "calyx-live-calls")]
fn dispatch_call(
    server: &McpServer,
    id: i64,
    tool_name: &str,
    arguments: Value,
) -> JsonRpcResponse {
    server.dispatch_with_authn(
        request(
            id,
            "tools/call",
            Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            })),
        ),
        Some(&authn()),
    )
}

fn request(id: i64, method: &str, params: Option<Value>) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
        id: Some(JsonRpcId::Number(id)),
    }
}

#[cfg(feature = "calyx-live-calls")]
fn authn() -> AuthN {
    AuthN::InProcess {
        host_app_id: "litd-engine-calyx-mcp".to_string(),
    }
}

fn tool_names(source_tools: &[ToolDef]) -> Vec<String> {
    source_tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>()
}

fn validate_tool_name_set(actual: &[String]) -> Result<(), EditorMcpError> {
    let expected = expected_tool_vec()?;
    let actual_set = actual.iter().cloned().collect::<BTreeSet<_>>();
    let expected_set = expected.iter().cloned().collect::<BTreeSet<_>>();
    if actual.len() != actual_set.len() {
        return invalid_calyx_surface("tool.name", "duplicate calyx tool name");
    }
    if actual_set != expected_set {
        let missing = expected_set
            .difference(&actual_set)
            .cloned()
            .collect::<Vec<_>>();
        let extra = actual_set
            .difference(&expected_set)
            .cloned()
            .collect::<Vec<_>>();
        return invalid_calyx_surface(
            "expected_tools",
            format!(
                "missing=[{}] extra=[{}]",
                missing.join(","),
                extra.join(",")
            ),
        );
    }
    Ok(())
}

fn validate_bridge_tool(tool: &EngineCalyxMcpToolBridge) -> Result<(), EditorMcpError> {
    if tool.tool_name != tool.source_tool_name
        || !tool.tool_name.starts_with("calyx.")
        || !tool
            .engine_route
            .starts_with("mcp://litd-engine/calyx/calyx.")
    {
        return invalid_calyx_surface(
            "tool.route",
            "bridge tool names must preserve the calyx.* source route",
        );
    }
    if tool.input_schema.as_object().is_none()
        || tool.source_description.trim().is_empty()
        || tool.source_use_when.trim().is_empty()
    {
        return invalid_calyx_surface(
            "tool.descriptor",
            format!("{} descriptor is incomplete", tool.tool_name),
        );
    }
    if !tool.local_only || !tool.preserves_jsonrpc_error_shape {
        return invalid_calyx_surface(
            "tool.safety",
            "calyx bridge tools must be local-only and JSON-RPC error preserving",
        );
    }
    if requires_authn(&tool.tool_name)? != tool.requires_authn {
        return invalid_calyx_surface(
            "tool.authn",
            format!("{} requires_authn metadata is stale", tool.tool_name),
        );
    }
    Ok(())
}

fn expected_tool_vec() -> Result<Vec<String>, EditorMcpError> {
    Ok(tool_names(&source_tool_defs()?))
}

fn requires_authn(tool_name: &str) -> Result<bool, EditorMcpError> {
    let server = source_server()?;
    requires_authn_on(&server, tool_name)
}

/// Classify whether `tool_name` mutates durable Calyx state by reading the tool's own
/// STATIC [`Tool::requires_authn`] declaration via [`McpServer::tool_requires_authn`]
/// (#1488).
///
/// This replaces the previous probe that *executed* the tool with empty arguments and
/// no authn and pattern-matched the error shape. That probe (a) ran real read-only tool
/// bodies against the live vault every time a descriptor was built — every AI-panel
/// frame — and (b) FAILED OPEN: any error other than the exact `CALYX_AUTHN_REQUIRED`
/// shape was swallowed as "no authn required", so a mutating tool whose error shape
/// drifted (or that panicked → internal error) was silently classified read-only,
/// defeating the mutation/replay guard.
///
/// The static declaration is authoritative and cannot fail open. An unregistered tool
/// yields `None` → we fail CLOSED with a structured error (callers such as the replay
/// guard must treat "cannot classify" as "do not replay / treat as mutating").
fn requires_authn_on(server: &McpServer, tool_name: &str) -> Result<bool, EditorMcpError> {
    match server.tool_requires_authn(tool_name) {
        Some(value) => Ok(value),
        None => {
            tracing::error!(
                target: "litd_editor::mcp_tools::calyx_surface",
                tool_name,
                "unknown calyx MCP tool authn metadata"
            );
            Err(EditorMcpError::InvalidConfig {
                field: "editor.mcp_tools.calyx_surface.tool.authn".to_string(),
                reason: format!("{tool_name} is not present in calyx-mcp surface authn metadata"),
            })
        }
    }
}

fn invalid_calyx_surface<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EditorMcpError> {
    let field = format!("editor.mcp_tools.calyx_surface.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_tools::calyx_surface",
        field = %field,
        reason = %reason,
        "invalid calyx MCP surface"
    );
    Err(EditorMcpError::InvalidConfig { field, reason })
}

#[cfg(test)]
#[path = "mcp_tools_calyx_surface_tests.rs"]
mod tests;
