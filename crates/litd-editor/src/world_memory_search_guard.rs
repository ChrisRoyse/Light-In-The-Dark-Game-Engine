use std::collections::BTreeSet;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::{
    WardPolicy, WorldMemoryIndex, WorldMemorySearchHit, validate_world_memory_index,
    world_memory_build::{invalid_world_memory, score_constellation, token_counts},
};

#[path = "world_memory_search_guard_store.rs"]
mod world_memory_search_guard_store;
#[path = "world_memory_search_guard_support.rs"]
mod world_memory_search_guard_support;

pub use world_memory_search_guard_store::{
    GuardedWorldMemorySearchWriteback, parse_guarded_world_memory_search_ledger,
    read_guarded_world_memory_search_ledger, write_guarded_world_memory_search_ledger,
};
use world_memory_search_guard_support::{
    SearchEvidence, matched_terms, matching_blocked_terms, query_fingerprint, score_milli,
    source_refs, validate_calibration, validate_request_identity, validate_ward_policy,
};

pub const GUARDED_WORLD_MEMORY_SEARCH_SCHEMA: &str = "litd.world_memory.guarded_search_ledger.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldMemorySearchAudience {
    Player,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardedWorldMemorySearchStatus {
    Admitted,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardedWorldMemoryQuery {
    pub query_id: String,
    pub audience: WorldMemorySearchAudience,
    pub query: String,
    pub limit: usize,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemorySearchCalibration {
    pub calibration_id: String,
    pub tau_milli: u16,
    pub borderline_margin_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardedWorldMemorySearchVerdict {
    pub schema: String,
    pub query_id: String,
    pub policy_id: String,
    pub calibration_id: Option<String>,
    pub audience: WorldMemorySearchAudience,
    pub status: GuardedWorldMemorySearchStatus,
    pub code: String,
    pub reasons: Vec<String>,
    pub query: String,
    pub query_fingerprint: String,
    pub requested_by: String,
    pub limit: usize,
    pub tau_milli: Option<u16>,
    pub borderline_margin_milli: Option<u16>,
    pub top_score: Option<u32>,
    pub top_score_milli: Option<u16>,
    pub matched_terms: Vec<String>,
    pub blocked_terms: Vec<String>,
    pub source_refs: Vec<String>,
    pub candidate_hit_count: usize,
    pub hit_count: usize,
    pub hits: Vec<WorldMemorySearchHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardedWorldMemorySearchLedger {
    pub schema: String,
    pub ledger_id: String,
    pub verdict_count: usize,
    pub verdicts: Vec<GuardedWorldMemorySearchVerdict>,
}

pub fn guarded_query_world_memory(
    index: &WorldMemoryIndex,
    request: &GuardedWorldMemoryQuery,
    ward: &WardPolicy,
    calibration: Option<&WorldMemorySearchCalibration>,
) -> Result<GuardedWorldMemorySearchVerdict, EngineError> {
    validate_request_identity(request)?;
    validate_ward_policy(ward)?;
    if request.query.trim().is_empty() {
        return Ok(refused(
            request,
            ward,
            None,
            "LITD-SEARCH-EMPTY-QUERY",
            "query must be non-empty",
            SearchEvidence::default(),
        ));
    }
    if request.limit == 0 {
        return Ok(refused(
            request,
            ward,
            None,
            "LITD-SEARCH-INVALID-LIMIT",
            "limit must be greater than zero",
            SearchEvidence::default(),
        ));
    }
    let blocked_terms = matching_blocked_terms(&request.query, &ward.blocked_terms);
    if !blocked_terms.is_empty() {
        return Ok(refused(
            request,
            ward,
            None,
            "LITD-SEARCH-REFUSED-UNSAFE",
            format!("query contained blocked terms: {}", blocked_terms.join("|")),
            SearchEvidence {
                blocked_terms,
                ..SearchEvidence::default()
            },
        ));
    }
    let Some(calibration) = calibration else {
        return Ok(refused(
            request,
            ward,
            None,
            "LITD-SEARCH-CALIBRATION-MISSING",
            "search calibration is required for player/agent-facing queries",
            SearchEvidence::default(),
        ));
    };
    validate_calibration(calibration)?;
    if index.world_count == 0 || index.constellations.is_empty() {
        return Ok(refused(
            request,
            ward,
            Some(calibration),
            "LITD-SEARCH-EMPTY-INDEX",
            "world-memory index is empty",
            SearchEvidence::default(),
        ));
    }
    if let Err(error) = validate_world_memory_index(index) {
        return Ok(refused(
            request,
            ward,
            Some(calibration),
            "LITD-SEARCH-INVALID-INDEX",
            error.to_string(),
            SearchEvidence::default(),
        ));
    }
    let query_terms = token_counts(&request.query);
    if query_terms.is_empty() {
        return Ok(refused(
            request,
            ward,
            Some(calibration),
            "LITD-SEARCH-EMPTY-QUERY",
            "query must contain searchable terms",
            SearchEvidence::default(),
        ));
    }
    let mut hits = index
        .constellations
        .iter()
        .filter_map(|constellation| score_constellation(constellation, &query_terms))
        .collect::<Vec<_>>();
    if hits.is_empty() {
        return Ok(refused(
            request,
            ward,
            Some(calibration),
            "LITD-SEARCH-REFUSED-OOD",
            "query had no grounded world-memory matches",
            SearchEvidence::default(),
        ));
    }
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.constellation_id.cmp(&right.constellation_id))
    });
    let candidate_hit_count = hits.len();
    let top_score = hits[0].score;
    let top_score_milli = score_milli(top_score, query_terms.len());
    let matched_terms = matched_terms(index, &query_terms, &hits);
    let source_refs = source_refs(&hits);
    let evidence = SearchEvidence {
        top_score: Some(top_score),
        top_score_milli: Some(top_score_milli),
        matched_terms,
        source_refs,
        candidate_hit_count,
        ..SearchEvidence::default()
    };
    if top_score_milli < calibration.tau_milli {
        return Ok(refused(
            request,
            ward,
            Some(calibration),
            "LITD-SEARCH-REFUSED-OOD",
            format!(
                "top score {top_score_milli}m is below tau {}m",
                calibration.tau_milli
            ),
            evidence,
        ));
    }
    if top_score_milli
        <= calibration
            .tau_milli
            .saturating_add(calibration.borderline_margin_milli)
    {
        return Ok(refused(
            request,
            ward,
            Some(calibration),
            "LITD-SEARCH-REFUSED-BORDERLINE",
            format!(
                "top score {top_score_milli}m is within borderline margin {}m above tau {}m",
                calibration.borderline_margin_milli, calibration.tau_milli
            ),
            evidence,
        ));
    }
    if evidence.matched_terms.len() < ward.min_grounded_terms
        || evidence.source_refs.len() < ward.min_source_anchors
    {
        return Ok(refused(
            request,
            ward,
            Some(calibration),
            "LITD-SEARCH-INSUFFICIENT-GROUNDING",
            format!(
                "matched {} terms/{} sources, need {} terms/{} sources",
                evidence.matched_terms.len(),
                evidence.source_refs.len(),
                ward.min_grounded_terms,
                ward.min_source_anchors
            ),
            evidence,
        ));
    }
    hits.truncate(request.limit);
    admitted(request, ward, calibration, evidence, hits)
}

pub fn build_guarded_world_memory_search_ledger(
    ledger_id: impl Into<String>,
    verdicts: Vec<GuardedWorldMemorySearchVerdict>,
) -> Result<GuardedWorldMemorySearchLedger, EngineError> {
    let ledger = GuardedWorldMemorySearchLedger {
        schema: GUARDED_WORLD_MEMORY_SEARCH_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        verdict_count: verdicts.len(),
        verdicts,
    };
    validate_guarded_world_memory_search_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_guarded_world_memory_search_ledger(
    ledger: &GuardedWorldMemorySearchLedger,
) -> Result<(), EngineError> {
    if ledger.schema != GUARDED_WORLD_MEMORY_SEARCH_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.verdict_count == 0
        || ledger.verdict_count != ledger.verdicts.len()
    {
        return invalid_world_memory(
            "search_guard.ledger",
            "schema, id, and verdict count must be valid",
        );
    }
    let mut seen = BTreeSet::new();
    for verdict in &ledger.verdicts {
        validate_guarded_world_memory_search_verdict(verdict)?;
        if !seen.insert(verdict.query_id.clone()) {
            return invalid_world_memory(
                "search_guard.ledger.duplicate",
                "duplicate guarded query verdict",
            );
        }
    }
    Ok(())
}

fn validate_guarded_world_memory_search_verdict(
    verdict: &GuardedWorldMemorySearchVerdict,
) -> Result<(), EngineError> {
    if verdict.schema != GUARDED_WORLD_MEMORY_SEARCH_SCHEMA
        || verdict.query_id.trim().is_empty()
        || verdict.policy_id.trim().is_empty()
        || verdict.code.trim().is_empty()
        || verdict.reasons.is_empty()
        || (verdict.query.trim().is_empty() && verdict.code != "LITD-SEARCH-EMPTY-QUERY")
        || verdict.query_fingerprint.trim().is_empty()
        || verdict.requested_by.trim().is_empty()
    {
        return invalid_world_memory(
            "search_guard.verdict",
            "required guarded-search verdict fields are missing",
        );
    }
    match verdict.status {
        GuardedWorldMemorySearchStatus::Admitted => {
            if verdict.code != "LITD-SEARCH-ADMITTED"
                || verdict.hits.is_empty()
                || verdict.hit_count != verdict.hits.len()
                || verdict.top_score.is_none()
                || verdict.top_score_milli.is_none()
                || verdict.tau_milli.is_none()
            {
                return invalid_world_memory(
                    "search_guard.verdict.admitted",
                    "admitted verdict must carry hits and score evidence",
                );
            }
        }
        GuardedWorldMemorySearchStatus::Refused => {
            if !verdict.hits.is_empty() || verdict.hit_count != 0 {
                return invalid_world_memory(
                    "search_guard.verdict.refused",
                    "refused verdicts must not return hits",
                );
            }
            if verdict.code == "LITD-SEARCH-REFUSED-UNSAFE" && verdict.blocked_terms.is_empty() {
                return invalid_world_memory(
                    "search_guard.verdict.unsafe",
                    "unsafe refusal must list blocked terms",
                );
            }
            if verdict.code == "LITD-SEARCH-CALIBRATION-MISSING"
                && (verdict.calibration_id.is_some() || verdict.tau_milli.is_some())
            {
                return invalid_world_memory(
                    "search_guard.verdict.calibration",
                    "missing-calibration refusal must not carry calibration values",
                );
            }
        }
    }
    Ok(())
}

fn admitted(
    request: &GuardedWorldMemoryQuery,
    ward: &WardPolicy,
    calibration: &WorldMemorySearchCalibration,
    evidence: SearchEvidence,
    hits: Vec<WorldMemorySearchHit>,
) -> Result<GuardedWorldMemorySearchVerdict, EngineError> {
    let verdict = GuardedWorldMemorySearchVerdict {
        schema: GUARDED_WORLD_MEMORY_SEARCH_SCHEMA.to_string(),
        query_id: request.query_id.clone(),
        policy_id: ward.policy_id.clone(),
        calibration_id: Some(calibration.calibration_id.clone()),
        audience: request.audience,
        status: GuardedWorldMemorySearchStatus::Admitted,
        code: "LITD-SEARCH-ADMITTED".to_string(),
        reasons: vec!["query passed Ward and calibrated relevance threshold".to_string()],
        query: request.query.clone(),
        query_fingerprint: query_fingerprint(request),
        requested_by: request.requested_by.clone(),
        limit: request.limit,
        tau_milli: Some(calibration.tau_milli),
        borderline_margin_milli: Some(calibration.borderline_margin_milli),
        top_score: evidence.top_score,
        top_score_milli: evidence.top_score_milli,
        matched_terms: evidence.matched_terms,
        blocked_terms: evidence.blocked_terms,
        source_refs: evidence.source_refs,
        candidate_hit_count: evidence.candidate_hit_count,
        hit_count: hits.len(),
        hits,
    };
    validate_guarded_world_memory_search_verdict(&verdict)?;
    Ok(verdict)
}

fn refused(
    request: &GuardedWorldMemoryQuery,
    ward: &WardPolicy,
    calibration: Option<&WorldMemorySearchCalibration>,
    code: &str,
    reason: impl Into<String>,
    evidence: SearchEvidence,
) -> GuardedWorldMemorySearchVerdict {
    GuardedWorldMemorySearchVerdict {
        schema: GUARDED_WORLD_MEMORY_SEARCH_SCHEMA.to_string(),
        query_id: request.query_id.clone(),
        policy_id: ward.policy_id.clone(),
        calibration_id: calibration.map(|item| item.calibration_id.clone()),
        audience: request.audience,
        status: GuardedWorldMemorySearchStatus::Refused,
        code: code.to_string(),
        reasons: vec![reason.into()],
        query: request.query.clone(),
        query_fingerprint: query_fingerprint(request),
        requested_by: request.requested_by.clone(),
        limit: request.limit,
        tau_milli: calibration.map(|item| item.tau_milli),
        borderline_margin_milli: calibration.map(|item| item.borderline_margin_milli),
        top_score: evidence.top_score,
        top_score_milli: evidence.top_score_milli,
        matched_terms: evidence.matched_terms,
        blocked_terms: evidence.blocked_terms,
        source_refs: evidence.source_refs,
        candidate_hit_count: evidence.candidate_hit_count,
        hit_count: 0,
        hits: Vec::new(),
    }
}
