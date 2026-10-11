use litd_engine::EngineError;

use super::{CalyxStructuredSubsystem, StructuredCapabilityToolUse};

pub(super) struct ToolSpec {
    pub name: &'static str,
    pub primary: CalyxStructuredSubsystem,
    pub support: &'static [CalyxStructuredSubsystem],
    pub structured_use: &'static str,
    pub evidence: &'static [&'static str],
}

impl ToolSpec {
    pub(super) fn to_tool_use(&self) -> Result<StructuredCapabilityToolUse, EngineError> {
        let requires_authn = crate::mcp_tools::calyx_mcp_tool_requires_authn(self.name)?;
        Ok(StructuredCapabilityToolUse {
            tool_name: self.name.to_string(),
            primary_subsystem: self.primary,
            supporting_subsystems: self.support.to_vec(),
            requires_authn,
            structured_use: self.structured_use.to_string(),
            evidence_refs: self
                .evidence
                .iter()
                .map(|item| (*item).to_string())
                .collect(),
        })
    }
}

pub(super) fn required_subsystems() -> Vec<CalyxStructuredSubsystem> {
    use CalyxStructuredSubsystem::*;
    vec![
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
    ]
}

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    use CalyxStructuredSubsystem::*;
    vec![
        tool(
            "calyx.abundance",
            LoomDda,
            &[Assay],
            "DDA abundance over structured constellations",
            &["issue:#1385", "issue:#1419", "issue:#1420"],
        ),
        tool(
            "calyx.add_lens",
            RegistryLenses,
            &[Aster, Mcp],
            "adds structured-data panel slots used by typed records",
            &["issue:#1353", "issue:#1364", "issue:#1419"],
        ),
        tool(
            "calyx.agree",
            Sextant,
            &[LoomDda],
            "finds structured entities that agree on a selected slot",
            &["issue:#1414", "issue:#1419"],
        ),
        tool(
            "calyx.anchor",
            Ledger,
            &[Aster, Assay],
            "attaches grounded structured outcomes for bits and kernels",
            &["issue:#1391", "issue:#1419"],
        ),
        tool(
            "calyx.anneal.status",
            Anneal,
            &[Assay],
            "reads structured panel optimization and growth state",
            &["issue:#1411", "issue:#1419"],
        ),
        tool(
            "calyx.answer_trace",
            Ledger,
            &[Oracle, LoweringValve],
            "traces grounded structured answers and lowered values",
            &["issue:#1417", "issue:#1419"],
        ),
        tool(
            "calyx.bits",
            Assay,
            &[Forge, Ledger],
            "measures bits per structured lens against anchors",
            &["issue:#1392", "issue:#1419", "issue:#1420"],
        ),
        tool(
            "calyx.create_vault",
            Aster,
            &[Mcp],
            "creates the Aster vault source of truth for structured data",
            &["issue:#1354", "issue:#1419"],
        ),
        tool(
            "calyx.define",
            Sextant,
            &[RegistryLenses],
            "defines structured entities from their measured slots",
            &["issue:#1414", "issue:#1419"],
        ),
        tool(
            "calyx.disagree",
            Sextant,
            &[LoomDda],
            "finds structured entities that disagree across slots",
            &["issue:#1414", "issue:#1419"],
        ),
        tool(
            "calyx.guard.calibrate",
            Ward,
            &[Assay, Ledger],
            "calibrates per-slot structured OOD guard regions",
            &["issue:#1404", "issue:#1419"],
        ),
        tool(
            "calyx.guard.check",
            Ward,
            &[Aster],
            "checks structured entities against calibrated trusted regions",
            &["issue:#1403", "issue:#1405", "issue:#1420"],
        ),
        tool(
            "calyx.guard_generate",
            Ward,
            &[Oracle],
            "guards generated structured content before editor acceptance",
            &["issue:#1403", "issue:#1419"],
        ),
        tool(
            "calyx.ingest",
            Aster,
            &[RegistryLenses, Ledger],
            "ingests retained structured-row text projections into constellations",
            &["issue:#1354", "issue:#1419"],
        ),
        tool(
            "calyx.ingest_media",
            Aster,
            &[Ledger],
            "ingests opaque asset bytes with structured metadata/provenance in Calyx",
            &["issue:#1122", "issue:#1419"],
        ),
        tool(
            "calyx.kernel",
            Lodestar,
            &[Assay, Ledger],
            "derives grounding kernels over structured game corpora",
            &["issue:#1399", "issue:#1419", "issue:#1420"],
        ),
        tool(
            "calyx.kernel_answer",
            Lodestar,
            &[Oracle, LoweringValve],
            "answers editor design questions from the structured kernel",
            &["issue:#1400", "issue:#1419", "issue:#1420"],
        ),
        tool(
            "calyx.list_panel",
            RegistryLenses,
            &[Aster],
            "reads the active structured panel and lens slots",
            &["issue:#1353", "issue:#1419"],
        ),
        tool(
            "calyx.measure",
            RegistryLenses,
            &[Forge],
            "measures retained text projections for hybrid structured records",
            &["issue:#1370", "issue:#1419"],
        ),
        tool(
            "calyx.neighbors",
            Sextant,
            &[LoomDda],
            "surfaces nearest structured entities across measured slots",
            &["issue:#1413", "issue:#1419", "issue:#1420"],
        ),
        tool(
            "calyx.park_lens",
            RegistryLenses,
            &[Assay],
            "parks low-signal structured lenses",
            &["issue:#1384", "issue:#1419"],
        ),
        tool(
            "calyx.profile_lens",
            RegistryLenses,
            &[Forge],
            "profiles structured lens capability and cost metadata",
            &["issue:#1353", "issue:#1419"],
        ),
        tool(
            "calyx.propose_lens",
            Anneal,
            &[Assay, RegistryLenses],
            "proposes new structured lenses for sufficiency deficits",
            &["issue:#1393", "issue:#1410"],
        ),
        tool(
            "calyx.provenance",
            Ledger,
            &[Aster],
            "reads provenance for structured constellations and values",
            &["issue:#1417", "issue:#1419"],
        ),
        tool(
            "calyx.reproduce",
            LoweringValve,
            &[Ledger],
            "reproduces lowered structured intelligence from provenance",
            &["issue:#1417", "issue:#1419"],
        ),
        tool(
            "calyx.retire_lens",
            RegistryLenses,
            &[Assay],
            "retires redundant structured lenses after correlation checks",
            &["issue:#1384", "issue:#1419"],
        ),
        tool(
            "calyx.search",
            Sextant,
            &[Ward],
            "searches structured entities with optional Ward guard",
            &["issue:#1413", "issue:#1419"],
        ),
        tool(
            "calyx.search_skill",
            Sextant,
            &[RegistryLenses],
            "routes structured search through capability skills",
            &["issue:#1413", "issue:#1419"],
        ),
        tool(
            "calyx.skills",
            RegistryLenses,
            &[Sextant],
            "reads structured search/authoring skill tree",
            &["issue:#1413", "issue:#1419"],
        ),
        tool(
            "calyx.traverse",
            Sextant,
            &[LoomDda],
            "walks structured association paths",
            &["issue:#1387", "issue:#1419"],
        ),
        tool(
            "calyx.verify_chain",
            Ledger,
            &[LoweringValve],
            "verifies provenance chains for structured artifacts",
            &["issue:#1417", "issue:#1419"],
        ),
    ]
}

fn tool(
    name: &'static str,
    primary: CalyxStructuredSubsystem,
    support: &'static [CalyxStructuredSubsystem],
    structured_use: &'static str,
    evidence: &'static [&'static str],
) -> ToolSpec {
    ToolSpec {
        name,
        primary,
        support,
        structured_use,
        evidence,
    }
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(error(field, reason))
}

pub(super) fn error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.structured_capability_coverage.{}", field.into()),
        reason: reason.into(),
    }
}
