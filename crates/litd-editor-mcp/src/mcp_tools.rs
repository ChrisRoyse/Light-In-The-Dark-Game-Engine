//! MCP domain tool groups over the shared editor capability layer.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::EditorMcpError;

use crate::{
    capabilities::{CapabilityLayerManifest, default_capability_layer_manifest},
    fingerprint::stable_fingerprint,
};

#[path = "mcp_tools_builders.rs"]
mod mcp_tools_builders;
#[cfg(feature = "calyx-surface")]
#[path = "mcp_tools_calyx_surface.rs"]
mod mcp_tools_calyx_surface;
#[path = "mcp_tools_catalog.rs"]
mod mcp_tools_catalog;
#[cfg(feature = "calyx-surface")]
#[path = "mcp_tools_structured_intelligence.rs"]
mod mcp_tools_structured_intelligence;

#[cfg(feature = "calyx-surface")]
pub use mcp_tools_calyx_surface::{
    EngineCalyxMcpCallReadback, EngineCalyxMcpSurfaceManifest, EngineCalyxMcpSurfaceReport,
    EngineCalyxMcpToolBridge, build_engine_calyx_mcp_surface_report, calyx_mcp_tool_def,
    calyx_mcp_tool_requires_authn, default_engine_calyx_mcp_surface_manifest,
    expected_calyx_mcp_tool_names, validate_engine_calyx_mcp_surface_manifest,
};
#[cfg(feature = "calyx-live-calls")]
pub use mcp_tools_calyx_surface::{
    call_engine_calyx_mcp_tool, readback_engine_calyx_mcp_tool_call,
};
#[cfg(feature = "calyx-surface")]
pub use mcp_tools_structured_intelligence::{
    STRUCTURED_INTELLIGENCE_MCP_ISSUE, StructuredIntelligenceMcpFamily,
    StructuredIntelligenceMcpSurfaceManifest, StructuredIntelligenceMcpSurfaceReport,
    build_structured_intelligence_mcp_surface_report,
    default_structured_intelligence_mcp_surface_manifest,
    validate_structured_intelligence_mcp_surface_manifest,
};

pub const REQUIRED_DOMAIN_IDS: [&str; 12] = [
    "terrain_world",
    "object_game_data",
    "trigger_runtime",
    "visual_story",
    "vfx_particles",
    "render_materials",
    "in_game_agents",
    "asset_pipeline",
    "editor_session",
    "project_playtest",
    "fsv_introspection",
    "agent_harness",
];

pub const REQUIRED_CHILD_ISSUES: [u32; 12] =
    [918, 919, 920, 921, 922, 923, 924, 925, 1467, 926, 927, 1178];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpToolKind {
    Mutation,
    Readback,
    Validation,
    Run,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpDomainToolsetManifest {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub root_mcp_epic: u32,
    pub source_mcp_epic: u32,
    pub capability_layer_issue: u32,
    pub capability_layer_id: String,
    pub transport: String,
    pub domains: Vec<McpToolDomainGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolDomainGroup {
    pub domain_id: String,
    pub title: String,
    pub child_issue: u32,
    pub source_issues: Vec<u32>,
    pub capability_namespaces: Vec<String>,
    pub gui_surface: String,
    pub mcp_prefixes: Vec<String>,
    pub default_readback_tool_name: String,
    /// Documentation pointers into `docs/readme/*.png` for composition/quality
    /// context ONLY. These are never a behavioral spec: HUD elements visible in
    /// legacy composites (notably `ED12-play-topdown.png`'s minimap, quest
    /// trackers, and player-position marker) are OBSOLETE and MUST NOT be
    /// implemented in packaged player runtime per ADR-944 / #944 / #1510.
    pub readme_refs: Vec<String>,
    pub tools: Vec<McpDomainTool>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpDomainTool {
    pub tool_name: String,
    pub operation_id: String,
    pub kind: McpToolKind,
    pub title: String,
    pub gui_route: String,
    pub input_schema_ref: String,
    pub output_schema_ref: String,
    pub mutates_state: bool,
    pub requires_lowering: bool,
    pub requires_ward: bool,
    pub records_ledger: bool,
    pub returns_structured_content: bool,
    pub fsv_readback_tool_name: String,
    pub local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpDomainToolsetReport {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub root_mcp_epic: u32,
    pub source_mcp_epic: u32,
    pub capability_layer_issue: u32,
    pub domain_count: usize,
    pub tool_count: usize,
    pub mutating_tool_count: usize,
    pub readback_tool_count: usize,
    pub fsv_tool_count: usize,
    pub child_issues: Vec<u32>,
    pub source_issues: Vec<u32>,
    pub readme_refs: Vec<String>,
    pub local_only_count: usize,
    pub structured_result_count: usize,
    pub lowering_guarded_mutation_count: usize,
    pub ward_guarded_mutation_count: usize,
    pub ledger_audited_count: usize,
    pub shared_capability_coverage_count: usize,
    pub shared_capability_fingerprint: String,
    pub toolset_fingerprint: String,
}

pub fn default_mcp_domain_toolset_manifest() -> Result<McpDomainToolsetManifest, EditorMcpError> {
    let capability_layer = default_capability_layer_manifest()?;
    let manifest = McpDomainToolsetManifest {
        schema_version: 1,
        parent_issue: 1145,
        root_mcp_epic: 1143,
        source_mcp_epic: 916,
        capability_layer_issue: capability_layer.parent_issue,
        capability_layer_id: capability_layer.registry.shared_layer_id.clone(),
        transport: "loopback_jsonrpc_mcp".to_string(),
        domains: mcp_tools_catalog::default_domains(),
    };
    validate_mcp_domain_toolset_manifest(&manifest, &capability_layer)?;
    Ok(manifest)
}

pub fn build_mcp_domain_toolset_report(
    manifest: &McpDomainToolsetManifest,
) -> Result<McpDomainToolsetReport, EditorMcpError> {
    let capability_layer = default_capability_layer_manifest()?;
    validate_mcp_domain_toolset_manifest(manifest, &capability_layer)?;
    let all_tools = manifest
        .domains
        .iter()
        .flat_map(|domain| domain.tools.iter())
        .collect::<Vec<_>>();
    let readme_refs = manifest
        .domains
        .iter()
        .flat_map(|domain| domain.readme_refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let shared_coverage = shared_capability_coverage_count(manifest, &capability_layer);
    Ok(McpDomainToolsetReport {
        schema_version: manifest.schema_version,
        parent_issue: manifest.parent_issue,
        root_mcp_epic: manifest.root_mcp_epic,
        source_mcp_epic: manifest.source_mcp_epic,
        capability_layer_issue: manifest.capability_layer_issue,
        domain_count: manifest.domains.len(),
        tool_count: all_tools.len(),
        mutating_tool_count: all_tools.iter().filter(|tool| tool.mutates_state).count(),
        readback_tool_count: all_tools
            .iter()
            .filter(|tool| tool.kind == McpToolKind::Readback)
            .count(),
        fsv_tool_count: manifest
            .domains
            .iter()
            .find(|domain| domain.domain_id == "fsv_introspection")
            .map(|domain| domain.tools.len())
            .unwrap_or_default(),
        child_issues: manifest
            .domains
            .iter()
            .map(|domain| domain.child_issue)
            .collect(),
        source_issues: manifest
            .domains
            .iter()
            .flat_map(|domain| domain.source_issues.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        readme_refs,
        local_only_count: all_tools.iter().filter(|tool| tool.local_only).count(),
        structured_result_count: all_tools
            .iter()
            .filter(|tool| tool.returns_structured_content)
            .count(),
        lowering_guarded_mutation_count: all_tools
            .iter()
            .filter(|tool| tool.mutates_state && tool.requires_lowering)
            .count(),
        ward_guarded_mutation_count: all_tools
            .iter()
            .filter(|tool| tool.mutates_state && tool.requires_ward)
            .count(),
        ledger_audited_count: all_tools.iter().filter(|tool| tool.records_ledger).count(),
        shared_capability_coverage_count: shared_coverage,
        shared_capability_fingerprint: stable_fingerprint(&capability_layer.registry)?,
        toolset_fingerprint: stable_fingerprint(manifest)?,
    })
}

pub fn validate_mcp_domain_toolset_manifest(
    manifest: &McpDomainToolsetManifest,
    capability_layer: &CapabilityLayerManifest,
) -> Result<(), EditorMcpError> {
    if manifest.schema_version != 1 {
        return invalid_mcp_tools("schema_version", "expected schema version 1");
    }
    if manifest.parent_issue != 1145
        || manifest.root_mcp_epic != 1143
        || manifest.source_mcp_epic != 916
    {
        return invalid_mcp_tools("issue_binding", "manifest must bind #1145 under #1143/#916");
    }
    if manifest.capability_layer_issue != capability_layer.parent_issue
        || manifest.capability_layer_id != capability_layer.registry.shared_layer_id
    {
        return invalid_mcp_tools(
            "capability_layer",
            "manifest must sit over #1144 shared layer",
        );
    }
    if manifest.transport != "loopback_jsonrpc_mcp" {
        return invalid_mcp_tools("transport", "MCP tools must use the loopback MCP transport");
    }
    validate_domain_set(manifest)?;
    validate_tools(manifest)?;
    validate_shared_capability_coverage(manifest, capability_layer)
}

fn validate_domain_set(manifest: &McpDomainToolsetManifest) -> Result<(), EditorMcpError> {
    let mut by_id = BTreeMap::new();
    let mut child_issues = BTreeSet::new();
    for domain in &manifest.domains {
        if by_id.insert(domain.domain_id.as_str(), domain).is_some() {
            return invalid_mcp_tools("domain.id", format!("duplicate {}", domain.domain_id));
        }
        if !child_issues.insert(domain.child_issue) {
            return invalid_mcp_tools(
                "domain.child_issue",
                format!("duplicate #{}", domain.child_issue),
            );
        }
        validate_domain(domain)?;
    }
    for required in REQUIRED_DOMAIN_IDS {
        if !by_id.contains_key(required) {
            return invalid_mcp_tools("domain.required", format!("missing {required}"));
        }
    }
    for required in REQUIRED_CHILD_ISSUES {
        if !child_issues.contains(&required) {
            return invalid_mcp_tools("domain.child_issue", format!("missing #{required}"));
        }
    }
    Ok(())
}

fn validate_domain(domain: &McpToolDomainGroup) -> Result<(), EditorMcpError> {
    if domain.title.trim().is_empty()
        || domain.capability_namespaces.is_empty()
        || domain.gui_surface.trim().is_empty()
        || domain.mcp_prefixes.is_empty()
        || domain.default_readback_tool_name.trim().is_empty()
        || domain.tools.is_empty()
    {
        return invalid_mcp_tools("domain.required_fields", "domain metadata is required");
    }
    if domain.status != "rebased_to_rust_engine_calyx_mcp" {
        return invalid_mcp_tools("domain.status", "domain must be marked rebased");
    }
    let domain_text = format!(
        "{} {} {} {}",
        domain.title,
        domain.gui_surface,
        domain.capability_namespaces.join(" "),
        domain.mcp_prefixes.join(" ")
    );
    reject_legacy_text("domain.legacy", &domain_text)?;
    if !domain.gui_surface.starts_with("egui://editor.") {
        return invalid_mcp_tools("domain.gui_surface", "GUI surface must be an editor route");
    }
    if domain
        .readme_refs
        .iter()
        .any(|path| !path.starts_with("docs/readme/") || !path.ends_with(".png"))
    {
        return invalid_mcp_tools("domain.readme_refs", "README refs must be docs/readme PNGs");
    }
    if !domain
        .tools
        .iter()
        .any(|tool| tool.kind == McpToolKind::Readback)
    {
        return invalid_mcp_tools("domain.readback", "each domain needs a readback tool");
    }
    Ok(())
}

fn validate_tools(manifest: &McpDomainToolsetManifest) -> Result<(), EditorMcpError> {
    let mut tools = BTreeMap::new();
    for domain in &manifest.domains {
        for tool in &domain.tools {
            if tools.insert(tool.tool_name.as_str(), tool).is_some() {
                return invalid_mcp_tools("tool.name", format!("duplicate {}", tool.tool_name));
            }
            validate_tool(domain, tool)?;
        }
    }
    for domain in &manifest.domains {
        for tool in &domain.tools {
            if tool.mutates_state && !tools.contains_key(tool.fsv_readback_tool_name.as_str()) {
                return invalid_mcp_tools(
                    "tool.readback",
                    format!("{} has no matching readback tool", tool.tool_name),
                );
            }
        }
    }
    Ok(())
}

fn validate_tool(domain: &McpToolDomainGroup, tool: &McpDomainTool) -> Result<(), EditorMcpError> {
    reject_legacy_text(
        "tool.legacy",
        &format!("{} {}", tool.operation_id, tool.title),
    )?;
    if !domain
        .mcp_prefixes
        .iter()
        .any(|prefix| tool.tool_name.starts_with(prefix))
        || !tool.tool_name.starts_with("litd.")
        || !(tool.operation_id.starts_with("editor.") || tool.operation_id.starts_with("engine."))
        || !tool.gui_route.starts_with("egui://editor.")
    {
        return invalid_mcp_tools(
            "tool.route",
            "tool name, operation id, and GUI route must share editor/engine roots",
        );
    }
    if tool.input_schema_ref.trim().is_empty() || tool.output_schema_ref.trim().is_empty() {
        return invalid_mcp_tools("tool.schema", "typed input/output schemas are required");
    }
    if !tool.local_only || !tool.returns_structured_content || !tool.records_ledger {
        return invalid_mcp_tools(
            "tool.safety",
            "tools must be local-only, structured, and ledger-audited",
        );
    }
    if tool.mutates_state
        && (!tool.requires_lowering
            || !tool.requires_ward
            || tool.fsv_readback_tool_name.trim().is_empty())
    {
        return invalid_mcp_tools(
            "tool.mutation_guard",
            "mutations require lowering, Ward, and readback",
        );
    }
    if !tool.mutates_state && tool.kind == McpToolKind::Mutation {
        return invalid_mcp_tools("tool.kind", "mutation tools must mutate state");
    }
    Ok(())
}

fn validate_shared_capability_coverage(
    manifest: &McpDomainToolsetManifest,
    capability_layer: &CapabilityLayerManifest,
) -> Result<(), EditorMcpError> {
    let covered = manifest
        .domains
        .iter()
        .flat_map(|domain| domain.capability_namespaces.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    for namespace in capability_layer
        .registry
        .operations
        .iter()
        .map(|operation| operation.capability_namespace.as_str())
    {
        if !covered.contains(namespace) {
            return invalid_mcp_tools(
                "capability_coverage",
                format!("missing shared capability namespace {namespace}"),
            );
        }
    }
    Ok(())
}

fn shared_capability_coverage_count(
    manifest: &McpDomainToolsetManifest,
    capability_layer: &CapabilityLayerManifest,
) -> usize {
    let covered = manifest
        .domains
        .iter()
        .flat_map(|domain| domain.capability_namespaces.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    capability_layer
        .registry
        .operations
        .iter()
        .filter(|operation| covered.contains(operation.capability_namespace.as_str()))
        .count()
}

fn reject_legacy_text(field: &str, text: &str) -> Result<(), EditorMcpError> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("g3n") || lower.contains("legacy go") {
        return invalid_mcp_tools(field, "legacy engine references are not allowed");
    }
    Ok(())
}

fn invalid_mcp_tools<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EditorMcpError> {
    Err(EditorMcpError::InvalidConfig {
        field: format!("editor.mcp_tools.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "mcp_tools_tests.rs"]
mod tests;
