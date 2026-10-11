//! Shared editor capability layer and loopback MCP surface manifest.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::EditorMcpError;

use crate::{
    fingerprint::stable_fingerprint,
    modules::{EditorModuleRebaseManifest, default_module_rebase_manifest},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLayerManifest {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub root_mcp_epic: u32,
    pub doctrine_issue: u32,
    pub registry: CapabilityRegistry,
    pub loopback_server: McpLoopbackServerManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRegistry {
    pub source_manifest_issue: u32,
    pub shared_layer_id: String,
    pub operations: Vec<CapabilityOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityOperation {
    pub operation_id: String,
    pub source_module_id: String,
    pub source_issue: u32,
    pub capability_namespace: String,
    pub gui_route: String,
    pub mcp_tool_name: String,
    pub readback_tool_name: String,
    pub input_schema_ref: String,
    pub output_schema_ref: String,
    pub calyx_backed: bool,
    pub lowering_boundary: bool,
    pub fsv_readback_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpLoopbackServerManifest {
    pub server_name: String,
    pub bind_host: String,
    pub port_policy: String,
    pub protocol: String,
    pub auth_scope: String,
    pub tool_count: usize,
    pub tools: Vec<McpToolManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolManifest {
    pub tool_name: String,
    pub operation_id: String,
    pub input_schema_ref: String,
    pub output_schema_ref: String,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLayerReport {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub root_mcp_epic: u32,
    pub operation_count: usize,
    pub gui_bound_count: usize,
    pub mcp_tool_count: usize,
    pub readback_tool_count: usize,
    pub calyx_backed_count: usize,
    pub lowering_boundary_count: usize,
    pub fsv_readback_required_count: usize,
    pub module_coverage_count: usize,
    pub source_issues: Vec<u32>,
    pub loopback_bind_host: String,
    pub loopback_only: bool,
    pub registry_fingerprint: String,
}

pub fn default_capability_layer_manifest() -> Result<CapabilityLayerManifest, EditorMcpError> {
    let modules = default_module_rebase_manifest()?;
    let registry = build_capability_registry(&modules)?;
    let loopback_server = build_loopback_server_manifest(&registry)?;
    let manifest = CapabilityLayerManifest {
        schema_version: 1,
        parent_issue: 1144,
        root_mcp_epic: 1143,
        doctrine_issue: 915,
        registry,
        loopback_server,
    };
    validate_capability_layer_manifest(&manifest, &modules)?;
    Ok(manifest)
}

pub fn build_capability_layer_report(
    manifest: &CapabilityLayerManifest,
) -> Result<CapabilityLayerReport, EditorMcpError> {
    let modules = default_module_rebase_manifest()?;
    validate_capability_layer_manifest(manifest, &modules)?;
    let module_coverage_count = manifest
        .registry
        .operations
        .iter()
        .map(|operation| operation.source_module_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let source_issues = manifest
        .registry
        .operations
        .iter()
        .map(|operation| operation.source_issue)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(CapabilityLayerReport {
        schema_version: manifest.schema_version,
        parent_issue: manifest.parent_issue,
        root_mcp_epic: manifest.root_mcp_epic,
        operation_count: manifest.registry.operations.len(),
        gui_bound_count: manifest
            .registry
            .operations
            .iter()
            .filter(|operation| operation.gui_route.starts_with("egui://"))
            .count(),
        mcp_tool_count: manifest.loopback_server.tools.len(),
        readback_tool_count: manifest
            .registry
            .operations
            .iter()
            .map(|operation| operation.readback_tool_name.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        calyx_backed_count: manifest
            .registry
            .operations
            .iter()
            .filter(|operation| operation.calyx_backed)
            .count(),
        lowering_boundary_count: manifest
            .registry
            .operations
            .iter()
            .filter(|operation| operation.lowering_boundary)
            .count(),
        fsv_readback_required_count: manifest
            .registry
            .operations
            .iter()
            .filter(|operation| operation.fsv_readback_required)
            .count(),
        module_coverage_count,
        source_issues,
        loopback_bind_host: manifest.loopback_server.bind_host.clone(),
        loopback_only: is_loopback_host(&manifest.loopback_server.bind_host),
        registry_fingerprint: stable_fingerprint(&manifest.registry)?,
    })
}

pub fn validate_capability_layer_manifest(
    manifest: &CapabilityLayerManifest,
    modules: &EditorModuleRebaseManifest,
) -> Result<(), EditorMcpError> {
    if manifest.schema_version != 1 {
        return invalid_capability("schema_version", "expected schema version 1");
    }
    if manifest.parent_issue != 1144 || manifest.root_mcp_epic != 1143 {
        return invalid_capability("issue_binding", "manifest must bind #1144 under #1143");
    }
    validate_registry(&manifest.registry, modules)?;
    validate_loopback_server(&manifest.loopback_server, &manifest.registry)
}

fn build_capability_registry(
    modules: &EditorModuleRebaseManifest,
) -> Result<CapabilityRegistry, EditorMcpError> {
    let mut operations = Vec::with_capacity(modules.modules.len());
    for module in &modules.modules {
        let action = action_for_module(&module.id)?;
        operations.push(CapabilityOperation {
            operation_id: format!("{}.{}", module.capability_namespace, action),
            source_module_id: module.id.clone(),
            source_issue: module.source_issue,
            capability_namespace: module.capability_namespace.clone(),
            gui_route: format!("egui://{}/{}", module.capability_namespace, action),
            mcp_tool_name: format!("litd.{}.{}", module.capability_namespace, action),
            readback_tool_name: format!("litd.{}.readback", module.capability_namespace),
            input_schema_ref: format!("schema://{}/input.v1", module.capability_namespace),
            output_schema_ref: format!("schema://{}/output.v1", module.capability_namespace),
            calyx_backed: true,
            lowering_boundary: true,
            fsv_readback_required: true,
        });
    }
    let registry = CapabilityRegistry {
        source_manifest_issue: modules.parent_issue,
        shared_layer_id: modules.shared_capability_layer.clone(),
        operations,
    };
    validate_registry(&registry, modules)?;
    Ok(registry)
}

fn build_loopback_server_manifest(
    registry: &CapabilityRegistry,
) -> Result<McpLoopbackServerManifest, EditorMcpError> {
    let tools = registry
        .operations
        .iter()
        .map(|operation| McpToolManifest {
            tool_name: operation.mcp_tool_name.clone(),
            operation_id: operation.operation_id.clone(),
            input_schema_ref: operation.input_schema_ref.clone(),
            output_schema_ref: operation.output_schema_ref.clone(),
            readback_tool_name: operation.readback_tool_name.clone(),
        })
        .collect::<Vec<_>>();
    let manifest = McpLoopbackServerManifest {
        server_name: "litd-engine-editor-mcp".to_string(),
        bind_host: "127.0.0.1".to_string(),
        port_policy: "loopback_ephemeral".to_string(),
        protocol: "jsonrpc-2.0/mcp".to_string(),
        auth_scope: "local_loopback_only".to_string(),
        tool_count: tools.len(),
        tools,
    };
    validate_loopback_server(&manifest, registry)?;
    Ok(manifest)
}

fn validate_registry(
    registry: &CapabilityRegistry,
    modules: &EditorModuleRebaseManifest,
) -> Result<(), EditorMcpError> {
    if registry.shared_layer_id.trim().is_empty() || registry.operations.is_empty() {
        return invalid_capability("registry", "shared layer and operations are required");
    }
    let known_modules = modules
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut operation_ids = BTreeSet::new();
    let mut mcp_names = BTreeSet::new();
    for operation in &registry.operations {
        if !known_modules.contains(operation.source_module_id.as_str()) {
            return invalid_capability(
                "registry.source_module",
                format!("unknown source module {}", operation.source_module_id),
            );
        }
        if !operation_ids.insert(operation.operation_id.as_str()) {
            return invalid_capability(
                "registry.operation_id",
                format!("duplicate operation {}", operation.operation_id),
            );
        }
        if !mcp_names.insert(operation.mcp_tool_name.as_str()) {
            return invalid_capability(
                "registry.mcp_tool_name",
                format!("duplicate MCP tool {}", operation.mcp_tool_name),
            );
        }
        validate_operation(operation)?;
    }
    if operation_ids.len() < known_modules.len() {
        return invalid_capability("registry.coverage", "each module needs an operation");
    }
    Ok(())
}

fn validate_operation(operation: &CapabilityOperation) -> Result<(), EditorMcpError> {
    if !operation.operation_id.starts_with("editor.")
        || !operation.capability_namespace.starts_with("editor.")
        || !operation.gui_route.starts_with("egui://editor.")
        || !operation.mcp_tool_name.starts_with("litd.editor.")
        || !operation.readback_tool_name.starts_with("litd.editor.")
    {
        return invalid_capability(
            "operation.route",
            "operation must share editor GUI and MCP route roots",
        );
    }
    if operation.input_schema_ref.trim().is_empty() || operation.output_schema_ref.trim().is_empty()
    {
        return invalid_capability("operation.schema", "typed schemas are required");
    }
    if !operation.calyx_backed || !operation.lowering_boundary || !operation.fsv_readback_required {
        return invalid_capability(
            "operation.doctrine",
            "Calyx backing, lowering boundary, and FSV readback are required",
        );
    }
    Ok(())
}

fn validate_loopback_server(
    server: &McpLoopbackServerManifest,
    registry: &CapabilityRegistry,
) -> Result<(), EditorMcpError> {
    if !is_loopback_host(&server.bind_host) {
        return invalid_capability("server.bind_host", "MCP server must bind loopback only");
    }
    if server.tool_count != server.tools.len() || server.tools.len() != registry.operations.len() {
        return invalid_capability("server.tools", "MCP tool count must match registry");
    }
    let operations = registry
        .operations
        .iter()
        .map(|operation| (operation.operation_id.as_str(), operation))
        .collect::<BTreeMap<_, _>>();
    for tool in &server.tools {
        let Some(operation) = operations.get(tool.operation_id.as_str()) else {
            return invalid_capability(
                "server.tool_operation",
                format!("tool {} has no operation", tool.tool_name),
            );
        };
        if tool.tool_name != operation.mcp_tool_name
            || tool.input_schema_ref != operation.input_schema_ref
            || tool.output_schema_ref != operation.output_schema_ref
            || tool.readback_tool_name != operation.readback_tool_name
        {
            return invalid_capability(
                "server.tool_mapping",
                "MCP tool metadata must mirror the operation",
            );
        }
    }
    Ok(())
}

fn action_for_module(module_id: &str) -> Result<&'static str, EditorMcpError> {
    match module_id {
        "wc3.terrain_world" => Ok("apply_terrain_stamp"),
        "wc3.object_data" => Ok("upsert_object_data"),
        "wc3.trigger_eca" => Ok("upsert_trigger_graph"),
        "wc3.runtime_systems" => Ok("upsert_runtime_system"),
        "wc3.play_distribute" => Ok("start_play_session"),
        "story.visual_director" => Ok("upsert_story_beat"),
        "rpg.flagship_systems" => Ok("upsert_rpg_table"),
        "economy.scarcity_full_loot" => Ok("upsert_economy_rule"),
        "lore.worldbuilding" => Ok("upsert_lore_constellation"),
        other => invalid_capability("module.action", format!("missing action for {other}")),
    }
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

fn invalid_capability<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EditorMcpError> {
    Err(EditorMcpError::InvalidConfig {
        field: format!("editor.capabilities.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
