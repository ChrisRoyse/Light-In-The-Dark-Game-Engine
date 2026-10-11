//! Authored Ward response corpora, sourced from ingested world content (#1552).
//!
//! # Why this exists
//!
//! The Ward policy that gates a model *response* (#1548) needs a `trusted_texts` corpus of exemplar
//! response narrations in the exact shape the subject is scored in — `"<verb_tag> <rationale>"` —
//! and a `calibration_ood_texts` set that is genuinely out-of-distribution. #1548 landed those as
//! hand-written lists inside the FSV example. That does not scale: every world and agent archetype
//! needs its own, and a hand-written list is neither provenance-backed nor readable-back as a
//! source of truth.
//!
//! This module builds the **authoring path**. A [`WardResponseCorpusSource`] is derived from a real
//! ingested content record (the world's grounded lexicon + the archetype's closed verb set); the
//! corpus is derived deterministically from it, guarded against degenerate calibration (#1552 pt.2),
//! persisted with provenance and a self-hash, and read back with the hash re-verified. The Ward
//! policy is then built from the *readback*, not from example code.
//!
//! # The single source of truth
//!
//! The persisted [`EngineCalyxRecordKind::WardResponseCorpus`] row is the source of truth. Its
//! `corpus_sha256` is computed over the derived texts, stored, and re-checked on read — a row
//! tampered with on disk cannot be loaded into a policy.

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use calyx_ward::{SeparationReport, SeparationThresholds, WardError, assess_separation};

use crate::{
    agent_response::EngineAgentVerbTag,
    agent_response_corpus_support::{
        derive_calibration_ood, derive_degenerate_ood, derive_trusted_narrations,
        grounded_terms_from_payload,
    },
    calyx_content::EngineContentRecord,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxPersistenceError, EngineCalyxRecord, EngineCalyxRecordKind,
    },
    calyx_ward_gate::EngineCalyxWardPolicy,
    calyx_ward_gate_support::slot_separation_scores,
};

pub const WARD_RESPONSE_CORPUS_SCHEMA: &str = "litd.engine.ward_response_corpus.v1";
pub const WARD_RESPONSE_CORPUS_READBACK_SCHEMA: &str =
    "litd.engine.ward_response_corpus_readback.v1";

/// How many calibration OOD texts to derive. Matches Ward's minimum bad-score count so the
/// calibrated profile is never provisional.
pub const WARD_RESPONSE_CORPUS_OOD_COUNT: usize = calyx_ward::MIN_BAD_SCORES;

/// A world/agent-archetype's grounded lexicon and closed verb set, sourced from ingested content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardResponseCorpusSource {
    pub world_id: String,
    pub archetype_id: String,
    /// The world's grounded vocabulary, read out of an ingested content record.
    pub grounded_terms: Vec<String>,
    /// The verbs this archetype may propose (a subset of the closed [`EngineAgentVerbTag`] enum).
    pub verb_tags: Vec<EngineAgentVerbTag>,
    /// Grounding refs to the source content (provenance).
    pub provenance_refs: Vec<String>,
}

/// A derived, hashed, provenance-backed Ward response corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WardResponseCorpus {
    pub schema: String,
    pub corpus_id: String,
    pub world_id: String,
    pub archetype_id: String,
    pub grounded_terms: Vec<String>,
    pub verb_tags: Vec<EngineAgentVerbTag>,
    pub provenance_refs: Vec<String>,
    pub trusted_texts: Vec<String>,
    pub calibration_ood_texts: Vec<String>,
    pub corpus_sha256: String,
}

/// One slot's separability, echoing [`calyx_ward::SeparationReport`] into a stored readback.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WardResponseCorpusSlotSeparation {
    pub slot: u16,
    pub report: SeparationReport,
}

/// Proof that the corpus is non-degenerate: per-slot separation of trusted vs OOD scores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardResponseCorpusSeparationReadback {
    pub schema: String,
    pub corpus_id: String,
    pub slots: Vec<WardResponseCorpusSlotSeparation>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WardResponseCorpusError {
    #[error("Ward response corpus field {field} is invalid: {reason}")]
    Invalid { field: String, reason: String },
    #[error("Ward response corpus calibration is degenerate at slot {slot}: {source}")]
    DegenerateCalibration { slot: u16, source: WardError },
    #[error("Ward response corpus persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("Ward response corpus codec failed: {0}")]
    Codec(String),
}

/// Derive a corpus source from an ingested content record (the "sourced from content" step).
///
/// The grounded lexicon is read out of the record payload + display name. The verbs are the
/// archetype's closed set. Provenance points at the record.
pub fn corpus_source_from_content(
    record: &EngineContentRecord,
    world_id: impl Into<String>,
    archetype_id: impl Into<String>,
    verb_tags: Vec<EngineAgentVerbTag>,
) -> Result<WardResponseCorpusSource, WardResponseCorpusError> {
    let grounded_terms = grounded_terms_from_payload(&record.display_name, &record.payload);
    let source = WardResponseCorpusSource {
        world_id: world_id.into(),
        archetype_id: archetype_id.into(),
        grounded_terms,
        verb_tags,
        provenance_refs: vec![
            format!("content://{}/{}", record.kind.as_str(), record.id),
            format!("source://{}", record.source.path),
        ],
    };
    validate_source(&source)?;
    Ok(source)
}

/// Derive, hash, and separation-guard a corpus. Fails loud on a degenerate calibration (#1552).
pub fn build_corpus(
    source: &WardResponseCorpusSource,
) -> Result<WardResponseCorpus, WardResponseCorpusError> {
    validate_source(source)?;
    let trusted_texts = derive_trusted_narrations(&source.grounded_terms, &source.verb_tags);
    let calibration_ood_texts = derive_calibration_ood(WARD_RESPONSE_CORPUS_OOD_COUNT);
    build_corpus_with_texts(source, trusted_texts, calibration_ood_texts)
}

/// Build a corpus from explicit texts (the FSV degenerate case supplies its own OOD set). Still
/// runs the separation guard, so a degenerate corpus can never be constructed.
pub fn build_corpus_with_texts(
    source: &WardResponseCorpusSource,
    trusted_texts: Vec<String>,
    calibration_ood_texts: Vec<String>,
) -> Result<WardResponseCorpus, WardResponseCorpusError> {
    validate_source(source)?;
    if trusted_texts.len() < 2 {
        return Err(invalid(
            "trusted_texts",
            "a corpus needs at least 2 trusted narrations to show ID variety",
        ));
    }
    if calibration_ood_texts.len() < calyx_ward::MIN_BAD_SCORES {
        return Err(invalid(
            "calibration_ood_texts",
            "not enough OOD texts to calibrate without a provisional profile",
        ));
    }
    let corpus_id = format!(
        "ward-response-corpus/{}/{}",
        source.world_id, source.archetype_id
    );
    let corpus_sha256 = corpus_hash(&corpus_id, &trusted_texts, &calibration_ood_texts);
    let corpus = WardResponseCorpus {
        schema: WARD_RESPONSE_CORPUS_SCHEMA.to_string(),
        corpus_id,
        world_id: source.world_id.clone(),
        archetype_id: source.archetype_id.clone(),
        grounded_terms: source.grounded_terms.clone(),
        verb_tags: source.verb_tags.clone(),
        provenance_refs: source.provenance_refs.clone(),
        trusted_texts,
        calibration_ood_texts,
        corpus_sha256,
    };
    // GUARD: a degenerate calibration must fail here, before the corpus can be persisted or used.
    assess_corpus_separation(&corpus)?;
    Ok(corpus)
}

/// Assess per-slot separability of the corpus's trusted vs OOD scores. Fails loud (naming the
/// overlap statistic) on a degenerate calibration; returns the readback when healthy.
pub fn assess_corpus_separation(
    corpus: &WardResponseCorpus,
) -> Result<WardResponseCorpusSeparationReadback, WardResponseCorpusError> {
    let thresholds = SeparationThresholds::default();
    let mut slots = Vec::new();
    for (slot, good_loo, bad) in
        slot_separation_scores(&corpus.trusted_texts, &corpus.calibration_ood_texts)
    {
        let report = assess_separation(&good_loo, &bad, thresholds)
            .map_err(|source| WardResponseCorpusError::DegenerateCalibration { slot, source })?;
        slots.push(WardResponseCorpusSlotSeparation { slot, report });
    }
    Ok(WardResponseCorpusSeparationReadback {
        schema: WARD_RESPONSE_CORPUS_READBACK_SCHEMA.to_string(),
        corpus_id: corpus.corpus_id.clone(),
        slots,
    })
}

/// Persist the corpus (guard re-run, provenance recorded, store readback verified by `put_record`).
pub fn persist_corpus(
    database: &EngineCalyxDatabase,
    corpus: &WardResponseCorpus,
) -> Result<WardResponseCorpus, WardResponseCorpusError> {
    // Never persist a corpus that has not passed the separation guard.
    assess_corpus_separation(corpus)?;
    if corpus.corpus_sha256
        != corpus_hash(
            &corpus.corpus_id,
            &corpus.trusted_texts,
            &corpus.calibration_ood_texts,
        )
    {
        return Err(invalid(
            "corpus_sha256",
            "corpus hash does not match its texts",
        ));
    }
    let record = EngineCalyxRecord::new(
        EngineCalyxRecordKind::WardResponseCorpus,
        corpus.corpus_id.clone(),
        serde_json::to_value(corpus).map_err(codec_failed)?,
        json!({
            "source_issue": 1552,
            "world_id": corpus.world_id,
            "archetype_id": corpus.archetype_id,
            "corpus_sha256": corpus.corpus_sha256,
            "trusted_count": corpus.trusted_texts.len(),
            "ood_count": corpus.calibration_ood_texts.len(),
            "provenance_refs": corpus.provenance_refs,
        }),
    );
    database.put_record(&record)?;
    read_corpus(database, &corpus.corpus_id)
}

/// Read a corpus back from the store, re-verifying its self-hash.
pub fn read_corpus(
    database: &EngineCalyxDatabase,
    corpus_id: &str,
) -> Result<WardResponseCorpus, WardResponseCorpusError> {
    let stored = database.get_record(EngineCalyxRecordKind::WardResponseCorpus, corpus_id)?;
    let corpus =
        serde_json::from_value::<WardResponseCorpus>(stored.payload).map_err(codec_failed)?;
    if corpus.schema != WARD_RESPONSE_CORPUS_SCHEMA {
        return Err(invalid(
            "schema",
            format!("expected {WARD_RESPONSE_CORPUS_SCHEMA}"),
        ));
    }
    let recomputed = corpus_hash(
        &corpus.corpus_id,
        &corpus.trusted_texts,
        &corpus.calibration_ood_texts,
    );
    if corpus.corpus_sha256 != recomputed {
        return Err(invalid(
            "corpus_sha256",
            format!(
                "stored corpus hash {} does not match recomputed {recomputed} — tampered row",
                corpus.corpus_sha256
            ),
        ));
    }
    Ok(corpus)
}

/// Build the Ward RESPONSE policy from a (read-back) corpus. Envelope numbers are the deployment's,
/// not the model's; the corpus supplies only the grounded terms and the trusted/OOD text.
pub fn policy_from_corpus(
    corpus: &WardResponseCorpus,
    blocked_terms: Vec<String>,
    min_grounding_refs: usize,
    min_matched_terms: usize,
    max_move_units_per_tick: u32,
    max_scale_milli: u32,
    max_emissive_milli: u32,
) -> EngineCalyxWardPolicy {
    EngineCalyxWardPolicy {
        policy_id: corpus.corpus_id.clone(),
        allowed_terms: corpus.grounded_terms.clone(),
        blocked_terms,
        trusted_texts: corpus.trusted_texts.clone(),
        calibration_ood_texts: corpus.calibration_ood_texts.clone(),
        min_grounding_refs,
        min_matched_terms,
        max_move_units_per_tick,
        allow_teleport: false,
        max_scale_milli,
        max_emissive_milli,
    }
}

/// Generate a DEGENERATE calibration OOD set (near-duplicates of the trusted narrations) for FSVs
/// and adversarial tests that must prove the separation guard fails loud. This is never used to
/// author a real corpus — `build_corpus`/`persist_corpus` will reject the result.
pub fn degenerate_calibration_ood(trusted_texts: &[String]) -> Vec<String> {
    derive_degenerate_ood(trusted_texts, WARD_RESPONSE_CORPUS_OOD_COUNT)
}

fn validate_source(source: &WardResponseCorpusSource) -> Result<(), WardResponseCorpusError> {
    if source.world_id.trim().is_empty() || source.archetype_id.trim().is_empty() {
        return Err(invalid("id", "world_id and archetype_id must be non-empty"));
    }
    if source.grounded_terms.len() < 3 {
        return Err(invalid(
            "grounded_terms",
            "need at least 3 grounded terms to derive varied narrations",
        ));
    }
    if source.verb_tags.is_empty() {
        return Err(invalid(
            "verb_tags",
            "archetype must allow at least one verb",
        ));
    }
    if source.provenance_refs.is_empty()
        || source
            .provenance_refs
            .iter()
            .any(|item| item.trim().is_empty())
    {
        return Err(invalid(
            "provenance_refs",
            "corpus must carry non-empty provenance",
        ));
    }
    Ok(())
}

fn corpus_hash(corpus_id: &str, trusted: &[String], ood: &[String]) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(corpus_id.as_bytes());
    bytes.push(0x00);
    for text in trusted {
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0x01);
    }
    bytes.push(0xff);
    for text in ood {
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0x01);
    }
    sha256_hex(&bytes)
}

fn invalid(field: impl Into<String>, reason: impl Into<String>) -> WardResponseCorpusError {
    WardResponseCorpusError::Invalid {
        field: field.into(),
        reason: reason.into(),
    }
}

fn codec_failed(error: impl std::fmt::Display) -> WardResponseCorpusError {
    WardResponseCorpusError::Codec(error.to_string())
}
