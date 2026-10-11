//! Compatibility wrapper over the lightweight editor MCP/capability manifest crate.

use litd_engine::EngineError;

pub use litd_editor_mcp::capabilities::{
    CapabilityLayerManifest, CapabilityLayerReport, CapabilityOperation, CapabilityRegistry,
    McpLoopbackServerManifest, McpToolManifest,
};
pub use litd_editor_mcp::modules::EditorModuleRebaseManifest;

pub fn default_capability_layer_manifest() -> Result<CapabilityLayerManifest, EngineError> {
    litd_editor_mcp::capabilities::default_capability_layer_manifest().map_err(map_mcp_error)
}

pub fn build_capability_layer_report(
    manifest: &CapabilityLayerManifest,
) -> Result<CapabilityLayerReport, EngineError> {
    litd_editor_mcp::capabilities::build_capability_layer_report(manifest).map_err(map_mcp_error)
}

pub fn validate_capability_layer_manifest(
    manifest: &CapabilityLayerManifest,
    modules: &EditorModuleRebaseManifest,
) -> Result<(), EngineError> {
    litd_editor_mcp::capabilities::validate_capability_layer_manifest(manifest, modules)
        .map_err(map_mcp_error)
}

fn map_mcp_error(error: litd_editor_mcp::EditorMcpError) -> EngineError {
    match error {
        litd_editor_mcp::EditorMcpError::InvalidConfig { field, reason } => {
            EngineError::UiRenderInvalidConfig { field, reason }
        }
        litd_editor_mcp::EditorMcpError::SerializationFailed { format, reason } => {
            EngineError::SerializationFailed { format, reason }
        }
    }
}
