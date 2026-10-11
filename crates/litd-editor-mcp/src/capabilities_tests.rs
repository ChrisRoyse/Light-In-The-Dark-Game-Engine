use super::*;

#[test]
fn default_capability_layer_maps_modules_to_gui_and_mcp_tools() {
    let manifest = default_capability_layer_manifest().unwrap();
    let report = build_capability_layer_report(&manifest).unwrap();

    assert_eq!(report.parent_issue, 1144);
    assert_eq!(report.root_mcp_epic, 1143);
    assert_eq!(report.operation_count, 9);
    assert_eq!(report.gui_bound_count, 9);
    assert_eq!(report.mcp_tool_count, 9);
    assert_eq!(report.readback_tool_count, 9);
    assert_eq!(report.calyx_backed_count, 9);
    assert_eq!(report.lowering_boundary_count, 9);
    assert_eq!(report.fsv_readback_required_count, 9);
    assert_eq!(report.module_coverage_count, 9);
    assert_eq!(
        report.source_issues,
        vec![801, 802, 803, 804, 805, 886, 933, 972, 1048]
    );
}

#[test]
fn loopback_server_tools_mirror_operation_metadata() {
    let manifest = default_capability_layer_manifest().unwrap();

    assert_eq!(manifest.loopback_server.bind_host, "127.0.0.1");
    assert_eq!(manifest.loopback_server.tool_count, 9);
    for operation in &manifest.registry.operations {
        let tool = manifest
            .loopback_server
            .tools
            .iter()
            .find(|tool| tool.operation_id == operation.operation_id)
            .unwrap();
        assert_eq!(tool.tool_name, operation.mcp_tool_name);
        assert_eq!(tool.input_schema_ref, operation.input_schema_ref);
        assert_eq!(tool.output_schema_ref, operation.output_schema_ref);
        assert_eq!(tool.readback_tool_name, operation.readback_tool_name);
    }
}

#[test]
fn non_loopback_server_fails_loud() {
    let modules = default_module_rebase_manifest().unwrap();
    let mut manifest = default_capability_layer_manifest().unwrap();
    manifest.loopback_server.bind_host = "0.0.0.0".to_string();

    match validate_capability_layer_manifest(&manifest, &modules) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.capabilities.server.bind_host");
            assert!(reason.contains("loopback"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn duplicate_operation_fails_loud() {
    let modules = default_module_rebase_manifest().unwrap();
    let mut manifest = default_capability_layer_manifest().unwrap();
    manifest.registry.operations[1].operation_id =
        manifest.registry.operations[0].operation_id.clone();

    match validate_capability_layer_manifest(&manifest, &modules) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.capabilities.registry.operation_id");
            assert!(reason.contains("duplicate"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn gui_and_mcp_routes_must_share_operation_roots() {
    let modules = default_module_rebase_manifest().unwrap();
    let mut manifest = default_capability_layer_manifest().unwrap();
    manifest.registry.operations[0].gui_route = "other://editor.world/apply".to_string();

    match validate_capability_layer_manifest(&manifest, &modules) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.capabilities.operation.route");
            assert!(reason.contains("GUI and MCP"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn mcp_tool_mapping_must_mirror_operation() {
    let modules = default_module_rebase_manifest().unwrap();
    let mut manifest = default_capability_layer_manifest().unwrap();
    manifest.loopback_server.tools[0].tool_name = "litd.editor.world.wrong".to_string();

    match validate_capability_layer_manifest(&manifest, &modules) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.capabilities.server.tool_mapping");
            assert!(reason.contains("mirror"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}
