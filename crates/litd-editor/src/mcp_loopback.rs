//! Local JSON-RPC/MCP loopback over the real editor tool registry.

use std::panic::{AssertUnwindSafe, catch_unwind};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mcp_tool_registry::{
    McpToolRegistry, default_mcp_tool_registry, dispatch_mcp_tool_call,
};

pub const TOOLS_LIST_METHOD: &str = "tools/list";
pub const TOOLS_CALL_METHOD: &str = "tools/call";
pub const INITIALIZE_METHOD: &str = "initialize";
/// MCP protocol revision spoken by this loopback (echoed in `initialize`), matching
/// the sibling calyx-mcp server so off-the-shelf MCP clients complete the handshake.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// The loopback-level call ledger (read/validate/types in the former mcp_loopback_store)
// was advertised via a public read API but never had a writer, and could not be
// populated from the loopback protocol (which carries only {name, arguments}, not the
// per-record request identity/routes its schema required). It had zero consumers and was
// redundant with the domain-level ledgers each tool already writes. Removed as the
// issue's "stop advertising it" fix (#1490) — see git history for the schema if a real
// loopback audit ledger is later implemented (it needs the protocol to carry request
// identity first).

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpLoopbackJsonRpcRequest {
    pub jsonrpc: String,
    /// Absent for JSON-RPC notifications. `None` (missing or explicit null) => the
    /// request gets NO response, per JSON-RPC 2.0 (#1490).
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpLoopbackJsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpLoopbackJsonRpcError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpLoopbackJsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpLoopbackToolCallParams {
    pub name: String,
    #[serde(default = "default_tool_arguments")]
    pub arguments: Value,
}

/// Dispatch a decoded loopback request. Returns `None` for a JSON-RPC notification
/// (no `id`): notifications — e.g. an MCP client's `notifications/initialized` — are
/// accepted silently and MUST NOT receive a response (#1490).
pub fn handle_mcp_loopback_request(
    request: McpLoopbackJsonRpcRequest,
) -> Option<McpLoopbackJsonRpcResponse> {
    // No id => notification: accept without replying (JSON-RPC 2.0).
    let id = request.id.clone()?;
    if request.jsonrpc != "2.0" {
        return Some(jsonrpc_error(
            id,
            -32600,
            "Invalid Request",
            json!({ "reason": "jsonrpc must equal 2.0" }),
        ));
    }
    if request.method == INITIALIZE_METHOD {
        return Some(initialize_result(id));
    }
    let registry = match default_mcp_tool_registry() {
        Ok(registry) => registry,
        Err(error) => {
            return Some(jsonrpc_error(
                id,
                -32000,
                "MCP registry unavailable",
                json!({ "error": error.to_string() }),
            ));
        }
    };
    Some(match request.method.as_str() {
        TOOLS_LIST_METHOD => tools_list(id, &registry),
        TOOLS_CALL_METHOD => tools_call(id, &registry, request.params),
        other => jsonrpc_error(id, -32601, "Method not found", json!({ "method": other })),
    })
}

/// Decode and dispatch one JSON-RPC value. Returns `None` for a notification (no
/// response). A malformed request cannot carry an id, so it replies with a null-id
/// error, per JSON-RPC 2.0.
pub fn handle_mcp_loopback_json(value: Value) -> Option<McpLoopbackJsonRpcResponse> {
    match serde_json::from_value::<McpLoopbackJsonRpcRequest>(value) {
        Ok(request) => handle_mcp_loopback_request(request),
        Err(error) => Some(jsonrpc_error(
            Value::Null,
            -32600,
            "Invalid Request",
            json!({ "reason": format!("decode JSON-RPC request: {error}") }),
        )),
    }
}

fn initialize_result(id: Value) -> McpLoopbackJsonRpcResponse {
    jsonrpc_result(
        id,
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "litd-editor-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            },
        }),
    )
}

fn tools_list(id: Value, registry: &McpToolRegistry) -> McpLoopbackJsonRpcResponse {
    let tools = registry
        .entries
        .iter()
        .map(|entry| {
            let tool = &entry.tool;
            json!({
                "name": tool.tool_name,
                "description": tool.title,
                "inputSchema": { "$ref": tool.input_schema_ref },
                "outputSchema": { "$ref": tool.output_schema_ref },
                "operationId": tool.operation_id,
                "domainId": entry.domain_id,
                "readbackToolName": tool.fsv_readback_tool_name,
                "mutatesState": tool.mutates_state,
                "recordsLedger": tool.records_ledger,
            })
        })
        .collect::<Vec<_>>();
    jsonrpc_result(
        id,
        json!({
            "server": {
                "server_name": "litd-engine-editor-mcp",
                "bind_host": "127.0.0.1",
                "stdio_binary": "litd-editor-mcp-stdio",
                "protocol": "jsonrpc-2.0/mcp",
                "transport": registry.manifest.transport,
                "auth_scope": "local_loopback_only",
                "domain_count": registry.manifest.domains.len(),
                "domain_tool_counts": registry.domain_counts()
            },
            "tools": tools,
            "tool_count": tools.len(),
        }),
    )
}

fn tools_call(id: Value, registry: &McpToolRegistry, params: Value) -> McpLoopbackJsonRpcResponse {
    let params = match serde_json::from_value::<McpLoopbackToolCallParams>(params) {
        Ok(params) => params,
        Err(error) => {
            return jsonrpc_error(
                id,
                -32602,
                "Invalid params",
                json!({ "reason": format!("decode tools/call params: {error}") }),
            );
        }
    };
    if let Err(error) = registry.entry(&params.name) {
        return jsonrpc_error(
            id,
            -32602,
            "Invalid params",
            json!({ "error": error.to_string(), "tool_name": params.name }),
        );
    }
    let McpLoopbackToolCallParams { name, arguments } = params;
    run_tool_dispatch(id, &name, || {
        dispatch_mcp_tool_call(registry, &name, arguments)
    })
}

/// Run a tool dispatch with panic isolation. One of the ~90 tools panicking (e.g. a
/// validated-elsewhere `.unwrap()` whose paired validation drifted) must NOT abort the
/// whole stdio process mid-request. Catch the unwind and return a JSON-RPC internal
/// error so the loop stays alive and the client gets a structured failure (#1490).
/// `AssertUnwindSafe` is sound: on panic we build a fresh error and touch no tool-owned
/// state afterward.
pub(crate) fn run_tool_dispatch(
    id: Value,
    tool_name: &str,
    dispatch: impl FnOnce() -> Result<Value, EngineError>,
) -> McpLoopbackJsonRpcResponse {
    match catch_unwind(AssertUnwindSafe(dispatch)) {
        Ok(Ok(output)) => call_success(id, tool_name, output),
        Ok(Err(error)) => jsonrpc_error(
            id,
            -32000,
            "Tool execution failed",
            json!({ "tool_name": tool_name, "error": error.to_string() }),
        ),
        Err(panic) => jsonrpc_error(
            id,
            -32603,
            "Internal error",
            json!({
                "tool_name": tool_name,
                "reason": format!("tool panicked: {}", panic_message(&*panic)),
            }),
        ),
    }
}

/// Extract a human-readable message from a caught panic payload.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn call_success(id: Value, tool_name: &str, output: Value) -> McpLoopbackJsonRpcResponse {
    jsonrpc_result(
        id,
        json!({
            "content": [{
                "type": "text",
                "text": format!("executed {tool_name}")
            }],
            "structuredContent": output,
            "isError": false
        }),
    )
}

fn jsonrpc_result(id: Value, result: Value) -> McpLoopbackJsonRpcResponse {
    McpLoopbackJsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn jsonrpc_error(
    id: Value,
    code: i64,
    message: impl Into<String>,
    data: Value,
) -> McpLoopbackJsonRpcResponse {
    let message = message.into();
    tracing::error!(
        target: "litd_editor::mcp_loopback",
        code,
        message = %message,
        data = %data,
        "MCP loopback JSON-RPC error"
    );
    McpLoopbackJsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(McpLoopbackJsonRpcError {
            code,
            message,
            data,
        }),
    }
}

fn default_tool_arguments() -> Value {
    json!({})
}

#[cfg(test)]
#[path = "mcp_loopback_tests.rs"]
mod tests;
