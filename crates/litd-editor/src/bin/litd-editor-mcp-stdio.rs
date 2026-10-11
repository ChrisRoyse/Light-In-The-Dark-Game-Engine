//! Stdio JSON-RPC transport for the editor MCP tool surface.
//!
//! SECURITY REQUIREMENT (#1487): filesystem-mutating tools (e.g.
//! `litd.editor.project.deliver_litdgame`) confine every tool-supplied path to a
//! trusted project root named by the `LITD_EDITOR_PROJECT_ROOT` environment variable.
//! Whatever launches this server MUST set `LITD_EDITOR_PROJECT_ROOT` to the absolute
//! path of the project/workspace directory it is serving. If it is unset, those tools
//! fail loud (they do NOT silently fall back to an unconfined path) — set the variable
//! and re-run. This is deliberate: the confinement boundary must be configured
//! explicitly, never guessed from the process working directory.

use std::io::{self, BufRead, Write};

use litd_editor::mcp_loopback::{
    McpLoopbackJsonRpcError, McpLoopbackJsonRpcResponse, handle_mcp_loopback_json,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(value) => handle_mcp_loopback_json(value),
            Err(error) => Some(invalid_request(format!("parse JSON-RPC line: {error}"))),
        };
        // A notification (no id) yields None — write nothing, per JSON-RPC 2.0 (#1490).
        if let Some(response) = response {
            serde_json::to_writer(&mut stdout, &response)?;
            writeln!(stdout)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn invalid_request(reason: String) -> McpLoopbackJsonRpcResponse {
    McpLoopbackJsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: Value::Null,
        result: None,
        error: Some(McpLoopbackJsonRpcError {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: json!({ "reason": reason }),
        }),
    }
}
