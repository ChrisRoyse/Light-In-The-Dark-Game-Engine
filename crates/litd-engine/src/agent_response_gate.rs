//! The agent-response admission choke point (#1548).
//!
//! # The bug this closes
//!
//! Ward gated the **prompt** (`ward_subject_from_agent_activation`) and nothing gated the
//! **response**. The prompt is content we authored; the response is untrusted input from a
//! third-party system. Gating the former and not the latter is backwards from the threat model,
//! and it is OWASP **LLM05 — Improper Output Handling**: treating model output as trusted and
//! passing it into another system without validation.
//!
//! # The six gates
//!
//! [`admit_agent_response`] is the ONLY function in this workspace that can turn provider bytes
//! into a [`SimOrder`]. The order of its gates is not negotiable and the tests assert it:
//!
//! | # | gate | what it stops |
//! |---|------|---------------|
//! | 1 | wire facts | a truncated (`finish_reason == "length"`) or blank body — **before any parse** |
//! | 2 | JSON Schema | `x_milli: 250000`, which serde would happily accept into an `i32` |
//! | 3 | serde | unknown fields, unknown verbs, wrong types |
//! | 4 | binding | a proposal aimed at an agent or body it was not bound to |
//! | 5 | Ward | novel / ungrounded free text, via `guard(.., high_stakes = true)` |
//! | 6 | envelope | a verb not on the allowlist, or numbers outside the deployment's bounds |
//!
//! # Ward is layer 5 of 6, and it is not the defense
//!
//! Be precise about this, because getting it wrong is how a system ends up with a security theatre
//! and a breach. `calyx_ward_gate_support::text_vector` is a hashed bag-of-n-grams with cosine
//! scoring — a **novelty detector**. It will flag distributionally weird text. It will NOT
//! reliably flag a fluent, on-topic injection ("the guard now trusts the player"). The published
//! record is unambiguous that detectors of this class fall to adaptive attacks.
//!
//! The load-bearing defense is **structural**: the model cannot emit a [`SimOrder`], only an
//! [`EngineAgentProposal`] over a closed enum with hard numeric ranges. A fully jailbroken model's
//! worst case is a *legal, in-envelope* order — because an illegal one is not a value of the type.
//! Ward is genuinely useful (it also fail-closes on an uncalibrated profile), but if Ward were
//! deleted tomorrow the structural guarantee would still hold. That is the property to protect.
//!
//! # The invariant
//!
//! ```text
//! readback.admitted == readback.lowering_allowed
//! readback.order.is_some() == (readback.admitted && verb != Hold)
//! readback.order_row_key.is_some() == readback.order.is_some()
//! ```
//!
//! Asserted inside the choke point, re-asserted by [`validate_agent_response_readback`] on the way
//! back out of the store (so a tampered row cannot lower), and proved on disk by the FSV: an
//! admitted response leaves a `lowered_agent_order` row; a refused one leaves only a ledger row,
//! and `get_record(LoweredAgentOrder, ..)` returns [`MissingRecord`].
//!
//! [`MissingRecord`]: crate::calyx_persistence::EngineCalyxPersistenceError::MissingRecord

use litd_hash::sha256_hex;
use serde_json::json;

use crate::{
    agent_response::{
        EngineAgentProposal, EngineAgentVerb, proposal_schema_sha256, validate_proposal_body,
    },
    agent_response_gate_support::{
        admission_fingerprint, check_envelope, check_wire_facts, lower_proposal, order_row_key,
        refusal, refused, validate_binding,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxRecord, EngineCalyxRecordKind},
    calyx_ward_gate::{
        EngineCalyxWardPolicy, evaluate_calyx_ward_gate, validate_gate_readback,
        ward_subject_from_agent_response,
    },
    sim_order::SimOrder,
};

// The data contracts live in `agent_response_gate_types` (#800 sec.6 line ceiling). Re-exported so
// the public path `litd_engine::agent_response_gate::*` is unchanged.
pub use crate::agent_response_gate_types::{
    CODE_ADMITTED, CODE_REFUSED_BINDING, CODE_REFUSED_ENVELOPE, CODE_REFUSED_SCHEMA,
    CODE_REFUSED_TRUNCATED, CODE_REFUSED_WIRE, ENGINE_AGENT_RESPONSE_ADMISSION_SCHEMA,
    EngineAgentAdmissionStage, EngineAgentBinding, EngineAgentOrderEnvelope,
    EngineAgentResponseError, EngineAgentResponseReadback, EngineAgentWireResponse,
};

/// **THE choke point.** The only path from provider bytes to a [`SimOrder`].
///
/// Returns `Ok(readback)` for both admission and refusal — see [`EngineAgentResponseError`] for
/// why. Writes an `agent_response_ledger` row on **every** call, and a `lowered_agent_order` row
/// **iff** an order was lowered.
///
/// There is no second path. [`EngineAgentProposal`] has no public conversion to a [`SimOrder`];
/// the mapping is `agent_response_gate_support::lower_proposal`, which is `pub(crate)` and called
/// from exactly here.
pub fn admit_agent_response(
    database: &EngineCalyxDatabase,
    admission_id: &str,
    binding: &EngineAgentBinding,
    policy: &EngineCalyxWardPolicy,
    envelope: &EngineAgentOrderEnvelope,
    wire: &EngineAgentWireResponse,
) -> Result<EngineAgentResponseReadback, EngineAgentResponseError> {
    if admission_id.trim().is_empty() {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "admission_id".to_string(),
            reason: "admission id must be non-empty".to_string(),
        });
    }
    validate_binding(binding)?;
    let response_sha256 = sha256_hex(wire.content.as_bytes());
    let mut readback = blank_readback(admission_id, wire, &response_sha256);

    // GATE 1 — wire facts. Runs BEFORE any parse. A body the provider truncated mid-JSON is not
    // data, it is a fragment; parsing it is how you end up acting on half a decision.
    if let Some((code, reason)) = check_wire_facts(wire) {
        return finish(
            database,
            refused(readback, EngineAgentAdmissionStage::Wire, code, reason),
        );
    }

    // GATE 2 + 3 — JSON Schema, then serde into the closed types.
    let proposal = match validate_proposal_body(&wire.content) {
        Ok(proposal) => proposal,
        Err(schema_errors) => {
            let reason = format!("{} schema violation(s)", schema_errors.len());
            readback.schema_errors = schema_errors;
            return finish(
                database,
                refused(
                    readback,
                    EngineAgentAdmissionStage::Schema,
                    CODE_REFUSED_SCHEMA,
                    reason,
                ),
            );
        }
    };
    readback.proposal = Some(proposal.clone());

    // GATE 4 — binding. The model restated who it is and what it may touch; check it.
    if let Some(reason) = binding_mismatch(binding, &proposal) {
        return finish(
            database,
            refused(
                readback,
                EngineAgentAdmissionStage::Binding,
                CODE_REFUSED_BINDING,
                reason,
            ),
        );
    }

    // GATE 5 — Ward, on the response's free text, through the ONE Ward call site.
    // `evaluate_calyx_ward_gate` -> `evaluate_subject` calls `guard(.., high_stakes = true)`.
    // We do not call `calyx_ward::guard` directly; there is one Ward call site and it stays that way.
    let subject =
        ward_subject_from_agent_response(admission_id, binding, &proposal, &response_sha256)?;
    let gate = evaluate_calyx_ward_gate(
        format!("agent-response/{admission_id}"),
        policy.clone(),
        &[subject],
    )?;
    validate_gate_readback(&gate)?;
    let verdict = gate
        .verdicts
        .first()
        .ok_or_else(|| EngineAgentResponseError::InvalidInput {
            field: "ward_gate".to_string(),
            reason: "Ward gate returned no verdict for the response subject".to_string(),
        })?
        .clone();
    let ward_code = verdict.code.clone();
    let ward_reasons = verdict.reasons.clone();
    let ward_admitted = verdict.admitted;
    readback.gate = Some(gate);
    if !ward_admitted {
        readback.reasons = ward_reasons;
        return finish(
            database,
            refusal(readback, EngineAgentAdmissionStage::Ward, ward_code),
        );
    }

    // GATE 6 — the deterministic envelope. Reject, never clamp.
    if let Some(reason) = check_envelope(envelope, binding, &proposal) {
        return finish(
            database,
            refused(
                readback,
                EngineAgentAdmissionStage::Envelope,
                CODE_REFUSED_ENVELOPE,
                reason,
            ),
        );
    }

    // ADMITTED. Lower. This is the only reachable call to `lower_proposal` in the workspace.
    let order = lower_proposal(binding, &proposal)
        .map_err(|error| EngineAgentResponseError::Lowering(error.to_string()))?;
    readback.stage = EngineAgentAdmissionStage::Lowered;
    readback.admitted = true;
    readback.lowering_allowed = true;
    readback.code = CODE_ADMITTED.to_string();
    readback.order_fingerprint = order.map(SimOrder::fingerprint);
    readback.order_row_key = order.map(|_| order_row_key(admission_id));
    readback.order = order;
    finish(database, readback)
}

/// Re-assert the choke-point invariant on a readback.
///
/// Called on the way *out of the store* as well as on the way in, so a row that was tampered with
/// on disk to flip `lowering_allowed` cannot be replayed into an order.
pub fn validate_agent_response_readback(
    readback: &EngineAgentResponseReadback,
) -> Result<(), EngineAgentResponseError> {
    if readback.schema != ENGINE_AGENT_RESPONSE_ADMISSION_SCHEMA {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "schema".to_string(),
            reason: format!("expected {ENGINE_AGENT_RESPONSE_ADMISSION_SCHEMA}"),
        });
    }
    if readback.admitted != readback.lowering_allowed {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "lowering_allowed".to_string(),
            reason: "only an admitted response may lower".to_string(),
        });
    }
    if !readback.admitted && readback.order.is_some() {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "order".to_string(),
            reason: "a refused response must carry no lowered order".to_string(),
        });
    }
    if readback.order.is_some() != readback.order_row_key.is_some() {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "order_row_key".to_string(),
            reason: "an order and its store row must exist together".to_string(),
        });
    }
    if readback.order.map(SimOrder::fingerprint) != readback.order_fingerprint {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "order_fingerprint".to_string(),
            reason: "order fingerprint mismatch".to_string(),
        });
    }
    let expected_admitted_verb_lowers = readback
        .proposal
        .as_ref()
        .is_some_and(|proposal| !matches!(proposal.verb, EngineAgentVerb::Hold(_)));
    if readback.admitted && expected_admitted_verb_lowers != readback.order.is_some() {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "order".to_string(),
            reason: "an admitted non-hold proposal must lower to exactly one order".to_string(),
        });
    }
    if readback.admission_fingerprint != admission_fingerprint(readback)? {
        return Err(EngineAgentResponseError::InvalidInput {
            field: "admission_fingerprint".to_string(),
            reason: "admission fingerprint mismatch".to_string(),
        });
    }
    Ok(())
}

/// Read an admission ledger row back, re-validating the invariant.
pub fn read_agent_response_admission(
    database: &EngineCalyxDatabase,
    admission_id: &str,
) -> Result<EngineAgentResponseReadback, EngineAgentResponseError> {
    let stored = database.get_record(EngineCalyxRecordKind::AgentResponseLedger, admission_id)?;
    let readback = serde_json::from_value::<EngineAgentResponseReadback>(stored.payload)
        .map_err(|error| EngineAgentResponseError::Codec(error.to_string()))?;
    validate_agent_response_readback(&readback)?;
    Ok(readback)
}

/// Read a lowered order back from the order store.
///
/// Returns [`EngineCalyxPersistenceError::MissingRecord`] when no order was lowered. That error is
/// the FSV's absence proof — it is a *feature*, not a failure.
pub fn read_lowered_agent_order(
    database: &EngineCalyxDatabase,
    admission_id: &str,
) -> Result<SimOrder, EngineAgentResponseError> {
    let stored = database.get_record(EngineCalyxRecordKind::LoweredAgentOrder, admission_id)?;
    let order = stored
        .payload
        .get("order")
        .cloned()
        .ok_or_else(|| EngineAgentResponseError::Codec("order row has no order".to_string()))?;
    serde_json::from_value::<SimOrder>(order)
        .map_err(|error| EngineAgentResponseError::Codec(error.to_string()))
}

fn binding_mismatch(
    binding: &EngineAgentBinding,
    proposal: &EngineAgentProposal,
) -> Option<String> {
    if proposal.agent_id != binding.agent_id {
        return Some(format!(
            "proposal claims agent {:?} but the binding is agent {:?}",
            proposal.agent_id, binding.agent_id
        ));
    }
    if proposal.target_body != binding.target_body.index() {
        return Some(format!(
            "proposal targets body {} but the binding is body {}",
            proposal.target_body,
            binding.target_body.index()
        ));
    }
    None
}

fn blank_readback(
    admission_id: &str,
    wire: &EngineAgentWireResponse,
    response_sha256: &str,
) -> EngineAgentResponseReadback {
    EngineAgentResponseReadback {
        schema: ENGINE_AGENT_RESPONSE_ADMISSION_SCHEMA.to_string(),
        admission_id: admission_id.to_string(),
        stage: EngineAgentAdmissionStage::Wire,
        admitted: false,
        lowering_allowed: false,
        code: String::new(),
        reasons: Vec::new(),
        schema_errors: Vec::new(),
        gate: None,
        proposal: None,
        order: None,
        order_fingerprint: None,
        model: wire.model.clone(),
        finish_reason: wire.finish_reason.clone(),
        response_sha256: response_sha256.to_string(),
        prompt_sha256: wire.prompt_sha256.clone(),
        schema_sha256: proposal_schema_sha256(),
        response_id: wire.response_id.clone(),
        ledger_row_key: format!("agent_response_ledger/{admission_id}"),
        order_row_key: None,
        admission_fingerprint: String::new(),
    }
}

/// Seal the readback and write it to the stores. Ledger always; order iff one exists.
fn finish(
    database: &EngineCalyxDatabase,
    mut readback: EngineAgentResponseReadback,
) -> Result<EngineAgentResponseReadback, EngineAgentResponseError> {
    readback.admission_fingerprint = admission_fingerprint(&readback)?;
    validate_agent_response_readback(&readback)?;

    let ledger = EngineCalyxRecord::new(
        EngineCalyxRecordKind::AgentResponseLedger,
        readback.admission_id.clone(),
        serde_json::to_value(&readback)
            .map_err(|error| EngineAgentResponseError::Codec(error.to_string()))?,
        json!({
            "source_issue": 1548,
            "stage": readback.stage,
            "code": readback.code,
            "admitted": readback.admitted,
            "response_id": readback.response_id,
            "response_sha256": readback.response_sha256,
            "schema_sha256": readback.schema_sha256,
        }),
    );
    database.put_record(&ledger)?;

    if let Some(order) = readback.order {
        let row = EngineCalyxRecord::new(
            EngineCalyxRecordKind::LoweredAgentOrder,
            readback.admission_id.clone(),
            json!({
                "order": order,
                "order_fingerprint": order.fingerprint(),
                "admission_id": readback.admission_id,
            }),
            json!({
                "source_issue": 1548,
                "admitted_by": readback.code,
                "response_id": readback.response_id,
                "response_sha256": readback.response_sha256,
            }),
        );
        database.put_record(&row)?;
    }
    Ok(readback)
}
