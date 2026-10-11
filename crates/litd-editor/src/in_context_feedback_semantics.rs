use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use sha2::{Digest, Sha256};

use crate::world_memory::{
    WorldMemoryConstellation, WorldMemoryFeatureVector, WorldMemoryIndex, WorldMemoryLens,
    WorldMemoryLensKind, WorldMemorySlotKind, WorldMemorySourceKind, typed_slot,
    validate_typed_slots, validate_world_memory_index,
};

use super::*;

const TOKEN_VECTOR_DIMENSION: usize = 16;

pub(super) fn feedback_constellation(
    project_id: &str,
    request: &ContextFeedbackCaptureRequest,
    annotation: &ContextFeedbackAnnotation,
    screenshot: &ContextFeedbackScreenshotEvidence,
    world_state: &ContextFeedbackWorldState,
    source_fingerprint: String,
) -> Result<WorldMemoryConstellation, EngineError> {
    let constellation_id = format!("{project_id}::feedback::{}", request.feedback_id);
    let source_id = request.feedback_id.clone();
    let text = format!(
        "{} {} {} {}",
        request.note, request.severity, request.marker.label, request.marker.target_object_id
    );
    let structured = format!(
        "feedback {} actor {} target {} kind {} screenshot_sha {} screenshot_bytes {} annotation {} x {} y {} z {}",
        request.feedback_id,
        request.actor_id,
        request.marker.target_object_id,
        world_state.target_object_kind,
        screenshot.sha256,
        screenshot.size_bytes,
        annotation.annotation_id,
        request.marker.world_position.x,
        request.marker.world_position.y,
        request.marker.world_position.z
    );
    let text_lens = lens(text)?;
    let structured_lens = lens(structured)?;
    let slots = vec![
        typed_slot(
            WorldMemorySlotKind::Text,
            &source_id,
            Some(&text_lens),
            Vec::new(),
            [("source_lens", "Text".to_string())],
        ),
        typed_slot(
            WorldMemorySlotKind::Structured,
            &source_id,
            Some(&structured_lens),
            Vec::new(),
            [("source_lens", "Structured".to_string())],
        ),
        typed_slot(
            WorldMemorySlotKind::VectorSet,
            &source_id,
            None,
            vec![
                token_vector(&source_id, WorldMemoryLensKind::Text, &text_lens.terms),
                token_vector(
                    &source_id,
                    WorldMemoryLensKind::Structured,
                    &structured_lens.terms,
                ),
            ],
            [("vector_source", "context_feedback_token_terms".to_string())],
        ),
    ];
    validate_typed_slots(WorldMemorySourceKind::Feedback, &slots)?;
    Ok(WorldMemoryConstellation {
        constellation_id,
        world_id: project_id.to_string(),
        source_kind: WorldMemorySourceKind::Feedback,
        source_id,
        title: format!("Feedback {}", request.feedback_id),
        source_fingerprint,
        provenance_refs: vec![
            annotation.annotation_id.clone(),
            format!("screenshot://sha256/{}", screenshot.sha256),
        ],
        lenses: BTreeMap::from([
            (WorldMemoryLensKind::Text, text_lens),
            (WorldMemoryLensKind::Structured, structured_lens),
        ]),
        slots,
        reuse_prompt: None,
    })
}

pub(super) fn feedback_index(
    project_id: &str,
    constellations: Vec<WorldMemoryConstellation>,
) -> Result<WorldMemoryIndex, EngineError> {
    let index = WorldMemoryIndex {
        schema: "litd.authoring.world_memory.v2.no_flatten_slots".to_string(),
        source_of_truth: "ContextFeedbackLedger.feedback_records persisted in Aster Graph CF"
            .to_string(),
        world_count: 1,
        constellation_count: constellations.len(),
        constellations,
        conflicts: Vec::new(),
    };
    if index
        .constellations
        .iter()
        .any(|constellation| constellation.world_id != project_id)
    {
        return invalid_context_feedback(
            "index.project",
            "feedback constellation project mismatch",
        );
    }
    validate_world_memory_index(&index)?;
    Ok(index)
}

pub(super) fn loom_association(
    request: &ContextFeedbackCaptureRequest,
    existing: &[&ContextFeedbackRecord],
) -> Result<ContextFeedbackLoomAssociation, EngineError> {
    let query_terms = token_set(&request.note);
    let mut duplicates = Vec::new();
    let mut highest = 0.0_f32;
    for record in existing {
        if record.annotation.target.selector.marker_object_id != request.marker.target_object_id {
            continue;
        }
        let candidate = token_set(&record.annotation.body.value);
        let overlap = overlap_ratio(&query_terms, &candidate);
        highest = highest.max(overlap);
        if overlap >= 0.6 {
            duplicates.push(record.feedback_id.clone());
        }
    }
    Ok(ContextFeedbackLoomAssociation {
        association_id: format!("loom://issue1184/{}", request.feedback_id),
        source_feedback_id: request.feedback_id.clone(),
        duplicate_feedback_ids: duplicates,
        highest_overlap: highest,
        matched_target_object_id: request.marker.target_object_id.clone(),
        method: "calyx_loom.normalized_note_marker_overlap.v1".to_string(),
    })
}

fn lens(text: String) -> Result<WorldMemoryLens, EngineError> {
    let terms = token_counts(&text);
    if terms.is_empty() {
        return invalid_context_feedback("lens.terms", "feedback lens has no searchable terms");
    }
    Ok(WorldMemoryLens { text, terms })
}

fn token_counts(text: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for token in text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
    {
        *counts.entry(token).or_insert(0) += 1;
    }
    counts
}

fn token_set(text: &str) -> BTreeSet<String> {
    token_counts(text).into_keys().collect()
}

fn overlap_ratio(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let overlap = left.intersection(right).count() as f32;
    overlap / left.len().max(right.len()) as f32
}

fn token_vector(
    source_id: &str,
    source_lens: WorldMemoryLensKind,
    terms: &BTreeMap<String, u32>,
) -> WorldMemoryFeatureVector {
    let mut values = vec![0_i32; TOKEN_VECTOR_DIMENSION];
    for (term, count) in terms {
        values[stable_bucket(term, TOKEN_VECTOR_DIMENSION)] +=
            i32::try_from(*count).unwrap_or(i32::MAX);
    }
    WorldMemoryFeatureVector {
        vector_id: format!("{source_id}::{source_lens:?}::context_feedback_terms"),
        source_lens,
        dimension: TOKEN_VECTOR_DIMENSION,
        values,
    }
}

fn stable_bucket(value: &str, dimension: usize) -> usize {
    let digest = Sha256::digest(value.as_bytes());
    let mut first = [0_u8; 8];
    first.copy_from_slice(&digest[..8]);
    (u64::from_be_bytes(first) as usize) % dimension
}
