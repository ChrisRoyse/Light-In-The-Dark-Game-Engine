//! Ward subject constructors — the three things this engine will submit to the Ward gate.
//!
//! Split out of `calyx_ward_gate.rs` in #1548 (that file was at its 500-line ceiling). The
//! constructors are re-exported from [`crate::calyx_ward_gate`], so every existing
//! `litd_engine::calyx_ward_gate::ward_subject_from_*` import path is unchanged.
//!
//! The three subjects are not peers. Two of them describe content **we authored**:
//! [`ward_subject_from_content`] and [`ward_subject_from_agent_activation`] (the activation is
//! the prompt we are about to send). The third, [`ward_subject_from_agent_response`], describes
//! **what a third-party model said back** — and that is the only one carrying genuinely untrusted
//! bytes. #1548 existed because it was missing.

use crate::{
    agent_response::EngineAgentProposal,
    agent_response_gate::EngineAgentBinding,
    calyx_agent_runtime::EngineCalyxAgentActivation,
    calyx_content::{
        ENGINE_CALYX_CONTENT_READBACK_SCHEMA, EngineContentConstellationReadback, EngineContentKind,
    },
    calyx_ward_gate::{
        EngineCalyxWardGateError, EngineCalyxWardSubject, EngineCalyxWardSubjectKind, invalid,
        normalize_refs,
    },
    calyx_ward_gate_support::{append_json_text, movement_units},
};

/// Ward subject for a piece of authored content.
pub fn ward_subject_from_content(
    readback: &EngineContentConstellationReadback,
) -> Result<EngineCalyxWardSubject, EngineCalyxWardGateError> {
    if readback.schema != ENGINE_CALYX_CONTENT_READBACK_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {ENGINE_CALYX_CONTENT_READBACK_SCHEMA}"),
        );
    }
    if readback.input_sha256.trim().is_empty() || readback.source_path.trim().is_empty() {
        return invalid("readback.provenance", "content provenance must be intact");
    }
    let mut text = format!(
        "{} {} {}",
        readback.kind.as_str(),
        readback.id,
        readback.display_name
    );
    append_json_text(&readback.payload, &mut text);
    let mut grounding_refs = vec![
        readback.source_path.clone(),
        readback.database.sha256.clone(),
    ];
    grounding_refs.extend(
        readback
            .constellation
            .anchors
            .iter()
            .map(|anchor| anchor.source.clone()),
    );
    Ok(EngineCalyxWardSubject {
        subject_id: format!("content/{}", readback.id),
        kind: EngineCalyxWardSubjectKind::Content,
        content_kind: Some(readback.kind),
        text,
        grounding_refs: normalize_refs(grounding_refs)?,
        provenance_refs: vec![format!("calyx://content/{}", readback.cx_id)],
        movement_units_per_tick: 0,
        teleport_requested: false,
        scale_milli: 1000,
        emissive_milli: 0,
        source_fingerprint: readback.input_sha256.clone(),
    })
}

/// Ward subject for an agent **activation** — i.e. the prompt we are about to send.
///
/// This gates content we authored. It is a useful check (a malformed authored prompt template is
/// a real bug) but it is NOT an output-handling defense, and #1548 exists because it was mistaken
/// for one.
pub fn ward_subject_from_agent_activation(
    activation: &EngineCalyxAgentActivation,
) -> Result<EngineCalyxWardSubject, EngineCalyxWardGateError> {
    let movement_units_per_tick = movement_units(&activation.lowered_order.kind)?;
    Ok(EngineCalyxWardSubject {
        subject_id: format!("agent_activation/{}", activation.activation_id),
        kind: EngineCalyxWardSubjectKind::AgentActivation,
        content_kind: Some(EngineContentKind::Agent),
        text: activation.prompt.clone(),
        grounding_refs: normalize_refs(activation.provenance_refs.clone())?,
        provenance_refs: activation.provenance_refs.clone(),
        movement_units_per_tick,
        teleport_requested: false,
        scale_milli: 1000,
        emissive_milli: 0,
        source_fingerprint: activation.order_fingerprint.to_string(),
    })
}

/// Ward subject for an agent **response** — what the model said back (#1548).
///
/// This is the untrusted one. It is reached only from the admission choke point
/// ([`crate::agent_response_gate::admit_agent_response`]), only after the response has already
/// survived the wire-fact, JSON Schema, serde, and binding gates — so by the time Ward sees it,
/// the value is a fully-typed [`EngineAgentProposal`] and the only attacker-controlled bytes left
/// in it are `rationale`.
///
/// Two deliberate choices:
///
/// * **The verb tag is prepended to the scored text.** A verb the trusted corpus never uses is
///   then lexically novel in its own right, so the cosine guard has something to say about the
///   *action* and not only the prose.
/// * **Movement units round UP** ([`EngineAgentVerb::movement_units_ceil`]), unlike
///   [`crate::calyx_ward_gate_support::movement_units`], which truncates. Truncation is fine for
///   authored content whose goldens depend on it; for an untrusted proposal it would hand the
///   model a free sub-unit of movement above the envelope.
///
/// [`EngineAgentVerb::movement_units_ceil`]: crate::agent_response::EngineAgentVerb::movement_units_ceil
pub fn ward_subject_from_agent_response(
    admission_id: &str,
    binding: &EngineAgentBinding,
    proposal: &EngineAgentProposal,
    response_sha256: &str,
) -> Result<EngineCalyxWardSubject, EngineCalyxWardGateError> {
    if admission_id.trim().is_empty() {
        return invalid("admission_id", "admission id must be non-empty");
    }
    if response_sha256.trim().is_empty() {
        return invalid(
            "response_sha256",
            "response fingerprint must be non-empty — a Ward subject with no provenance is not gateable",
        );
    }
    Ok(EngineCalyxWardSubject {
        subject_id: format!("agent_response/{admission_id}"),
        kind: EngineCalyxWardSubjectKind::AgentResponse,
        content_kind: Some(EngineContentKind::Agent),
        text: format!(
            "{} {}",
            proposal.verb.tag().as_str(),
            proposal.rationale.trim()
        ),
        grounding_refs: normalize_refs(binding.grounding_refs.clone())?,
        provenance_refs: binding.grounding_refs.clone(),
        movement_units_per_tick: proposal.verb.movement_units_ceil(),
        teleport_requested: false,
        scale_milli: 1000,
        emissive_milli: 0,
        source_fingerprint: response_sha256.to_string(),
    })
}
