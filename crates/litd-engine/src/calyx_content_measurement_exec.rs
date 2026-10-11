//! Content measurement execution (#1791).
//!
//! Splits the measure-time logic out of `calyx_content_measurement` (config).
//! Two doctrine rules meet here:
//!
//! - Authoring persists UNCONDITIONALLY. The explicit/deterministic constellation
//!   (input bytes, fingerprint, grounded anchors, panel-declared structure) is
//!   always produced. The learned-encoder enrichment is an ENRICHMENT: when the
//!   AI addon (ONNX runtime + models) is unavailable, every declared learned slot
//!   degrades to `absent(LensUnavailable)` -- exactly mirroring the calyx-mcp
//!   ingest path (`Calyx/crates/calyx-mcp/src/tools/ingest.rs:385`) -- and the row
//!   is recorded as `pending_measurement` for addon-time backfill.
//! - The >=10 learned-lens gate STAYS binding for MEASUREMENT claims (#778 at its
//!   real scope). A panel must still DECLARE at least `min_learned_lenses` learned
//!   encoders across at least `min_association_families` families, and a
//!   measurement only counts as `complete` when they actually produced vectors.

use std::collections::{BTreeMap, BTreeSet};

use calyx_assay::a37_association_family;
use calyx_core::{
    AbsentReason, Anchor, AnchorKind, Constellation, CxFlags, CxId, Input, InputRef, LedgerRef,
    Modality, Slot, SlotId, SlotState, SlotVector, VaultId,
};
use calyx_registry::{CapabilitySignalKind, VaultPanelState, signal_kind_from_spec};

use crate::{
    calyx_content::{EngineCalyxContentError, EngineContentRecord},
    calyx_content_a37_identity::{
        ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE, ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        EngineContentMeasurementReadback, EngineContentPendingLens, measured_frozen_roster,
    },
    calyx_content_measurement::{
        ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA, ENGINE_CONTENT_PANEL_TEMPLATE,
        EngineContentMeasurementConfig, measurement_failed,
    },
    calyx_persistence::EngineCalyxDatabase,
};

const ENGINE_CONTENT_CREATED_AT: u64 = 1_786_000_000;
const ENGINE_CONTENT_VAULT_SALT: &[u8] = b"litd-engine-content-constellation-issue1116";
const LENS_UNREACHABLE_CODE: &str = "CALYX_LENS_UNREACHABLE";

pub(crate) fn measure_content_record(
    database: &EngineCalyxDatabase,
    measurement: &EngineContentMeasurementConfig,
    record: &EngineContentRecord,
    input_bytes: Vec<u8>,
    source_pointer: String,
    input_hash: [u8; 32],
) -> Result<(Constellation, EngineContentMeasurementReadback), EngineCalyxContentError> {
    let input = Input::new(Modality::Text, input_bytes).with_pointer(&source_pointer);
    let (panel_state, panel_source) = measurement.load_state()?;
    let gate = validate_measurement_gate(&panel_state, measurement)?;
    measure_content_constellation(MeasureContentConstellationArgs {
        database,
        record,
        state: &panel_state,
        panel_source: &panel_source,
        gate: &gate,
        measurement,
        input,
        input_hash,
    })
}

#[derive(Clone, Debug)]
struct MeasurementGate {
    declared_slots: Vec<DeclaredSlot>,
    association_families: Vec<String>,
}

#[derive(Clone, Debug)]
struct DeclaredSlot {
    slot_id: SlotId,
    name: String,
}

/// Declaration gate (#778 at its real scope): the panel must DECLARE at least
/// `min_learned_lenses` learned encoders over at least `min_association_families`
/// families. This is a panel property, readable from frozen specs WITHOUT the ONNX
/// runtime, so it stays binding even when the addon is absent. Whether those
/// declared lenses actually produced vectors is decided later (complete vs
/// pending), never here.
fn validate_measurement_gate(
    state: &VaultPanelState,
    measurement: &EngineContentMeasurementConfig,
) -> Result<MeasurementGate, EngineCalyxContentError> {
    let mut declared_slots = Vec::new();
    let mut families = BTreeSet::new();
    for slot in &state.panel.slots {
        if !content_text_slot(slot) || !state.registry.contains(slot.lens_id) {
            continue;
        }
        let Some(spec) = state.registry.lens_spec(slot.lens_id) else {
            continue;
        };
        if signal_kind_from_spec(spec) != CapabilitySignalKind::LearnedEncoder {
            continue;
        }
        declared_slots.push(DeclaredSlot {
            slot_id: slot.slot_id,
            name: spec.name.clone(),
        });
        families.insert(a37_association_family(&spec.name).to_string());
    }
    if declared_slots.len() < measurement.min_learned_lenses() {
        return Err(measurement_failed(
            "panel_gate",
            format!(
                "content ingest requires at least {} active text learned-encoder lenses; panel v{} has {}",
                measurement.min_learned_lenses(),
                state.panel.version,
                declared_slots.len()
            ),
        ));
    }
    if families.len() < measurement.min_association_families() {
        return Err(measurement_failed(
            "panel_gate",
            format!(
                "content ingest requires at least {} learned association families; panel v{} has {}",
                measurement.min_association_families(),
                state.panel.version,
                families.len()
            ),
        ));
    }
    Ok(MeasurementGate {
        declared_slots,
        association_families: families.into_iter().collect(),
    })
}

struct MeasureContentConstellationArgs<'a> {
    database: &'a EngineCalyxDatabase,
    record: &'a EngineContentRecord,
    state: &'a VaultPanelState,
    panel_source: &'a str,
    gate: &'a MeasurementGate,
    measurement: &'a EngineContentMeasurementConfig,
    input: Input,
    input_hash: [u8; 32],
}

fn measure_content_constellation(
    args: MeasureContentConstellationArgs<'_>,
) -> Result<(Constellation, EngineContentMeasurementReadback), EngineCalyxContentError> {
    let MeasureContentConstellationArgs {
        database,
        record,
        state,
        panel_source,
        gate,
        measurement,
        input,
        input_hash,
    } = args;
    let declared_slot_ids = gate
        .declared_slots
        .iter()
        .map(|slot| slot.slot_id)
        .collect::<BTreeSet<_>>();
    let mut slots = BTreeMap::new();
    let mut degraded = false;
    let mut measured_learned: Vec<DeclaredSlot> = Vec::new();
    let mut pending_learned: Vec<EngineContentPendingLens> = Vec::new();
    for slot in &state.panel.slots {
        let vector = measure_panel_slot(slot, state, &input)?;
        if declared_slot_ids.contains(&slot.slot_id) {
            classify_declared_slot(
                slot,
                state,
                &vector,
                &mut measured_learned,
                &mut pending_learned,
            )?;
        }
        degraded |= slot.counts_toward_degraded(input.modality) && vector.is_absent();
        slots.insert(slot.slot_id, vector);
    }
    let measurement_state = classify_measurement_state(
        measured_learned.len(),
        pending_learned.len(),
        gate.declared_slots.len(),
        measurement.min_learned_lenses(),
    )?;
    let grounded_anchors = grounded_anchors(record);
    let dropped_synthetic_anchor_count = record.anchors.len() - grounded_anchors.len();
    let cx_id = CxId::from_input(&input.bytes, state.panel.version, ENGINE_CONTENT_VAULT_SALT);
    let vault_id = database
        .config()
        .vault_id
        .parse::<VaultId>()
        .map_err(|error| EngineCalyxContentError::InvalidContent {
            field: "vault_id".to_string(),
            reason: error.to_string(),
        })?;
    let measured_slot_ids = measured_learned
        .iter()
        .map(|slot| slot.slot_id)
        .collect::<Vec<_>>();
    let frozen_roster =
        measured_frozen_roster(state, ENGINE_CONTENT_PANEL_TEMPLATE, &measured_slot_ids)?;
    let measurement_readback = EngineContentMeasurementReadback {
        schema: ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA.to_string(),
        panel_template: ENGINE_CONTENT_PANEL_TEMPLATE.to_string(),
        panel_source: panel_source.to_string(),
        panel_version: state.panel.version,
        input_modality: "text".to_string(),
        min_learned_lenses: measurement.min_learned_lenses(),
        measured_learned_lenses: measured_learned.len(),
        min_association_families: measurement.min_association_families(),
        association_family_count: gate.association_families.len(),
        association_families: gate.association_families.clone(),
        measured_slot_ids: measured_learned.iter().map(|slot| slot.slot_id.get()).collect(),
        measured_lens_names: measured_learned.iter().map(|slot| slot.name.clone()).collect(),
        frozen_roster,
        dropped_synthetic_anchor_count,
        sufficiency_state: "not_measured_single_entity".to_string(),
        sufficiency_reason: "Assay ensemble sufficiency requires a labeled anchor corpus; single-entity content ingest persists measurement provenance and remains ungrounded until real outcome anchors exist".to_string(),
        measurement_state: measurement_state.to_string(),
        declared_learned_lenses: gate.declared_slots.len(),
        pending_learned_lenses: pending_learned,
    };
    let metadata = build_metadata(
        panel_source,
        &measurement_readback,
        dropped_synthetic_anchor_count,
    );
    Ok((
        Constellation {
            cx_id,
            vault_id,
            panel_version: state.panel.version,
            created_at: ENGINE_CONTENT_CREATED_AT,
            input_ref: InputRef {
                hash: input_hash,
                pointer: input.pointer,
                redacted: false,
            },
            modality: input.modality,
            slots,
            scalars: BTreeMap::new(),
            metadata,
            anchors: grounded_anchors.clone(),
            provenance: LedgerRef {
                seq: 0,
                hash: input_hash,
            },
            flags: CxFlags {
                ungrounded: grounded_anchors.is_empty(),
                degraded,
                novel_region: false,
                redacted_input: false,
            },
        },
        measurement_readback,
    ))
}

/// Measure one panel slot, degrading an unavailable learned lens to
/// `absent(LensUnavailable)` instead of failing (mirrors calyx-mcp ingest:385).
/// A non-`CALYX_LENS_UNREACHABLE` error is a real fault and still fails loud.
fn measure_panel_slot(
    slot: &Slot,
    state: &VaultPanelState,
    input: &Input,
) -> Result<SlotVector, EngineCalyxContentError> {
    if slot.state != SlotState::Active {
        return Ok(absent(AbsentReason::LensInactive));
    }
    if slot.modality != input.modality {
        return Ok(absent(AbsentReason::NotApplicable));
    }
    if !state.registry.contains(slot.lens_id) {
        return Ok(absent(AbsentReason::LensUnavailable));
    }
    match state.registry.measure(slot.lens_id, input) {
        Ok(vector) => Ok(vector),
        Err(error) if error.code == LENS_UNREACHABLE_CODE => {
            Ok(absent(AbsentReason::LensUnavailable))
        }
        Err(error) => Err(measurement_failed(
            "measure_slot",
            format!(
                "slot {} lens {} failed: {}: {}",
                slot.slot_key.key(),
                slot.lens_id,
                error.code,
                error.message
            ),
        )),
    }
}

fn classify_declared_slot(
    slot: &Slot,
    state: &VaultPanelState,
    vector: &SlotVector,
    measured: &mut Vec<DeclaredSlot>,
    pending: &mut Vec<EngineContentPendingLens>,
) -> Result<(), EngineCalyxContentError> {
    match vector {
        SlotVector::Absent {
            reason: AbsentReason::LensUnavailable,
        } => {
            let contract = state
                .registry
                .frozen_contract(slot.lens_id)
                .ok_or_else(|| {
                    measurement_failed(
                        "pending_lens",
                        format!(
                            "declared learned slot {} lens {} has no frozen contract to defer",
                            slot.slot_key.key(),
                            slot.lens_id
                        ),
                    )
                })?;
            pending.push(EngineContentPendingLens {
                slot: slot.slot_id.get(),
                lens_id: slot.lens_id.to_string(),
                name: contract.name().to_string(),
            });
            Ok(())
        }
        SlotVector::Absent { reason } => Err(measurement_failed(
            "measure_slot",
            format!(
                "declared learned slot {} lens {} returned absent {:?}; only LensUnavailable is a defer-able addon-absent state",
                slot.slot_key.key(),
                slot.lens_id,
                reason
            ),
        )),
        _ => {
            let name = state
                .registry
                .lens_spec(slot.lens_id)
                .map(|spec| spec.name.clone())
                .unwrap_or_else(|| slot.slot_key.key().to_string());
            measured.push(DeclaredSlot {
                slot_id: slot.slot_id,
                name,
            });
            Ok(())
        }
    }
}

/// Decide complete vs pending vs fail-loud.
///
/// - `measured >= min`             -> complete.
/// - `measured == 0 && pending >= min` -> pending (addon absent; backfill later).
/// - anything else                 -> a partial/inconsistent runtime: fail loud.
fn classify_measurement_state(
    measured: usize,
    pending: usize,
    declared: usize,
    min: usize,
) -> Result<&'static str, EngineCalyxContentError> {
    if measured >= min {
        Ok(ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE)
    } else if measured == 0 && pending >= min {
        Ok(ENGINE_CONTENT_MEASUREMENT_STATE_PENDING)
    } else {
        Err(measurement_failed(
            "measurement_availability",
            format!(
                "partial learned-lens availability: {measured} measured, {pending} unavailable of {declared} declared; the AI addon runtime is inconsistent -- provide all >= {min} CPU-explicit learned encoders (set ORT_DYLIB_PATH + model cache) or none, never a subset"
            ),
        ))
    }
}

fn build_metadata(
    panel_source: &str,
    readback: &EngineContentMeasurementReadback,
    dropped_synthetic_anchor_count: usize,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "measurement_schema".to_string(),
        ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA.to_string(),
    );
    metadata.insert(
        "measurement_panel_source".to_string(),
        panel_source.to_string(),
    );
    metadata.insert(
        "measurement_learned_lens_count".to_string(),
        readback.measured_learned_lenses.to_string(),
    );
    metadata.insert(
        "measurement_declared_learned_lens_count".to_string(),
        readback.declared_learned_lenses.to_string(),
    );
    metadata.insert(
        "measurement_pending_learned_lens_count".to_string(),
        readback.pending_learned_lenses.len().to_string(),
    );
    metadata.insert(
        "measurement_state".to_string(),
        readback.measurement_state.clone(),
    );
    metadata.insert(
        "measurement_association_family_count".to_string(),
        readback.association_family_count.to_string(),
    );
    metadata.insert(
        "measurement_sufficiency_state".to_string(),
        readback.sufficiency_state.clone(),
    );
    metadata.insert(
        "measurement_input_modality".to_string(),
        readback.input_modality.clone(),
    );
    metadata.insert(
        "synthetic_testpass_anchors_dropped".to_string(),
        dropped_synthetic_anchor_count.to_string(),
    );
    metadata
}

fn content_text_slot(slot: &Slot) -> bool {
    slot.state == SlotState::Active
        && slot.modality == Modality::Text
        && !slot.retrieval_only
        && !slot.excluded_from_dedup
}

fn grounded_anchors(record: &EngineContentRecord) -> Vec<Anchor> {
    record
        .anchors
        .iter()
        .filter(|anchor| anchor.kind != AnchorKind::TestPass)
        .cloned()
        .collect()
}

fn absent(reason: AbsentReason) -> SlotVector {
    SlotVector::Absent { reason }
}
