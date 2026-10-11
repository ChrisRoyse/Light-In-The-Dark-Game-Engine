use litd_engine::EngineError;

use super::{
    SemanticAuthoringHit, SemanticAuthoringOperation, SemanticAuthoringReport,
    SemanticAuthoringStatus,
};

use super::semantic_authoring_ops_engine_support::invalid_semantic_authoring;

pub(in crate::semantic_authoring_ops) fn validate_report(
    report: &SemanticAuthoringReport,
) -> Result<(), EngineError> {
    if report.schema != super::SEMANTIC_AUTHORING_OPS_SCHEMA
        || report.query_id.trim().is_empty()
        || report.source_of_truth.trim().is_empty()
        || report.requested_by.trim().is_empty()
        || report.k == 0
        || report.code.trim().is_empty()
        || report.subject.constellation_id.trim().is_empty()
        || report.subject.calyx_cx_id.trim().is_empty()
    {
        return invalid_semantic_authoring("report", "report identity fields must be valid");
    }
    match report.status {
        SemanticAuthoringStatus::Returned => {
            if report.operation == SemanticAuthoringOperation::Define {
                validate_definition(report)?;
            } else if report.hits.is_empty() {
                return invalid_semantic_authoring(
                    "report.hits",
                    "returned non-define reports require hits",
                );
            }
        }
        SemanticAuthoringStatus::NoNeighbors => {
            if !report.hits.is_empty() || report.definition.is_some() || report.reasons.is_empty() {
                return invalid_semantic_authoring(
                    "report.no_neighbors",
                    "no-neighbor reports must have no hits/definition and at least one reason",
                );
            }
        }
    }
    for hit in &report.hits {
        validate_hit(hit)?;
    }
    Ok(())
}

fn validate_definition(report: &SemanticAuthoringReport) -> Result<(), EngineError> {
    let Some(definition) = report.definition.as_ref() else {
        return invalid_semantic_authoring(
            "report.definition",
            "define reports require definition evidence",
        );
    };
    if definition.subject_constellation_id != report.subject.constellation_id
        || definition.calyx_cx_id != report.subject.calyx_cx_id
        || definition.slots.is_empty()
    {
        return invalid_semantic_authoring(
            "report.definition",
            "definition subject and slots must match report",
        );
    }
    for slot in &definition.slots {
        if slot.dimension == 0
            || slot.dimension != slot.centroid.len()
            || slot.centroid.iter().any(|value| !value.is_finite())
        {
            return invalid_semantic_authoring(
                "report.definition.slot",
                "definition centroid slots must be finite and dimensionally valid",
            );
        }
    }
    Ok(())
}

fn validate_hit(hit: &SemanticAuthoringHit) -> Result<(), EngineError> {
    if hit.rank == 0
        || hit.constellation_id.trim().is_empty()
        || hit.calyx_cx_id.trim().is_empty()
        || !hit.score.is_finite()
        || hit.provenance_refs.is_empty()
    {
        return invalid_semantic_authoring("report.hit", "hit identity and score must be valid");
    }
    for value in [hit.mean_cosine, hit.min_cosine, hit.max_cosine, hit.spread]
        .into_iter()
        .flatten()
    {
        if !value.is_finite() {
            return invalid_semantic_authoring("report.hit.cosine", "cosines must be finite");
        }
    }
    for slot in &hit.per_slot {
        if !slot.cosine.is_finite() {
            return invalid_semantic_authoring("report.hit.slot", "slot cosine must be finite");
        }
    }
    Ok(())
}
