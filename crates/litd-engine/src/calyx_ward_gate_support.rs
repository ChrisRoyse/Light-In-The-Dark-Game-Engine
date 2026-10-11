use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use calyx_core::{FixedClock, SlotId, dense_cosine};
use calyx_ward::{
    CalibrationInput, GuardId, GuardPolicy, GuardProfile, GuardVerdict, MatchedSlots,
    NoveltyAction, ProducedSlots, SlotKind, calibrate, guard,
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    calyx_ward_gate::{
        EngineCalyxWardCalibrationReadback, EngineCalyxWardGateError, EngineCalyxWardGateReadback,
        EngineCalyxWardPolicy, EngineCalyxWardSubject, ward_failed,
    },
    math::Fixed,
    sim_order::SimOrderKind,
};

const ENGINE_WARD_GUARD_ID: &str = "018f48a4-9a79-74d2-8a5c-9ad7f6b8c145";
const ENGINE_WARD_PANEL_VERSION: u64 = 1;
const ENGINE_WARD_CALIBRATION_TS: u64 = 1_786_000_000;
const ENGINE_WARD_ALPHA: f32 = 0.01;
const ENGINE_WARD_TARGET_FAR: f32 = 0.0;
/// Dimensionality of the hashed feature vectors.
///
/// Was 64, which is far too small for the hashing trick and made the n-gram slots (2 and 3) report
/// noise (#1548). A single 12-word subject produces ~10 bigrams, ~10 trigrams and ~120 character
/// n-grams; hashing those into 64 buckets guarantees pervasive collisions, so two texts sharing
/// *no* n-grams still scored a substantial cosine. Measured: an off-domain calibration text ("the
/// stock market closed higher on tuesday...") scored **0.376** against the trusted corpus in the
/// bigram slot — a similarity it does not have. That noise floor became `tau`, and a legitimate
/// response then missed admission by 0.0001.
///
/// 2048 buckets puts collisions well below the signal for subjects of this length, so an unrelated
/// text scores ~0 and the threshold reflects real similarity. This path is out-of-band (never the
/// sim hot path), so the extra 8 KB per vector is free.
const ENGINE_WARD_DIM: usize = 2048;
const ENGINE_WARD_SLOTS: [u16; 3] = [1, 2, 3];

pub(crate) struct EngineWardGuard {
    pub(crate) calibration: EngineCalyxWardCalibrationReadback,
    profile: GuardProfile,
    trusted_vectors: BTreeMap<SlotId, Vec<Vec<f32>>>,
}

pub(crate) struct EngineWardSimilarity {
    pub(crate) verdict: GuardVerdict,
}

pub(crate) fn build_ward_guard(
    policy: &EngineCalyxWardPolicy,
) -> Result<EngineWardGuard, EngineCalyxWardGateError> {
    let trusted_vectors = trusted_vectors(&policy.trusted_texts);
    let slots = ward_slots();
    let template = GuardProfile {
        guard_id: guard_id()?,
        panel_version: ENGINE_WARD_PANEL_VERSION,
        domain: format!("litd-engine/{}", policy.policy_id.trim()),
        tau: BTreeMap::new(),
        required_slots: slots.clone(),
        policy: GuardPolicy::AllRequired,
        calibration: None,
        novelty_action: NoveltyAction::RejectClosed,
    };
    let inputs = slots
        .iter()
        .map(|slot| CalibrationInput {
            slot: *slot,
            good_scores: good_scores(*slot, &trusted_vectors),
            bad_scores: bad_scores(*slot, &trusted_vectors, &policy.calibration_ood_texts),
            slot_kind: SlotKind::Content,
            target_far: ENGINE_WARD_TARGET_FAR,
        })
        .collect::<Vec<_>>();
    let clock = FixedClock::new(ENGINE_WARD_CALIBRATION_TS);
    let profile = calibrate(template, inputs, ENGINE_WARD_ALPHA, &clock).map_err(ward_failed)?;
    let calibration = calibration_readback(&profile)?;
    Ok(EngineWardGuard {
        calibration,
        profile,
        trusted_vectors,
    })
}

pub(crate) fn evaluate_ward_similarity(
    guard_state: &EngineWardGuard,
    subject: &EngineCalyxWardSubject,
) -> Result<EngineWardSimilarity, EngineCalyxWardGateError> {
    let produced = produced_slots(&subject.text);
    let matched = matched_slots(&produced, &guard_state.trusted_vectors)?;
    let verdict = guard(&guard_state.profile, &produced, &matched, true).map_err(ward_failed)?;
    Ok(EngineWardSimilarity { verdict })
}

/// Per-slot calibration scores for the degenerate-calibration separation guard (#1552).
///
/// For each Ward slot, returns `(slot, good_leave_one_out, bad)` where:
/// * `good_leave_one_out` — each trusted vector's best cosine against the OTHER trusted vectors
///   (self excluded). This reflects the corpus's genuine intra-distribution variety, NOT the
///   trivial self-match of 1.0 that `good_scores()` (used only for FAR/FRR reporting) produces.
///   Using the self-match would hide the #1548 degeneracy: near-duplicate negatives that score as
///   high as a real paraphrase look fine next to a corpus whose "good" scores are all 1.0.
/// * `bad` — each OOD text's best cosine against the trusted set.
///
/// Feeding these to `calyx_ward::assess_separation` catches a corpus whose negatives sit inside the
/// in-distribution spread before a useless tau is ever calibrated.
pub(crate) fn slot_separation_scores(
    trusted_texts: &[String],
    ood_texts: &[String],
) -> Vec<(u16, Vec<f32>, Vec<f32>)> {
    let trusted_vectors = trusted_vectors(trusted_texts);
    ward_slots()
        .into_iter()
        .map(|slot| {
            let vectors = &trusted_vectors[&slot];
            let good_loo = vectors
                .iter()
                .enumerate()
                .map(|(index, vector)| {
                    best_cosine(
                        vector,
                        vectors
                            .iter()
                            .enumerate()
                            .filter(|(other, _)| *other != index)
                            .map(|(_, candidate)| candidate),
                    )
                })
                .collect::<Vec<_>>();
            let bad = ood_texts
                .iter()
                .map(|text| best_cosine(&text_vector(text, slot), vectors.iter()))
                .collect::<Vec<_>>();
            (slot.get(), good_loo, bad)
        })
        .collect()
}

pub(crate) fn token_set(text: &str) -> BTreeSet<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| term.len() >= 2)
        .collect()
}

pub(crate) fn movement_units(kind: &SimOrderKind) -> Result<u32, EngineCalyxWardGateError> {
    match kind {
        SimOrderKind::SetVelocity(value)
        | SimOrderKind::SetAcceleration(value)
        | SimOrderKind::AddVelocity(value) => {
            let max_raw = [
                value.x.raw().unsigned_abs(),
                value.y.raw().unsigned_abs(),
                value.z.raw().unsigned_abs(),
            ]
            .into_iter()
            .max()
            .unwrap_or(0);
            Ok((max_raw / Fixed::ONE_RAW as u64) as u32)
        }
        SimOrderKind::ApplyAttack { .. } => Ok(0),
    }
}

pub(crate) fn gate_fingerprint(
    gate: &EngineCalyxWardGateReadback,
) -> Result<String, EngineCalyxWardGateError> {
    serde_json::to_vec(&json!({
        "schema": gate.schema,
        "gate_id": gate.gate_id,
        "policy": gate.policy,
        "ward_calibration": gate.ward_calibration,
        "subject_count": gate.subject_count,
        "verdicts": gate.verdicts,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(codec_failed)
}

pub(crate) fn append_json_text(value: &Value, out: &mut String) {
    match value {
        Value::Null => {}
        Value::Bool(value) => out.push_str(if *value { " true" } else { " false" }),
        Value::Number(value) => out.push_str(&format!(" {value}")),
        Value::String(value) => out.push_str(&format!(" {value}")),
        Value::Array(values) => values.iter().for_each(|value| append_json_text(value, out)),
        Value::Object(values) => {
            for (key, value) in values {
                out.push_str(&format!(" {key}"));
                append_json_text(value, out);
            }
        }
    }
}

fn trusted_vectors(texts: &[String]) -> BTreeMap<SlotId, Vec<Vec<f32>>> {
    let mut by_slot = BTreeMap::new();
    for slot in ward_slots() {
        by_slot.insert(
            slot,
            texts
                .iter()
                .map(|text| text_vector(text, slot))
                .collect::<Vec<_>>(),
        );
    }
    by_slot
}

fn good_scores(slot: SlotId, trusted_vectors: &BTreeMap<SlotId, Vec<Vec<f32>>>) -> Vec<f32> {
    trusted_vectors
        .get(&slot)
        .into_iter()
        .flatten()
        .map(|vector| best_cosine(vector, trusted_vectors[&slot].iter()))
        .collect()
}

fn bad_scores(
    slot: SlotId,
    trusted_vectors: &BTreeMap<SlotId, Vec<Vec<f32>>>,
    texts: &[String],
) -> Vec<f32> {
    texts
        .iter()
        .map(|text| best_cosine(&text_vector(text, slot), trusted_vectors[&slot].iter()))
        .collect()
}

fn produced_slots(text: &str) -> ProducedSlots {
    ward_slots()
        .into_iter()
        .map(|slot| (slot, text_vector(text, slot)))
        .collect()
}

fn matched_slots(
    produced: &ProducedSlots,
    trusted_vectors: &BTreeMap<SlotId, Vec<Vec<f32>>>,
) -> Result<MatchedSlots, EngineCalyxWardGateError> {
    produced
        .iter()
        .map(|(slot, vector)| {
            best_vector(vector, trusted_vectors.get(slot).into_iter().flatten())
                .map(|matched| (*slot, matched))
                .ok_or_else(|| EngineCalyxWardGateError::InvalidInput {
                    field: "policy.trusted_texts".to_string(),
                    reason: format!("no trusted vector for Ward slot {}", slot.get()),
                })
        })
        .collect()
}

fn text_vector(text: &str, slot: SlotId) -> Vec<f32> {
    let words = words(text);
    let mut vector = vec![0.0; ENGINE_WARD_DIM];
    match slot.get() {
        1 => words
            .iter()
            .for_each(|word| add_feature(&mut vector, "word", word, 1.0)),
        2 => {
            for pair in words.windows(2) {
                add_feature(&mut vector, "bigram", &pair.join(" "), 1.4);
            }
            for triple in words.windows(3) {
                add_feature(&mut vector, "trigram", &triple.join(" "), 1.8);
            }
        }
        _ => {
            let normalized = format!(" {} ", words.join(" "));
            for gram in normalized.as_bytes().windows(3) {
                add_feature(
                    &mut vector,
                    "char3",
                    std::str::from_utf8(gram).unwrap_or_default(),
                    0.7,
                );
            }
            for gram in normalized.as_bytes().windows(4) {
                add_feature(
                    &mut vector,
                    "char4",
                    std::str::from_utf8(gram).unwrap_or_default(),
                    0.9,
                );
            }
        }
    }
    normalize(vector)
}

/// The subject's tokens **in the order they were written**, duplicates kept.
///
/// This used to be `token_set(text).into_iter().collect()` — a `BTreeSet`, i.e. sorted and
/// deduplicated. That silently broke the two n-gram slots that depend on it (#1548):
///
/// * Slot 2 ("bigram"/"trigram") was computing n-grams over an *alphabetically sorted* token list,
///   so it captured no word order whatsoever — the very thing it exists to capture — and became a
///   pure function of the vocabulary set, making it indistinguishable in kind from slot 1.
/// * Worse, sorting makes the features *unstable under vocabulary change*: adding one token
///   re-sorts the list and rewrites every adjacent pair. A legitimately-worded subject that merely
///   used a different word scored `cos=0.1065` against `tau=0.3790` — not because it was novel,
///   but because sorting had shredded its n-grams.
///
/// Tokenization rules are otherwise identical to [`token_set`], which stays a set: term matching
/// (allowed/blocked) genuinely wants set semantics. Only the n-gram vectors want sequence.
fn words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| term.len() >= 2)
        .collect()
}

fn add_feature(vector: &mut [f32], seed: &str, feature: &str, weight: f32) {
    if feature.is_empty() {
        return;
    }
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.update([0xff]);
    hasher.update(feature.as_bytes());
    let hash = hasher.finalize();
    let index = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]) as usize % vector.len();
    let sign = if hash[4] & 1 == 0 { 1.0 } else { -1.0 };
    vector[index] += sign * weight;
}

fn normalize(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        vector[0] = 1.0;
        return vector;
    }
    vector.iter_mut().for_each(|value| *value /= norm);
    vector
}

fn best_cosine<'a>(vector: &[f32], candidates: impl Iterator<Item = &'a Vec<f32>>) -> f32 {
    candidates
        .filter_map(|candidate| dense_cosine(vector, candidate))
        .max_by(|left, right| left.total_cmp(right))
        .map(unit_cosine)
        .unwrap_or(0.0)
}

fn best_vector<'a>(
    vector: &[f32],
    candidates: impl Iterator<Item = &'a Vec<f32>>,
) -> Option<Vec<f32>> {
    candidates
        .filter_map(|candidate| dense_cosine(vector, candidate).map(|score| (score, candidate)))
        .max_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, candidate)| candidate.clone())
}

fn unit_cosine(value: f32) -> f32 {
    value.clamp(-1.0, 1.0)
}

fn calibration_readback(
    profile: &GuardProfile,
) -> Result<EngineCalyxWardCalibrationReadback, EngineCalyxWardGateError> {
    let calibration =
        profile
            .calibration
            .as_ref()
            .ok_or_else(|| EngineCalyxWardGateError::InvalidInput {
                field: "calyx_ward.calibration".to_string(),
                reason: "Calyx Ward profile must be calibrated for high-stakes use".to_string(),
            })?;
    Ok(EngineCalyxWardCalibrationReadback {
        guard_id: profile.guard_id.to_string(),
        calibrated: true,
        estimator: calibration.estimator.clone(),
        far: calibration.far,
        frr: calibration.frr,
        confidence: calibration.confidence,
        required_slots: profile
            .required_slots
            .iter()
            .map(|slot| slot.get())
            .collect(),
    })
}

fn ward_slots() -> Vec<SlotId> {
    ENGINE_WARD_SLOTS.into_iter().map(SlotId::new).collect()
}

fn guard_id() -> Result<GuardId, EngineCalyxWardGateError> {
    GuardId::from_str(ENGINE_WARD_GUARD_ID).map_err(|error| {
        EngineCalyxWardGateError::InvalidInput {
            field: "calyx_ward.guard_id".to_string(),
            reason: error.to_string(),
        }
    })
}

fn codec_failed(error: impl std::fmt::Display) -> EngineCalyxWardGateError {
    EngineCalyxWardGateError::Codec(error.to_string())
}
