use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

#[path = "structured_capability_coverage_store.rs"]
mod structured_capability_coverage_store;
#[path = "structured_capability_coverage_support.rs"]
mod structured_capability_coverage_support;
#[path = "structured_capability_coverage_surface_parity.rs"]
mod structured_capability_coverage_surface_parity;

pub use structured_capability_coverage_store::{
    StructuredCapabilityCoverageWriteback, parse_structured_capability_coverage_audit,
    read_structured_capability_coverage_audit, write_structured_capability_coverage_audit,
};
use structured_capability_coverage_support::{invalid, required_subsystems, tool_specs};
use structured_capability_coverage_surface_parity::{
    build_surface_parity_report, validate_surface_parity_report,
};

pub const STRUCTURED_CAPABILITY_COVERAGE_SCHEMA: &str =
    "litd.structured_capability_coverage.audit.v1";
pub const STRUCTURED_CAPABILITY_COVERAGE_ISSUE: u32 = 1421;
pub const STRUCTURED_CAPABILITY_COVERAGE_PARENT_ISSUE: u32 = 1350;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalyxStructuredSubsystem {
    Aster,
    RegistryLenses,
    LoomDda,
    Assay,
    Lodestar,
    Ward,
    Oracle,
    Anneal,
    Sextant,
    Ledger,
    Forge,
    Mcp,
    LoweringValve,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredCapabilityToolUse {
    pub tool_name: String,
    pub primary_subsystem: CalyxStructuredSubsystem,
    pub supporting_subsystems: Vec<CalyxStructuredSubsystem>,
    pub requires_authn: bool,
    pub structured_use: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredCapabilitySubsystemUse {
    pub subsystem: CalyxStructuredSubsystem,
    pub tool_names: Vec<String>,
    pub structured_use: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredCapabilitySurfaceExemption {
    pub issue: u32,
    pub decision: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredCapabilitySurfaceDeclaration {
    pub capability_id: String,
    pub domain_id: String,
    pub source_issue: u32,
    pub mcp_tool_name: Option<String>,
    pub gui_route: Option<String>,
    pub readback_tool_name: String,
    pub evidence_refs: Vec<String>,
    pub exemption: Option<StructuredCapabilitySurfaceExemption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredCapabilitySurfaceParityReport {
    pub issue: u32,
    pub declaration_count: usize,
    pub both_surface_count: usize,
    pub documented_exemption_count: usize,
    pub missing_mcp_surface_declarations: Vec<String>,
    pub undeclared_single_surface_capabilities: Vec<String>,
    pub declarations: Vec<StructuredCapabilitySurfaceDeclaration>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredCapabilityCoverageAudit {
    pub schema: String,
    pub issue: u32,
    pub parent_issue: u32,
    pub audit_id: String,
    pub source_of_truth: String,
    pub advertised_tool_count: usize,
    pub mapped_tool_count: usize,
    pub mutating_tool_count: usize,
    pub readback_tool_count: usize,
    pub subsystem_count: usize,
    pub covered_subsystem_count: usize,
    pub live_tool_names: Vec<String>,
    pub tool_uses: Vec<StructuredCapabilityToolUse>,
    pub subsystem_uses: Vec<StructuredCapabilitySubsystemUse>,
    pub missing_tools: Vec<String>,
    pub unknown_tools: Vec<String>,
    pub idle_subsystems: Vec<CalyxStructuredSubsystem>,
    pub surface_parity: StructuredCapabilitySurfaceParityReport,
    pub tool_fingerprint: String,
    pub coverage_fingerprint: String,
}

pub fn build_structured_capability_coverage_audit(
    audit_id: impl Into<String>,
    source_of_truth: impl Into<String>,
    live_tool_names: Vec<String>,
) -> Result<StructuredCapabilityCoverageAudit, EngineError> {
    let audit_id = audit_id.into();
    let source_of_truth = source_of_truth.into();
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(&audit_id)
        || source_of_truth.trim().is_empty()
    {
        return invalid("binding", "audit id and source of truth are required");
    }
    let expected_tools = expected_tool_names()?;
    let live_tool_names = normalize_live_tools(live_tool_names)?;
    if live_tool_names != expected_tools {
        return invalid(
            "live_tools",
            "runtime tools/list must exactly match the live calyx-mcp registry",
        );
    }

    let specs = tool_specs();
    let tool_uses = specs
        .iter()
        .map(|spec| spec.to_tool_use())
        .collect::<Result<Vec<_>, _>>()?;
    let (missing_tools, unknown_tools) = tool_gaps(&expected_tools, &tool_uses);
    let subsystem_uses = subsystem_coverage(&tool_uses)?;
    let idle_subsystems = idle_subsystems(&subsystem_uses);
    let surface_parity = build_surface_parity_report()?;
    let audit = StructuredCapabilityCoverageAudit {
        schema: STRUCTURED_CAPABILITY_COVERAGE_SCHEMA.to_string(),
        issue: STRUCTURED_CAPABILITY_COVERAGE_ISSUE,
        parent_issue: STRUCTURED_CAPABILITY_COVERAGE_PARENT_ISSUE,
        audit_id,
        source_of_truth,
        advertised_tool_count: expected_tools.len(),
        mapped_tool_count: tool_uses.len(),
        mutating_tool_count: tool_uses.iter().filter(|tool| tool.requires_authn).count(),
        readback_tool_count: tool_uses.iter().filter(|tool| !tool.requires_authn).count(),
        subsystem_count: required_subsystems().len(),
        covered_subsystem_count: subsystem_uses.len(),
        live_tool_names,
        tool_uses,
        subsystem_uses,
        missing_tools,
        unknown_tools,
        idle_subsystems,
        surface_parity,
        tool_fingerprint: stable_fingerprint(&expected_tools)?,
        coverage_fingerprint: String::new(),
    };
    let audit = StructuredCapabilityCoverageAudit {
        coverage_fingerprint: stable_fingerprint(&(
            &audit.tool_uses,
            &audit.subsystem_uses,
            &audit.missing_tools,
            &audit.unknown_tools,
            &audit.idle_subsystems,
            &audit.surface_parity,
        ))?,
        ..audit
    };
    validate_structured_capability_coverage_audit(&audit)?;
    Ok(audit)
}

pub fn validate_structured_capability_coverage_audit(
    audit: &StructuredCapabilityCoverageAudit,
) -> Result<(), EngineError> {
    if audit.schema != STRUCTURED_CAPABILITY_COVERAGE_SCHEMA
        || audit.issue != STRUCTURED_CAPABILITY_COVERAGE_ISSUE
        || audit.parent_issue != STRUCTURED_CAPABILITY_COVERAGE_PARENT_ISSUE
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&audit.audit_id)
        || audit.source_of_truth.trim().is_empty()
    {
        return invalid("audit", "structured capability audit binding drifted");
    }
    if !audit.missing_tools.is_empty()
        || !audit.unknown_tools.is_empty()
        || !audit.idle_subsystems.is_empty()
        || !audit
            .surface_parity
            .missing_mcp_surface_declarations
            .is_empty()
        || !audit
            .surface_parity
            .undeclared_single_surface_capabilities
            .is_empty()
    {
        return invalid(
            "coverage",
            "all current tools, required subsystems, and GUI/MCP surface declarations must be covered",
        );
    }
    if audit.live_tool_names != expected_tool_names()?
        || audit.advertised_tool_count != audit.live_tool_names.len()
        || audit.mapped_tool_count != audit.tool_uses.len()
        || audit.subsystem_count != required_subsystems().len()
        || audit.covered_subsystem_count != audit.subsystem_uses.len()
        || audit.tool_fingerprint.trim().is_empty()
        || audit.coverage_fingerprint.trim().is_empty()
    {
        return invalid(
            "counts",
            "coverage counts and fingerprints must match readback",
        );
    }
    validate_tool_uses(&audit.tool_uses)?;
    validate_subsystem_uses(&audit.subsystem_uses)?;
    validate_surface_parity_report(&audit.surface_parity)?;
    Ok(())
}

fn normalize_live_tools(mut names: Vec<String>) -> Result<Vec<String>, EngineError> {
    if names.iter().any(|name| name.trim().is_empty()) {
        return invalid("live_tools", "tool names must be non-empty");
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn expected_tool_names() -> Result<Vec<String>, EngineError> {
    let mut names = crate::mcp_tools::expected_calyx_mcp_tool_names()?;
    names.sort();
    Ok(names)
}

fn tool_gaps(
    expected_tools: &[String],
    tool_uses: &[StructuredCapabilityToolUse],
) -> (Vec<String>, Vec<String>) {
    let expected = expected_tools
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mapped = tool_uses
        .iter()
        .map(|tool| tool.tool_name.as_str())
        .collect::<BTreeSet<_>>();
    let missing = expected
        .difference(&mapped)
        .map(|tool| (*tool).to_string())
        .collect();
    let unknown = mapped
        .difference(&expected)
        .map(|tool| (*tool).to_string())
        .collect();
    (missing, unknown)
}

fn subsystem_coverage(
    tool_uses: &[StructuredCapabilityToolUse],
) -> Result<Vec<StructuredCapabilitySubsystemUse>, EngineError> {
    let mut by_subsystem: BTreeMap<CalyxStructuredSubsystem, Vec<&StructuredCapabilityToolUse>> =
        BTreeMap::new();
    for tool in tool_uses {
        by_subsystem
            .entry(tool.primary_subsystem)
            .or_default()
            .push(tool);
        for subsystem in &tool.supporting_subsystems {
            by_subsystem.entry(*subsystem).or_default().push(tool);
        }
    }
    required_subsystems()
        .iter()
        .map(|required| {
            let tools = by_subsystem.get(required).ok_or_else(|| {
                structured_capability_coverage_support::error(
                    "subsystem",
                    format!("{required:?} has no mapped tools"),
                )
            })?;
            Ok(StructuredCapabilitySubsystemUse {
                subsystem: *required,
                tool_names: tools.iter().map(|tool| tool.tool_name.clone()).collect(),
                structured_use: format!(
                    "{required:?} has live structured-data capability coverage"
                ),
                evidence_refs: tools
                    .iter()
                    .flat_map(|tool| tool.evidence_refs.iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            })
        })
        .collect()
}

fn idle_subsystems(
    subsystem_uses: &[StructuredCapabilitySubsystemUse],
) -> Vec<CalyxStructuredSubsystem> {
    let covered = subsystem_uses
        .iter()
        .map(|item| item.subsystem)
        .collect::<BTreeSet<_>>();
    required_subsystems()
        .into_iter()
        .filter(|subsystem| !covered.contains(subsystem))
        .collect()
}

fn validate_tool_uses(tool_uses: &[StructuredCapabilityToolUse]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for tool in tool_uses {
        if !seen.insert(tool.tool_name.as_str())
            || tool.structured_use.trim().is_empty()
            || tool.evidence_refs.is_empty()
        {
            return invalid(
                "tool_use",
                "tool coverage requires unique names, use text, and evidence refs",
            );
        }
        if crate::mcp_tools::calyx_mcp_tool_requires_authn(&tool.tool_name)? != tool.requires_authn
        {
            return invalid(
                "tool_use.authn",
                format!("{} authn metadata drifted", tool.tool_name),
            );
        }
    }
    Ok(())
}

fn validate_subsystem_uses(
    subsystem_uses: &[StructuredCapabilitySubsystemUse],
) -> Result<(), EngineError> {
    let required = required_subsystems();
    if subsystem_uses
        .iter()
        .map(|item| item.subsystem)
        .collect::<Vec<_>>()
        != required
    {
        return invalid("subsystem", "required subsystem order drifted");
    }
    for subsystem in subsystem_uses {
        if subsystem.tool_names.is_empty()
            || subsystem.structured_use.trim().is_empty()
            || subsystem.evidence_refs.is_empty()
        {
            return invalid(
                "subsystem",
                "subsystem coverage must include tools and evidence",
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "structured_capability_coverage_tests.rs"]
mod tests;
