use std::collections::BTreeSet;

use litd_engine::EngineError;

use crate::editor_commands::{COMMAND_MODERATION_SUBMIT_REPORT, default_editor_command_registry};
use crate::history::stable_fingerprint;
use crate::mcp_project_tools::{
    MODERATION_SAFETY_TOOL, MULTIPLAYER_SESSION_TOOL, SERVER_BROWSER_TOOL, SOCIAL_GRAPH_TOOL,
};

use super::structured_capability_coverage_support::invalid;
use super::{
    StructuredCapabilitySurfaceDeclaration, StructuredCapabilitySurfaceExemption,
    StructuredCapabilitySurfaceParityReport,
};

pub(super) fn build_surface_parity_report()
-> Result<StructuredCapabilitySurfaceParityReport, EngineError> {
    let registry = crate::mcp_tool_registry::default_mcp_tool_registry()?;
    let moderation_command_registered =
        default_editor_command_registry()?.contains(COMMAND_MODERATION_SUBMIT_REPORT);
    if !moderation_command_registered {
        return invalid(
            "surface_parity.moderation_gui",
            "moderation GUI command must be registered before #1467 parity can pass",
        );
    }

    let mut declarations = registry
        .entries
        .iter()
        .map(|entry| {
            let mut gui_route = Some(entry.tool.gui_route.clone());
            let exemption = surface_exemption(entry.tool.tool_name.as_str());
            if exemption.is_some() {
                gui_route = None;
            }
            if entry.tool.tool_name == MODERATION_SAFETY_TOOL {
                gui_route = Some(format!(
                    "egui://editor.commands/{COMMAND_MODERATION_SUBMIT_REPORT}"
                ));
            }
            StructuredCapabilitySurfaceDeclaration {
                capability_id: entry.tool.operation_id.clone(),
                domain_id: entry.domain_id.clone(),
                source_issue: registry
                    .manifest
                    .domains
                    .iter()
                    .find(|domain| domain.domain_id == entry.domain_id)
                    .map(|domain| domain.child_issue)
                    .unwrap_or_default(),
                mcp_tool_name: Some(entry.tool.tool_name.clone()),
                gui_route,
                readback_tool_name: entry.tool.fsv_readback_tool_name.clone(),
                evidence_refs: surface_evidence_refs(entry.tool.tool_name.as_str()),
                exemption,
            }
        })
        .collect::<Vec<_>>();
    declarations.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));
    let missing_mcp_surface_declarations = missing_mcp_surface_declarations(&declarations)?;
    let undeclared_single_surface_capabilities =
        undeclared_single_surface_capabilities(&declarations);
    let both_surface_count = declarations
        .iter()
        .filter(|item| item.gui_route.is_some() && item.mcp_tool_name.is_some())
        .count();
    let documented_exemption_count = declarations
        .iter()
        .filter(|item| item.exemption.is_some())
        .count();
    let mut report = StructuredCapabilitySurfaceParityReport {
        issue: 1467,
        declaration_count: declarations.len(),
        both_surface_count,
        documented_exemption_count,
        missing_mcp_surface_declarations,
        undeclared_single_surface_capabilities,
        declarations,
        fingerprint: String::new(),
    };
    report.fingerprint = stable_fingerprint(&(
        &report.declarations,
        &report.missing_mcp_surface_declarations,
        &report.undeclared_single_surface_capabilities,
    ))?;
    validate_surface_parity_report(&report)?;
    Ok(report)
}

fn surface_exemption(tool_name: &str) -> Option<StructuredCapabilitySurfaceExemption> {
    match tool_name {
        MULTIPLAYER_SESSION_TOOL => Some(surface_deferral(
            "project multiplayer session GUI panel is deferred; MCP remains the guarded source-of-truth mutation path",
        )),
        SOCIAL_GRAPH_TOOL => Some(surface_deferral(
            "project social graph GUI panel is deferred; MCP remains the guarded source-of-truth mutation path",
        )),
        SERVER_BROWSER_TOOL => Some(surface_deferral(
            "project server browser GUI panel is deferred; MCP remains the guarded source-of-truth mutation path",
        )),
        _ => None,
    }
}

fn surface_deferral(reason: &str) -> StructuredCapabilitySurfaceExemption {
    StructuredCapabilitySurfaceExemption {
        issue: 1467,
        decision: "documented_gui_deferral".to_string(),
        reason: reason.to_string(),
    }
}

fn surface_evidence_refs(tool_name: &str) -> Vec<String> {
    match tool_name {
        MODERATION_SAFETY_TOOL => vec![
            "issue:#1467".to_string(),
            "gui:editor.project.moderation.submit_report".to_string(),
            "mcp:litd.editor.project.manage_moderation_safety".to_string(),
        ],
        MULTIPLAYER_SESSION_TOOL | SOCIAL_GRAPH_TOOL | SERVER_BROWSER_TOOL => vec![
            "issue:#1467".to_string(),
            "decision:explicit_gui_deferral".to_string(),
        ],
        _ => vec![
            "mcp:live-registry".to_string(),
            "gui:catalog-route".to_string(),
        ],
    }
}

fn missing_mcp_surface_declarations(
    declarations: &[StructuredCapabilitySurfaceDeclaration],
) -> Result<Vec<String>, EngineError> {
    let declared = declarations
        .iter()
        .filter_map(|item| item.mcp_tool_name.as_deref())
        .collect::<BTreeSet<_>>();
    Ok(crate::mcp_tool_registry::implemented_mcp_tool_names()
        .into_iter()
        .filter(|tool| !declared.contains(tool.as_str()))
        .collect())
}

fn undeclared_single_surface_capabilities(
    declarations: &[StructuredCapabilitySurfaceDeclaration],
) -> Vec<String> {
    declarations
        .iter()
        .filter(|item| {
            (item.gui_route.is_some() ^ item.mcp_tool_name.is_some()) && item.exemption.is_none()
        })
        .map(|item| item.capability_id.clone())
        .collect()
}

pub(super) fn validate_surface_parity_report(
    report: &StructuredCapabilitySurfaceParityReport,
) -> Result<(), EngineError> {
    if report.issue != 1467
        || report.declaration_count != report.declarations.len()
        || report.fingerprint.trim().is_empty()
    {
        return invalid(
            "surface_parity",
            "#1467 surface parity report binding drifted",
        );
    }
    if !report.missing_mcp_surface_declarations.is_empty()
        || !report.undeclared_single_surface_capabilities.is_empty()
    {
        return invalid(
            "surface_parity",
            "single-surface capabilities require explicit declarations or exemptions",
        );
    }
    let expected_both = report
        .declarations
        .iter()
        .filter(|item| item.gui_route.is_some() && item.mcp_tool_name.is_some())
        .count();
    let expected_exemptions = report
        .declarations
        .iter()
        .filter(|item| item.exemption.is_some())
        .count();
    if report.both_surface_count != expected_both
        || report.documented_exemption_count != expected_exemptions
    {
        return invalid(
            "surface_parity.counts",
            "surface parity counts must match declarations",
        );
    }
    let implemented = crate::mcp_tool_registry::implemented_mcp_tool_names()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut capabilities = BTreeSet::new();
    for declaration in &report.declarations {
        validate_surface_declaration(declaration, &implemented)?;
        if !capabilities.insert(declaration.capability_id.as_str()) {
            return invalid(
                "surface_parity.capability_id",
                format!("duplicate {}", declaration.capability_id),
            );
        }
    }
    Ok(())
}

fn validate_surface_declaration(
    declaration: &StructuredCapabilitySurfaceDeclaration,
    implemented: &BTreeSet<String>,
) -> Result<(), EngineError> {
    if declaration.capability_id.trim().is_empty()
        || declaration.domain_id.trim().is_empty()
        || declaration.source_issue == 0
        || declaration.readback_tool_name.trim().is_empty()
        || declaration.evidence_refs.is_empty()
    {
        return invalid(
            "surface_parity.declaration",
            "surface declarations require ids, issue, readback, and evidence",
        );
    }
    if let Some(tool) = &declaration.mcp_tool_name
        && !implemented.contains(tool)
    {
        return invalid(
            "surface_parity.mcp_tool",
            format!("{tool} is not implemented by the MCP dispatcher"),
        );
    }
    if let Some(route) = &declaration.gui_route
        && !route.starts_with("egui://editor.")
    {
        return invalid(
            "surface_parity.gui_route",
            format!("{route} is not an editor GUI route"),
        );
    }
    if declaration.gui_route.is_none() && declaration.mcp_tool_name.is_none() {
        return invalid(
            "surface_parity.declaration",
            format!("{} has no GUI or MCP surface", declaration.capability_id),
        );
    }
    if (declaration.gui_route.is_some() ^ declaration.mcp_tool_name.is_some())
        && declaration.exemption.is_none()
    {
        return invalid(
            "surface_parity.exemption",
            format!("{} needs an explicit exemption", declaration.capability_id),
        );
    }
    if let Some(exemption) = &declaration.exemption
        && (exemption.issue != 1467
            || exemption.decision.trim().is_empty()
            || exemption.reason.trim().is_empty())
    {
        return invalid(
            "surface_parity.exemption",
            "surface exemptions must bind to #1467 with a decision and reason",
        );
    }
    Ok(())
}
