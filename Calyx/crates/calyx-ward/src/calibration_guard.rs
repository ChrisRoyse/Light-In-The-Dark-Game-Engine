//! Degenerate-calibration guard for per-slot conformal Ward calibration.
//!
//! # Why this exists
//!
//! `calibrate_slot` will happily emit a `tau` for *any* set of bad scores. If the calibration
//! negatives are near-duplicates of the trusted (in-distribution) corpus — the classic
//! contaminated-negative pitfall — the conformal threshold is pushed up into the region where
//! legitimate answers live, and the resulting detector refuses everything (or, symmetrically,
//! admits everything). Nothing downstream can tell a *useless* tau from a *good* one: the profile
//! looks calibrated, carries an estimator and a FAR, and silently rejects every real query.
//!
//! This guard makes that failure **loud**. Before trusting a calibration, assess whether the
//! good/bad score distributions are actually separable, and if they overlap past a margin, fail
//! with [`WardError::DegenerateCalibration`] naming the overlap statistic — never return a tau.
//!
//! # The statistics (grounded in the OOD-detection literature)
//!
//! A detector is only as good as the negatives it was calibrated on. Three complementary,
//! threshold-honest statistics are reported; any one out of bounds fails the calibration:
//!
//! * **AUROC** — threshold-*independent* separability, `P(good > bad)`. The canonical OOD
//!   separability metric (Hendrycks & Gimpel 2017). 0.5 means the score cannot tell ID from OOD at
//!   all; 1.0 means perfect separation.
//! * **FPR@TPR** (a.k.a. FPR95) — the false-positive rate at the operating point that admits
//!   `tpr_target` of the trusted corpus. This is the standard fixed-threshold OOD metric
//!   (Humblot-Renaux et al. 2023; "Beyond AUROC & co."). Contaminated negatives inflate it
//!   directly: the arXiv:2503.07330 study measured near-in-distribution negatives inflating FPR95
//!   by ~10 percentage points. It is the statistic that most directly captures "the negatives sit
//!   where legitimate answers do".
//! * **separation margin** — `id_pass_floor - ood_ceiling`, the raw cosine gap between the ID score
//!   below which only `1 - tpr_target` of the trusted corpus falls and the highest OOD score. A
//!   non-positive margin means at least one negative scores as high as the ID pass floor.
//!
//! The default bounds encode the FPR95 convention (95% TPR, ≤5% FPR) plus a strict AUROC floor and
//! a strictly-positive margin. Callers author high-stakes response corpora against these; a corpus
//! that cannot clear them is not a stricter detector, it is a broken one.

use serde::{Deserialize, Serialize};

use crate::error::WardError;

/// Default true-positive-rate operating point: admit 95% of the trusted corpus, then measure the
/// false-positive rate there (the FPR95 convention).
pub const DEFAULT_TPR_TARGET: f32 = 0.95;
/// Default ceiling on FPR at the TPR operating point (FPR95 ≤ 5%).
pub const DEFAULT_MAX_FPR_AT_TPR: f32 = 0.05;
/// Default AUROC floor. Below this the score cannot separate ID from OOD well enough to calibrate.
pub const DEFAULT_MIN_AUROC: f32 = 0.90;
/// Default minimum cosine separation margin between the ID pass floor and the OOD ceiling.
pub const DEFAULT_MIN_SEPARATION_MARGIN: f32 = 0.0;

/// Thresholds for [`assess_separation`]. Defaults encode the FPR95 convention.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeparationThresholds {
    /// Fraction of the trusted (good) corpus that must be admitted when choosing the operating
    /// point. The FPR is then measured at that threshold.
    pub tpr_target: f32,
    /// Maximum tolerated false-positive rate (bad scores admitted) at the TPR operating point.
    pub max_fpr_at_tpr: f32,
    /// Minimum tolerated AUROC between good and bad scores.
    pub min_auroc: f32,
    /// Minimum tolerated cosine separation margin (`id_pass_floor - ood_ceiling`).
    pub min_separation_margin: f32,
}

impl Default for SeparationThresholds {
    fn default() -> Self {
        Self {
            tpr_target: DEFAULT_TPR_TARGET,
            max_fpr_at_tpr: DEFAULT_MAX_FPR_AT_TPR,
            min_auroc: DEFAULT_MIN_AUROC,
            min_separation_margin: DEFAULT_MIN_SEPARATION_MARGIN,
        }
    }
}

/// The computed separability of one slot's good/bad calibration scores.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeparationReport {
    pub good_count: usize,
    pub bad_count: usize,
    /// `P(good > bad)`, ties counted as 0.5. Threshold-independent separability.
    pub auroc: f32,
    /// The cosine threshold that admits `tpr_target` of the good scores.
    pub id_pass_floor: f32,
    /// The highest bad score (the OOD ceiling).
    pub ood_ceiling: f32,
    /// `id_pass_floor - ood_ceiling`.
    pub separation_margin: f32,
    /// Fraction of bad scores admitted at `id_pass_floor` (FPR at the TPR operating point).
    pub fpr_at_tpr: f32,
    /// The TPR operating point the FPR was measured at.
    pub tpr_target: f32,
}

/// Assess whether good/bad calibration scores are separable enough to trust a conformal tau.
///
/// Returns the [`SeparationReport`] when the calibration is healthy; returns
/// [`WardError::DegenerateCalibration`] — naming the specific overlap statistic that failed — when
/// the distributions overlap past the configured margin. Never returns a tau: a degenerate
/// calibration must fail loud, not silently produce a useless threshold.
pub fn assess_separation(
    good_scores: &[f32],
    bad_scores: &[f32],
    thresholds: SeparationThresholds,
) -> Result<SeparationReport, WardError> {
    validate(good_scores, bad_scores, thresholds)?;

    let mut good = good_scores.to_vec();
    good.sort_by(|a, b| a.total_cmp(b));
    let mut bad = bad_scores.to_vec();
    bad.sort_by(|a, b| a.total_cmp(b));

    let auroc = auroc(&good, &bad);
    let id_pass_floor = lower_quantile(&good, 1.0 - thresholds.tpr_target);
    let ood_ceiling = *bad.last().expect("bad non-empty after validate");
    let separation_margin = id_pass_floor - ood_ceiling;
    let fpr_at_tpr = fraction(
        bad.iter().filter(|score| **score >= id_pass_floor).count(),
        bad.len(),
    );

    let report = SeparationReport {
        good_count: good.len(),
        bad_count: bad.len(),
        auroc,
        id_pass_floor,
        ood_ceiling,
        separation_margin,
        fpr_at_tpr,
        tpr_target: thresholds.tpr_target,
    };

    // FPR@TPR is the primary named statistic: it directly measures negatives intruding into the
    // region legitimate answers occupy. AUROC and the margin are reported alongside so a failure is
    // diagnosable, not just loud.
    if report.fpr_at_tpr > thresholds.max_fpr_at_tpr {
        return Err(degenerate(
            "fpr_at_tpr",
            report.fpr_at_tpr,
            thresholds.max_fpr_at_tpr,
            &report,
        ));
    }
    if report.auroc < thresholds.min_auroc {
        return Err(degenerate(
            "auroc",
            report.auroc,
            thresholds.min_auroc,
            &report,
        ));
    }
    if report.separation_margin <= thresholds.min_separation_margin {
        return Err(degenerate(
            "separation_margin",
            report.separation_margin,
            thresholds.min_separation_margin,
            &report,
        ));
    }
    Ok(report)
}

fn validate(good: &[f32], bad: &[f32], thresholds: SeparationThresholds) -> Result<(), WardError> {
    if good.len() < 2 {
        return Err(WardError::InvalidCalibrationInput {
            reason: "separation guard needs at least 2 trusted scores to measure ID variety",
        });
    }
    if bad.is_empty() {
        return Err(WardError::InvalidCalibrationInput {
            reason: "separation guard needs at least 1 bad score",
        });
    }
    if good.iter().chain(bad).any(|score| !score.is_finite()) {
        return Err(WardError::InvalidCalibrationInput {
            reason: "separation scores must be finite",
        });
    }
    if !(0.0..=1.0).contains(&thresholds.tpr_target)
        || !(0.0..=1.0).contains(&thresholds.max_fpr_at_tpr)
        || !(0.0..=1.0).contains(&thresholds.min_auroc)
    {
        return Err(WardError::InvalidCalibrationInput {
            reason: "separation thresholds must be fractions in [0,1]",
        });
    }
    Ok(())
}

fn degenerate(statistic: &str, value: f32, bound: f32, report: &SeparationReport) -> WardError {
    WardError::DegenerateCalibration {
        statistic: statistic.to_string(),
        value: f64::from(value),
        bound: f64::from(bound),
        detail: format!(
            "good/bad calibration score distributions overlap: auroc={:.4} fpr_at_tpr={:.4} \
             (tpr={:.2}) id_pass_floor={:.4} ood_ceiling={:.4} margin={:.4} good_n={} bad_n={}",
            report.auroc,
            report.fpr_at_tpr,
            report.tpr_target,
            report.id_pass_floor,
            report.ood_ceiling,
            report.separation_margin,
            report.good_count,
            report.bad_count,
        ),
    }
}

/// AUROC via the Mann-Whitney U statistic: the probability a random good score exceeds a random
/// bad score, ties counted as 0.5. Both inputs are sorted ascending.
fn auroc(good_sorted: &[f32], bad_sorted: &[f32]) -> f32 {
    let mut wins = 0.0_f64;
    for &g in good_sorted {
        // bad below g contribute a full win; bad equal to g contribute half.
        let below = bad_sorted.partition_point(|&b| b < g);
        let not_above = bad_sorted.partition_point(|&b| b <= g);
        let ties = not_above - below;
        wins += below as f64 + 0.5 * ties as f64;
    }
    (wins / (good_sorted.len() as f64 * bad_sorted.len() as f64)) as f32
}

/// The value below which fraction `q` of the sorted-ascending scores fall (nearest-rank).
fn lower_quantile(sorted_ascending: &[f32], q: f32) -> f32 {
    let n = sorted_ascending.len();
    if n == 0 {
        return 0.0;
    }
    let q = q.clamp(0.0, 1.0);
    let index = ((q * (n as f32 - 1.0)).round() as usize).min(n - 1);
    sorted_ascending[index]
}

fn fraction(count: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        count as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_separation_passes() {
        // ID cohesive near 0.8, OOD near 0.0 — cleanly separable.
        let good = vec![0.78, 0.81, 0.79, 0.83, 0.80];
        let bad = vec![0.02, 0.05, -0.01, 0.10, 0.03, 0.00];
        let report = assess_separation(&good, &bad, SeparationThresholds::default())
            .expect("healthy calibration must pass");
        assert!(report.auroc > 0.99, "auroc={}", report.auroc);
        assert!(report.fpr_at_tpr <= 0.05, "fpr={}", report.fpr_at_tpr);
        assert!(report.separation_margin > 0.0);
    }

    #[test]
    fn near_duplicate_negatives_fail_loud_on_fpr() {
        // The #1548 degeneracy: near-duplicate negatives copy the whole trusted vocabulary, so they
        // score *higher* than the trusted corpus's own leave-one-out paraphrase variety. Every
        // negative lands above the 95%-TPR id_pass_floor, so FPR@TPR is 1.0.
        let good = vec![0.55, 0.62, 0.58, 0.70, 0.60];
        let bad = vec![0.75, 0.78, 0.76, 0.77, 0.80, 0.74];
        let err = assess_separation(&good, &bad, SeparationThresholds::default())
            .expect_err("near-duplicate negatives must fail loud");
        match err {
            WardError::DegenerateCalibration {
                ref statistic,
                value,
                bound,
                ..
            } => {
                assert_eq!(statistic, "fpr_at_tpr");
                assert!(value > bound, "value={value} bound={bound}");
            }
            other => panic!("expected DegenerateCalibration, got {other:?}"),
        }
        assert_eq!(err.code(), "CALYX_GUARD_DEGENERATE_CALIBRATION");
    }

    #[test]
    fn indistinguishable_distributions_fail() {
        let good = vec![0.5, 0.5, 0.5, 0.5];
        let bad = vec![0.5, 0.5, 0.5, 0.5, 0.5];
        let err = assess_separation(&good, &bad, SeparationThresholds::default())
            .expect_err("identical distributions cannot calibrate");
        assert!(matches!(err, WardError::DegenerateCalibration { .. }));
    }

    #[test]
    fn too_few_good_scores_is_invalid_not_degenerate() {
        let err = assess_separation(&[0.9], &[0.1, 0.2], SeparationThresholds::default())
            .expect_err("one trusted score cannot show ID variety");
        assert!(matches!(err, WardError::InvalidCalibrationInput { .. }));
    }
}
