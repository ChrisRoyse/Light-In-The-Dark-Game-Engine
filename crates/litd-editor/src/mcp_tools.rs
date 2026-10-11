//! Compatibility wrapper over the lightweight editor MCP tool-surface crate.

use litd_engine::EngineError;
use serde_json::Value;

pub use litd_editor_mcp::capabilities::CapabilityLayerManifest;
pub use litd_editor_mcp::mcp_tools::{
    EngineCalyxMcpCallReadback, EngineCalyxMcpSurfaceManifest, EngineCalyxMcpSurfaceReport,
    EngineCalyxMcpToolBridge, McpDomainTool, McpDomainToolsetManifest, McpDomainToolsetReport,
    McpToolDomainGroup, McpToolKind, REQUIRED_CHILD_ISSUES, REQUIRED_DOMAIN_IDS,
    STRUCTURED_INTELLIGENCE_MCP_ISSUE, StructuredIntelligenceMcpFamily,
    StructuredIntelligenceMcpSurfaceManifest, StructuredIntelligenceMcpSurfaceReport,
};

pub fn default_mcp_domain_toolset_manifest() -> Result<McpDomainToolsetManifest, EngineError> {
    litd_editor_mcp::mcp_tools::default_mcp_domain_toolset_manifest().map_err(map_mcp_error)
}

pub fn build_mcp_domain_toolset_report(
    manifest: &McpDomainToolsetManifest,
) -> Result<McpDomainToolsetReport, EngineError> {
    litd_editor_mcp::mcp_tools::build_mcp_domain_toolset_report(manifest).map_err(map_mcp_error)
}

pub fn validate_mcp_domain_toolset_manifest(
    manifest: &McpDomainToolsetManifest,
    capability_layer: &CapabilityLayerManifest,
) -> Result<(), EngineError> {
    litd_editor_mcp::mcp_tools::validate_mcp_domain_toolset_manifest(manifest, capability_layer)
        .map_err(map_mcp_error)
}

pub fn default_engine_calyx_mcp_surface_manifest()
-> Result<EngineCalyxMcpSurfaceManifest, EngineError> {
    litd_editor_mcp::mcp_tools::default_engine_calyx_mcp_surface_manifest().map_err(map_mcp_error)
}

pub fn build_engine_calyx_mcp_surface_report(
    manifest: &EngineCalyxMcpSurfaceManifest,
) -> Result<EngineCalyxMcpSurfaceReport, EngineError> {
    litd_editor_mcp::mcp_tools::build_engine_calyx_mcp_surface_report(manifest)
        .map_err(map_mcp_error)
}

pub fn validate_engine_calyx_mcp_surface_manifest(
    manifest: &EngineCalyxMcpSurfaceManifest,
) -> Result<(), EngineError> {
    litd_editor_mcp::mcp_tools::validate_engine_calyx_mcp_surface_manifest(manifest)
        .map_err(map_mcp_error)
}

pub fn expected_calyx_mcp_tool_names() -> Result<Vec<String>, EngineError> {
    litd_editor_mcp::mcp_tools::expected_calyx_mcp_tool_names().map_err(map_mcp_error)
}

pub fn calyx_mcp_tool_requires_authn(tool_name: &str) -> Result<bool, EngineError> {
    litd_editor_mcp::mcp_tools::calyx_mcp_tool_requires_authn(tool_name).map_err(map_mcp_error)
}

pub fn default_structured_intelligence_mcp_surface_manifest()
-> Result<StructuredIntelligenceMcpSurfaceManifest, EngineError> {
    litd_editor_mcp::mcp_tools::default_structured_intelligence_mcp_surface_manifest()
        .map_err(map_mcp_error)
}

pub fn build_structured_intelligence_mcp_surface_report(
    manifest: &StructuredIntelligenceMcpSurfaceManifest,
) -> Result<StructuredIntelligenceMcpSurfaceReport, EngineError> {
    litd_editor_mcp::mcp_tools::build_structured_intelligence_mcp_surface_report(manifest)
        .map_err(map_mcp_error)
}

pub fn validate_structured_intelligence_mcp_surface_manifest(
    manifest: &StructuredIntelligenceMcpSurfaceManifest,
) -> Result<(), EngineError> {
    litd_editor_mcp::mcp_tools::validate_structured_intelligence_mcp_surface_manifest(manifest)
        .map_err(map_mcp_error)
}

pub fn call_engine_calyx_mcp_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<calyx_mcp::JsonRpcResponse, EngineError> {
    litd_editor_mcp::mcp_tools::call_engine_calyx_mcp_tool(tool_name, arguments)
        .map_err(map_mcp_error)
}

pub fn readback_engine_calyx_mcp_tool_call(
    tool_name: &str,
    arguments: Value,
) -> Result<EngineCalyxMcpCallReadback, EngineError> {
    litd_editor_mcp::mcp_tools::readback_engine_calyx_mcp_tool_call(tool_name, arguments)
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

#[cfg(test)]
#[path = "mcp_tools_tests.rs"]
mod tests;
