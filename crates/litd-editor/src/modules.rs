//! Compatibility wrapper over the lightweight editor MCP/module manifest crate.

use litd_engine::EngineError;

pub use litd_editor_mcp::modules::{
    CalyxBinding, EditorModuleFamily, EditorModuleRebase, EditorModuleRebaseManifest,
    EditorModuleRebaseReport, EngineBinding, REQUIRED_MODULE_IDS,
};

pub fn default_module_rebase_manifest() -> Result<EditorModuleRebaseManifest, EngineError> {
    litd_editor_mcp::modules::default_module_rebase_manifest().map_err(map_mcp_error)
}

pub fn build_module_rebase_report(
    manifest: &EditorModuleRebaseManifest,
) -> Result<EditorModuleRebaseReport, EngineError> {
    litd_editor_mcp::modules::build_module_rebase_report(manifest).map_err(map_mcp_error)
}

pub fn validate_module_rebase_manifest(
    manifest: &EditorModuleRebaseManifest,
) -> Result<(), EngineError> {
    litd_editor_mcp::modules::validate_module_rebase_manifest(manifest).map_err(map_mcp_error)
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
