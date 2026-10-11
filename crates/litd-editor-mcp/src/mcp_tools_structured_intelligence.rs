use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{EditorMcpError, fingerprint::stable_fingerprint};

use super::mcp_tools_calyx_surface::{
    calyx_mcp_tool_def, calyx_mcp_tool_requires_authn, expected_calyx_mcp_tool_names,
};

pub const STRUCTURED_INTELLIGENCE_MCP_ISSUE: u32 = 1419;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredIntelligenceMcpSurfaceManifest {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub source_issue: u32,
    pub transport: String,
    pub families: Vec<StructuredIntelligenceMcpFamily>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredIntelligenceMcpFamily {
    pub family: String,
    pub purpose: String,
    pub required_tools: Vec<String>,
    pub mutating_tools: Vec<String>,
    pub readback_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredIntelligenceMcpSurfaceReport {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub family_count: usize,
    pub required_tool_count: usize,
    pub mutating_tool_count: usize,
    pub readback_tool_count: usize,
    pub advertised_tool_count: usize,
    pub family_fingerprint: String,
    pub surface_fingerprint: String,
}

pub fn default_structured_intelligence_mcp_surface_manifest()
-> Result<StructuredIntelligenceMcpSurfaceManifest, EditorMcpError> {
    let families = family_specs()
        .into_iter()
        .map(|spec| {
            let required_tools = spec
                .tools
                .iter()
                .map(|tool| (*tool).to_string())
                .collect::<Vec<_>>();
            let (mutating_tools, readback_tools) = partition_by_authn(&required_tools)?;
            Ok(StructuredIntelligenceMcpFamily {
                family: spec.family.to_string(),
                purpose: spec.purpose.to_string(),
                required_tools,
                mutating_tools,
                readback_tools,
            })
        })
        .collect::<Result<Vec<_>, EditorMcpError>>()?;
    let manifest = StructuredIntelligenceMcpSurfaceManifest {
        schema_version: 1,
        parent_issue: STRUCTURED_INTELLIGENCE_MCP_ISSUE,
        source_issue: 1201,
        transport: "loopback_jsonrpc_mcp".to_string(),
        families,
    };
    validate_structured_intelligence_mcp_surface_manifest(&manifest)?;
    Ok(manifest)
}

pub fn build_structured_intelligence_mcp_surface_report(
    manifest: &StructuredIntelligenceMcpSurfaceManifest,
) -> Result<StructuredIntelligenceMcpSurfaceReport, EditorMcpError> {
    validate_structured_intelligence_mcp_surface_manifest(manifest)?;
    let required_tools = unique_required_tools(manifest);
    let (mutating_tools, readback_tools) =
        partition_by_authn(&required_tools.iter().cloned().collect::<Vec<_>>())?;
    Ok(StructuredIntelligenceMcpSurfaceReport {
        schema_version: manifest.schema_version,
        parent_issue: manifest.parent_issue,
        family_count: manifest.families.len(),
        required_tool_count: required_tools.len(),
        mutating_tool_count: mutating_tools.len(),
        readback_tool_count: readback_tools.len(),
        advertised_tool_count: expected_calyx_mcp_tool_names()?.len(),
        family_fingerprint: stable_fingerprint(&manifest.families)?,
        surface_fingerprint: stable_fingerprint(manifest)?,
    })
}

pub fn validate_structured_intelligence_mcp_surface_manifest(
    manifest: &StructuredIntelligenceMcpSurfaceManifest,
) -> Result<(), EditorMcpError> {
    if manifest.schema_version != 1
        || manifest.parent_issue != STRUCTURED_INTELLIGENCE_MCP_ISSUE
        || manifest.source_issue != 1201
        || manifest.transport != "loopback_jsonrpc_mcp"
    {
        return invalid(
            "binding",
            "structured-intelligence MCP manifest binding drifted",
        );
    }
    let advertised_names = expected_calyx_mcp_tool_names()?;
    let advertised = advertised_names
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected_families = family_specs()
        .iter()
        .map(|spec| spec.family)
        .collect::<Vec<_>>();
    if manifest
        .families
        .iter()
        .map(|family| family.family.as_str())
        .collect::<Vec<_>>()
        != expected_families
    {
        return invalid("families", "structured-intelligence family set drifted");
    }
    for family in &manifest.families {
        validate_family(family, &advertised)?;
    }
    Ok(())
}

fn validate_family(
    family: &StructuredIntelligenceMcpFamily,
    advertised: &BTreeSet<&str>,
) -> Result<(), EditorMcpError> {
    if family.required_tools.is_empty()
        || family.purpose.trim().is_empty()
        || family.mutating_tools.len() + family.readback_tools.len() != family.required_tools.len()
    {
        return invalid("family", format!("{} family is incomplete", family.family));
    }
    let mut unique = BTreeSet::new();
    for tool in &family.required_tools {
        if !unique.insert(tool.as_str()) {
            return invalid("tool", format!("{} duplicates {tool}", family.family));
        }
        if !advertised.contains(tool.as_str()) {
            return invalid(
                "tool",
                format!("{} missing advertised tool {tool}", family.family),
            );
        }
        let authn = calyx_mcp_tool_requires_authn(tool)?;
        let in_mutating = family.mutating_tools.contains(tool);
        let in_readback = family.readback_tools.contains(tool);
        if authn != in_mutating || authn == in_readback || in_mutating == in_readback {
            return invalid("tool", format!("{} authn partition is stale", tool));
        }
        let def = calyx_mcp_tool_def(tool)?;
        if def.description.trim().is_empty()
            || def.use_when.trim().is_empty()
            || !def.input_schema.is_object()
        {
            return invalid("descriptor", format!("{tool} descriptor is incomplete"));
        }
    }
    Ok(())
}

fn unique_required_tools(manifest: &StructuredIntelligenceMcpSurfaceManifest) -> BTreeSet<String> {
    manifest
        .families
        .iter()
        .flat_map(|family| family.required_tools.iter().cloned())
        .collect()
}

fn partition_by_authn(
    required_tools: &[String],
) -> Result<(Vec<String>, Vec<String>), EditorMcpError> {
    let mut mutating_tools = Vec::new();
    let mut readback_tools = Vec::new();
    for tool in required_tools {
        if calyx_mcp_tool_requires_authn(tool)? {
            mutating_tools.push(tool.clone());
        } else {
            readback_tools.push(tool.clone());
        }
    }
    Ok((mutating_tools, readback_tools))
}

struct FamilySpec {
    family: &'static str,
    purpose: &'static str,
    tools: &'static [&'static str],
}

fn family_specs() -> Vec<FamilySpec> {
    vec![
        FamilySpec {
            family: "ingest",
            purpose: "durably store typed structured records and retained source inputs",
            tools: &["calyx.ingest", "calyx.ingest_media"],
        },
        FamilySpec {
            family: "measure",
            purpose: "measure candidate records without durable Base CF writes",
            tools: &["calyx.measure"],
        },
        FamilySpec {
            family: "associate",
            purpose: "surface neighbors, agreement, disagreement, definitions, and graph walks",
            tools: &[
                "calyx.neighbors",
                "calyx.agree",
                "calyx.disagree",
                "calyx.define",
                "calyx.traverse",
            ],
        },
        FamilySpec {
            family: "bits",
            purpose: "measure abundance, per-anchor bits, and deficit-driven lens proposals",
            tools: &["calyx.abundance", "calyx.bits", "calyx.propose_lens"],
        },
        FamilySpec {
            family: "kernel",
            purpose: "build/read the grounding kernel and answer from it",
            tools: &["calyx.kernel", "calyx.kernel_answer"],
        },
        FamilySpec {
            family: "oracle",
            purpose: "answer, trace, reproduce, and gate generated content from grounded evidence",
            tools: &[
                "calyx.kernel_answer",
                "calyx.answer_trace",
                "calyx.reproduce",
                "calyx.guard_generate",
            ],
        },
        FamilySpec {
            family: "guard",
            purpose: "calibrate and apply fail-closed Ward boundaries",
            tools: &[
                "calyx.guard.calibrate",
                "calyx.guard.check",
                "calyx.guard_generate",
                "calyx.search",
            ],
        },
    ]
}

fn invalid<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EditorMcpError> {
    Err(EditorMcpError::InvalidConfig {
        field: format!("editor.mcp_tools.structured_intelligence.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue1419_structured_intelligence_families_are_advertised() {
        let manifest = default_structured_intelligence_mcp_surface_manifest().unwrap();
        let report = build_structured_intelligence_mcp_surface_report(&manifest).unwrap();

        assert_eq!(report.parent_issue, 1419);
        assert_eq!(report.family_count, 7);
        assert!(report.required_tool_count >= 16);
        assert!(report.mutating_tool_count > 0);
        assert!(report.readback_tool_count > 0);
        assert_eq!(
            manifest
                .families
                .iter()
                .map(|family| family.family.as_str())
                .collect::<Vec<_>>(),
            vec![
                "ingest",
                "measure",
                "associate",
                "bits",
                "kernel",
                "oracle",
                "guard"
            ]
        );
        assert!(
            manifest
                .families
                .iter()
                .any(|family| { family.required_tools.contains(&"calyx.ingest".to_string()) })
        );
        assert!(
            manifest
                .families
                .iter()
                .any(|family| { family.required_tools.contains(&"calyx.measure".to_string()) })
        );
    }
}
