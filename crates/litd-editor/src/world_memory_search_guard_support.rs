use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, render::shader::shader_fingerprint};

use super::{GuardedWorldMemoryQuery, WorldMemorySearchCalibration};
use crate::authoring::WardPolicy;

use super::super::{
    WorldMemoryIndex, WorldMemorySearchHit, world_memory_build::invalid_world_memory,
};

#[derive(Debug, Clone, Default)]
pub(super) struct SearchEvidence {
    pub top_score: Option<u32>,
    pub top_score_milli: Option<u16>,
    pub matched_terms: Vec<String>,
    pub blocked_terms: Vec<String>,
    pub source_refs: Vec<String>,
    pub candidate_hit_count: usize,
}

pub(super) fn validate_request_identity(
    request: &GuardedWorldMemoryQuery,
) -> Result<(), EngineError> {
    if request.query_id.trim().is_empty()
        || request.query_id.contains(['\n', '\r', '\t'])
        || request.requested_by.trim().is_empty()
        || request.requested_by.contains(['\n', '\r', '\t'])
    {
        return invalid_world_memory(
            "search_guard.request",
            "query_id and requested_by must be non-empty and single-line",
        );
    }
    Ok(())
}

pub(super) fn validate_ward_policy(ward: &WardPolicy) -> Result<(), EngineError> {
    if ward.policy_id.trim().is_empty()
        || ward.min_source_anchors == 0
        || ward.min_grounded_terms == 0
    {
        return invalid_world_memory(
            "search_guard.ward",
            "Ward policy id and thresholds must be valid",
        );
    }
    for term in &ward.blocked_terms {
        if term.trim().is_empty() || term.contains(['\n', '\r', '\t']) {
            return invalid_world_memory(
                "search_guard.ward.blocked_terms",
                "blocked terms must be non-empty and single-line",
            );
        }
    }
    Ok(())
}

pub(super) fn validate_calibration(
    calibration: &WorldMemorySearchCalibration,
) -> Result<(), EngineError> {
    if calibration.calibration_id.trim().is_empty()
        || calibration.calibration_id.contains(['\n', '\r', '\t'])
        || calibration.tau_milli == 0
        || calibration.tau_milli >= 1000
    {
        return invalid_world_memory(
            "search_guard.calibration",
            "calibration id and tau must be valid",
        );
    }
    if calibration
        .tau_milli
        .saturating_add(calibration.borderline_margin_milli)
        >= 1000
    {
        return invalid_world_memory(
            "search_guard.calibration.margin",
            "tau plus borderline margin must stay below 1000",
        );
    }
    Ok(())
}

pub(super) fn score_milli(score: u32, unique_query_terms: usize) -> u16 {
    let max_score = unique_query_terms.saturating_mul(60).max(1) as u32;
    (((score.min(max_score) * 1000) / max_score) as u16).min(1000)
}

pub(super) fn matched_terms(
    index: &WorldMemoryIndex,
    query_terms: &BTreeMap<String, u32>,
    hits: &[WorldMemorySearchHit],
) -> Vec<String> {
    let hit_ids = hits
        .iter()
        .map(|hit| hit.constellation_id.as_str())
        .collect::<BTreeSet<_>>();
    query_terms
        .keys()
        .filter(|term| {
            index
                .constellations
                .iter()
                .filter(|constellation| hit_ids.contains(constellation.constellation_id.as_str()))
                .flat_map(|constellation| constellation.slots.iter())
                .any(|slot| slot.terms.contains_key(*term))
        })
        .cloned()
        .collect()
}

pub(super) fn source_refs(hits: &[WorldMemorySearchHit]) -> Vec<String> {
    hits.iter()
        .flat_map(|hit| hit.provenance_refs.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn matching_blocked_terms(query: &str, blocked_terms: &[String]) -> Vec<String> {
    let haystack = query.to_ascii_lowercase();
    blocked_terms
        .iter()
        .filter(|term| contains_term(&haystack, term))
        .map(|term| term.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn query_fingerprint(request: &GuardedWorldMemoryQuery) -> String {
    shader_fingerprint(&format!(
        "{}|{:?}|{}|{}|{}",
        request.query_id, request.audience, request.query, request.limit, request.requested_by
    ))
}

fn contains_term(haystack: &str, term: &str) -> bool {
    let needle = term.trim().to_ascii_lowercase();
    !needle.is_empty() && haystack.contains(&needle)
}
