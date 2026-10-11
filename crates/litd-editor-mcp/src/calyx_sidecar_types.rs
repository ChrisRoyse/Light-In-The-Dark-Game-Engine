//! Wire types, typed errors, resolution config and availability readback for the
//! child-process `calyx-mcp` stdio sidecar transport (#1792 / #1790).
//!
//! Scope: this is the transport + presence-probe layer ONLY. It does not touch the
//! in-process loopback `McpServer` surface, and nothing yet routes `calyx.*` callers
//! through it -- that wiring lands with the binary-diet refactor (remaining work on
//! #1792). This module links NONE of the Calyx ML crates on purpose: the sidecar
//! transport must be usable from a base editor that dropped
//! `calyx-registry`/`calyx-ward`/ORT. The JSON-RPC surface is re-declared here as
//! minimal serde structs rather than reused from `calyx-mcp` (which would pull the
//! whole ML dependency graph back in), so the wire contract stays identical while the
//! link graph stays lean.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Environment variable naming the installed `calyx-mcp` addon binary.
pub const CALYX_MCP_BIN_ENV: &str = "LITD_CALYX_MCP_BIN";

pub const SIDECAR_ONLINE: &str = "LITD-CALYX-SIDECAR-ONLINE";
pub const SIDECAR_BINARY_UNSET: &str = "LITD-CALYX-SIDECAR-BINARY-UNSET";
pub const SIDECAR_BINARY_MISSING: &str = "LITD-CALYX-SIDECAR-BINARY-MISSING";
pub const SIDECAR_SPAWN_FAILED: &str = "LITD-CALYX-SIDECAR-SPAWN-FAILED";
pub const SIDECAR_HANDSHAKE_FAILED: &str = "LITD-CALYX-SIDECAR-HANDSHAKE-FAILED";
pub const SIDECAR_WRITE_FAILED: &str = "LITD-CALYX-SIDECAR-WRITE-FAILED";
pub const SIDECAR_READ_FAILED: &str = "LITD-CALYX-SIDECAR-READ-FAILED";
pub const SIDECAR_CLOSED: &str = "LITD-CALYX-SIDECAR-CLOSED";
pub const SIDECAR_DECODE_FAILED: &str = "LITD-CALYX-SIDECAR-DECODE-FAILED";
pub const SIDECAR_PROTOCOL_ERROR: &str = "LITD-CALYX-SIDECAR-PROTOCOL-ERROR";

/// Fail-closed typed error for the sidecar transport. Every variant carries a stable
/// diagnostic `code` and an operator-facing `remediation`; there is no silent fallback
/// (an absent addon is reported through [`CalyxSidecarAvailability`], not this error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalyxSidecarError {
    pub code: &'static str,
    pub message: String,
    pub remediation: &'static str,
}

impl std::fmt::Display for CalyxSidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.code, self.message, self.remediation)
    }
}

impl std::error::Error for CalyxSidecarError {}

impl CalyxSidecarError {
    pub(crate) fn new(
        code: &'static str,
        message: impl Into<String>,
        remediation: &'static str,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            remediation,
        }
    }
}

/// Resolution config: where the installed `calyx-mcp.exe` addon lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxSidecarConfig {
    /// Explicit binary path. When `None`, [`resolve_binary`] falls back to the
    /// `LITD_CALYX_MCP_BIN` environment variable.
    pub binary: Option<PathBuf>,
    /// Milliseconds to wait for the spawned child's first response before declaring
    /// the handshake failed. Bounds a hung addon so the editor never blocks forever.
    pub handshake_timeout_ms: u64,
    /// Extra process arguments passed to the spawned binary. Empty for the real
    /// `calyx-mcp.exe` addon (it takes none); non-empty only for the hermetic test
    /// stub, which selects a fault-injection mode via its first argument.
    #[serde(default)]
    pub args: Vec<String>,
}

impl Default for CalyxSidecarConfig {
    fn default() -> Self {
        Self {
            binary: None,
            handshake_timeout_ms: 5_000,
            args: Vec::new(),
        }
    }
}

impl CalyxSidecarConfig {
    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: Some(binary.into()),
            ..Self::default()
        }
    }

    /// Set the process arguments (used by the hermetic test stub to pick a mode).
    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Resolve the addon binary path from the explicit field or the environment.
    /// Returns a typed error (never a panic / never a fabricated path) when neither
    /// is present or the resolved path is not an existing file.
    pub fn resolve_binary(&self) -> Result<PathBuf, CalyxSidecarError> {
        let candidate = match &self.binary {
            Some(path) => path.clone(),
            None => match std::env::var_os(CALYX_MCP_BIN_ENV) {
                Some(value) => PathBuf::from(value),
                None => {
                    return Err(CalyxSidecarError::new(
                        SIDECAR_BINARY_UNSET,
                        format!(
                            "no calyx-mcp addon binary configured and {CALYX_MCP_BIN_ENV} is unset"
                        ),
                        "install the Calyx AI addon and point LITD_CALYX_MCP_BIN at calyx-mcp.exe",
                    ));
                }
            },
        };
        if !candidate.is_file() {
            return Err(CalyxSidecarError::new(
                SIDECAR_BINARY_MISSING,
                format!(
                    "calyx-mcp addon binary not found at {}",
                    candidate.display()
                ),
                "install the Calyx AI addon so calyx-mcp.exe exists at the configured path",
            ));
        }
        Ok(candidate)
    }
}

/// A single tool advertised by `tools/list`. Field-for-field the calyx-mcp `ToolDef`
/// wire shape, decoded independently of the calyx-mcp crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarToolDescriptor {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub use_when: String,
    // The MCP wire key is camelCase `inputSchema` (see calyx-mcp ToolDef). Decoding it
    // under the wrong key would silently null the schema -- exactly the divergence the
    // parity FSV caught.
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
}

/// A decoded JSON-RPC response frame from the sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<SidecarJsonRpcError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarJsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Single-source-of-truth readback for Calyx AI addon presence over the sidecar.
///
/// This is a total function of the environment: it NEVER errors. An absent or
/// unspawnable addon becomes `available = false` with a reason code, so callers
/// grey out AI features instead of failing (#701/#1790 ward-off-by-vacuity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxSidecarAvailabilityReadback {
    pub schema: String,
    pub source_of_truth: String,
    pub available: bool,
    pub reason_code: String,
    pub reason: String,
    pub binary_path: Option<String>,
    pub tool_count: usize,
    pub tool_names: Vec<String>,
    /// Stated in the UI per #1790: with no addon there is no LLM generation, so the
    /// Ward gate has nothing to gate and gating-off is safe by vacuity.
    pub ward_off_by_vacuity: bool,
}

pub const SIDECAR_AVAILABILITY_SCHEMA: &str = "litd.editor.calyx_sidecar_availability.v1";
