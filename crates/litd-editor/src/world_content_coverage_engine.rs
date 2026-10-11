use std::collections::{BTreeMap, BTreeSet};

use calyx_core::{CxId, SlotId};
use calyx_loom::{
    AbundanceReport, CeilingEstimate, LoomStore, NeffEstimate, Severity, detect_blind_spot,
};
use litd_engine::EngineError;

use crate::world_memory::{
    WorldMemoryConstellation, WorldMemoryFeatureVector, WorldMemoryIndex, WorldMemoryLensKind,
    WorldMemorySlotKind, WorldMemorySlotState, validate_world_memory_index,
};

use super::{
    WORLD_CONTENT_COVERAGE_SCHEMA, WorldContentCoverageReport, WorldContentCoverageSpec,
    WorldCoverageAgreementEdge, WorldCoverageBucketReport, WorldCoverageBucketSpec,
    WorldCoverageBucketStatus, WorldCoverageFinding, WorldCoverageFindingKind,
    WorldCoverageLensBlindSpot, WorldCoverageSeverity, WorldCoverageStatus,
};

const CONTENT_COVERAGE_CX_SALT: &[u8] = b"litd-world-content-coverage-v1";

struct LoomSummary {
    abundance: AbundanceReport,
    measured_count: usize,
    derived_count: usize,
    materialized_count: usize,
    agreement_edges: Vec<WorldCoverageAgreementEdge>,
    cx_by_constellation: BTreeMap<String, CxId>,
}

pub(super) fn run_world_content_coverage_analysis(
    source_of_truth: String,
    index: &WorldMemoryIndex,
    spec: &WorldContentCoverageSpec,
) -> Result<WorldContentCoverageReport, EngineError> {
    validate_world_memory_index(index)?;
    let loom = build_loom_summary(index)?;
    let mut bucket_reports = Vec::new();
    let mut findings = Vec::new();
    let mut lens_blind_spots = Vec::new();
    for bucket in &spec.buckets {
        let report = bucket_report(index, bucket);
        findings.extend(bucket_findings(bucket, &report));
        lens_blind_spots.extend(bucket_lens_blind_spots(index, bucket, &loom)?);
        bucket_reports.push(report);
    }
    for blind_spot in &lens_blind_spots {
        findings.push(WorldCoverageFinding {
            kind: WorldCoverageFindingKind::LensMismatch,
            severity: blind_spot.severity,
            bucket_id: blind_spot.bucket_id.clone(),
            reason: blind_spot.reason.clone(),
            actual_count: 0,
            expected_min_count: None,
            expected_max_count: None,
            actual_share_milli: 0,
            threshold_milli: None,
            evidence_constellation_ids: vec![blind_spot.constellation_id.clone()],
        });
    }
    let status = if findings.is_empty() {
        WorldCoverageStatus::Balanced
    } else {
        WorldCoverageStatus::Findings
    };
    Ok(WorldContentCoverageReport {
        schema: WORLD_CONTENT_COVERAGE_SCHEMA.to_string(),
        analysis_id: spec.analysis_id.clone(),
        source_index_id: spec.source_index_id.clone(),
        source_of_truth,
        requested_by: spec.requested_by.clone(),
        status,
        abundance: loom.abundance,
        measured_count: loom.measured_count,
        derived_count: loom.derived_count,
        materialized_count: loom.materialized_count,
        bucket_reports,
        findings,
        lens_blind_spots,
        agreement_edges: loom.agreement_edges,
    })
}

pub(super) fn validate_world_content_coverage_report(
    report: &WorldContentCoverageReport,
) -> Result<(), EngineError> {
    if report.schema != WORLD_CONTENT_COVERAGE_SCHEMA
        || report.analysis_id.trim().is_empty()
        || report.source_index_id.trim().is_empty()
        || report.source_of_truth.trim().is_empty()
        || report.requested_by.trim().is_empty()
        || report.bucket_reports.is_empty()
        || report.measured_count == 0
    {
        return invalid_world_content_coverage(
            "report",
            "report identity, source, buckets, and measured count must be valid",
        );
    }
    if report.materialized_count != report.derived_count
        || report.materialized_count != report.abundance.materialized
        || report.measured_count != report.abundance.measured_count
    {
        return invalid_world_content_coverage(
            "report.abundance",
            "Loom abundance counts must match report counters",
        );
    }
    if report.status == WorldCoverageStatus::Balanced && !report.findings.is_empty() {
        return invalid_world_content_coverage(
            "report.status",
            "balanced reports cannot carry findings",
        );
    }
    for bucket in &report.bucket_reports {
        if bucket.bucket_id.trim().is_empty()
            || bucket.label.trim().is_empty()
            || bucket.share_milli > 1000
            || bucket.match_count != bucket.matched_constellation_ids.len()
            || bucket.status != WorldCoverageBucketStatus::Balanced && bucket.reasons.is_empty()
        {
            return invalid_world_content_coverage(
                "report.bucket",
                "bucket report counts, ids, status, and reasons must be valid",
            );
        }
    }
    for finding in &report.findings {
        if finding.bucket_id.trim().is_empty()
            || finding.reason.trim().is_empty()
            || finding.actual_share_milli > 1000
        {
            return invalid_world_content_coverage("report.finding", "finding must be valid");
        }
    }
    for blind_spot in &report.lens_blind_spots {
        if blind_spot.bucket_id.trim().is_empty()
            || blind_spot.constellation_id.trim().is_empty()
            || blind_spot.calyx_cx_id.trim().is_empty()
            || !blind_spot.delta.is_finite()
        {
            return invalid_world_content_coverage(
                "report.blind_spot",
                "lens blind spot must be finite and identified",
            );
        }
    }
    Ok(())
}

fn build_loom_summary(index: &WorldMemoryIndex) -> Result<LoomSummary, EngineError> {
    let mut store = LoomStore::new(256);
    let mut active_slots = BTreeSet::new();
    let mut cx_by_constellation = BTreeMap::new();
    for constellation in &index.constellations {
        let cx_id = cx_for_constellation(&constellation.constellation_id);
        if cx_by_constellation
            .insert(constellation.constellation_id.clone(), cx_id)
            .is_some()
        {
            return invalid_world_content_coverage(
                "index.duplicate",
                format!("duplicate {}", constellation.constellation_id),
            );
        }
        let slots = loom_slots(constellation)?;
        if slots.len() < 2 {
            return invalid_world_content_coverage(
                "index.vectors",
                format!(
                    "{} needs at least two Loom vector slots",
                    constellation.constellation_id
                ),
            );
        }
        active_slots.extend(slots.keys().copied());
        store
            .weave(cx_id, &slots)
            .map_err(loom_phase_error("weave"))?;
    }
    let measured_count = store.measured_count();
    let materialized_count = store.xterm_count();
    let n_eff = measured_count as f32 / index.constellation_count.max(1) as f32;
    let abundance = AbundanceReport::new(
        active_slots.len(),
        index.constellation_count,
        materialized_count,
        NeffEstimate::Provisional { value: n_eff },
        CeilingEstimate::Provisional {
            bits: (index.constellation_count.max(1) as f32 + 1.0).log2(),
        },
        measured_count,
        materialized_count,
    );
    let agreement_edges = store
        .agreement_graph()
        .into_iter()
        .map(|edge| WorldCoverageAgreementEdge {
            a_slot_id: edge.a.get(),
            b_slot_id: edge.b.get(),
            raw_mean_agreement: edge.raw_mean_agreement,
            mean_agreement: edge.mean_agreement,
            agreement_weight: edge.agreement_weight,
            n: edge.n,
        })
        .collect();
    Ok(LoomSummary {
        abundance,
        measured_count,
        derived_count: materialized_count,
        materialized_count,
        agreement_edges,
        cx_by_constellation,
    })
}

fn bucket_report(
    index: &WorldMemoryIndex,
    bucket: &WorldCoverageBucketSpec,
) -> WorldCoverageBucketReport {
    let eligible = index
        .constellations
        .iter()
        .filter(|row| {
            bucket.source_kinds.is_empty() || bucket.source_kinds.contains(&row.source_kind)
        })
        .collect::<Vec<_>>();
    let matched = eligible
        .iter()
        .copied()
        .filter(|row| lens_has_terms(row, bucket.lens, &bucket.required_terms))
        .map(|row| row.constellation_id.clone())
        .collect::<Vec<_>>();
    let share_milli = share_milli(matched.len(), eligible.len());
    let mut status = WorldCoverageBucketStatus::Balanced;
    let mut reasons = Vec::new();
    if matched.len() < bucket.min_count {
        status = if matched.is_empty() {
            WorldCoverageBucketStatus::BlindSpot
        } else {
            WorldCoverageBucketStatus::Underrepresented
        };
        reasons.push(format!(
            "count {} below required minimum {}",
            matched.len(),
            bucket.min_count
        ));
    } else if bucket.min_share_milli > 0 && share_milli < bucket.min_share_milli {
        status = WorldCoverageBucketStatus::Underrepresented;
        reasons.push(format!(
            "share {} below required minimum {}",
            share_milli, bucket.min_share_milli
        ));
    }
    if bucket
        .max_count
        .is_some_and(|max_count| matched.len() > max_count)
    {
        status = WorldCoverageBucketStatus::Overrepresented;
        reasons.push(format!(
            "count {} above maximum {}",
            matched.len(),
            bucket.max_count.unwrap()
        ));
    }
    if bucket
        .max_share_milli
        .is_some_and(|max_share| share_milli > max_share)
    {
        status = WorldCoverageBucketStatus::Overrepresented;
        reasons.push(format!(
            "share {} above maximum {}",
            share_milli,
            bucket.max_share_milli.unwrap()
        ));
    }
    WorldCoverageBucketReport {
        bucket_id: bucket.bucket_id.clone(),
        label: bucket.label.clone(),
        lens: bucket.lens,
        eligible_count: eligible.len(),
        match_count: matched.len(),
        share_milli,
        status,
        matched_constellation_ids: matched,
        reasons,
    }
}

fn bucket_findings(
    bucket: &WorldCoverageBucketSpec,
    report: &WorldCoverageBucketReport,
) -> Vec<WorldCoverageFinding> {
    if report.status == WorldCoverageBucketStatus::Balanced {
        return Vec::new();
    }
    let kind = match report.status {
        WorldCoverageBucketStatus::BlindSpot => WorldCoverageFindingKind::BlindSpot,
        WorldCoverageBucketStatus::Underrepresented => WorldCoverageFindingKind::Underrepresented,
        WorldCoverageBucketStatus::Overrepresented => WorldCoverageFindingKind::Overrepresented,
        WorldCoverageBucketStatus::Balanced => return Vec::new(),
    };
    vec![WorldCoverageFinding {
        kind,
        severity: severity(report),
        bucket_id: bucket.bucket_id.clone(),
        reason: report.reasons.join("; "),
        actual_count: report.match_count,
        expected_min_count: (bucket.min_count > 0).then_some(bucket.min_count),
        expected_max_count: bucket.max_count,
        actual_share_milli: report.share_milli,
        threshold_milli: bucket.max_share_milli.or(Some(bucket.min_share_milli)),
        evidence_constellation_ids: report.matched_constellation_ids.clone(),
    }]
}

fn bucket_lens_blind_spots(
    index: &WorldMemoryIndex,
    bucket: &WorldCoverageBucketSpec,
    loom: &LoomSummary,
) -> Result<Vec<WorldCoverageLensBlindSpot>, EngineError> {
    if bucket.lens == WorldMemoryLensKind::Text {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for row in &index.constellations {
        if !bucket.source_kinds.is_empty() && !bucket.source_kinds.contains(&row.source_kind) {
            continue;
        }
        if !lens_has_terms(row, WorldMemoryLensKind::Text, &bucket.required_terms)
            || lens_has_terms(row, bucket.lens, &bucket.required_terms)
        {
            continue;
        }
        let cx_id = *loom
            .cx_by_constellation
            .get(&row.constellation_id)
            .ok_or_else(|| coverage_error("cx", "missing Calyx cx id for constellation"))?;
        let Some(alert) = detect_blind_spot(
            cx_id,
            lens_slot(WorldMemoryLensKind::Text),
            lens_slot(bucket.lens),
            1.0,
            0.0,
        ) else {
            continue;
        };
        out.push(WorldCoverageLensBlindSpot {
            bucket_id: bucket.bucket_id.clone(),
            constellation_id: row.constellation_id.clone(),
            calyx_cx_id: alert.cx_id.to_string(),
            text_slot_id: alert.a.get(),
            target_slot_id: alert.b.get(),
            delta: alert.delta,
            severity: severity_from_loom(alert.severity),
            reason: format!(
                "text lens contains bucket terms but {:?} lens does not",
                bucket.lens
            ),
        });
    }
    Ok(out)
}

fn loom_slots(
    constellation: &WorldMemoryConstellation,
) -> Result<BTreeMap<SlotId, Vec<f32>>, EngineError> {
    let slot = constellation
        .slots
        .iter()
        .find(|slot| slot.kind == WorldMemorySlotKind::VectorSet)
        .ok_or_else(|| coverage_error("index.vector_slot", "missing vector-set slot"))?;
    if slot.state != WorldMemorySlotState::Populated || slot.vectors.is_empty() {
        return invalid_world_content_coverage(
            "index.vector_slot",
            format!(
                "{} vector-set slot is not populated",
                constellation.constellation_id
            ),
        );
    }
    let mut out = BTreeMap::new();
    for vector in &slot.vectors {
        let slot_id = lens_slot(vector.source_lens);
        if out
            .insert(slot_id, feature_vector_values(vector)?)
            .is_some()
        {
            return invalid_world_content_coverage(
                "index.vector_slot",
                format!(
                    "{} has duplicate {:?} vector",
                    constellation.constellation_id, vector.source_lens
                ),
            );
        }
    }
    Ok(out)
}

fn feature_vector_values(vector: &WorldMemoryFeatureVector) -> Result<Vec<f32>, EngineError> {
    if vector.dimension == 0 || vector.values.len() != vector.dimension {
        return invalid_world_content_coverage(
            "index.vector.shape",
            format!("{} has invalid dimensions", vector.vector_id),
        );
    }
    if vector.values.iter().all(|value| *value == 0) {
        return invalid_world_content_coverage(
            "index.vector.zero",
            format!("{} has zero norm", vector.vector_id),
        );
    }
    Ok(vector.values.iter().map(|value| *value as f32).collect())
}

fn lens_has_terms(
    constellation: &WorldMemoryConstellation,
    lens: WorldMemoryLensKind,
    terms: &[String],
) -> bool {
    let Some(lens) = constellation.lenses.get(&lens) else {
        return false;
    };
    terms.iter().all(|term| {
        term.split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .all(|token| lens.terms.contains_key(&token.to_ascii_lowercase()))
    })
}

fn severity(report: &WorldCoverageBucketReport) -> WorldCoverageSeverity {
    match report.status {
        WorldCoverageBucketStatus::BlindSpot => WorldCoverageSeverity::High,
        WorldCoverageBucketStatus::Underrepresented => {
            if report.share_milli <= 250 {
                WorldCoverageSeverity::High
            } else {
                WorldCoverageSeverity::Medium
            }
        }
        WorldCoverageBucketStatus::Overrepresented => {
            if report.share_milli >= 750 {
                WorldCoverageSeverity::High
            } else {
                WorldCoverageSeverity::Medium
            }
        }
        WorldCoverageBucketStatus::Balanced => WorldCoverageSeverity::Low,
    }
}

fn severity_from_loom(value: Severity) -> WorldCoverageSeverity {
    match value {
        Severity::Low => WorldCoverageSeverity::Low,
        Severity::Medium => WorldCoverageSeverity::Medium,
        Severity::High => WorldCoverageSeverity::High,
    }
}

fn share_milli(count: usize, total: usize) -> u16 {
    (count.saturating_mul(1000) + total / 2)
        .checked_div(total)
        .map_or(0, |share| share.min(1000) as u16)
}

fn lens_slot(lens: WorldMemoryLensKind) -> SlotId {
    match lens {
        WorldMemoryLensKind::Text => SlotId::new(1),
        WorldMemoryLensKind::Structured => SlotId::new(2),
        WorldMemoryLensKind::Code => SlotId::new(3),
    }
}

fn cx_for_constellation(constellation_id: &str) -> CxId {
    CxId::from_input(constellation_id.as_bytes(), 1, CONTENT_COVERAGE_CX_SALT)
}

fn loom_phase_error(phase: &'static str) -> impl Fn(calyx_core::CalyxError) -> EngineError {
    move |error| coverage_error(phase, format!("{}: {}", error.code, error.message))
}

pub(super) fn invalid_world_content_coverage<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(coverage_error(field, reason.into()))
}

fn coverage_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_content_coverage.{}", field.into()),
        reason: reason.to_string(),
    }
}
