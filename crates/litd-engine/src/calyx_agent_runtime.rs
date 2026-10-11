//! Calyx-backed agent activation runtime.
//!
//! Calyx content defines agents and world-state signals decide when to prompt
//! them. Prompting is out-of-band; the sim receives only deterministic orders.

use std::collections::BTreeSet;

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::calyx_agent_runtime_support::{
    codec_failed, lowered_agent_id, merged_refs, normalize_terms, string_array, string_field,
    u16_field, u32_field, velocity_field,
};
use crate::{
    calyx_content::{
        ENGINE_CALYX_CONTENT_READBACK_SCHEMA, EngineContentConstellationReadback, EngineContentKind,
    },
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
    math::FixedVec3,
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource, SimOrderSourceKind},
};

pub const ENGINE_CALYX_AGENT_RUNTIME_SCHEMA: &str = "litd.engine.calyx_agent_runtime.v1";
pub const ENGINE_CALYX_AGENT_RUNTIME_READBACK_SCHEMA: &str =
    "litd.engine.calyx_agent_runtime_readback.v1";
pub const ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY: &str = "lowered_orders_only";
/// Default agent activation strength (milli) applied when a profile omits the
/// optional `min_strength_milli` field. Documented, explicitly-labeled default
/// per #1464 — absence is intended (authoring often omits it); a present but
/// malformed value still fails loud.
pub const DEFAULT_MIN_AGENT_ACTIVATION_STRENGTH_MILLI: u16 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineWorldSignalKind {
    Combat,
    Proximity,
    Economy,
    Story,
    PlayerAction,
}

impl EngineWorldSignalKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Combat => "combat",
            Self::Proximity => "proximity",
            Self::Economy => "economy",
            Self::Story => "story",
            Self::PlayerAction => "player_action",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWorldSignal {
    pub signal_id: String,
    pub world_id: String,
    pub tick: u64,
    pub kind: EngineWorldSignalKind,
    pub strength_milli: u16,
    pub terms: Vec<String>,
    pub summary: String,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAgentProfile {
    pub agent_id: String,
    pub display_name: String,
    pub source_cx_id: String,
    pub source_fingerprint: String,
    pub trigger_terms: Vec<String>,
    pub min_strength_milli: u16,
    pub prompt_template: String,
    pub target_body: u32,
    pub order_velocity: FixedVec3,
    pub lowered_agent_id: u32,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAgentActivation {
    pub activation_id: String,
    pub agent_id: String,
    pub signal_id: String,
    pub source_cx_id: String,
    pub prompt: String,
    pub execute_tick: u64,
    pub lowered_order: SimOrder,
    pub order_fingerprint: u64,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxAgentRuntimeReadback {
    pub schema: String,
    pub runtime_id: String,
    pub agent_count: usize,
    pub signal_count: usize,
    pub activation_count: usize,
    pub order_count: usize,
    pub hot_path_policy: String,
    pub calyx_live_query_allowed_in_hot_path: bool,
    pub lowering_boundary: String,
    pub profiles: Vec<EngineCalyxAgentProfile>,
    pub activations: Vec<EngineCalyxAgentActivation>,
    pub runtime_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxAgentRuntimeStoredReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub runtime: EngineCalyxAgentRuntimeReadback,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineCalyxAgentRuntimeError {
    #[error("engine Calyx agent runtime field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx agent runtime persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx agent runtime codec failed: {0}")]
    Codec(String),
}

pub fn activate_calyx_agents(
    runtime_id: impl Into<String>,
    agent_readbacks: &[EngineContentConstellationReadback],
    signals: &[EngineWorldSignal],
) -> Result<EngineCalyxAgentRuntimeReadback, EngineCalyxAgentRuntimeError> {
    let runtime_id = runtime_id.into();
    if runtime_id.trim().is_empty() {
        return invalid("runtime_id", "runtime id must be non-empty");
    }
    if agent_readbacks.is_empty() {
        return invalid(
            "agent_readbacks",
            "agent runtime requires at least one agent",
        );
    }
    for signal in signals {
        validate_signal(signal)?;
    }
    let mut profiles = agent_readbacks
        .iter()
        .map(agent_profile)
        .collect::<Result<Vec<_>, _>>()?;
    profiles.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    let mut activations = Vec::new();
    for signal in signals {
        for profile in &profiles {
            if !profile_matches_signal(profile, signal) {
                continue;
            }
            let activation =
                activation_from_match(&runtime_id, profile, signal, activations.len() as u64)?;
            activations.push(activation);
        }
    }
    let mut readback = EngineCalyxAgentRuntimeReadback {
        schema: ENGINE_CALYX_AGENT_RUNTIME_SCHEMA.to_string(),
        runtime_id,
        agent_count: profiles.len(),
        signal_count: signals.len(),
        activation_count: activations.len(),
        order_count: activations.len(),
        hot_path_policy: ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY.to_string(),
        calyx_live_query_allowed_in_hot_path: false,
        lowering_boundary:
            "Calyx activates/prompts agents out-of-band; sim consumes deterministic lowered orders"
                .to_string(),
        profiles,
        activations,
        runtime_fingerprint: String::new(),
    };
    readback.runtime_fingerprint = runtime_fingerprint(&readback)?;
    validate_runtime_readback(&readback)?;
    Ok(readback)
}

pub fn store_calyx_agent_runtime(
    database: &EngineCalyxDatabase,
    runtime: &EngineCalyxAgentRuntimeReadback,
) -> Result<EngineCalyxAgentRuntimeStoredReadback, EngineCalyxAgentRuntimeError> {
    validate_runtime_readback(runtime)?;
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::AgentRuntimeLedger,
        runtime.runtime_id.clone(),
        serde_json::to_value(runtime).map_err(codec_failed)?,
        json!({
            "source_issue": 1120,
            "runtime_fingerprint": runtime.runtime_fingerprint,
            "activation_count": runtime.activation_count,
            "hot_path_policy": runtime.hot_path_policy
        }),
    );
    parse_stored_runtime(database.put_record(&record)?)
}

pub fn read_calyx_agent_runtime(
    database: &EngineCalyxDatabase,
    runtime_id: &str,
) -> Result<EngineCalyxAgentRuntimeStoredReadback, EngineCalyxAgentRuntimeError> {
    if runtime_id.trim().is_empty() {
        return invalid("runtime_id", "runtime id must be non-empty");
    }
    parse_stored_runtime(
        database.get_record(EngineCalyxRecordKind::AgentRuntimeLedger, runtime_id)?,
    )
}

pub fn validate_runtime_readback(
    runtime: &EngineCalyxAgentRuntimeReadback,
) -> Result<(), EngineCalyxAgentRuntimeError> {
    if runtime.schema != ENGINE_CALYX_AGENT_RUNTIME_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_CALYX_AGENT_RUNTIME_SCHEMA}"),
        );
    }
    if runtime.agent_count != runtime.profiles.len()
        || runtime.activation_count != runtime.activations.len()
        || runtime.order_count != runtime.activations.len()
    {
        return invalid(
            "counts",
            "runtime counts must match profile and activation vectors",
        );
    }
    if runtime.hot_path_policy != ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY {
        return invalid(
            "hot_path_policy",
            format!("expected {ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY}"),
        );
    }
    if runtime.calyx_live_query_allowed_in_hot_path {
        return invalid(
            "calyx_live_query_allowed_in_hot_path",
            "live Calyx queries are forbidden in deterministic sim ticks",
        );
    }
    if runtime.runtime_fingerprint != runtime_fingerprint(runtime)? {
        return invalid("runtime_fingerprint", "runtime fingerprint mismatch");
    }
    for activation in &runtime.activations {
        if activation.lowered_order.source.kind != SimOrderSourceKind::LoweredAgent {
            return invalid(
                "activation.lowered_order.source",
                "orders must be lowered-agent sourced",
            );
        }
        if activation.order_fingerprint != activation.lowered_order.fingerprint() {
            return invalid("activation.order_fingerprint", "order fingerprint mismatch");
        }
    }
    Ok(())
}

fn agent_profile(
    readback: &EngineContentConstellationReadback,
) -> Result<EngineCalyxAgentProfile, EngineCalyxAgentRuntimeError> {
    validate_agent_readback(readback)?;
    let payload = readback
        .payload
        .as_object()
        .ok_or_else(|| invalid_error("payload", "agent payload must be an object"))?;
    let trigger_terms = string_array(payload.get("trigger_terms"), "trigger_terms")?;
    let prompt_template = string_field(payload.get("prompt_template"), "prompt_template")?;
    let target_body = u32_field(payload.get("target_body"), "target_body")?;
    // `min_strength_milli` is an optional activation-threshold tuning field.
    // Authoring commonly omits it (mcp_agent_tools_authoring never sets it), so
    // absence is a deliberate, documented default — NOT a silent fallback. A
    // *present but malformed* value still fails loud via `u16_field`.
    let min_strength_milli = payload
        .get("min_strength_milli")
        .map(|value| u16_field(Some(value), "min_strength_milli"))
        .transpose()?
        .unwrap_or(DEFAULT_MIN_AGENT_ACTIVATION_STRENGTH_MILLI);
    if min_strength_milli == 0 || min_strength_milli > 1000 {
        return invalid("min_strength_milli", "must be within 1..=1000");
    }
    Ok(EngineCalyxAgentProfile {
        // #1464: the agent's identity is fundamental — a missing or malformed
        // `agent_id` must fail loud, never silently remap to the content id
        // (which would activate the agent under a different identity).
        agent_id: string_field(payload.get("agent_id"), "agent_id")?,
        display_name: readback.display_name.clone(),
        source_cx_id: readback.cx_id.clone(),
        source_fingerprint: readback.input_sha256.clone(),
        trigger_terms: normalize_terms(trigger_terms)?,
        min_strength_milli,
        prompt_template,
        target_body,
        order_velocity: velocity_field(payload.get("order_velocity"))?,
        lowered_agent_id: lowered_agent_id(&readback.input_sha256)?,
        provenance_refs: vec![format!("calyx://content/agent/{}", readback.cx_id)],
    })
}

fn validate_agent_readback(
    readback: &EngineContentConstellationReadback,
) -> Result<(), EngineCalyxAgentRuntimeError> {
    if readback.schema != ENGINE_CALYX_CONTENT_READBACK_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {ENGINE_CALYX_CONTENT_READBACK_SCHEMA}"),
        );
    }
    if readback.kind != EngineContentKind::Agent {
        return invalid("readback.kind", "agent runtime only accepts agent content");
    }
    if readback.lowering.source_cx_id != readback.cx_id || readback.input_sha256.trim().is_empty() {
        return invalid(
            "readback.provenance",
            "agent readback provenance must be intact",
        );
    }
    Ok(())
}

fn activation_from_match(
    runtime_id: &str,
    profile: &EngineCalyxAgentProfile,
    signal: &EngineWorldSignal,
    sequence: u64,
) -> Result<EngineCalyxAgentActivation, EngineCalyxAgentRuntimeError> {
    let execute_tick = signal
        .tick
        .checked_add(1)
        .ok_or_else(|| invalid_error("signal.tick", "execute tick overflow"))?;
    let order = SimOrder::new(
        execute_tick,
        SimOrderSource::lowered_agent(profile.lowered_agent_id),
        sequence,
        SimBodyId::from_raw(profile.target_body),
        SimOrderKind::SetVelocity(profile.order_velocity),
    );
    let prompt = render_prompt(profile, signal)?;
    let activation_id = sha256_hex(
        format!(
            "{runtime_id}:{}:{}:{}",
            profile.agent_id, signal.signal_id, sequence
        )
        .as_bytes(),
    );
    Ok(EngineCalyxAgentActivation {
        activation_id,
        agent_id: profile.agent_id.clone(),
        signal_id: signal.signal_id.clone(),
        source_cx_id: profile.source_cx_id.clone(),
        prompt,
        execute_tick,
        order_fingerprint: order.fingerprint(),
        lowered_order: order,
        provenance_refs: merged_refs(&profile.provenance_refs, &signal.provenance_refs),
    })
}

fn profile_matches_signal(profile: &EngineCalyxAgentProfile, signal: &EngineWorldSignal) -> bool {
    if signal.strength_milli < profile.min_strength_milli {
        return false;
    }
    let signal_terms = signal
        .terms
        .iter()
        .map(|term| term.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    profile
        .trigger_terms
        .iter()
        .any(|term| signal_terms.contains(term))
}

fn validate_signal(signal: &EngineWorldSignal) -> Result<(), EngineCalyxAgentRuntimeError> {
    if signal.signal_id.trim().is_empty() || signal.world_id.trim().is_empty() {
        return invalid("signal", "signal id and world id must be non-empty");
    }
    if signal.strength_milli == 0 || signal.strength_milli > 1000 {
        return invalid("signal.strength_milli", "must be within 1..=1000");
    }
    if signal.summary.trim().is_empty() {
        return invalid("signal.summary", "summary must be non-empty");
    }
    normalize_terms(signal.terms.clone()).map(|_| ())
}

fn render_prompt(
    profile: &EngineCalyxAgentProfile,
    signal: &EngineWorldSignal,
) -> Result<String, EngineCalyxAgentRuntimeError> {
    let prompt = profile
        .prompt_template
        .replace("{agent}", &profile.display_name)
        .replace("{signal}", &signal.summary)
        .replace("{world}", &signal.world_id)
        .replace("{terms}", &signal.terms.join(" "));
    if prompt.trim().is_empty() {
        return invalid("prompt", "rendered prompt must be non-empty");
    }
    Ok(prompt)
}

fn runtime_fingerprint(
    runtime: &EngineCalyxAgentRuntimeReadback,
) -> Result<String, EngineCalyxAgentRuntimeError> {
    serde_json::to_vec(&json!({
        "schema": runtime.schema,
        "runtime_id": runtime.runtime_id,
        "profiles": runtime.profiles,
        "activations": runtime.activations,
        "hot_path_policy": runtime.hot_path_policy,
        "calyx_live_query_allowed_in_hot_path": runtime.calyx_live_query_allowed_in_hot_path,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(codec_failed)
}

fn parse_stored_runtime(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineCalyxAgentRuntimeStoredReadback, EngineCalyxAgentRuntimeError> {
    let runtime =
        serde_json::from_value::<EngineCalyxAgentRuntimeReadback>(database.payload.clone())
            .map_err(codec_failed)?;
    validate_runtime_readback(&runtime)?;
    Ok(EngineCalyxAgentRuntimeStoredReadback {
        schema: ENGINE_CALYX_AGENT_RUNTIME_READBACK_SCHEMA.to_string(),
        database,
        runtime,
    })
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxAgentRuntimeError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineCalyxAgentRuntimeError {
    EngineCalyxAgentRuntimeError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    }
}
