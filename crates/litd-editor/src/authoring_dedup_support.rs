use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use crate::history::EditorObject;

use super::{CatalogDedupRecord, CatalogEntityKind, DedupDecisionKind};

pub(super) fn anchor_conflicts(
    left: &CatalogDedupRecord,
    right: &CatalogDedupRecord,
) -> Vec<String> {
    let mut conflicts = Vec::new();
    if left.world_id != right.world_id {
        conflicts.push(format!("world_id:{}!={}", left.world_id, right.world_id));
    }
    if disjoint_non_empty(&left.anchor_ids, &right.anchor_ids) {
        conflicts.push("anchor_ids_disjoint".to_string());
    }
    if disjoint_non_empty(&left.source_refs, &right.source_refs) {
        conflicts.push("source_refs_disjoint".to_string());
    }
    conflicts
}

pub(super) fn merge_suggestion(
    decision: DedupDecisionKind,
    left: &CatalogDedupRecord,
    right: &CatalogDedupRecord,
) -> String {
    match decision {
        DedupDecisionKind::ExactDuplicate => {
            format!(
                "collapse exact duplicate; keep {} as survivor",
                left.record_id
            )
        }
        DedupDecisionKind::NearDuplicate => format!(
            "merge candidate; compare provenance and materialize one canonical {} record",
            kind_name(left.kind)
        ),
        DedupDecisionKind::AnchorConflict => format!(
            "do not auto-merge {}; review anchor/world conflict between {} and {}",
            kind_name(left.kind),
            left.record_id,
            right.record_id
        ),
    }
}

pub(super) fn token_counts(text: &str) -> BTreeMap<String, u32> {
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

pub(super) fn cosine_milli(left: &BTreeMap<String, u32>, right: &BTreeMap<String, u32>) -> u16 {
    let dot = left
        .iter()
        .filter_map(|(term, left_count)| {
            right.get(term).map(|right_count| left_count * right_count)
        })
        .sum::<u32>() as f64;
    let left_norm = left.values().map(|count| count * count).sum::<u32>() as f64;
    let right_norm = right.values().map(|count| count * count).sum::<u32>() as f64;
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0;
    }
    ((dot / (left_norm.sqrt() * right_norm.sqrt())) * 1000.0).round() as u16
}

pub(super) fn optional_property_values(object: &EditorObject, key: &str) -> Vec<String> {
    object
        .properties
        .get(key)
        .map(|value| vec![value.clone()])
        .unwrap_or_default()
}

pub(super) fn normalize_values(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn decision_rank(decision: DedupDecisionKind) -> u8 {
    match decision {
        DedupDecisionKind::ExactDuplicate => 0,
        DedupDecisionKind::NearDuplicate => 1,
        DedupDecisionKind::AnchorConflict => 2,
    }
}

pub(super) fn invalid_dedup<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.authoring_dedup.{}", field.into()),
        reason: reason.into(),
    })
}

fn disjoint_non_empty(left: &[String], right: &[String]) -> bool {
    !left.is_empty()
        && !right.is_empty()
        && left
            .iter()
            .collect::<BTreeSet<_>>()
            .is_disjoint(&right.iter().collect())
}

fn kind_name(kind: CatalogEntityKind) -> &'static str {
    match kind {
        CatalogEntityKind::Ability => "ability",
        CatalogEntityKind::Unit => "unit",
        CatalogEntityKind::Item => "item",
        CatalogEntityKind::World => "world",
    }
}
