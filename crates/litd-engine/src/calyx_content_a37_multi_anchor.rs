use std::collections::BTreeMap;

use calyx_assay::{
    A37_DIVERSITY_DIAGNOSTIC_ONLY, A37_DIVERSITY_GATE_PASSED, DEFAULT_GATE_PANEL_LENSES,
    DEFAULT_MAX_REDUNDANCY, DEFAULT_MIN_MARGINAL_BITS, EnsembleCard, a37_association_family,
};
use serde::{Deserialize, Serialize};

use litd_hash::sha256_hex;

use crate::{
    calyx_content::EngineCalyxContentError,
    calyx_content_a37_admission::{
        EngineContentA37AdmissionReport, EngineContentA37LensEvidence,
        EngineContentA37TargetLensValue, EngineContentA37TargetSummary,
    },
    calyx_content_a37_identity::{EngineContentA37FrozenRoster, roster_hash_from_evidence},
    calyx_content_assay::EngineContentAssayReport,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineContentA37InputReport {
    pub target_class: usize,
    pub domain: String,
    pub source: String,
    pub card: EnsembleCard,
}

impl EngineContentA37InputReport {
    pub fn from_assay_report(
        target_class: usize,
        domain: impl Into<String>,
        source: impl Into<String>,
        report: &EngineContentAssayReport,
    ) -> Self {
        Self {
            target_class,
            domain: domain.into(),
            source: source.into(),
            card: report.ensemble.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineContentA37MultiAnchorConfig {
    pub min_lenses: usize,
    pub min_marginal_bits: f32,
    pub max_redundancy: f32,
    pub mode: String,
}

impl Default for EngineContentA37MultiAnchorConfig {
    fn default() -> Self {
        Self {
            min_lenses: DEFAULT_GATE_PANEL_LENSES,
            min_marginal_bits: DEFAULT_MIN_MARGINAL_BITS,
            max_redundancy: DEFAULT_MAX_REDUNDANCY,
            mode: "gate".to_string(),
        }
    }
}

pub fn build_engine_content_a37_admission_report(
    inputs: &[EngineContentA37InputReport],
    config: &EngineContentA37MultiAnchorConfig,
    roster: &EngineContentA37FrozenRoster,
) -> Result<EngineContentA37AdmissionReport, EngineCalyxContentError> {
    validate_config(config)?;
    validate_inputs(inputs, config.min_lenses)?;
    bind_frozen_roster(inputs, roster)?;
    let lens_count = inputs[0].card.lenses.len();
    let target_summaries = inputs
        .iter()
        .map(|input| target_summary(input, config.max_redundancy))
        .collect::<Result<Vec<_>, _>>()?;
    let redundancy_bound_pass = target_summaries
        .iter()
        .all(|target| target.redundancy_bound_pass);
    let (families, family_span_pass) = association_families(&inputs[0].card);
    let lenses = lens_evidence(inputs, config.min_marginal_bits, roster)?;
    let evidence_hash = roster_hash_from_evidence(&lenses);
    if evidence_hash != roster.registry_roster_sha256 {
        return a37_error(format!(
            "built A37 admission roster hash {evidence_hash} does not match measured roster {}",
            roster.registry_roster_sha256
        ));
    }
    let passing_lens_count = lenses.iter().filter(|lens| lens.passed).count();
    let no_collapse_pass = passing_lens_count == lens_count;
    let min_best_marginal_bits = lenses
        .iter()
        .map(|lens| lens.best_marginal_bits)
        .fold(f32::INFINITY, f32::min);
    let max_best_marginal_bits = lenses
        .iter()
        .map(|lens| lens.best_marginal_bits)
        .fold(f32::NEG_INFINITY, f32::max);
    let weakest_lens = lenses
        .iter()
        .min_by(|left, right| left.best_marginal_bits.total_cmp(&right.best_marginal_bits))
        .map(|lens| lens.name.clone())
        .unwrap_or_else(|| "none".to_string());
    let gate_passed = family_span_pass && redundancy_bound_pass && no_collapse_pass;
    Ok(EngineContentA37AdmissionReport {
        schema_version: 1,
        role: "a37_multi_anchor_admission_card".to_string(),
        status: if gate_passed {
            A37_DIVERSITY_GATE_PASSED
        } else {
            A37_DIVERSITY_DIAGNOSTIC_ONLY
        }
        .to_string(),
        mode: config.mode.clone(),
        panel_template: roster.panel_template.clone(),
        panel_version: roster.panel_version,
        panel_state_sha256: roster.panel_state_sha256.clone(),
        registry_roster_sha256: roster.registry_roster_sha256.clone(),
        source_card_sha256: source_card_hash(inputs),
        source_of_truth: format!(
            "panel:{}@v{}#{}",
            roster.panel_template, roster.panel_version, roster.registry_roster_sha256
        ),
        gate_passed,
        report_count: inputs.len(),
        lens_count,
        passing_lens_count,
        min_lenses: config.min_lenses,
        min_marginal_bits: config.min_marginal_bits,
        max_redundancy: config.max_redundancy,
        family_span_pass,
        redundancy_bound_pass,
        no_collapse_pass,
        association_family_count: families.len(),
        association_families: families,
        min_best_marginal_bits,
        max_best_marginal_bits,
        weakest_lens,
        target_summaries,
        lenses,
        source_reports: inputs.iter().map(|input| input.source.clone()).collect(),
    })
}

fn validate_config(
    config: &EngineContentA37MultiAnchorConfig,
) -> Result<(), EngineCalyxContentError> {
    if config.min_lenses == 0 {
        return a37_error("multi-anchor admission requires min_lenses > 0");
    }
    if !config.min_marginal_bits.is_finite() || config.min_marginal_bits < 0.0 {
        return a37_error(
            "multi-anchor admission min_marginal_bits must be finite and non-negative",
        );
    }
    if !config.max_redundancy.is_finite() || !(0.0..=1.0).contains(&config.max_redundancy) {
        return a37_error("multi-anchor admission max_redundancy must be finite in [0,1]");
    }
    if config.mode.trim().is_empty() {
        return a37_error("multi-anchor admission mode must be non-empty");
    }
    Ok(())
}

/// Fails closed unless the ensemble roster is exactly the measured frozen roster
/// (same order, same slot ids, same names). This binds the card the builder emits
/// to the trusted local roster before any frozen identity is copied in.
fn bind_frozen_roster(
    inputs: &[EngineContentA37InputReport],
    roster: &EngineContentA37FrozenRoster,
) -> Result<(), EngineCalyxContentError> {
    let card = &inputs[0].card;
    if roster.lenses.len() != card.lenses.len() {
        return a37_error(format!(
            "measured frozen roster has {} lenses; ensemble cards have {}",
            roster.lenses.len(),
            card.lenses.len()
        ));
    }
    for (lens, frozen) in card.lenses.iter().zip(roster.lenses.iter()) {
        if lens.slot.get() != frozen.slot || lens.name != frozen.name {
            return a37_error(format!(
                "ensemble lens {}:{} does not match measured frozen roster entry {}:{}",
                lens.slot.get(),
                lens.name,
                frozen.slot,
                frozen.name
            ));
        }
        if frozen.frozen_lens_id.trim().is_empty()
            || frozen.weights_sha256.trim().is_empty()
            || frozen.runtime_fingerprint.trim().is_empty()
        {
            return a37_error(format!(
                "measured frozen roster entry {}:{} is missing frozen identity",
                frozen.slot, frozen.name
            ));
        }
    }
    Ok(())
}

fn source_card_hash(inputs: &[EngineContentA37InputReport]) -> String {
    let mut canonical = String::new();
    for input in inputs {
        canonical.push_str(&format!(
            "{}|{}|{}|{:.6}|{:.6}\n",
            input.target_class, input.domain, input.source, input.card.panel_bits, input.card.n_eff
        ));
    }
    sha256_hex(canonical.as_bytes())
}

fn validate_inputs(
    inputs: &[EngineContentA37InputReport],
    min_lenses: usize,
) -> Result<(), EngineCalyxContentError> {
    if inputs.len() < 2 {
        return a37_error("multi-anchor admission requires at least two input reports");
    }
    let first = &inputs[0].card.lenses;
    if first.len() < min_lenses {
        return a37_error(format!(
            "multi-anchor admission requires at least {min_lenses} lenses; got {}",
            first.len()
        ));
    }
    let expected = first
        .iter()
        .map(|lens| (lens.slot.get(), lens.name.clone()))
        .collect::<Vec<_>>();
    for input in inputs {
        if input.domain.trim().is_empty() || input.source.trim().is_empty() {
            return a37_error("multi-anchor input domain and source must be non-empty");
        }
        validate_card(input)?;
        let got = input
            .card
            .lenses
            .iter()
            .map(|lens| (lens.slot.get(), lens.name.clone()))
            .collect::<Vec<_>>();
        if got != expected {
            return a37_error(format!(
                "multi-anchor input {} lens roster differs from first report",
                input.source
            ));
        }
    }
    Ok(())
}

fn validate_card(input: &EngineContentA37InputReport) -> Result<(), EngineCalyxContentError> {
    if input.card.lenses.is_empty() {
        return a37_error(format!("multi-anchor input {} has no lenses", input.source));
    }
    finite(&input.source, "panel_bits", input.card.panel_bits)?;
    finite(&input.source, "n_eff", input.card.n_eff)?;
    finite(
        &input.source,
        "mean_pairwise_corr",
        input.card.a37_diversity.mean_pairwise_corr,
    )?;
    finite(
        &input.source,
        "mean_pairwise_nmi",
        input.card.a37_diversity.mean_pairwise_nmi,
    )?;
    for lens in &input.card.lenses {
        finite(&input.source, "lens.solo_bits", lens.solo_bits)?;
        finite(&input.source, "lens.marginal_bits", lens.marginal_bits)?;
    }
    Ok(())
}

fn target_summary(
    input: &EngineContentA37InputReport,
    max_redundancy: f32,
) -> Result<EngineContentA37TargetSummary, EngineCalyxContentError> {
    let card = &input.card;
    let max_marginal_bits = card
        .lenses
        .iter()
        .map(|lens| lens.marginal_bits)
        .fold(f32::NEG_INFINITY, f32::max);
    finite(&input.source, "target.max_marginal_bits", max_marginal_bits)?;
    let n_eff_floor = card.lenses.len().max(DEFAULT_GATE_PANEL_LENSES) as f32 * 0.6;
    let redundancy_bound_pass = card.n_eff >= n_eff_floor
        && card.a37_diversity.mean_pairwise_corr <= max_redundancy
        && card.a37_diversity.mean_pairwise_nmi <= max_redundancy;
    Ok(EngineContentA37TargetSummary {
        target_class: input.target_class,
        domain: input.domain.clone(),
        report_path: input.source.clone(),
        status: card.a37_diversity.status.clone(),
        no_collapse_pass: card.a37_diversity.no_collapse_pass,
        family_span_pass: card.a37_diversity.family_span_pass,
        redundancy_bound_pass,
        n_eff: card.n_eff,
        panel_bits: card.panel_bits,
        max_marginal_bits,
        keep_count: card.keep_count,
        park_count: card.park_count,
    })
}

fn association_families(card: &EnsembleCard) -> (BTreeMap<String, Vec<u16>>, bool) {
    let mut families = BTreeMap::<String, Vec<u16>>::new();
    for lens in &card.lenses {
        let family = a37_association_family(&lens.name);
        if family != "temporal_sidecar" {
            families
                .entry(family.to_string())
                .or_default()
                .push(lens.slot.get());
        }
    }
    let pass = families.len() >= 2;
    (families, pass)
}

fn lens_evidence(
    inputs: &[EngineContentA37InputReport],
    min_marginal_bits: f32,
    roster: &EngineContentA37FrozenRoster,
) -> Result<Vec<EngineContentA37LensEvidence>, EngineCalyxContentError> {
    let mut out = Vec::new();
    for lens_idx in 0..inputs[0].card.lenses.len() {
        let first = &inputs[0].card.lenses[lens_idx];
        let frozen = &roster.lenses[lens_idx];
        let target_values = inputs
            .iter()
            .map(|input| {
                let lens = &input.card.lenses[lens_idx];
                EngineContentA37TargetLensValue {
                    target_class: input.target_class,
                    domain: input.domain.clone(),
                    marginal_bits: lens.marginal_bits,
                    solo_bits: lens.solo_bits,
                    decision: format!("{:?}", lens.decision),
                }
            })
            .collect::<Vec<_>>();
        let best = target_values
            .iter()
            .max_by(|left, right| left.marginal_bits.total_cmp(&right.marginal_bits))
            .ok_or_else(|| a37_error_value("multi-anchor lens has no target values"))?;
        out.push(EngineContentA37LensEvidence {
            slot: first.slot.get(),
            name: first.name.clone(),
            frozen_lens_id: frozen.frozen_lens_id.clone(),
            weights_sha256: frozen.weights_sha256.clone(),
            runtime_fingerprint: frozen.runtime_fingerprint.clone(),
            association_family: a37_association_family(&first.name).to_string(),
            passed: best.marginal_bits >= min_marginal_bits,
            best_target_class: best.target_class,
            best_domain: best.domain.clone(),
            best_marginal_bits: best.marginal_bits,
            best_solo_bits: best.solo_bits,
            target_values,
        });
    }
    Ok(out)
}

fn finite(source: &str, field: &str, value: f32) -> Result<(), EngineCalyxContentError> {
    if value.is_finite() {
        Ok(())
    } else {
        a37_error(format!(
            "multi-anchor input {source} has non-finite {field}"
        ))
    }
}

fn a37_error<T>(reason: impl ToString) -> Result<T, EngineCalyxContentError> {
    Err(a37_error_value(reason))
}

fn a37_error_value(reason: impl ToString) -> EngineCalyxContentError {
    EngineCalyxContentError::Measurement {
        phase: "a37_admission".to_string(),
        reason: reason.to_string(),
    }
}
