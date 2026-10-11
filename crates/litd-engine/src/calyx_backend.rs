//! Typed architecture contract for the engine/Calyx boundary.
//!
//! Calyx is the durable backend and knowledge layer for the editor/engine. The
//! deterministic sim tick and render frame consume only frozen lowered data.

use serde::Serialize;

pub const CALYX_BACKEND_ARCHITECTURE_SCHEMA: &str = "litd.calyx_backend_architecture.v1";
pub const CALYX_BACKEND_READBACK_SCHEMA: &str = "litd.calyx_backend_readback.v1";

const REQUIRED_DOCTRINE_REFS: [&str; 5] = ["#777", "#800", "#1072", "#1113", "#1114"];
const REQUIRED_ROLES: [CalyxBackendRole; 8] = [
    CalyxBackendRole::Persistence,
    CalyxBackendRole::WorldContentAssetData,
    CalyxBackendRole::SemanticIntelligence,
    CalyxBackendRole::Provenance,
    CalyxBackendRole::AgentRuntime,
    CalyxBackendRole::Orchestration,
    CalyxBackendRole::LoweringValve,
    CalyxBackendRole::SourceOfTruthReadback,
];
const HOT_PATHS: [CalyxRuntimeBoundary; 2] = [
    CalyxRuntimeBoundary::DeterministicSimTick,
    CalyxRuntimeBoundary::RenderFrame,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalyxBackendRole {
    Persistence,
    WorldContentAssetData,
    SemanticIntelligence,
    Provenance,
    AgentRuntime,
    Orchestration,
    LoweringValve,
    SourceOfTruthReadback,
}

impl CalyxBackendRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Persistence => "persistence",
            Self::WorldContentAssetData => "world_content_asset_data",
            Self::SemanticIntelligence => "semantic_intelligence",
            Self::Provenance => "provenance",
            Self::AgentRuntime => "agent_runtime",
            Self::Orchestration => "orchestration",
            Self::LoweringValve => "lowering_valve",
            Self::SourceOfTruthReadback => "source_of_truth_readback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalyxRuntimeBoundary {
    EditorBackend,
    EngineBackend,
    LoweringValve,
    DeterministicSimTick,
    RenderFrame,
}

impl CalyxRuntimeBoundary {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EditorBackend => "editor_backend",
            Self::EngineBackend => "engine_backend",
            Self::LoweringValve => "lowering_valve",
            Self::DeterministicSimTick => "deterministic_sim_tick",
            Self::RenderFrame => "render_frame",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalyxAccessMode {
    DirectCalyxBackend,
    FrozenLoweredDataOnly,
}

impl CalyxAccessMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectCalyxBackend => "direct_calyx_backend",
            Self::FrozenLoweredDataOnly => "frozen_lowered_data_only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalyxTransportContract {
    pub calyx_is_rust: bool,
    pub engine_is_rust: bool,
    pub in_process_allowed: bool,
    pub loopback_allowed: bool,
    pub cgo_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalyxBoundaryRule {
    pub boundary: CalyxRuntimeBoundary,
    pub access_mode: CalyxAccessMode,
    pub blocking_calyx_allowed: bool,
    pub required_input: &'static str,
    pub output: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalyxBackendArchitecture {
    pub schema: &'static str,
    pub doctrine_refs: Vec<&'static str>,
    pub calyx_source_of_truth: &'static str,
    pub roles: Vec<CalyxBackendRole>,
    pub transport: CalyxTransportContract,
    pub boundary_rules: Vec<CalyxBoundaryRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalyxBackendContractReadback {
    pub schema: &'static str,
    pub doctrine_refs: Vec<&'static str>,
    pub backend_role_count: usize,
    pub hot_path_exclusion_count: usize,
    pub lowered_only_hot_paths: Vec<&'static str>,
    pub calyx_source_of_truth: &'static str,
    pub transport_readback: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CalyxBackendContractError {
    #[error("Calyx backend architecture field {field} is missing: {reason}")]
    Missing { field: &'static str, reason: String },
    #[error("Calyx backend architecture field {field} is invalid: {reason}")]
    Invalid { field: &'static str, reason: String },
}

pub fn engine_calyx_architecture() -> CalyxBackendArchitecture {
    CalyxBackendArchitecture {
        schema: CALYX_BACKEND_ARCHITECTURE_SCHEMA,
        doctrine_refs: REQUIRED_DOCTRINE_REFS.to_vec(),
        calyx_source_of_truth: "Calyx Aster vaults, Calyx ledger rows, constellations, provenance, and lowered artifact fingerprints",
        roles: REQUIRED_ROLES.to_vec(),
        transport: CalyxTransportContract {
            calyx_is_rust: true,
            engine_is_rust: true,
            in_process_allowed: true,
            loopback_allowed: true,
            cgo_allowed: false,
        },
        boundary_rules: vec![
            CalyxBoundaryRule {
                boundary: CalyxRuntimeBoundary::EditorBackend,
                access_mode: CalyxAccessMode::DirectCalyxBackend,
                blocking_calyx_allowed: true,
                required_input: "editor command, authored content, or source-of-truth readback",
                output: "durable Calyx rows plus structured readback",
            },
            CalyxBoundaryRule {
                boundary: CalyxRuntimeBoundary::EngineBackend,
                access_mode: CalyxAccessMode::DirectCalyxBackend,
                blocking_calyx_allowed: true,
                required_input: "non-hot-path engine service request",
                output: "durable Calyx rows, intelligence output, or verified readback",
            },
            CalyxBoundaryRule {
                boundary: CalyxRuntimeBoundary::LoweringValve,
                access_mode: CalyxAccessMode::DirectCalyxBackend,
                blocking_calyx_allowed: true,
                required_input: "verified Calyx source-of-truth records",
                output: "frozen fingerprinted runtime artifact",
            },
            CalyxBoundaryRule {
                boundary: CalyxRuntimeBoundary::DeterministicSimTick,
                access_mode: CalyxAccessMode::FrozenLoweredDataOnly,
                blocking_calyx_allowed: false,
                required_input: "frozen fingerprinted fixed-point lowered data",
                output: "deterministic hashable sim state",
            },
            CalyxBoundaryRule {
                boundary: CalyxRuntimeBoundary::RenderFrame,
                access_mode: CalyxAccessMode::FrozenLoweredDataOnly,
                blocking_calyx_allowed: false,
                required_input: "frozen fingerprinted render-ready lowered data",
                output: "non-blocking GPU submission data",
            },
        ],
    }
}

pub fn verify_engine_calyx_architecture(
    contract: &CalyxBackendArchitecture,
) -> Result<CalyxBackendContractReadback, CalyxBackendContractError> {
    if contract.schema != CALYX_BACKEND_ARCHITECTURE_SCHEMA {
        return invalid(
            "schema",
            format!("expected {CALYX_BACKEND_ARCHITECTURE_SCHEMA}"),
        );
    }
    for doctrine_ref in REQUIRED_DOCTRINE_REFS {
        if !contract.doctrine_refs.contains(&doctrine_ref) {
            return missing("doctrine_refs", format!("missing {doctrine_ref}"));
        }
    }
    for role in REQUIRED_ROLES {
        if !contract.roles.contains(&role) {
            return missing("roles", format!("missing {}", role.as_str()));
        }
    }
    if !contract.transport.calyx_is_rust || !contract.transport.engine_is_rust {
        return invalid(
            "transport",
            "Calyx and engine must both be Rust".to_string(),
        );
    }
    if !contract.transport.in_process_allowed && !contract.transport.loopback_allowed {
        return invalid(
            "transport",
            "in-process or loopback Calyx transport must be allowed".to_string(),
        );
    }
    if contract.transport.cgo_allowed {
        return invalid("transport.cgo_allowed", "cgo must be disabled".to_string());
    }

    let mut lowered_only_hot_paths = Vec::with_capacity(HOT_PATHS.len());
    for boundary in HOT_PATHS {
        let rule = rule_for(contract, boundary)?;
        if rule.access_mode != CalyxAccessMode::FrozenLoweredDataOnly {
            return invalid(
                "boundary_rules.access_mode",
                format!(
                    "{} must consume frozen lowered data only, got {}",
                    boundary.as_str(),
                    rule.access_mode.as_str()
                ),
            );
        }
        if rule.blocking_calyx_allowed {
            return invalid(
                "boundary_rules.blocking_calyx_allowed",
                format!("{} must not block on Calyx", boundary.as_str()),
            );
        }
        if !rule.required_input.contains("frozen")
            || !rule.required_input.contains("fingerprinted")
            || !rule.required_input.contains("lowered")
        {
            return invalid(
                "boundary_rules.required_input",
                format!(
                    "{} must require frozen fingerprinted lowered data",
                    boundary.as_str()
                ),
            );
        }
        lowered_only_hot_paths.push(boundary.as_str());
    }
    let lowering = rule_for(contract, CalyxRuntimeBoundary::LoweringValve)?;
    if lowering.output != "frozen fingerprinted runtime artifact" {
        return invalid(
            "boundary_rules.lowering_valve.output",
            "lowering valve must emit frozen fingerprinted runtime artifacts".to_string(),
        );
    }

    Ok(CalyxBackendContractReadback {
        schema: CALYX_BACKEND_READBACK_SCHEMA,
        doctrine_refs: contract.doctrine_refs.clone(),
        backend_role_count: contract.roles.len(),
        hot_path_exclusion_count: lowered_only_hot_paths.len(),
        lowered_only_hot_paths,
        calyx_source_of_truth: contract.calyx_source_of_truth,
        transport_readback: "rust_in_process_or_loopback_no_cgo",
    })
}

fn rule_for(
    contract: &CalyxBackendArchitecture,
    boundary: CalyxRuntimeBoundary,
) -> Result<&CalyxBoundaryRule, CalyxBackendContractError> {
    contract
        .boundary_rules
        .iter()
        .find(|rule| rule.boundary == boundary)
        .ok_or_else(|| CalyxBackendContractError::Missing {
            field: "boundary_rules",
            reason: format!("missing {}", boundary.as_str()),
        })
}

fn missing<T>(field: &'static str, reason: String) -> Result<T, CalyxBackendContractError> {
    Err(CalyxBackendContractError::Missing { field, reason })
}

fn invalid<T>(field: &'static str, reason: String) -> Result<T, CalyxBackendContractError> {
    Err(CalyxBackendContractError::Invalid { field, reason })
}
