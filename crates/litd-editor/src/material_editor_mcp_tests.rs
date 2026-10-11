use super::*;

#[test]
fn dispatch_rejects_unknown_tool() {
    let error = call_mcp_material_tool("litd.editor.material.bogus", json!({}));
    assert!(matches!(
        error,
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { .. }
        ))
    ));
}

#[test]
fn author_rejects_missing_arguments() {
    // Missing `material` field -> arg decode error before any persistence.
    let error = call_mcp_material_tool(
        AUTHOR_PBR_MATERIAL_TOOL,
        json!({
            "vault_path": "unused",
            "request_id": "req-1",
            "requested_by": "agent"
        }),
    );
    assert!(matches!(
        error,
        Err(EngineError::Rendering(litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { field, .. })) if field.contains("arguments")
    ));
}

#[test]
fn author_rejects_blank_identity() {
    // Well-formed material but empty request_id -> identity validation fails
    // before the vault is opened.
    let error = call_mcp_material_tool(
        AUTHOR_PBR_MATERIAL_TOOL,
        json!({
            "vault_path": "unused",
            "request_id": "",
            "requested_by": "agent",
            "material": {
                "material_id": "m1",
                "display_name": "M1",
                "author": "agent",
                "base_color": [0.8, 0.8, 0.8, 1.0],
                "metallic": 0.0,
                "roughness": 0.5,
                "normal_scale": 1.0,
                "occlusion": 1.0,
                "emissive": [0.0, 0.0, 0.0]
            }
        }),
    );
    assert!(matches!(
        error,
        Err(EngineError::Rendering(litd_engine::error::RenderingError::MaterialCatalogInvalidConfig { field, .. })) if field.contains("request_id")
    ));
}
