//! Frozen roster identity for A37 DB admissions (#1511).
//!
//! An A37 admission must bind to the exact measured frozen roster and panel
//! artifact, not to slot numbers and lens names alone. This module captures the
//! frozen identity (frozen lens id, weights hash, runtime/shape fingerprint) for
//! every measured learned lens directly from the trusted local `VaultPanelState`
//! (the source of truth), and verifies a stored admission card against it. A
//! card from a different runtime/weights roster — even one that reuses names and
//! slots — is rejected loudly at `a37_admission` with expected/actual hashes and
//! no raw model bytes. There is no name-only compatibility fallback.

use calyx_core::{AbsentReason, Constellation, SlotId, SlotVector};
use calyx_registry::{FrozenLensContract, VaultPanelState};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{
    calyx_content::EngineCalyxContentError,
    calyx_content_a37_admission::{
        EngineContentA37AdmissionReport, EngineContentA37LensEvidence,
        validate_engine_content_a37_admission_report,
    },
};

pub const ENGINE_CONTENT_A37_ROSTER_SCHEMA: &str = "litd.engine.calyx_content.a37_frozen_roster.v1";

/// Measurement is complete: at least `min_learned_lenses` learned encoders were
/// measured into real vectors. This is the only state a retrieval/FSV/assay
/// measurement claim may rely on (#778 gate at its real scope, #1791).
pub const ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE: &str = "complete";
/// Measurement is pending: the explicit/deterministic constellation persisted
/// unconditionally, but the learned-encoder enrichment could not run because the
/// AI addon (ONNX runtime + models) was unavailable. Every declared learned slot
/// is `absent(LensUnavailable)` and awaits addon-time backfill (#1791). A pending
/// row is durable and readable, but NEVER satisfies a measurement claim.
pub const ENGINE_CONTENT_MEASUREMENT_STATE_PENDING: &str = "pending_measurement";

/// Identity of one declared learned lens whose measurement is deferred to
/// addon-time backfill (#1791). Carries the exact slot + frozen lens id so the
/// backfill pass fills the same cell and the two-phase provenance is auditable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineContentPendingLens {
    pub slot: u16,
    pub lens_id: String,
    pub name: String,
}

fn measurement_state_complete_default() -> String {
    ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE.to_string()
}

/// Frozen identity of a single measured learned lens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineContentA37FrozenLens {
    pub slot: u16,
    pub name: String,
    /// Content-addressed frozen lens id (hex).
    pub frozen_lens_id: String,
    /// Declared weights hash (hex).
    pub weights_sha256: String,
    /// Runtime/shape fingerprint (hex over shape, modality, dtype, norm, corpus).
    pub runtime_fingerprint: String,
}

/// The exact frozen roster and panel artifact a measurement was taken against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineContentA37FrozenRoster {
    pub schema: String,
    pub panel_template: String,
    pub panel_version: u32,
    /// Hash over the full panel slot layout (all slots, ordered).
    pub panel_state_sha256: String,
    /// Hash over the ordered learned frozen roster below.
    pub registry_roster_sha256: String,
    pub lenses: Vec<EngineContentA37FrozenLens>,
}

/// Structured measurement provenance persisted for content ingest (#1456/#1511).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineContentMeasurementReadback {
    pub schema: String,
    pub panel_template: String,
    pub panel_source: String,
    pub panel_version: u32,
    pub input_modality: String,
    pub min_learned_lenses: usize,
    pub measured_learned_lenses: usize,
    pub min_association_families: usize,
    pub association_family_count: usize,
    pub association_families: Vec<String>,
    pub measured_slot_ids: Vec<u16>,
    pub measured_lens_names: Vec<String>,
    /// Exact frozen roster identity for the measured learned lenses (#1511).
    pub frozen_roster: EngineContentA37FrozenRoster,
    pub dropped_synthetic_anchor_count: usize,
    pub sufficiency_state: String,
    pub sufficiency_reason: String,
    /// `complete` or `pending_measurement` (#1791). Old rows predate the field and
    /// were complete by construction, so they default to `complete`.
    #[serde(default = "measurement_state_complete_default")]
    pub measurement_state: String,
    /// Learned encoders the panel DECLARES (the >=10 gate is measured against
    /// this). In a complete measurement this equals `measured_learned_lenses`;
    /// in a pending measurement it is the count still awaiting backfill.
    #[serde(default)]
    pub declared_learned_lenses: usize,
    /// Declared learned lenses whose vectors are deferred to addon-time backfill.
    /// Empty in a complete measurement (#1791).
    #[serde(default)]
    pub pending_learned_lenses: Vec<EngineContentPendingLens>,
}

/// Captures the frozen roster for `learned_slot_ids` from the trusted panel state.
///
/// The order of `learned_slot_ids` is preserved: it is the measured roster order.
/// Any learned slot without a registered frozen contract, or whose registry key
/// disagrees with its content-addressed contract id, is a fail-closed error.
pub fn measured_frozen_roster(
    state: &VaultPanelState,
    panel_template: &str,
    learned_slot_ids: &[SlotId],
) -> Result<EngineContentA37FrozenRoster, EngineCalyxContentError> {
    let panel_state_sha256 = panel_state_hash(state, panel_template);
    let mut lenses = Vec::with_capacity(learned_slot_ids.len());
    for slot_id in learned_slot_ids {
        let slot = state
            .panel
            .slots
            .iter()
            .find(|slot| slot.slot_id == *slot_id)
            .ok_or_else(|| {
                roster_error(format!(
                    "measured learned slot {} is absent from panel v{}",
                    slot_id.get(),
                    state.panel.version
                ))
            })?;
        let contract = state
            .registry
            .frozen_contract(slot.lens_id)
            .ok_or_else(|| {
                roster_error(format!(
                    "measured learned slot {} lens {} has no frozen contract in the panel registry",
                    slot_id.get(),
                    slot.lens_id
                ))
            })?;
        if contract.lens_id() != slot.lens_id {
            return Err(roster_error(format!(
                "panel slot {} lens id {} disagrees with its frozen contract id {}",
                slot_id.get(),
                slot.lens_id,
                contract.lens_id()
            )));
        }
        lenses.push(EngineContentA37FrozenLens {
            slot: slot_id.get(),
            name: contract.name().to_string(),
            frozen_lens_id: slot.lens_id.to_string(),
            weights_sha256: hex32(&contract.weights_sha256()),
            runtime_fingerprint: runtime_fingerprint(contract),
        });
    }
    let registry_roster_sha256 = frozen_roster_hash(&lenses);
    Ok(EngineContentA37FrozenRoster {
        schema: ENGINE_CONTENT_A37_ROSTER_SCHEMA.to_string(),
        panel_template: panel_template.to_string(),
        panel_version: state.panel.version,
        panel_state_sha256,
        registry_roster_sha256,
        lenses,
    })
}

/// Verifies a stored admission card against the exact measured frozen roster.
///
/// This is the trust gate: it first enforces self-consistency (structural gate +
/// frozen fields present + internally consistent roster hash), then requires the
/// card's panel artifact and every lens's frozen identity to match the measured
/// roster exactly. Reorder, rename, weight change, runtime change, panel change,
/// or any hash mismatch fails loud at `a37_admission`.
pub fn verify_admission_against_measured_roster(
    report: &EngineContentA37AdmissionReport,
    expected: &EngineContentA37FrozenRoster,
) -> Result<(), EngineCalyxContentError> {
    validate_engine_content_a37_admission_report(report)?;
    check(report.panel_template == expected.panel_template, || {
        format!(
            "panel template mismatch: expected {} actual {}",
            expected.panel_template, report.panel_template
        )
    })?;
    check(report.panel_version == expected.panel_version, || {
        format!(
            "panel version mismatch: expected {} actual {}",
            expected.panel_version, report.panel_version
        )
    })?;
    check(
        report.panel_state_sha256 == expected.panel_state_sha256,
        || {
            format!(
                "panel state hash mismatch (panel-changed): expected {} actual {}",
                expected.panel_state_sha256, report.panel_state_sha256
            )
        },
    )?;
    check(report.lenses.len() == expected.lenses.len(), || {
        format!(
            "roster length mismatch: expected {} actual {}",
            expected.lenses.len(),
            report.lenses.len()
        )
    })?;
    for (idx, (lens, want)) in report.lenses.iter().zip(expected.lenses.iter()).enumerate() {
        if lens.slot != want.slot
            || lens.name != want.name
            || lens.frozen_lens_id != want.frozen_lens_id
            || lens.weights_sha256 != want.weights_sha256
            || lens.runtime_fingerprint != want.runtime_fingerprint
        {
            return Err(roster_error(format!(
                "frozen roster mismatch at index {idx}: expected slot {} name {} lens_id {} weights {} fingerprint {}; actual slot {} name {} lens_id {} weights {} fingerprint {}",
                want.slot,
                want.name,
                want.frozen_lens_id,
                want.weights_sha256,
                want.runtime_fingerprint,
                lens.slot,
                lens.name,
                lens.frozen_lens_id,
                lens.weights_sha256,
                lens.runtime_fingerprint
            )));
        }
    }
    check(
        report.registry_roster_sha256 == expected.registry_roster_sha256,
        || {
            format!(
                "registry roster hash mismatch: expected {} actual {}",
                expected.registry_roster_sha256, report.registry_roster_sha256
            )
        },
    )?;
    Ok(())
}

/// Canonical hash over the ordered frozen lens evidence carried by a card.
///
/// Shared by the builder (populate), the structural gate (self-consistency), and
/// roster capture, so all three agree byte-for-byte.
pub fn roster_hash_from_evidence(lenses: &[EngineContentA37LensEvidence]) -> String {
    let mut canonical = String::new();
    for lens in lenses {
        canonical.push_str(&format!(
            "{}:{}:{}:{}:{}\n",
            lens.slot,
            lens.name,
            lens.frozen_lens_id,
            lens.weights_sha256,
            lens.runtime_fingerprint
        ));
    }
    sha256_hex(canonical.as_bytes())
}

/// Canonical hash over an ordered frozen roster. Kept byte-identical to
/// [`roster_hash_from_evidence`] so a card and its measured roster agree.
pub fn frozen_roster_hash(lenses: &[EngineContentA37FrozenLens]) -> String {
    let mut canonical = String::new();
    for lens in lenses {
        canonical.push_str(&format!(
            "{}:{}:{}:{}:{}\n",
            lens.slot,
            lens.name,
            lens.frozen_lens_id,
            lens.weights_sha256,
            lens.runtime_fingerprint
        ));
    }
    sha256_hex(canonical.as_bytes())
}

fn panel_state_hash(state: &VaultPanelState, panel_template: &str) -> String {
    let mut slots = state.panel.slots.iter().collect::<Vec<_>>();
    slots.sort_by_key(|slot| slot.slot_id.get());
    let mut canonical = format!(
        "template:{panel_template}\nversion:{}\n",
        state.panel.version
    );
    for slot in slots {
        canonical.push_str(&format!(
            "{}:{}:{:?}:{:?}\n",
            slot.slot_id.get(),
            slot.lens_id,
            slot.state,
            slot.modality
        ));
    }
    sha256_hex(canonical.as_bytes())
}

fn runtime_fingerprint(contract: &FrozenLensContract) -> String {
    let canonical = format!(
        "shape={:?}|modality={:?}|dtype={:?}|norm={:?}|corpus={}",
        contract.shape(),
        contract.modality(),
        contract.dtype(),
        contract.norm_policy(),
        hex32(&contract.corpus_hash())
    );
    sha256_hex(canonical.as_bytes())
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn check(condition: bool, reason: impl FnOnce() -> String) -> Result<(), EngineCalyxContentError> {
    if condition {
        Ok(())
    } else {
        Err(roster_error(reason()))
    }
}

fn roster_error(reason: impl ToString) -> EngineCalyxContentError {
    EngineCalyxContentError::Measurement {
        phase: "a37_admission".to_string(),
        reason: reason.to_string(),
    }
}

/// Enforce the two-state measurement contract on a readback (#1791).
///
/// - `complete`: at least `min_learned_lenses` learned encoders measured, and
///   every measured slot carries a real (non-absent) vector. This is the ONLY
///   state a retrieval/FSV/assay measurement claim may rely on (#778 at scope).
/// - `pending_measurement`: nothing measured yet (AI addon absent). The panel
///   must still DECLARE >= min learned lenses, at least one is deferred, and
///   every deferred slot is `absent(LensUnavailable)`, ready for backfill.
///
/// Any other state, a forged `complete` with too few / absent measured slots, or
/// a `pending` that hides a measured vector, fails loud as a readback mismatch.
pub(crate) fn validate_measurement_state(
    measurement: &EngineContentMeasurementReadback,
    constellation: &Constellation,
) -> Result<(), EngineCalyxContentError> {
    match measurement.measurement_state.as_str() {
        ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE => {
            if measurement.measured_learned_lenses < measurement.min_learned_lenses {
                return readback_mismatch(
                    "measurement.measured_learned_lenses",
                    measurement.min_learned_lenses.to_string(),
                    measurement.measured_learned_lenses.to_string(),
                );
            }
            for slot in &measurement.measured_slot_ids {
                let slot_id = SlotId::new(*slot);
                match constellation.slots.get(&slot_id) {
                    Some(vector) if !vector.is_absent() => {}
                    Some(_) => {
                        return readback_mismatch(
                            "measurement.slot_vector",
                            "measured",
                            format!("absent slot {slot}"),
                        );
                    }
                    None => {
                        return readback_mismatch(
                            "measurement.slot_vector",
                            "present",
                            format!("missing slot {slot}"),
                        );
                    }
                }
            }
            Ok(())
        }
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING => {
            if measurement.measured_learned_lenses != 0 {
                return readback_mismatch(
                    "measurement.measured_learned_lenses",
                    "0",
                    measurement.measured_learned_lenses.to_string(),
                );
            }
            if !measurement.measured_slot_ids.is_empty() {
                return readback_mismatch(
                    "measurement.measured_slot_ids",
                    "empty",
                    format!("{} measured slots", measurement.measured_slot_ids.len()),
                );
            }
            if measurement.declared_learned_lenses < measurement.min_learned_lenses {
                return readback_mismatch(
                    "measurement.declared_learned_lenses",
                    measurement.min_learned_lenses.to_string(),
                    measurement.declared_learned_lenses.to_string(),
                );
            }
            if measurement.pending_learned_lenses.is_empty() {
                return readback_mismatch(
                    "measurement.pending_learned_lenses",
                    "non_empty",
                    "0 pending lenses",
                );
            }
            for pending in &measurement.pending_learned_lenses {
                let slot_id = SlotId::new(pending.slot);
                match constellation.slots.get(&slot_id) {
                    Some(SlotVector::Absent {
                        reason: AbsentReason::LensUnavailable,
                    }) => {}
                    Some(other) => {
                        return readback_mismatch(
                            "measurement.pending_slot_vector",
                            "absent_lens_unavailable",
                            format!("slot {} vector {other:?}", pending.slot),
                        );
                    }
                    None => {
                        return readback_mismatch(
                            "measurement.pending_slot_vector",
                            "present",
                            format!("missing slot {}", pending.slot),
                        );
                    }
                }
            }
            Ok(())
        }
        other => readback_mismatch(
            "measurement.measurement_state",
            "complete|pending_measurement",
            other.to_string(),
        ),
    }
}

fn readback_mismatch<T>(
    field: &str,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> Result<T, EngineCalyxContentError> {
    Err(EngineCalyxContentError::ReadbackMismatch {
        field: field.to_string(),
        expected: expected.into(),
        actual: actual.into(),
    })
}
