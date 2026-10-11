use std::collections::BTreeSet;

use calyx_assay::{
    A37_DIVERSITY_GATE_PASSED, EnsembleCard, EnsembleConfig, EnsembleLensInput, ensemble_card,
};
use calyx_core::{Modality, SlotId, SlotVector};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    calyx_content::{
        ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA, EngineCalyxContentError,
        EngineContentConstellationReadback,
    },
    calyx_content_a37_admission::{
        EngineContentA37AdmissionReadback, EngineContentA37AdmissionRef,
        read_engine_content_a37_admission,
    },
    calyx_content_a37_identity::{
        EngineContentA37FrozenRoster, verify_admission_against_measured_roster,
    },
    calyx_content_vectors::{
        ContentAssayVector, dense_assay_vector, non_dense_assay_vector_from_metadata,
    },
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxRecord, EngineCalyxRecordKind,
    },
};

pub const ENGINE_CALYX_CONTENT_ASSAY_SCHEMA: &str = "litd.engine.calyx_content_assay.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentAssayReport {
    pub schema: String,
    pub assay_id: String,
    pub sample_count: usize,
    pub positive_count: usize,
    pub negative_count: usize,
    pub content_keys: Vec<String>,
    pub panel_source: String,
    pub panel_version: u32,
    pub measured_learned_lenses: usize,
    pub assayed_dense_lenses: usize,
    #[serde(default)]
    pub assayed_lens_count: usize,
    pub measured_slot_ids: Vec<u16>,
    /// Exact measured frozen roster the admission must bind to (#1511).
    pub measured_frozen_roster: EngineContentA37FrozenRoster,
    pub assayed_slot_ids: Vec<u16>,
    pub assayed_lens_names: Vec<String>,
    #[serde(default)]
    pub assayed_lenses: Vec<EngineContentAssayLensReport>,
    #[serde(default)]
    pub a37_gate_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a37_admission: Option<EngineContentA37AdmissionReadback>,
    pub sufficiency_state: String,
    pub ensemble: EnsembleCard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentAssayLensReport {
    pub slot_id: u16,
    pub lens_name: String,
    pub source_vector_kind: String,
    pub source_dim: u32,
    pub source_count: usize,
    pub assay_projection: String,
    pub assay_dim: usize,
    pub assay_vector_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentAssayReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub report: EngineContentAssayReport,
}

pub fn store_engine_content_assay(
    database: &EngineCalyxDatabase,
    assay_id: &str,
    readbacks: &[EngineContentConstellationReadback],
    labels: &[bool],
) -> Result<EngineContentAssayReadback, EngineCalyxContentError> {
    store_engine_content_assay_inner(database, assay_id, readbacks, labels, None)
}

pub fn store_engine_content_assay_with_a37_admission(
    database: &EngineCalyxDatabase,
    assay_id: &str,
    readbacks: &[EngineContentConstellationReadback],
    labels: &[bool],
    admission: &EngineContentA37AdmissionRef,
) -> Result<EngineContentAssayReadback, EngineCalyxContentError> {
    store_engine_content_assay_inner(database, assay_id, readbacks, labels, Some(admission))
}

pub fn measure_engine_content_assay_report(
    assay_id: &str,
    readbacks: &[EngineContentConstellationReadback],
    labels: &[bool],
) -> Result<EngineContentAssayReport, EngineCalyxContentError> {
    validate_assay_id(assay_id)?;
    validate_samples(readbacks, labels)?;
    let assayed = ensemble_lenses(readbacks)?;
    let lenses = assayed
        .iter()
        .map(|lens| lens.input.clone())
        .collect::<Vec<_>>();
    let config = EnsembleConfig {
        source: format!("engine_content_assay:{assay_id}"),
        ..EnsembleConfig::default()
    };
    let ensemble = ensemble_card(&lenses, labels, None, &config).map_err(|error| {
        EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "engine content ensemble assay failed: {}: {}",
                error.code, error.message
            ),
        }
    })?;
    let first = &readbacks[0].measurement;
    let assayed_lenses = assayed
        .iter()
        .map(|lens| lens.report.clone())
        .collect::<Vec<_>>();
    let assayed_dense_lenses = assayed_lenses
        .iter()
        .filter(|lens| lens.source_vector_kind == "dense")
        .count();
    Ok(EngineContentAssayReport {
        schema: ENGINE_CALYX_CONTENT_ASSAY_SCHEMA.to_string(),
        assay_id: assay_id.to_string(),
        sample_count: readbacks.len(),
        positive_count: labels.iter().filter(|label| **label).count(),
        negative_count: labels.iter().filter(|label| !**label).count(),
        content_keys: readbacks
            .iter()
            .map(|readback| format!("{}/{}", readback.kind.as_str(), readback.id))
            .collect(),
        panel_source: first.panel_source.clone(),
        panel_version: first.panel_version,
        measured_learned_lenses: first.measured_learned_lenses,
        assayed_dense_lenses,
        assayed_lens_count: lenses.len(),
        measured_slot_ids: first.measured_slot_ids.clone(),
        measured_frozen_roster: first.frozen_roster.clone(),
        assayed_slot_ids: lenses.iter().map(|lens| lens.slot.get()).collect(),
        assayed_lens_names: lenses.iter().map(|lens| lens.name.clone()).collect(),
        assayed_lenses,
        a37_gate_source: "diagnostic_only".to_string(),
        a37_admission: None,
        sufficiency_state: "measured".to_string(),
        ensemble,
    })
}

fn store_engine_content_assay_inner(
    database: &EngineCalyxDatabase,
    assay_id: &str,
    readbacks: &[EngineContentConstellationReadback],
    labels: &[bool],
    admission: Option<&EngineContentA37AdmissionRef>,
) -> Result<EngineContentAssayReadback, EngineCalyxContentError> {
    let mut report = measure_engine_content_assay_report(assay_id, readbacks, labels)?;
    let (a37_gate_source, a37_admission) = match validate_a37_gate(&report.ensemble) {
        Ok(()) => ("single_anchor_ensemble".to_string(), None),
        Err(error) => match admission {
            Some(admission) => (
                "db_native_multi_anchor_admission".to_string(),
                Some(read_engine_content_a37_admission(
                    admission,
                    &report.measured_frozen_roster,
                )?),
            ),
            None => return Err(error),
        },
    };
    report.a37_gate_source = a37_gate_source;
    report.a37_admission = a37_admission;
    let row_id = assay_row_id(assay_id);
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::Provenance,
        row_id.clone(),
        serde_json::to_value(&report).map_err(content_store_failed)?,
        json!({
            "source_issue": 1456,
            "assay_id": assay_id,
            "sample_count": report.sample_count,
            "assayed_dense_lenses": report.assayed_dense_lenses,
            "assayed_lens_count": report.assayed_lens_count,
            "sufficient": report.ensemble.sufficient,
            "a37_status": report.ensemble.a37_diversity.status,
            "a37_gate_source": report.a37_gate_source,
            "a37_admission_row_sha256": report.a37_admission.as_ref().map(|admission| admission.value_sha256.clone()),
            "a37_no_collapse_pass": report.ensemble.a37_diversity.no_collapse_pass,
            "a37_keep_count": report.ensemble.keep_count,
            "a37_park_count": report.ensemble.park_count
        }),
    );
    let database_readback = database.put_record(&record)?;
    parse_assay_readback(database_readback, &row_id)
}

pub fn read_engine_content_assay(
    database: &EngineCalyxDatabase,
    assay_id: &str,
) -> Result<EngineContentAssayReadback, EngineCalyxContentError> {
    validate_assay_id(assay_id)?;
    let row_id = assay_row_id(assay_id);
    let database_readback = database.get_record(EngineCalyxRecordKind::Provenance, &row_id)?;
    parse_assay_readback(database_readback, &row_id)
}

fn ensemble_lenses(
    readbacks: &[EngineContentConstellationReadback],
) -> Result<Vec<AssayedLens>, EngineCalyxContentError> {
    let first = &readbacks[0];
    let measured = first
        .measurement
        .measured_slot_ids
        .iter()
        .zip(first.measurement.measured_lens_names.iter())
        .collect::<Vec<_>>();
    let mut lenses = Vec::new();
    for (slot_id, lens_name) in measured {
        let slot = SlotId::new(*slot_id);
        let rows = readbacks
            .iter()
            .map(|readback| assay_row(readback, slot))
            .collect::<Result<Vec<_>, _>>()?;
        let first_row = rows
            .first()
            .ok_or_else(|| EngineCalyxContentError::Measurement {
                phase: "assay".to_string(),
                reason: format!("slot {} has no assay rows", slot.get()),
            })?;
        let report = EngineContentAssayLensReport {
            slot_id: slot.get(),
            lens_name: lens_name.clone(),
            source_vector_kind: first_row.source_vector_kind.clone(),
            source_dim: first_row.source_dim,
            source_count: first_row.source_count,
            assay_projection: first_row.assay_projection.clone(),
            assay_dim: first_row.assay_dim,
            assay_vector_sha256: first_row.assay_vector_sha256.clone(),
        };
        lenses.push(AssayedLens {
            input: EnsembleLensInput::new(
                lens_name.clone(),
                slot,
                rows.into_iter().map(|row| row.vector).collect(),
            ),
            report,
        });
    }
    if lenses.len() < first.measurement.min_learned_lenses {
        return Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!(
                "content assay requires at least {} measured learned lenses; got {}",
                first.measurement.min_learned_lenses,
                lenses.len()
            ),
        });
    }
    Ok(lenses)
}

#[derive(Clone)]
struct AssayedLens {
    input: EnsembleLensInput,
    report: EngineContentAssayLensReport,
}

fn assay_row(
    readback: &EngineContentConstellationReadback,
    slot: SlotId,
) -> Result<ContentAssayVector, EngineCalyxContentError> {
    match readback.constellation.slots.get(&slot) {
        Some(SlotVector::Dense { dim, data }) => dense_assay_vector(*dim, data),
        Some(SlotVector::Sparse { .. } | SlotVector::Multi { .. }) => {
            non_dense_assay_vector_from_metadata(&readback.constellation.metadata, slot)
        }
        Some(SlotVector::Absent { .. }) => Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!("measured slot {} is absent in {}", slot.get(), readback.id),
        }),
        None => Err(EngineCalyxContentError::Measurement {
            phase: "assay".to_string(),
            reason: format!("measured slot {} is missing in {}", slot.get(), readback.id),
        }),
    }
}

fn validate_samples(
    readbacks: &[EngineContentConstellationReadback],
    labels: &[bool],
) -> Result<(), EngineCalyxContentError> {
    if readbacks.len() != labels.len() {
        return invalid_assay(
            "labels",
            format!(
                "labels len {} != readbacks len {}",
                labels.len(),
                readbacks.len()
            ),
        );
    }
    if labels.iter().all(|label| *label) || labels.iter().all(|label| !*label) {
        return invalid_assay("labels", "content assay labels require both classes");
    }
    let first = readbacks
        .first()
        .ok_or_else(|| EngineCalyxContentError::InvalidContent {
            field: "content_assay.readbacks".to_string(),
            reason: "at least one measured content readback is required".to_string(),
        })?;
    let expected_slots = &first.measurement.measured_slot_ids;
    let expected_roster = &first.measurement.frozen_roster;
    let expected_source = &first.measurement.panel_source;
    let expected_version = first.measurement.panel_version;
    let mut seen = BTreeSet::new();
    for readback in readbacks {
        if readback.constellation.modality != Modality::Text {
            return invalid_assay(
                "readback.modality",
                "content assay requires text constellations",
            );
        }
        if readback.measurement.schema != ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA {
            return invalid_assay(
                "readback.measurement.schema",
                "unexpected measurement schema",
            );
        }
        if &readback.measurement.measured_slot_ids != expected_slots {
            return invalid_assay(
                "readback.measurement.measured_slot_ids",
                "all samples must share the same measured slot roster",
            );
        }
        if &readback.measurement.frozen_roster != expected_roster {
            return invalid_assay(
                "readback.measurement.frozen_roster",
                "all samples must share the same measured frozen roster identity",
            );
        }
        if &readback.measurement.panel_source != expected_source
            || readback.measurement.panel_version != expected_version
        {
            return invalid_assay(
                "readback.measurement.panel",
                "all samples must share the same measured panel source and version",
            );
        }
        if !seen.insert(format!("{}/{}", readback.kind.as_str(), readback.id)) {
            return invalid_assay("readback.id", format!("duplicate sample {}", readback.id));
        }
    }
    Ok(())
}

fn parse_assay_readback(
    database: EngineCalyxDatabaseReadback,
    expected_row_id: &str,
) -> Result<EngineContentAssayReadback, EngineCalyxContentError> {
    if database.id != expected_row_id {
        return Err(EngineCalyxContentError::ReadbackMismatch {
            field: "assay.database.id".to_string(),
            expected: expected_row_id.to_string(),
            actual: database.id.clone(),
        });
    }
    let report = serde_json::from_value::<EngineContentAssayReport>(database.payload.clone())
        .map_err(content_store_failed)?;
    if report.schema != ENGINE_CALYX_CONTENT_ASSAY_SCHEMA {
        return Err(EngineCalyxContentError::ReadbackMismatch {
            field: "assay.schema".to_string(),
            expected: ENGINE_CALYX_CONTENT_ASSAY_SCHEMA.to_string(),
            actual: report.schema.clone(),
        });
    }
    if report.sufficiency_state != "measured" {
        return invalid_assay("sufficiency_state", "assay sufficiency was not measured");
    }
    validate_report_a37_gate(&report)?;
    Ok(EngineContentAssayReadback {
        schema: ENGINE_CALYX_CONTENT_ASSAY_SCHEMA.to_string(),
        database,
        report,
    })
}

fn validate_report_a37_gate(
    report: &EngineContentAssayReport,
) -> Result<(), EngineCalyxContentError> {
    if report.a37_gate_source == "db_native_multi_anchor_admission" {
        let admission =
            report
                .a37_admission
                .as_ref()
                .ok_or_else(|| EngineCalyxContentError::Measurement {
                    phase: "a37_admission".to_string(),
                    reason: "content assay declares DB-native A37 admission without readback"
                        .to_string(),
                })?;
        if !admission.readback_matches {
            return Err(EngineCalyxContentError::Measurement {
                phase: "a37_admission".to_string(),
                reason: "DB-native A37 admission readback did not match".to_string(),
            });
        }
        return verify_admission_against_measured_roster(
            &admission.report,
            &report.measured_frozen_roster,
        );
    }
    validate_a37_gate(&report.ensemble)
}

fn validate_a37_gate(ensemble: &EnsembleCard) -> Result<(), EngineCalyxContentError> {
    let gate = &ensemble.a37_diversity;
    if gate.status == A37_DIVERSITY_GATE_PASSED
        && gate.family_span_pass
        && gate.redundancy_bound_pass
        && gate.no_collapse_pass
    {
        return Ok(());
    }
    Err(EngineCalyxContentError::Measurement {
        phase: "a37_gate".to_string(),
        reason: format!(
            "A37 content assay refused: status={} family_span_pass={} redundancy_bound_pass={} no_collapse_pass={} content_lens_count={} association_family_count={} n_eff={:.6} n_eff_floor={:.6} mean_pairwise_corr={:.6} mean_pairwise_nmi={:.6} min_marginal_bits={:.6} sum_unique_pid_bits={:.6} keep_count={} park_count={} retire_count={}",
            gate.status,
            gate.family_span_pass,
            gate.redundancy_bound_pass,
            gate.no_collapse_pass,
            gate.content_lens_count,
            gate.association_family_count,
            gate.n_eff,
            gate.n_eff_floor,
            gate.mean_pairwise_corr,
            gate.mean_pairwise_nmi,
            gate.min_marginal_bits,
            gate.sum_unique_pid_bits,
            ensemble.keep_count,
            ensemble.park_count,
            ensemble.retire_count
        ),
    })
}

fn validate_assay_id(assay_id: &str) -> Result<(), EngineCalyxContentError> {
    if assay_id.trim().is_empty() || assay_id.chars().any(char::is_control) {
        return invalid_assay("assay_id", "assay id must be non-empty and printable");
    }
    Ok(())
}

fn assay_row_id(assay_id: &str) -> String {
    format!("content_assay/{assay_id}")
}

fn invalid_assay<T>(
    field: impl Into<String>,
    reason: impl ToString,
) -> Result<T, EngineCalyxContentError> {
    Err(EngineCalyxContentError::InvalidContent {
        field: format!("content_assay.{}", field.into()),
        reason: reason.to_string(),
    })
}

fn content_store_failed(error: impl std::fmt::Display) -> EngineCalyxContentError {
    EngineCalyxContentError::Codec(error.to_string())
}
