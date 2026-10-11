use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use bincode::config;
use calyx_assay::{A37_DIVERSITY_GATE_PASSED, DEFAULT_GATE_PANEL_LENSES};
use calyx_aster::cf::{CfRouter, ColumnFamily};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

use crate::{
    calyx_content::EngineCalyxContentError,
    calyx_content_a37_identity::{
        EngineContentA37FrozenRoster, roster_hash_from_evidence,
        verify_admission_against_measured_roster,
    },
};

const KEY_PREFIX: &[u8] = b"calyx/a37/admission/v1/";
const VALUE_MAGIC: &[u8] = b"CA37ADM1\0";
const CF_MEMTABLE_CAP: usize = 1_048_576;
const ADMISSION_ROLE: &str = "a37_multi_anchor_admission_card";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineContentA37AdmissionRef {
    pub cf_root: PathBuf,
    pub association_key: String,
}

impl EngineContentA37AdmissionRef {
    pub fn new(cf_root: impl Into<PathBuf>, association_key: impl Into<String>) -> Self {
        Self {
            cf_root: cf_root.into(),
            association_key: association_key.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentA37AdmissionReadback {
    pub cf_root: String,
    pub association_key: String,
    pub row_key_sha256: String,
    pub value_bytes: usize,
    pub value_sha256: String,
    pub readback_matches: bool,
    pub report: EngineContentA37AdmissionReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentA37AdmissionReport {
    pub schema_version: u32,
    pub role: String,
    pub status: String,
    pub mode: String,
    /// Panel template the roster was measured against (#1511).
    pub panel_template: String,
    /// Panel version the roster was measured against (#1511).
    pub panel_version: u32,
    /// Hash over the full panel slot layout the roster was measured against (#1511).
    pub panel_state_sha256: String,
    /// Hash over the ordered learned frozen roster (#1511).
    pub registry_roster_sha256: String,
    /// Hash over the source assay cards this admission was built from (#1511).
    pub source_card_sha256: String,
    /// Source-of-truth identity for the admission (panel path/row) (#1511).
    pub source_of_truth: String,
    pub gate_passed: bool,
    pub report_count: usize,
    pub lens_count: usize,
    pub passing_lens_count: usize,
    pub min_lenses: usize,
    pub min_marginal_bits: f32,
    pub max_redundancy: f32,
    pub family_span_pass: bool,
    pub redundancy_bound_pass: bool,
    pub no_collapse_pass: bool,
    pub association_family_count: usize,
    pub association_families: BTreeMap<String, Vec<u16>>,
    pub min_best_marginal_bits: f32,
    pub max_best_marginal_bits: f32,
    pub weakest_lens: String,
    pub target_summaries: Vec<EngineContentA37TargetSummary>,
    pub lenses: Vec<EngineContentA37LensEvidence>,
    pub source_reports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentA37TargetSummary {
    pub target_class: usize,
    pub domain: String,
    pub report_path: String,
    pub status: String,
    pub no_collapse_pass: bool,
    pub family_span_pass: bool,
    pub redundancy_bound_pass: bool,
    pub n_eff: f32,
    pub panel_bits: f32,
    pub max_marginal_bits: f32,
    pub keep_count: usize,
    pub park_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentA37LensEvidence {
    pub slot: u16,
    pub name: String,
    /// Content-addressed frozen lens id (hex) — bound to the measured roster (#1511).
    pub frozen_lens_id: String,
    /// Declared weights hash (hex) for the measured lens (#1511).
    pub weights_sha256: String,
    /// Runtime/shape fingerprint (hex) for the measured lens (#1511).
    pub runtime_fingerprint: String,
    pub association_family: String,
    pub passed: bool,
    pub best_target_class: usize,
    pub best_domain: String,
    pub best_marginal_bits: f32,
    pub best_solo_bits: f32,
    pub target_values: Vec<EngineContentA37TargetLensValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentA37TargetLensValue {
    pub target_class: usize,
    pub domain: String,
    pub marginal_bits: f32,
    pub solo_bits: f32,
    pub decision: String,
}

pub fn read_engine_content_a37_admission(
    admission: &EngineContentA37AdmissionRef,
    expected_roster: &EngineContentA37FrozenRoster,
) -> Result<EngineContentA37AdmissionReadback, EngineCalyxContentError> {
    let (report, readback) = read_record::<EngineContentA37AdmissionReport>(
        &admission.cf_root,
        &admission.association_key,
    )?;
    verify_admission_against_measured_roster(&report, expected_roster)?;
    Ok(EngineContentA37AdmissionReadback { report, ..readback })
}

pub fn write_engine_content_a37_admission(
    admission: &EngineContentA37AdmissionRef,
    report: &EngineContentA37AdmissionReport,
) -> Result<EngineContentA37AdmissionReadback, EngineCalyxContentError> {
    validate_engine_content_a37_admission_report(report)?;
    let readback = write_record(&admission.cf_root, &admission.association_key, report)?;
    Ok(EngineContentA37AdmissionReadback {
        report: report.clone(),
        ..readback
    })
}

/// Structural + self-consistency gate for an admission card.
///
/// This proves a card is well-formed and carries a complete, internally
/// consistent frozen roster (no name-only legacy cards, no tampered roster hash).
/// The exact identity match against the measured local roster is enforced
/// separately by [`verify_admission_against_measured_roster`].
pub(crate) fn validate_engine_content_a37_admission_report(
    report: &EngineContentA37AdmissionReport,
) -> Result<(), EngineCalyxContentError> {
    let lens_count = report.lenses.len();
    let min_bits = report.min_marginal_bits;
    let header_passed = report.schema_version == 1
        && report.role == ADMISSION_ROLE
        && report.status == A37_DIVERSITY_GATE_PASSED
        && report.gate_passed
        && report.family_span_pass
        && report.redundancy_bound_pass
        && report.no_collapse_pass;
    if !header_passed {
        return admission_invalid(format!(
            "A37 admission refused: role={} status={} gate_passed={} family_span_pass={} redundancy_bound_pass={} no_collapse_pass={}",
            report.role,
            report.status,
            report.gate_passed,
            report.family_span_pass,
            report.redundancy_bound_pass,
            report.no_collapse_pass
        ));
    }
    if report.lens_count != lens_count
        || report.passing_lens_count != lens_count
        || lens_count < report.min_lenses.max(DEFAULT_GATE_PANEL_LENSES)
    {
        return admission_invalid(format!(
            "A37 admission roster count mismatch: lens_count={} lenses={} passing={} min_lenses={}",
            report.lens_count, lens_count, report.passing_lens_count, report.min_lenses
        ));
    }
    validate_admission_frozen_identity(report)?;
    if report.report_count == 0
        || report.target_summaries.is_empty()
        || report.source_reports.is_empty()
        || report.association_family_count < 2
        || !finite(min_bits)
        || !finite(report.min_best_marginal_bits)
        || !finite(report.max_best_marginal_bits)
        || report.min_best_marginal_bits < min_bits
    {
        return admission_invalid("A37 admission aggregate metrics are not gate-valid");
    }
    for target in &report.target_summaries {
        if target.domain.trim().is_empty()
            || !finite(target.n_eff)
            || !finite(target.panel_bits)
            || !finite(target.max_marginal_bits)
            || !target.redundancy_bound_pass
        {
            return admission_invalid(
                "A37 admission target summary is not finite and redundant-bound",
            );
        }
    }
    let mut seen_slots = BTreeSet::new();
    for lens in &report.lenses {
        if lens.name.trim().is_empty()
            || lens.frozen_lens_id.trim().is_empty()
            || lens.weights_sha256.trim().is_empty()
            || lens.runtime_fingerprint.trim().is_empty()
        {
            return admission_invalid(format!(
                "A37 admission lens slot {} is missing frozen identity (name-only cards are rejected)",
                lens.slot
            ));
        }
        if !seen_slots.insert(lens.slot)
            || !lens.passed
            || lens.association_family.trim().is_empty()
            || lens.target_values.is_empty()
            || !finite(lens.best_marginal_bits)
            || !finite(lens.best_solo_bits)
            || lens.best_marginal_bits < min_bits
        {
            return admission_invalid(format!(
                "A37 admission lens {}:{} is not individually admitted",
                lens.slot, lens.name
            ));
        }
        for value in &lens.target_values {
            if value.domain.trim().is_empty()
                || !finite(value.marginal_bits)
                || !finite(value.solo_bits)
            {
                return admission_invalid(format!(
                    "A37 admission lens {}:{} has invalid target value",
                    lens.slot, lens.name
                ));
            }
        }
    }
    Ok(())
}

/// Rejects cards whose panel/source identity is missing or whose carried roster
/// hash does not recompute from the card's own lens evidence (tamper-evident).
fn validate_admission_frozen_identity(
    report: &EngineContentA37AdmissionReport,
) -> Result<(), EngineCalyxContentError> {
    if report.panel_template.trim().is_empty()
        || report.panel_state_sha256.trim().is_empty()
        || report.registry_roster_sha256.trim().is_empty()
        || report.source_card_sha256.trim().is_empty()
        || report.source_of_truth.trim().is_empty()
        || report.panel_version == 0
    {
        return admission_invalid(
            "A37 admission is missing panel/source identity (panel-changed or name-only card)",
        );
    }
    let recomputed = roster_hash_from_evidence(&report.lenses);
    if recomputed != report.registry_roster_sha256 {
        return admission_invalid(format!(
            "A37 admission registry roster hash is not self-consistent: carried {} recomputed {}",
            report.registry_roster_sha256, recomputed
        ));
    }
    Ok(())
}

fn write_record<T>(
    cf_root: &Path,
    association_key: &str,
    record: &T,
) -> Result<EngineContentA37AdmissionReadback, EngineCalyxContentError>
where
    T: Serialize,
{
    let row_key = row_key(association_key)?;
    let value = encode(record)?;
    let mut router = CfRouter::open(cf_root, CF_MEMTABLE_CAP).map_err(admission_store_failed)?;
    router
        .put(ColumnFamily::Graph, &row_key, &value)
        .map_err(admission_store_failed)?;
    router
        .flush_cf(ColumnFamily::Graph)
        .map_err(admission_store_failed)?;
    let reopened = CfRouter::open(cf_root, CF_MEMTABLE_CAP).map_err(admission_store_failed)?;
    let readback = reopened
        .get(ColumnFamily::Graph, &row_key)
        .map_err(admission_store_failed)?
        .ok_or_else(|| admission_error("A37 admission row missing after write"))?;
    if readback != value {
        return Err(admission_error("A37 admission row changed after write"));
    }
    Ok(readback_report(
        cf_root,
        association_key,
        &row_key,
        &readback,
    ))
}

fn read_record<T>(
    cf_root: &Path,
    association_key: &str,
) -> Result<(T, EngineContentA37AdmissionReadback), EngineCalyxContentError>
where
    T: DeserializeOwned,
{
    let row_key = row_key(association_key)?;
    let router = CfRouter::open(cf_root, CF_MEMTABLE_CAP).map_err(admission_store_failed)?;
    let value = router
        .get(ColumnFamily::Graph, &row_key)
        .map_err(admission_store_failed)?
        .ok_or_else(|| admission_error("A37 admission row missing in Graph CF"))?;
    let record = decode(&value)?;
    Ok((
        record,
        readback_report(cf_root, association_key, &row_key, &value),
    ))
}

fn row_key(association_key: &str) -> Result<Vec<u8>, EngineCalyxContentError> {
    if association_key.trim().is_empty() {
        return Err(admission_error(
            "A37 admission association key must be non-empty",
        ));
    }
    let mut key = Vec::with_capacity(KEY_PREFIX.len() + association_key.len());
    key.extend_from_slice(KEY_PREFIX);
    key.extend_from_slice(association_key.as_bytes());
    Ok(key)
}

fn encode<T>(record: &T) -> Result<Vec<u8>, EngineCalyxContentError>
where
    T: Serialize,
{
    let mut bytes = VALUE_MAGIC.to_vec();
    let payload = bincode::serde::encode_to_vec(record, config::standard())
        .map_err(|error| admission_error(format!("encode A37 admission failed: {error}")))?;
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

fn decode<T>(bytes: &[u8]) -> Result<T, EngineCalyxContentError>
where
    T: DeserializeOwned,
{
    let payload = bytes
        .strip_prefix(VALUE_MAGIC)
        .ok_or_else(|| admission_error("A37 admission row has invalid magic"))?;
    let (record, consumed): (T, usize) =
        bincode::serde::decode_from_slice(payload, config::standard())
            .map_err(|error| admission_error(format!("decode A37 admission failed: {error}")))?;
    if consumed != payload.len() {
        return Err(admission_error("A37 admission row has trailing bytes"));
    }
    Ok(record)
}

fn readback_report(
    cf_root: &Path,
    association_key: &str,
    row_key: &[u8],
    value: &[u8],
) -> EngineContentA37AdmissionReadback {
    EngineContentA37AdmissionReadback {
        cf_root: cf_root.display().to_string(),
        association_key: association_key.to_string(),
        row_key_sha256: hex_sha256(row_key),
        value_bytes: value.len(),
        value_sha256: hex_sha256(value),
        readback_matches: true,
        report: placeholder_report(),
    }
}

fn placeholder_report() -> EngineContentA37AdmissionReport {
    EngineContentA37AdmissionReport {
        schema_version: 0,
        role: String::new(),
        status: String::new(),
        mode: String::new(),
        panel_template: String::new(),
        panel_version: 0,
        panel_state_sha256: String::new(),
        registry_roster_sha256: String::new(),
        source_card_sha256: String::new(),
        source_of_truth: String::new(),
        gate_passed: false,
        report_count: 0,
        lens_count: 0,
        passing_lens_count: 0,
        min_lenses: 0,
        min_marginal_bits: 0.0,
        max_redundancy: 0.0,
        family_span_pass: false,
        redundancy_bound_pass: false,
        no_collapse_pass: false,
        association_family_count: 0,
        association_families: BTreeMap::new(),
        min_best_marginal_bits: 0.0,
        max_best_marginal_bits: 0.0,
        weakest_lens: String::new(),
        target_summaries: Vec::new(),
        lenses: Vec::new(),
        source_reports: Vec::new(),
    }
}

fn finite(value: f32) -> bool {
    value.is_finite()
}

fn admission_invalid<T>(reason: impl ToString) -> Result<T, EngineCalyxContentError> {
    Err(EngineCalyxContentError::Measurement {
        phase: "a37_admission".to_string(),
        reason: reason.to_string(),
    })
}

fn admission_error(reason: impl ToString) -> EngineCalyxContentError {
    EngineCalyxContentError::Measurement {
        phase: "a37_admission".to_string(),
        reason: reason.to_string(),
    }
}

fn admission_store_failed(error: impl std::fmt::Display) -> EngineCalyxContentError {
    admission_error(error.to_string())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
