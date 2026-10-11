use super::{McpDomainTool, McpToolDomainGroup, McpToolKind};

/// Cohesive descriptor for one MCP tool-domain catalog entry (#1545). Groups the
/// ten fields that `domain()` previously took positionally so call sites read as
/// named fields and the builder stays under the clippy argument threshold.
pub(super) struct DomainSpec<'a> {
    pub domain_id: &'a str,
    pub title: &'a str,
    pub child_issue: u32,
    pub source_issues: &'a [u32],
    pub capability_namespaces: &'a [&'a str],
    pub gui_surface: &'a str,
    pub mcp_prefixes: &'a [&'a str],
    pub default_readback_tool_name: &'a str,
    pub readme_refs: &'a [&'a str],
    pub tools: Vec<McpDomainTool>,
}

pub(super) fn domain(spec: DomainSpec) -> McpToolDomainGroup {
    let DomainSpec {
        domain_id,
        title,
        child_issue,
        source_issues,
        capability_namespaces,
        gui_surface,
        mcp_prefixes,
        default_readback_tool_name,
        readme_refs,
        tools,
    } = spec;
    McpToolDomainGroup {
        domain_id: domain_id.to_string(),
        title: title.to_string(),
        child_issue,
        source_issues: source_issues.to_vec(),
        capability_namespaces: strings(capability_namespaces),
        gui_surface: gui_surface.to_string(),
        mcp_prefixes: strings(mcp_prefixes),
        default_readback_tool_name: default_readback_tool_name.to_string(),
        readme_refs: strings(readme_refs),
        tools,
        status: "rebased_to_rust_engine_calyx_mcp".to_string(),
    }
}

pub(super) fn mut_tool(
    operation_id: &str,
    title: &str,
    gui_route: &str,
    readback: &str,
) -> McpDomainTool {
    tool(
        operation_id,
        McpToolKind::Mutation,
        title,
        gui_route,
        true,
        readback,
    )
}

pub(super) fn read_tool(operation_id: &str, title: &str, gui_route: &str) -> McpDomainTool {
    let tool_name = format!("litd.{operation_id}");
    tool(
        operation_id,
        McpToolKind::Readback,
        title,
        gui_route,
        false,
        &tool_name,
    )
}

fn tool(
    operation_id: &str,
    kind: McpToolKind,
    title: &str,
    gui_route: &str,
    mutates_state: bool,
    readback: &str,
) -> McpDomainTool {
    McpDomainTool {
        tool_name: format!("litd.{operation_id}"),
        operation_id: operation_id.to_string(),
        kind,
        title: title.to_string(),
        gui_route: gui_route.to_string(),
        input_schema_ref: format!("schema://mcp/{operation_id}/input.v1"),
        output_schema_ref: format!("schema://mcp/{operation_id}/output.v1"),
        mutates_state,
        requires_lowering: mutates_state,
        requires_ward: mutates_state,
        records_ledger: true,
        returns_structured_content: true,
        fsv_readback_tool_name: readback.to_string(),
        local_only: true,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}
