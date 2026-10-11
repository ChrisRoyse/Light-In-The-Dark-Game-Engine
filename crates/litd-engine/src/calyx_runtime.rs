//! Engine-side Calyx runtime integration boundary and lifecycle.
//!
//! The engine links Calyx crates in-process by default. Loopback is an explicit
//! long-lived service boundary, not a per-call subprocess escape hatch.

use serde::{Deserialize, Serialize};

pub const ENGINE_CALYX_RUNTIME_SCHEMA: &str = "litd.engine.calyx_runtime.v1";
pub const ENGINE_CALYX_RUNTIME_READBACK_SCHEMA: &str = "litd.engine.calyx_runtime_readback.v1";
pub const CALYX_RUNTIME_VRAM_BUDGET_CEILING_MIB: u32 = 30_000;

const IN_PROCESS_CRATES: [&str; 8] = [
    "calyx-core",
    "calyx-aster",
    "calyx-ledger",
    "calyx-assay",
    "calyx-loom",
    "calyx-oracle",
    "calyx-anneal",
    "calyx-forge",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineCalyxTransport {
    InProcess,
    Loopback { endpoint: String },
}

impl EngineCalyxTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InProcess => "in_process",
            Self::Loopback { .. } => "loopback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineCalyxLifecycleState {
    Configured,
    Started,
    WarmLoaded,
    Stopped,
}

impl EngineCalyxLifecycleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Started => "started",
            Self::WarmLoaded => "warm_loaded",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxRuntimeConfig {
    pub transport: EngineCalyxTransport,
    pub panel_template: String,
    pub warm_load_lenses: usize,
    pub vram_budget_mib: u32,
    pub warm_load_vram_mib: u32,
    pub per_call_process_allowed: bool,
}

impl EngineCalyxRuntimeConfig {
    pub fn in_process(
        panel_template: impl Into<String>,
        warm_load_lenses: usize,
        vram_budget_mib: u32,
        warm_load_vram_mib: u32,
    ) -> Self {
        Self {
            transport: EngineCalyxTransport::InProcess,
            panel_template: panel_template.into(),
            warm_load_lenses,
            vram_budget_mib,
            warm_load_vram_mib,
            per_call_process_allowed: false,
        }
    }

    pub fn loopback(
        endpoint: impl Into<String>,
        panel_template: impl Into<String>,
        warm_load_lenses: usize,
        vram_budget_mib: u32,
        warm_load_vram_mib: u32,
    ) -> Self {
        Self {
            transport: EngineCalyxTransport::Loopback {
                endpoint: endpoint.into(),
            },
            panel_template: panel_template.into(),
            warm_load_lenses,
            vram_budget_mib,
            warm_load_vram_mib,
            per_call_process_allowed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxRuntimeReadback {
    pub schema: String,
    pub transport: String,
    pub lifecycle_state: String,
    pub panel_template: String,
    pub warm_load_lenses: usize,
    pub vram_budget_mib: u32,
    pub warm_load_vram_mib: u32,
    pub per_call_process_allowed: bool,
    pub process_boundary: String,
    pub linked_calyx_crates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EngineCalyxRuntime {
    config: EngineCalyxRuntimeConfig,
    state: EngineCalyxLifecycleState,
}

impl EngineCalyxRuntime {
    pub fn configure(config: EngineCalyxRuntimeConfig) -> Result<Self, EngineCalyxRuntimeError> {
        validate_config(&config)?;
        Ok(Self {
            config,
            state: EngineCalyxLifecycleState::Configured,
        })
    }

    pub fn start(&mut self) -> Result<EngineCalyxRuntimeReadback, EngineCalyxRuntimeError> {
        if self.state != EngineCalyxLifecycleState::Configured {
            return invalid_state("start", self.state, EngineCalyxLifecycleState::Configured);
        }
        self.state = EngineCalyxLifecycleState::Started;
        Ok(self.readback())
    }

    pub fn warm_load(&mut self) -> Result<EngineCalyxRuntimeReadback, EngineCalyxRuntimeError> {
        if self.state != EngineCalyxLifecycleState::Started {
            return invalid_state("warm_load", self.state, EngineCalyxLifecycleState::Started);
        }
        if self.config.warm_load_lenses == 0 {
            return invalid_config("warm_load_lenses", "warm-load requires at least one lens");
        }
        self.state = EngineCalyxLifecycleState::WarmLoaded;
        Ok(self.readback())
    }

    pub fn stop(&mut self) -> EngineCalyxRuntimeReadback {
        self.state = EngineCalyxLifecycleState::Stopped;
        self.readback()
    }

    pub fn readback(&self) -> EngineCalyxRuntimeReadback {
        EngineCalyxRuntimeReadback {
            schema: ENGINE_CALYX_RUNTIME_READBACK_SCHEMA.to_string(),
            transport: self.config.transport.as_str().to_string(),
            lifecycle_state: self.state.as_str().to_string(),
            panel_template: self.config.panel_template.clone(),
            warm_load_lenses: self.config.warm_load_lenses,
            vram_budget_mib: self.config.vram_budget_mib,
            warm_load_vram_mib: self.config.warm_load_vram_mib,
            per_call_process_allowed: self.config.per_call_process_allowed,
            process_boundary: process_boundary(&self.config.transport).to_string(),
            linked_calyx_crates: IN_PROCESS_CRATES
                .iter()
                .map(|name| name.to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineCalyxRuntimeError {
    #[error("engine Calyx runtime config field {field} is invalid: {reason}")]
    InvalidConfig { field: String, reason: String },
    #[error(
        "engine Calyx runtime operation {operation} invalid in state {actual}; expected {expected}"
    )]
    InvalidState {
        operation: String,
        actual: String,
        expected: String,
    },
}

fn validate_config(config: &EngineCalyxRuntimeConfig) -> Result<(), EngineCalyxRuntimeError> {
    if config.panel_template.trim().is_empty() {
        return invalid_config("panel_template", "panel template must be non-empty");
    }
    if config.per_call_process_allowed {
        return invalid_config(
            "per_call_process_allowed",
            "per-call Calyx subprocesses are forbidden; use in-process crates or a long-lived loopback service",
        );
    }
    if config.vram_budget_mib == 0 || config.vram_budget_mib > CALYX_RUNTIME_VRAM_BUDGET_CEILING_MIB
    {
        return invalid_config(
            "vram_budget_mib",
            format!("must be within 1..={CALYX_RUNTIME_VRAM_BUDGET_CEILING_MIB} MiB"),
        );
    }
    if config.warm_load_vram_mib > config.vram_budget_mib {
        return invalid_config(
            "warm_load_vram_mib",
            "warm-load VRAM reservation must not exceed runtime VRAM budget",
        );
    }
    if let EngineCalyxTransport::Loopback { endpoint } = &config.transport {
        validate_loopback_endpoint(endpoint)?;
    }
    Ok(())
}

fn validate_loopback_endpoint(endpoint: &str) -> Result<(), EngineCalyxRuntimeError> {
    if endpoint.trim().is_empty() {
        return invalid_config("transport.endpoint", "loopback endpoint must be non-empty");
    }
    let endpoint = endpoint.trim();
    let loopback = endpoint.starts_with("http://127.0.0.1:")
        || endpoint.starts_with("https://127.0.0.1:")
        || endpoint.starts_with("http://[::1]:")
        || endpoint.starts_with("https://[::1]:");
    if !loopback {
        return invalid_config(
            "transport.endpoint",
            "loopback Calyx endpoint must use literal 127.0.0.1 or [::1]",
        );
    }
    Ok(())
}

fn process_boundary(transport: &EngineCalyxTransport) -> &'static str {
    match transport {
        EngineCalyxTransport::InProcess => "rust_crate_in_process",
        EngineCalyxTransport::Loopback { .. } => "long_lived_loopback_service",
    }
}

fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxRuntimeError> {
    Err(EngineCalyxRuntimeError::InvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

fn invalid_state<T>(
    operation: &str,
    actual: EngineCalyxLifecycleState,
    expected: EngineCalyxLifecycleState,
) -> Result<T, EngineCalyxRuntimeError> {
    Err(EngineCalyxRuntimeError::InvalidState {
        operation: operation.to_string(),
        actual: actual.as_str().to_string(),
        expected: expected.as_str().to_string(),
    })
}
