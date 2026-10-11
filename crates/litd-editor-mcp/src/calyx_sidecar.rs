//! Child-process stdio JSON-RPC transport to the installed `calyx-mcp` addon (#1792).
//!
//! Scope: transport + presence-probe layer ONLY. This provides the spawned
//! `calyx-mcp.exe` connection that speaks newline-delimited JSON-RPC over stdin/stdout
//! and the total-function availability probe. It is intended to replace the in-process
//! loopback `McpServer` (see `mcp_tools_calyx_surface`), but that in-process path is
//! left UNTOUCHED here and no `calyx.*` caller is rerouted yet: the wiring lands with
//! the binary-diet refactor (remaining work on #1792). The JSON-RPC surface is
//! identical so callers stay unchanged when the switch happens (#915), and the base
//! editor links none of the Calyx ML crates through this path; the addon carries the
//! models.
//!
//! Protocol discipline (MCP stdio spec 2025-11-25): stdout carries ONLY protocol frames,
//! stderr carries diagnostics. Each frame is one line with no embedded newline. A
//! dedicated reader thread drains stdout into a channel and a second thread drains
//! stderr into tracing, so neither pipe backpressures the other (the classic stdio
//! deadlock) and reads are bounded by a timeout instead of blocking the editor forever.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

#[path = "calyx_sidecar_types.rs"]
mod calyx_sidecar_types;

pub use calyx_sidecar_types::{
    CALYX_MCP_BIN_ENV, CalyxSidecarAvailabilityReadback, CalyxSidecarConfig, CalyxSidecarError,
    SIDECAR_AVAILABILITY_SCHEMA, SIDECAR_BINARY_MISSING, SIDECAR_BINARY_UNSET, SIDECAR_CLOSED,
    SIDECAR_DECODE_FAILED, SIDECAR_HANDSHAKE_FAILED, SIDECAR_ONLINE, SIDECAR_PROTOCOL_ERROR,
    SIDECAR_READ_FAILED, SIDECAR_SPAWN_FAILED, SIDECAR_WRITE_FAILED, SidecarJsonRpcError,
    SidecarResponse, SidecarToolDescriptor,
};

/// A live child-process connection to the `calyx-mcp` addon.
///
/// One request is outstanding at a time (the editor's AI panels are request/response),
/// so responses arrive in order on the reader channel. Dropping the sidecar closes the
/// child's stdin (EOF -> clean shutdown per the binary's stdin loop) and reaps it.
pub struct CalyxSidecar {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<std::io::Result<String>>,
    next_id: i64,
    binary: PathBuf,
    timeout: Duration,
}

impl CalyxSidecar {
    /// Spawn the addon binary resolved from `config`. Fails closed with a typed error
    /// (unset / missing / spawn-failed) -- callers that want a non-erroring presence
    /// check use [`probe_calyx_sidecar_availability`] instead.
    pub fn spawn(config: &CalyxSidecarConfig) -> Result<Self, CalyxSidecarError> {
        let binary = config.resolve_binary()?;
        let mut child = Command::new(&binary)
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                CalyxSidecarError::new(
                    SIDECAR_SPAWN_FAILED,
                    format!("spawn {}: {error}", binary.display()),
                    "verify the calyx-mcp addon binary is executable and not blocked by policy",
                )
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            CalyxSidecarError::new(
                SIDECAR_SPAWN_FAILED,
                "child stdin pipe unavailable",
                "the OS did not grant a stdin pipe; retry the spawn",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CalyxSidecarError::new(
                SIDECAR_SPAWN_FAILED,
                "child stdout pipe unavailable",
                "the OS did not grant a stdout pipe; retry the spawn",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CalyxSidecarError::new(
                SIDECAR_SPAWN_FAILED,
                "child stderr pipe unavailable",
                "the OS did not grant a stderr pipe; retry the spawn",
            )
        })?;

        let (tx, rx) = channel();
        thread::Builder::new()
            .name("calyx-sidecar-stdout".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break, // EOF: child closed stdout.
                        Ok(_) => {
                            let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
                            if trimmed.is_empty() {
                                continue;
                            }
                            if tx.send(Ok(trimmed)).is_err() {
                                break; // Receiver dropped.
                            }
                        }
                        Err(error) => {
                            let _ = tx.send(Err(error));
                            break;
                        }
                    }
                }
            })
            .map_err(|error| {
                CalyxSidecarError::new(
                    SIDECAR_SPAWN_FAILED,
                    format!("start stdout reader thread: {error}"),
                    "the OS refused a helper thread; free resources and retry",
                )
            })?;

        // Drain stderr so the addon's diagnostics never backpressure the protocol pipe.
        let _ = thread::Builder::new()
            .name("calyx-sidecar-stderr".to_string())
            .spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    tracing::debug!(target: "litd_editor_mcp::calyx_sidecar", "calyx-mcp: {line}");
                }
            });

        Ok(Self {
            child,
            stdin,
            rx,
            next_id: 1,
            binary,
            timeout: Duration::from_millis(config.handshake_timeout_ms.max(1)),
        })
    }

    /// The resolved addon binary backing this connection.
    pub fn binary_path(&self) -> &PathBuf {
        &self.binary
    }

    /// OS process id of the spawned child. Used by the hermetic Drop test to assert the
    /// child was reaped (no zombie) after the sidecar is dropped.
    pub fn child_pid(&self) -> u32 {
        self.child.id()
    }

    /// Send a JSON-RPC request and read the correlated response, bounded by the config
    /// timeout. Preserves the JSON-RPC error shape: an addon-side `error` object is
    /// returned inside the [`SidecarResponse`], not turned into a transport error.
    pub fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<SidecarResponse, CalyxSidecarError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut frame = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if let Some(params) = params {
            frame["params"] = params;
        }
        let mut line = serde_json::to_string(&frame).map_err(|error| {
            CalyxSidecarError::new(
                SIDECAR_WRITE_FAILED,
                format!("encode {method} request: {error}"),
                "this is an internal encoding bug; report the failing method",
            )
        })?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).map_err(|error| {
            CalyxSidecarError::new(
                SIDECAR_WRITE_FAILED,
                format!("write {method} request to calyx-mcp stdin: {error}"),
                "the addon process closed its input; respawn the sidecar",
            )
        })?;
        self.stdin.flush().map_err(|error| {
            CalyxSidecarError::new(
                SIDECAR_WRITE_FAILED,
                format!("flush {method} request: {error}"),
                "the addon process closed its input; respawn the sidecar",
            )
        })?;

        let raw = match self.rx.recv_timeout(self.timeout) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                return Err(CalyxSidecarError::new(
                    SIDECAR_READ_FAILED,
                    format!("read calyx-mcp response: {error}"),
                    "the addon stream faulted; respawn the sidecar",
                ));
            }
            Err(RecvTimeoutError::Timeout) => {
                return Err(CalyxSidecarError::new(
                    SIDECAR_HANDSHAKE_FAILED,
                    format!(
                        "calyx-mcp did not answer {method} within {:?}",
                        self.timeout
                    ),
                    "the addon is unresponsive; raise handshake_timeout_ms or respawn it",
                ));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(CalyxSidecarError::new(
                    SIDECAR_CLOSED,
                    format!("calyx-mcp exited before answering {method}"),
                    "the addon process terminated; check its stderr log and respawn it",
                ));
            }
        };

        let response: SidecarResponse = serde_json::from_str(&raw).map_err(|error| {
            CalyxSidecarError::new(
                SIDECAR_DECODE_FAILED,
                format!("decode calyx-mcp response frame: {error}"),
                "the addon emitted a non-JSON-RPC frame; verify the addon version",
            )
        })?;
        if response.id.as_ref().and_then(Value::as_i64) != Some(id) {
            return Err(CalyxSidecarError::new(
                SIDECAR_PROTOCOL_ERROR,
                format!(
                    "response id {:?} does not match request id {id}",
                    response.id
                ),
                "the addon violated 1:1 JSON-RPC id correlation; verify the addon version",
            ));
        }
        Ok(response)
    }

    /// `tools/list`: the tools the addon advertises. Identical surface to the in-process
    /// `McpServer`, decoded independently of the calyx-mcp crate.
    pub fn tools_list(&mut self) -> Result<Vec<SidecarToolDescriptor>, CalyxSidecarError> {
        let response = self.request("tools/list", None)?;
        if let Some(error) = response.error {
            return Err(CalyxSidecarError::new(
                SIDECAR_PROTOCOL_ERROR,
                format!("tools/list failed: {} {}", error.code, error.message),
                "the addon rejected tools/list; verify the addon version",
            ));
        }
        let result = response.result.ok_or_else(|| {
            CalyxSidecarError::new(
                SIDECAR_PROTOCOL_ERROR,
                "tools/list returned no result",
                "the addon returned neither result nor error; verify the addon version",
            )
        })?;
        serde_json::from_value(result["tools"].clone()).map_err(|error| {
            CalyxSidecarError::new(
                SIDECAR_DECODE_FAILED,
                format!("decode tools/list descriptors: {error}"),
                "the addon tools/list shape drifted; verify the addon version",
            )
        })
    }

    /// `tools/call`: invoke one `calyx.*` tool. The JSON-RPC error shape is preserved in
    /// the returned response (an addon-side tool error is data, not a transport failure).
    pub fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<SidecarResponse, CalyxSidecarError> {
        self.request(
            "tools/call",
            Some(json!({ "name": tool_name, "arguments": arguments })),
        )
    }
}

impl Drop for CalyxSidecar {
    fn drop(&mut self) {
        // Closing stdin gives the addon EOF -> its stdin loop returns ExitCode::SUCCESS.
        // We do not hold a second stdin handle, so dropping `self.stdin` (a field) closes
        // it. Then reap the child so no zombie/handle leaks.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Single source of truth for Calyx AI addon presence, per #701/#1790. Total function:
/// it NEVER errors. Absent or unresponsive addon -> `available = false` with a reason
/// code so callers grey out AI features instead of failing.
pub fn probe_calyx_sidecar_availability(
    config: &CalyxSidecarConfig,
) -> CalyxSidecarAvailabilityReadback {
    let binary_path = config
        .resolve_binary()
        .ok()
        .map(|path| path.display().to_string());
    match CalyxSidecar::spawn(config) {
        Err(error) => absent(error.code, error.message, binary_path),
        Ok(mut sidecar) => {
            let path = Some(sidecar.binary_path().display().to_string());
            match sidecar.tools_list() {
                Ok(tools) => {
                    let mut tool_names: Vec<String> =
                        tools.iter().map(|tool| tool.name.clone()).collect();
                    tool_names.sort();
                    CalyxSidecarAvailabilityReadback {
                        schema: SIDECAR_AVAILABILITY_SCHEMA.to_string(),
                        source_of_truth:
                            "spawned calyx-mcp child process tools/list over stdio JSON-RPC"
                                .to_string(),
                        available: true,
                        reason_code: SIDECAR_ONLINE.to_string(),
                        reason: format!(
                            "Calyx AI addon online: {} tools over the calyx-mcp sidecar",
                            tool_names.len()
                        ),
                        binary_path: path,
                        tool_count: tool_names.len(),
                        tool_names,
                        ward_off_by_vacuity: false,
                    }
                }
                Err(error) => absent(error.code, error.message, path),
            }
        }
    }
}

fn absent(
    reason_code: &str,
    reason: impl Into<String>,
    binary_path: Option<String>,
) -> CalyxSidecarAvailabilityReadback {
    CalyxSidecarAvailabilityReadback {
        schema: SIDECAR_AVAILABILITY_SCHEMA.to_string(),
        source_of_truth: "calyx-mcp sidecar spawn/handshake probe".to_string(),
        available: false,
        reason_code: reason_code.to_string(),
        reason: format!("Calyx AI addon absent: {}", reason.into()),
        binary_path,
        tool_count: 0,
        tool_names: Vec::new(),
        // Addon absent => no LLM generation => nothing for the Ward gate to gate.
        ward_off_by_vacuity: true,
    }
}

#[cfg(test)]
#[path = "calyx_sidecar_tests.rs"]
mod tests;
