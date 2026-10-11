//! Types for the agent-response admission choke point (#1548).
//!
//! Split out of `agent_response_gate.rs` to keep every file under the 500-line ceiling (#800 sec.6).
//! These are the data contracts the gate operates on; the gate logic itself — and the invariant
//! that binds them — lives in [`crate::agent_response_gate`], which re-exports everything here so
//! the public path `litd_engine::agent_response_gate::*` is unchanged.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    agent_response::{EngineAgentProposal, EngineAgentSchemaError, EngineAgentVerbTag},
    calyx_persistence::EngineCalyxPersistenceError,
    calyx_ward_gate::{EngineCalyxWardGateError, EngineCalyxWardGateReadback},
    sim::SimBodyId,
    sim_order::SimOrder,
};

/// Schema id of an admission readback.
pub const ENGINE_AGENT_RESPONSE_ADMISSION_SCHEMA: &str = "litd.engine.agent_response_admission.v1";

/// Admitted, and lowered into an order.
pub const CODE_ADMITTED: &str = "LITD-AGENT-ADMITTED";
/// The provider truncated the body (`finish_reason == "length"`). Refused **before parsing**.
pub const CODE_REFUSED_TRUNCATED: &str = "LITD-AGENT-REFUSED-TRUNCATED";
/// The wire facts were unusable for some other reason (unknown finish reason, blank content).
pub const CODE_REFUSED_WIRE: &str = "LITD-AGENT-REFUSED-WIRE";
/// The body did not satisfy the JSON Schema or the closed Rust contract.
pub const CODE_REFUSED_SCHEMA: &str = "LITD-AGENT-REFUSED-SCHEMA";
/// The proposal named an agent or body it was not bound to.
pub const CODE_REFUSED_BINDING: &str = "LITD-AGENT-REFUSED-BINDING";
/// The proposal was outside the deployment's numeric or verb envelope.
pub const CODE_REFUSED_ENVELOPE: &str = "LITD-AGENT-REFUSED-ENVELOPE";

/// Raw provider facts, transport-agnostic. The engine trusts none of them.
///
/// Deliberately NOT `calyx_agent::AgentCompletion`: this gate is a security boundary and must not
/// assume its caller already validated anything. `finish_reason` is a `String`, not an enum, so
/// that an unrecognised value must be explicitly refused rather than silently failing to parse
/// into a variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineAgentWireResponse {
    /// The provider's request id. The forensic anchor — trace this in sanitized logs.
    pub response_id: String,
    /// The model the provider actually served.
    pub model: String,
    /// The provider's finish reason. Only `"stop"` is admissible.
    pub finish_reason: String,
    /// The raw assistant message body, unparsed.
    pub content: String,
    /// SHA-256 of the fully rendered prompt that produced this response.
    pub prompt_sha256: String,
}

/// The authority binding: everything the model is NOT allowed to choose.
///
/// Authored by the caller from the activation that produced the prompt. The model restates
/// `agent_id` and `target_body` and is checked for equality; it never supplies the tick, the order
/// source, or the sequence. This is what makes a jailbroken model's blast radius a *choice among
/// legal orders* rather than an arbitrary order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineAgentBinding {
    /// The agent this response must be from.
    pub agent_id: String,
    /// The body this response may act on. Any other body is refused.
    pub target_body: SimBodyId,
    /// Lowered-agent order source id.
    pub source_id: u32,
    /// The tick the order executes on. AUTHORED — the model cannot schedule itself.
    pub execute_tick: u64,
    /// Deterministic order sequence within the tick.
    pub sequence: u64,
    /// Grounding refs inherited from the activation. Ward requires these.
    pub grounding_refs: Vec<String>,
}

/// The deterministic, LLM-free numeric envelope. DATA, tunable per deployment.
///
/// Strictly tighter than the schema ceilings in [`crate::agent_response`], which are CODE. A
/// response can be schema-valid and still refused here — that separation is the point.
///
/// Bounds are **rejected, never clamped**. Clamping an out-of-envelope proposal would silently
/// convert an attack into a legal order and destroy the evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineAgentOrderEnvelope {
    /// The closed verb allowlist. Empty means nothing is admissible.
    pub allowed_verbs: BTreeSet<EngineAgentVerbTag>,
    /// Hard cap on movement units per tick (measured with ceiling rounding).
    ///
    /// DEFENSE IN DEPTH (decision, #1552 / docs2/decisions/DEC-1552): movement is bounded here in
    /// the order envelope (GATE 6) AND in [`crate::calyx_ward_gate::EngineCalyxWardPolicy::
    /// max_move_units_per_tick`] (GATE 5). The two are intentionally redundant, not accidentally
    /// duplicated: Ward measures the movement *claimed by the subject text* pre-lowering, while this
    /// envelope measures the *actual lowered order kind* — different inputs, same bound. Because the
    /// Ward bound fires first, #1548's over-envelope FSV case reports `stage=Ward`; if Ward were
    /// ever relaxed or removed, THIS bound still refuses the identical order. The order envelope is
    /// the authoritative one and must never be loosened to match Ward. Keep them equal.
    pub max_move_units_per_tick: u32,
    /// Hard cap on proposed damage. Note: `calyx_ward_gate_support::movement_units` returns 0 for
    /// an attack, so damage was NEVER envelope-checked anywhere before #1548. It is now.
    pub max_damage_milli: u32,
    /// Hard cap on the free-text field, tighter than the schema's.
    pub max_rationale_chars: usize,
    /// Whether an agent may propose attacking its own bound body.
    pub allow_self_attack: bool,
}

/// Which gate the response reached. The stage a refusal stopped at is itself evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineAgentAdmissionStage {
    /// Stopped at the wire-facts gate — never parsed.
    Wire,
    /// Stopped at the JSON Schema / serde gate.
    Schema,
    /// Stopped at the binding gate.
    Binding,
    /// Stopped at the Ward gate.
    Ward,
    /// Stopped at the numeric/verb envelope gate.
    Envelope,
    /// Admitted and lowered.
    Lowered,
}

/// The full forensic record of one admission attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineAgentResponseReadback {
    /// Schema id.
    pub schema: String,
    /// Caller-supplied id; the row key in both stores.
    pub admission_id: String,
    /// The furthest gate reached.
    pub stage: EngineAgentAdmissionStage,
    /// Whether the response was admitted.
    pub admitted: bool,
    /// Always equal to `admitted`. The single choke point.
    pub lowering_allowed: bool,
    /// Machine-readable outcome code.
    pub code: String,
    /// Human-readable refusal reasons.
    pub reasons: Vec<String>,
    /// Every JSON Schema violation, with JSON Pointers. Empty when the body was never parsed —
    /// which is itself the proof that a truncated body was refused before parsing.
    pub schema_errors: Vec<EngineAgentSchemaError>,
    /// The Ward gate readback. `None` iff Ward was never reached.
    pub gate: Option<EngineCalyxWardGateReadback>,
    /// The typed proposal. `None` iff it never survived the schema gate.
    pub proposal: Option<EngineAgentProposal>,
    /// The lowered order. `None` unless admitted (and not a `Hold`).
    pub order: Option<SimOrder>,
    /// Fingerprint of `order`.
    pub order_fingerprint: Option<u64>,
    /// The model the provider served.
    pub model: String,
    /// The provider's finish reason.
    pub finish_reason: String,
    /// SHA-256 of the raw response body.
    pub response_sha256: String,
    /// SHA-256 of the rendered prompt.
    pub prompt_sha256: String,
    /// SHA-256 of the proposal JSON Schema in force. Distinguishes "the model changed" from
    /// "the contract changed".
    pub schema_sha256: String,
    /// The provider's request id.
    pub response_id: String,
    /// Ledger row key. Always written.
    pub ledger_row_key: String,
    /// Order-store row key. `Some` iff an order was lowered.
    pub order_row_key: Option<String>,
    /// Fingerprint over the whole admission.
    pub admission_fingerprint: String,
}

/// Errors that mean **the gate itself is unusable** — a bad policy, an uncalibrated Ward profile,
/// a broken store.
///
/// Note the deliberate asymmetry: **a refusal is not an error.** A refused response returns
/// `Ok(readback)` with `admitted == false`. Refusals are a normal, expected, forensically-recorded
/// outcome of a gate that is working; conflating them with gate failure would make it impossible
/// to tell "we blocked an attack" from "the gate is broken".
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineAgentResponseError {
    /// A caller-supplied input was invalid.
    #[error("engine agent response field {field} is invalid: {reason}")]
    InvalidInput {
        /// The offending field.
        field: String,
        /// Why it was rejected.
        reason: String,
    },
    /// The Ward gate could not run.
    #[error("engine agent response Ward gate failed: {0}")]
    Ward(#[from] EngineCalyxWardGateError),
    /// The store could not be written or read.
    #[error("engine agent response persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    /// Lowering overflowed fixed-point. Carried as text because `EngineError` is neither `Clone`
    /// nor `PartialEq`; the message is preserved verbatim so the failure stays diagnosable.
    #[error("engine agent response lowering failed: {0}")]
    Lowering(String),
    /// A readback could not be encoded or decoded.
    #[error("engine agent response codec failed: {0}")]
    Codec(String),
}
