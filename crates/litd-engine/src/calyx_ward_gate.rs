//! Engine-side Calyx Ward grounding and envelope gate for authored content.

use std::collections::BTreeSet;

use calyx_ward::{CALYX_GUARD_OOD, WardError};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    calyx_content::EngineContentKind,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
    calyx_ward_gate_support::{
        EngineWardGuard, build_ward_guard, evaluate_ward_similarity, gate_fingerprint, token_set,
    },
};

// The subject constructors live in `calyx_ward_subjects` (#1548 — this file was at its 500-line
// ceiling). Re-exported here so every existing `calyx_ward_gate::ward_subject_from_*` path still
// resolves.
pub use crate::calyx_ward_subjects::{
    ward_subject_from_agent_activation, ward_subject_from_agent_response, ward_subject_from_content,
};

pub const ENGINE_CALYX_WARD_GATE_SCHEMA: &str = "litd.engine.calyx_ward_gate.v1";
pub const ENGINE_CALYX_WARD_GATE_READBACK_SCHEMA: &str = "litd.engine.calyx_ward_gate_readback.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxWardPolicy {
    pub policy_id: String,
    pub allowed_terms: Vec<String>,
    pub blocked_terms: Vec<String>,
    pub trusted_texts: Vec<String>,
    pub calibration_ood_texts: Vec<String>,
    pub min_grounding_refs: usize,
    pub min_matched_terms: usize,
    /// DEFENSE IN DEPTH (decision, #1552 / docs2/decisions/DEC-1552): duplicated intentionally with
    /// [`crate::agent_response_gate::EngineAgentOrderEnvelope::max_move_units_per_tick`]. This bound
    /// is checked on the Ward *subject* (the movement claimed by the response text) before lowering;
    /// the envelope bound is checked on the *lowered order kind* after. Keep the two equal — the
    /// order envelope is authoritative and must never be relaxed on the strength of Ward passing.
    pub max_move_units_per_tick: u32,
    pub allow_teleport: bool,
    pub max_scale_milli: u32,
    pub max_emissive_milli: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineCalyxWardSubjectKind {
    Content,
    /// The prompt we are about to send. Authored by us.
    AgentActivation,
    /// What the model said back. Untrusted third-party input (#1548).
    AgentResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCalyxWardSubject {
    pub subject_id: String,
    pub kind: EngineCalyxWardSubjectKind,
    pub content_kind: Option<EngineContentKind>,
    pub text: String,
    pub grounding_refs: Vec<String>,
    pub provenance_refs: Vec<String>,
    pub movement_units_per_tick: u32,
    pub teleport_requested: bool,
    pub scale_milli: u32,
    pub emissive_milli: u32,
    pub source_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxWardVerdict {
    pub subject_id: String,
    pub kind: EngineCalyxWardSubjectKind,
    pub admitted: bool,
    pub lowering_allowed: bool,
    pub code: String,
    pub reasons: Vec<String>,
    pub matched_terms: Vec<String>,
    pub blocked_terms: Vec<String>,
    pub grounding_refs: Vec<String>,
    pub legacy_token_admission_would_pass: bool,
    pub ward_overall_pass: bool,
    pub ward_provisional: bool,
    pub ward_slots: Vec<EngineCalyxWardSlotVerdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxWardSlotVerdict {
    pub slot: u16,
    pub cos: f32,
    pub tau: f32,
    pub pass: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxWardCalibrationReadback {
    pub guard_id: String,
    pub calibrated: bool,
    pub estimator: String,
    pub far: f32,
    pub frr: f32,
    pub confidence: f32,
    pub required_slots: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxWardGateReadback {
    pub schema: String,
    pub gate_id: String,
    pub policy: EngineCalyxWardPolicy,
    pub ward_calibration: EngineCalyxWardCalibrationReadback,
    pub subject_count: usize,
    pub admitted_count: usize,
    pub refused_count: usize,
    pub fail_closed: bool,
    pub verdicts: Vec<EngineCalyxWardVerdict>,
    pub gate_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxWardGateStoredReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub gate: EngineCalyxWardGateReadback,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineCalyxWardGateError {
    #[error("engine Calyx Ward field {field} is invalid: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("engine Calyx Ward persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx Ward codec failed: {0}")]
    Codec(String),
}

pub fn evaluate_calyx_ward_gate(
    gate_id: impl Into<String>,
    policy: EngineCalyxWardPolicy,
    subjects: &[EngineCalyxWardSubject],
) -> Result<EngineCalyxWardGateReadback, EngineCalyxWardGateError> {
    let gate_id = gate_id.into();
    validate_policy(&policy)?;
    if gate_id.trim().is_empty() {
        return invalid("gate_id", "gate id must be non-empty");
    }
    if subjects.is_empty() {
        return invalid("subjects", "Ward gate requires at least one subject");
    }
    let ward_guard = build_ward_guard(&policy)?;
    let mut verdicts = subjects
        .iter()
        .map(|subject| evaluate_subject(&policy, &ward_guard, subject))
        .collect::<Result<Vec<_>, _>>()?;
    verdicts.sort_by(|left, right| left.subject_id.cmp(&right.subject_id));
    let admitted_count = verdicts.iter().filter(|verdict| verdict.admitted).count();
    let refused_count = verdicts.len().saturating_sub(admitted_count);
    let mut readback = EngineCalyxWardGateReadback {
        schema: ENGINE_CALYX_WARD_GATE_SCHEMA.to_string(),
        gate_id,
        policy,
        ward_calibration: ward_guard.calibration.clone(),
        subject_count: subjects.len(),
        admitted_count,
        refused_count,
        fail_closed: refused_count > 0,
        verdicts,
        gate_fingerprint: String::new(),
    };
    readback.gate_fingerprint = gate_fingerprint(&readback)?;
    validate_gate_readback(&readback)?;
    Ok(readback)
}

pub fn store_calyx_ward_gate(
    database: &EngineCalyxDatabase,
    gate: &EngineCalyxWardGateReadback,
) -> Result<EngineCalyxWardGateStoredReadback, EngineCalyxWardGateError> {
    validate_gate_readback(gate)?;
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::WardGateLedger,
        gate.gate_id.clone(),
        serde_json::to_value(gate).map_err(codec_failed)?,
        json!({
            "source_schema": ENGINE_CALYX_WARD_GATE_SCHEMA,
            "gate_id": gate.gate_id,
            "gate_fingerprint": gate.gate_fingerprint,
            "admitted_count": gate.admitted_count,
            "refused_count": gate.refused_count
        }),
    );
    parse_stored_gate(database.put_record(&record)?)
}

pub fn read_calyx_ward_gate(
    database: &EngineCalyxDatabase,
    gate_id: &str,
) -> Result<EngineCalyxWardGateStoredReadback, EngineCalyxWardGateError> {
    if gate_id.trim().is_empty() {
        return invalid("gate_id", "gate id must be non-empty");
    }
    parse_stored_gate(database.get_record(EngineCalyxRecordKind::WardGateLedger, gate_id)?)
}

pub fn validate_gate_readback(
    gate: &EngineCalyxWardGateReadback,
) -> Result<(), EngineCalyxWardGateError> {
    if gate.schema != ENGINE_CALYX_WARD_GATE_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_CALYX_WARD_GATE_SCHEMA}"),
        );
    }
    if gate.subject_count != gate.verdicts.len()
        || gate.admitted_count + gate.refused_count != gate.subject_count
    {
        return invalid("counts", "gate counts must match verdicts");
    }
    if !gate.ward_calibration.calibrated
        || gate.ward_calibration.guard_id.trim().is_empty()
        || gate.ward_calibration.required_slots.is_empty()
        || gate.ward_calibration.estimator.trim().is_empty()
    {
        return invalid(
            "ward_calibration",
            "high-stakes Ward gates require calibrated profile readback",
        );
    }
    if gate
        .verdicts
        .iter()
        .any(|verdict| verdict.admitted != verdict.lowering_allowed)
    {
        return invalid(
            "verdict.lowering_allowed",
            "only admitted content may lower",
        );
    }
    if gate.gate_fingerprint != gate_fingerprint(gate)? {
        return invalid("gate_fingerprint", "gate fingerprint mismatch");
    }
    Ok(())
}

fn evaluate_subject(
    policy: &EngineCalyxWardPolicy,
    ward_guard: &EngineWardGuard,
    subject: &EngineCalyxWardSubject,
) -> Result<EngineCalyxWardVerdict, EngineCalyxWardGateError> {
    validate_subject(subject)?;
    let terms = token_set(&subject.text);
    let allowed = normalize_refs(policy.allowed_terms.clone())?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let blocked_policy = normalize_refs(policy.blocked_terms.clone())?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let matched_terms = terms.intersection(&allowed).cloned().collect::<Vec<_>>();
    let blocked_terms = terms
        .intersection(&blocked_policy)
        .cloned()
        .collect::<Vec<_>>();
    let mut reasons = Vec::new();
    if subject.grounding_refs.len() < policy.min_grounding_refs {
        reasons.push("insufficient grounding refs".to_string());
    }
    if subject.teleport_requested && !policy.allow_teleport {
        reasons.push("teleport is outside Ward movement envelope".to_string());
    }
    if subject.movement_units_per_tick > policy.max_move_units_per_tick {
        reasons.push("movement exceeds Ward envelope".to_string());
    }
    if subject.scale_milli > policy.max_scale_milli
        || subject.emissive_milli > policy.max_emissive_milli
    {
        reasons.push("visual envelope exceeded".to_string());
    }
    let legacy_token_admission_would_pass = reasons.is_empty()
        && matched_terms.len() >= policy.min_matched_terms
        && blocked_terms.is_empty();
    if matched_terms.len() < policy.min_matched_terms {
        reasons.push("insufficient matched grounded terms for token prefilter".to_string());
    }
    if !blocked_terms.is_empty() {
        reasons.push("token prefilter refused blocked terms before Ward admission".to_string());
    }
    let ward = evaluate_ward_similarity(ward_guard, subject)?;
    if !ward.verdict.overall_pass {
        reasons.push("calyx-ward dense-cosine conformal guard rejected subject".to_string());
    }
    if ward.verdict.provisional {
        reasons.push("calyx-ward guard is provisional".to_string());
    }
    let admitted = reasons.is_empty();
    let code = if admitted {
        "LITD-WARD-ADMITTED"
    } else if subject.movement_units_per_tick > policy.max_move_units_per_tick
        || (subject.teleport_requested && !policy.allow_teleport)
        || subject.scale_milli > policy.max_scale_milli
        || subject.emissive_milli > policy.max_emissive_milli
    {
        "LITD-WARD-REFUSED-ENVELOPE"
    } else if !ward.verdict.overall_pass {
        CALYX_GUARD_OOD
    } else {
        "LITD-WARD-REFUSED-UNGROUNDED"
    };
    Ok(EngineCalyxWardVerdict {
        subject_id: subject.subject_id.clone(),
        kind: subject.kind,
        admitted,
        lowering_allowed: admitted,
        code: code.to_string(),
        reasons,
        matched_terms,
        blocked_terms,
        grounding_refs: subject.grounding_refs.clone(),
        legacy_token_admission_would_pass,
        ward_overall_pass: ward.verdict.overall_pass,
        ward_provisional: ward.verdict.provisional,
        ward_slots: ward
            .verdict
            .per_slot
            .iter()
            .map(|slot| EngineCalyxWardSlotVerdict {
                slot: slot.slot.get(),
                cos: slot.cos,
                tau: slot.tau,
                pass: slot.pass,
            })
            .collect(),
    })
}

fn validate_policy(policy: &EngineCalyxWardPolicy) -> Result<(), EngineCalyxWardGateError> {
    if policy.policy_id.trim().is_empty()
        || policy.allowed_terms.is_empty()
        || policy.trusted_texts.is_empty()
        || policy.calibration_ood_texts.len() < calyx_ward::MIN_BAD_SCORES
        || policy.min_grounding_refs == 0
        || policy.min_matched_terms == 0
        || policy.max_move_units_per_tick == 0
        || policy.max_scale_milli == 0
    {
        return invalid(
            "policy",
            "policy id, terms, thresholds, and envelopes are required",
        );
    }
    normalize_refs(policy.allowed_terms.clone())?;
    normalize_refs(policy.blocked_terms.clone())?;
    if policy
        .trusted_texts
        .iter()
        .any(|text| text.trim().is_empty())
    {
        return invalid("policy.trusted_texts", "trusted texts must be non-empty");
    }
    if policy
        .calibration_ood_texts
        .iter()
        .any(|text| text.trim().is_empty())
    {
        return invalid(
            "policy.calibration_ood_texts",
            "calibration OOD texts must be non-empty",
        );
    }
    Ok(())
}

fn validate_subject(subject: &EngineCalyxWardSubject) -> Result<(), EngineCalyxWardGateError> {
    if subject.subject_id.trim().is_empty()
        || subject.text.trim().is_empty()
        || subject.source_fingerprint.trim().is_empty()
    {
        return invalid(
            "subject",
            "subject id, text, and source fingerprint are required",
        );
    }
    if subject
        .grounding_refs
        .iter()
        .any(|item| item.trim().is_empty())
    {
        return invalid("subject.grounding_refs", "grounding refs must be non-empty");
    }
    Ok(())
}

fn parse_stored_gate(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineCalyxWardGateStoredReadback, EngineCalyxWardGateError> {
    let gate = serde_json::from_value::<EngineCalyxWardGateReadback>(database.payload.clone())
        .map_err(codec_failed)?;
    validate_gate_readback(&gate)?;
    Ok(EngineCalyxWardGateStoredReadback {
        schema: ENGINE_CALYX_WARD_GATE_READBACK_SCHEMA.to_string(),
        database,
        gate,
    })
}

pub(crate) fn normalize_refs(values: Vec<String>) -> Result<Vec<String>, EngineCalyxWardGateError> {
    let normalized = values
        .into_iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        return invalid("refs", "refs/terms must be non-empty");
    }
    Ok(normalized)
}

pub(crate) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxWardGateError> {
    Err(EngineCalyxWardGateError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    })
}

fn codec_failed(error: impl std::fmt::Display) -> EngineCalyxWardGateError {
    EngineCalyxWardGateError::Codec(error.to_string())
}

pub(crate) fn ward_failed(error: WardError) -> EngineCalyxWardGateError {
    EngineCalyxWardGateError::InvalidInput {
        field: "calyx_ward".to_string(),
        reason: format!("{}: {error}", error.code()),
    }
}
