use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::calyx_persistence::{EngineCalyxDatabaseReadback, EngineCalyxPersistenceError};

pub const ENGINE_CALYX_OBSERVABILITY_SCHEMA: &str = "litd.engine.calyx_observability.v1";
pub const ENGINE_CALYX_OBSERVABILITY_READBACK_SCHEMA: &str =
    "litd.engine.calyx_observability_readback.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineTelemetryEventKind {
    Request,
    Error,
    ChainVerify,
    Crash,
    AgentOod,
    PerfFrame,
    MetricsEndpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineTelemetryEvent {
    pub event_id: String,
    pub source: String,
    pub kind: EngineTelemetryEventKind,
    pub observed_at_ms: u64,
    pub latency_ms: Option<u64>,
    pub error_code: Option<String>,
    pub chain_ok: Option<bool>,
    pub crash_signature: Option<String>,
    pub agent_ood_score_milli: Option<u32>,
    pub frame_time_ns: Option<u64>,
    pub endpoint_ok: Option<bool>,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxObservabilityPolicy {
    pub policy_id: String,
    pub max_latency_ms: u64,
    pub max_error_rate_milli: u32,
    pub max_agent_ood_milli: u32,
    pub max_frame_time_ns: u64,
    pub max_crash_repeats: usize,
    pub require_chain_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAlert {
    pub alert_id: String,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub event_ids: Vec<String>,
    pub quarantine_refs: Vec<String>,
    pub remediation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxObservabilityMetrics {
    pub request_count: usize,
    pub error_count: usize,
    pub error_rate_milli: u32,
    pub max_latency_ms: u64,
    pub max_frame_time_ns: u64,
    pub crash_signature_counts: BTreeMap<String, usize>,
    pub agent_ood_max_milli: u32,
    pub metrics_endpoint_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxObservabilityReadback {
    pub schema: String,
    pub run_id: String,
    pub policy: EngineCalyxObservabilityPolicy,
    pub event_count: usize,
    pub alert_count: usize,
    pub normal_count: usize,
    pub metrics: EngineCalyxObservabilityMetrics,
    pub alerts: Vec<EngineCalyxAlert>,
    pub observability_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxObservabilityStoredReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub observability: EngineCalyxObservabilityReadback,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineCalyxObservabilityError {
    #[error("engine Calyx observability field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx observability persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx observability codec failed: {0}")]
    Codec(String),
}
