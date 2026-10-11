//! MCP tool surface for the no-code PBR material editor (#862 / #915).
//!
//! The engine-is-an-MCP-server doctrine (#915): the egui inspector and these MCP
//! tools are two front-ends over ONE capability layer ([`crate::material_editor`]).
//! An agent authors and reads materials through the same `put_content` real
//! persist path a human uses in the GUI, so agent-authored materials round-trip
//! and lower identically.

use std::path::PathBuf;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::material_editor::{
    PbrMaterialAuthorReadback, PbrMaterialAuthorRequest, PbrMaterialReadback, author_pbr_material,
    read_pbr_material,
};

pub const AUTHOR_PBR_MATERIAL_TOOL: &str = "litd.editor.material.author_pbr_material";
pub const READ_PBR_MATERIAL_TOOL: &str = "litd.editor.material.read_pbr_material";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorPbrMaterialArgs {
    pub vault_path: PathBuf,
    pub request_id: String,
    pub requested_by: String,
    pub material: PbrMaterialAuthorRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadPbrMaterialArgs {
    pub vault_path: PathBuf,
    pub material_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpMaterialToolCallOutput {
    pub tool_name: String,
    pub source_of_truth: String,
    pub request_id: String,
    pub requested_by: String,
    pub result: Value,
    pub author_readback: Option<PbrMaterialAuthorReadback>,
    pub read_readback: Option<PbrMaterialReadback>,
}

/// Dispatch entry for the material MCP tool family.
pub fn call_mcp_material_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpMaterialToolCallOutput, EngineError> {
    match tool_name {
        AUTHOR_PBR_MATERIAL_TOOL => author_material(arguments),
        READ_PBR_MATERIAL_TOOL => read_material(arguments),
        other => Err(invalid_mcp_material(
            "tool_name",
            format!("{other} is not a registered material MCP tool"),
        )),
    }
}

fn author_material(arguments: Value) -> Result<McpMaterialToolCallOutput, EngineError> {
    let args: AuthorPbrMaterialArgs = parse_args(AUTHOR_PBR_MATERIAL_TOOL, arguments)?;
    validate_identity(&args.request_id, &args.requested_by)?;
    let readback = author_pbr_material(&args.vault_path, &args.material)?;
    Ok(McpMaterialToolCallOutput {
        tool_name: AUTHOR_PBR_MATERIAL_TOOL.to_string(),
        source_of_truth: "Calyx engine content vault (put_content + lowering)".to_string(),
        request_id: args.request_id,
        requested_by: args.requested_by,
        result: json!({
            "material_id": readback.material_id,
            "cx_id": readback.cx_id,
            "content_input_sha256": readback.content_input_sha256,
            "lowered_fingerprint": readback.lowered_fingerprint,
            "measurement_state": readback.measurement_state,
            "preview_uniform_fingerprint": readback.preview_uniform_fingerprint,
            "texture_mask": readback.texture_mask,
            "map_count": readback.map_count,
        }),
        author_readback: Some(readback),
        read_readback: None,
    })
}

fn read_material(arguments: Value) -> Result<McpMaterialToolCallOutput, EngineError> {
    let args: ReadPbrMaterialArgs = parse_args(READ_PBR_MATERIAL_TOOL, arguments)?;
    let readback = read_pbr_material(&args.vault_path, &args.material_id)?;
    Ok(McpMaterialToolCallOutput {
        tool_name: READ_PBR_MATERIAL_TOOL.to_string(),
        source_of_truth: "Calyx engine content vault (get_content_for_editor)".to_string(),
        request_id: String::new(),
        requested_by: String::new(),
        result: json!({
            "material_id": readback.material_id,
            "cx_id": readback.cx_id,
            "content_input_sha256": readback.content_input_sha256,
            "lowered_fingerprint": readback.lowered_fingerprint,
            "measurement_state": readback.measurement_state,
            "preview_uniform_fingerprint": readback.preview_uniform_fingerprint,
            "base_color": readback.base_color,
            "metallic": readback.metallic,
            "roughness": readback.roughness,
            "texture_mask": readback.texture_mask,
        }),
        author_readback: None,
        read_readback: Some(readback),
    })
}

fn validate_identity(request_id: &str, requested_by: &str) -> Result<(), EngineError> {
    if request_id.trim().is_empty() {
        return Err(invalid_mcp_material(
            "request_id",
            "request_id must be non-empty",
        ));
    }
    if requested_by.trim().is_empty() {
        return Err(invalid_mcp_material(
            "requested_by",
            "requested_by must be non-empty",
        ));
    }
    Ok(())
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_material(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

fn invalid_mcp_material(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::MaterialCatalogInvalidConfig {
            field: format!("editor.material_editor_mcp.{}", field.into()),
            reason: reason.into(),
        },
    )
}

#[cfg(test)]
#[path = "material_editor_mcp_tests.rs"]
mod tests;
