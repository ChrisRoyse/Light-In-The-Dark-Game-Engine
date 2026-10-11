//! Private helpers for the agent-response admission choke point (#1548).
//!
//! [`lower_proposal`] lives here and is `pub(crate)`: it is the sole mapping from a model-supplied
//! [`EngineAgentProposal`] to a [`SimOrder`], and it is reachable from exactly one call site —
//! [`crate::agent_response_gate::admit_agent_response`], after all six gates have passed.

use litd_hash::sha256_hex;
use serde_json::json;

use crate::{
    agent_response::{EngineAgentProposal, EngineAgentVelocity, EngineAgentVerb},
    agent_response_gate::{
        CODE_REFUSED_TRUNCATED, CODE_REFUSED_WIRE, EngineAgentAdmissionStage, EngineAgentBinding,
        EngineAgentOrderEnvelope, EngineAgentResponseError, EngineAgentResponseReadback,
        EngineAgentWireResponse,
    },
    error::EngineError,
    math::{Fixed, FixedVec3},
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

/// The only finish reason that may be parsed.
const FINISH_REASON_STOP: &str = "stop";
/// The provider's truncation signal.
const FINISH_REASON_LENGTH: &str = "length";

/// GATE 1 — the wire facts, checked **before the body is parsed**.
///
/// `finish_reason == "length"` means the provider stopped mid-token. The body is a fragment of a
/// decision, and a fragment of a decision is not a smaller decision — it is a different one. The
/// DeepSeek docs concede that `json_object` mode "may occasionally return empty content" and can
/// truncate JSON midway, so this is a live failure mode, not a theoretical one.
///
/// Returns `Some((code, reason))` when the response must be refused.
pub(crate) fn check_wire_facts(wire: &EngineAgentWireResponse) -> Option<(&'static str, String)> {
    if wire.response_id.trim().is_empty() {
        return Some((
            CODE_REFUSED_WIRE,
            "provider response carried no request id — it is not traceable and cannot be admitted"
                .to_string(),
        ));
    }
    if wire.finish_reason == FINISH_REASON_LENGTH {
        return Some((
            CODE_REFUSED_TRUNCATED,
            format!(
                "provider truncated the body (finish_reason={FINISH_REASON_LENGTH:?}); refused before parsing (request id {})",
                wire.response_id
            ),
        ));
    }
    if wire.finish_reason != FINISH_REASON_STOP {
        return Some((
            CODE_REFUSED_WIRE,
            format!(
                "provider finish_reason was {:?}, only {FINISH_REASON_STOP:?} is admissible (request id {})",
                wire.finish_reason, wire.response_id
            ),
        ));
    }
    if wire.content.trim().is_empty() {
        return Some((
            CODE_REFUSED_WIRE,
            format!(
                "provider returned a blank body (request id {})",
                wire.response_id
            ),
        ));
    }
    None
}

/// GATE 6 — the deterministic envelope. Reject, never clamp.
///
/// Clamping would silently turn an out-of-envelope proposal into a legal order and destroy the
/// evidence that an attack occurred. A refusal is loud and leaves a ledger row.
pub(crate) fn check_envelope(
    envelope: &EngineAgentOrderEnvelope,
    binding: &EngineAgentBinding,
    proposal: &EngineAgentProposal,
) -> Option<String> {
    let tag = proposal.verb.tag();
    if !envelope.allowed_verbs.contains(&tag) {
        return Some(format!(
            "verb {:?} is not on this deployment's allowlist",
            tag.as_str()
        ));
    }
    if proposal.rationale.chars().count() > envelope.max_rationale_chars {
        return Some(format!(
            "rationale is {} chars, envelope allows {}",
            proposal.rationale.chars().count(),
            envelope.max_rationale_chars
        ));
    }
    let units = proposal.verb.movement_units_ceil();
    if units > envelope.max_move_units_per_tick {
        return Some(format!(
            "movement is {units} units/tick, envelope allows {}",
            envelope.max_move_units_per_tick
        ));
    }
    if let EngineAgentVerb::Attack(attack) = &proposal.verb {
        if attack.damage_milli > envelope.max_damage_milli {
            return Some(format!(
                "damage is {} milli, envelope allows {}",
                attack.damage_milli, envelope.max_damage_milli
            ));
        }
        if !envelope.allow_self_attack && attack.attacker_body == binding.target_body.index() {
            return Some("proposal is a self-attack and this deployment forbids them".to_string());
        }
    }
    None
}

/// The SOLE mapping from a model-supplied proposal to a sim order.
///
/// Look at what comes from where. The tick, the order source, the sequence, and the target body
/// are all taken from the **authored** binding. The model contributes exactly one thing: the
/// [`SimOrderKind`], and every number inside it has already passed a JSON Schema range check and a
/// deployment envelope check. That is the whole structural claim of #1548, and it fits on a page.
///
/// [`EngineAgentVerb::Hold`] lowers to `None` — the named safe default. "Do nothing" is a
/// first-class answer, not an absence of one.
pub(crate) fn lower_proposal(
    binding: &EngineAgentBinding,
    proposal: &EngineAgentProposal,
) -> Result<Option<SimOrder>, EngineError> {
    let kind = match &proposal.verb {
        EngineAgentVerb::Hold(_) => return Ok(None),
        EngineAgentVerb::SetVelocity(vector) => SimOrderKind::SetVelocity(fixed_vec3(vector)?),
        EngineAgentVerb::AddVelocity(vector) => SimOrderKind::AddVelocity(fixed_vec3(vector)?),
        EngineAgentVerb::SetAcceleration(vector) => {
            SimOrderKind::SetAcceleration(fixed_vec3(vector)?)
        }
        EngineAgentVerb::Attack(attack) => SimOrderKind::ApplyAttack {
            attacker: SimBodyId::from_raw(attack.attacker_body),
            damage_fp: Fixed::from_ratio(i64::from(attack.damage_milli), 1_000)?.raw(),
        },
    };
    Ok(Some(SimOrder::new(
        binding.execute_tick,
        SimOrderSource::lowered_agent(binding.source_id),
        binding.sequence,
        binding.target_body,
        kind,
    )))
}

/// Milli-integers to Q32.32 fixed point. Exact: no float ever enters the sim.
fn fixed_vec3(vector: &EngineAgentVelocity) -> Result<FixedVec3, EngineError> {
    Ok(FixedVec3::new(
        Fixed::from_ratio(i64::from(vector.x_milli), 1_000)?,
        Fixed::from_ratio(i64::from(vector.y_milli), 1_000)?,
        Fixed::from_ratio(i64::from(vector.z_milli), 1_000)?,
    ))
}

pub(crate) fn validate_binding(
    binding: &EngineAgentBinding,
) -> Result<(), EngineAgentResponseError> {
    if binding.agent_id.trim().is_empty() {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "binding.agent_id".to_string(),
            reason: "binding agent id must be non-empty".to_string(),
        });
    }
    if binding.grounding_refs.is_empty()
        || binding
            .grounding_refs
            .iter()
            .any(|item| item.trim().is_empty())
    {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "binding.grounding_refs".to_string(),
            reason: "an ungrounded binding cannot be Ward-gated".to_string(),
        });
    }
    Ok(())
}

/// Mark a readback refused at `stage` with `code`, appending `reason`.
pub(crate) fn refused(
    mut readback: EngineAgentResponseReadback,
    stage: EngineAgentAdmissionStage,
    code: &str,
    reason: impl Into<String>,
) -> EngineAgentResponseReadback {
    readback.reasons.push(reason.into());
    refusal(readback, stage, code.to_string())
}

/// Mark a readback refused at `stage` with an already-computed `code` and reasons.
pub(crate) fn refusal(
    mut readback: EngineAgentResponseReadback,
    stage: EngineAgentAdmissionStage,
    code: String,
) -> EngineAgentResponseReadback {
    readback.stage = stage;
    readback.admitted = false;
    readback.lowering_allowed = false;
    readback.code = code;
    readback.order = None;
    readback.order_fingerprint = None;
    readback.order_row_key = None;
    readback
}

pub(crate) fn order_row_key(admission_id: &str) -> String {
    format!("lowered_agent_order/{admission_id}")
}

/// Fingerprint over everything that decided the outcome.
pub(crate) fn admission_fingerprint(
    readback: &EngineAgentResponseReadback,
) -> Result<String, EngineAgentResponseError> {
    serde_json::to_vec(&json!({
        "schema": readback.schema,
        "admission_id": readback.admission_id,
        "stage": readback.stage,
        "admitted": readback.admitted,
        "lowering_allowed": readback.lowering_allowed,
        "code": readback.code,
        "reasons": readback.reasons,
        "schema_errors": readback.schema_errors,
        "proposal": readback.proposal,
        "order": readback.order,
        "response_sha256": readback.response_sha256,
        "prompt_sha256": readback.prompt_sha256,
        "schema_sha256": readback.schema_sha256,
        "response_id": readback.response_id,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(|error| EngineAgentResponseError::Codec(error.to_string()))
}
