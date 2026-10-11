//! Lightweight editor MCP manifest and capability-layer model.
//!
//! This crate intentionally avoids the editor GUI/renderer dependency graph so
//! manifest and tool-surface checks can run quickly against real production
//! definitions.

mod error;
mod fingerprint;

#[cfg(feature = "calyx-sidecar")]
pub mod calyx_sidecar;

pub mod capabilities;
pub mod mcp_tools;
pub mod modules;

pub use error::EditorMcpError;
