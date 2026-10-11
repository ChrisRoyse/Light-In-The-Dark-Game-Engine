use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use crate::history::EditorObject;

use super::{
    WorldMemoryConflict, WorldMemoryConstellation, WorldMemoryLens, WorldMemoryLensKind,
    WorldMemorySearchHit, WorldMemorySourceKind, world_memory_slots::typed_slots_from_lenses,
};

pub(crate) struct MakeConstellationArgs<T0: IntoIterator<Item = (WorldMemoryLensKind, String)>> {
    pub(crate) constellation_id: String,
    pub(crate) world_id: String,
    pub(crate) source_kind: WorldMemorySourceKind,
    pub(crate) source_id: String,
    pub(crate) title: String,
    pub(crate) source_fingerprint: String,
    pub(crate) provenance_refs: Vec<String>,
    pub(crate) reuse_prompt: Option<String>,
    pub(crate) lenses: T0,
}

pub(super) fn make_constellation(
    args: MakeConstellationArgs<impl IntoIterator<Item = (WorldMemoryLensKind, String)>>,
) -> Result<WorldMemoryConstellation, EngineError> {
    let MakeConstellationArgs {
        constellation_id,
        world_id,
        source_kind,
        source_id,
        title,
        source_fingerprint,
        provenance_refs,
        reuse_prompt,
        lenses,
    } = args;
    let lenses = lenses
        .into_iter()
        .map(|(kind, text)| Ok((kind, lens_from_text(kind, text)?)))
        .collect::<Result<BTreeMap<_, _>, EngineError>>()?;
    if !lenses.contains_key(&WorldMemoryLensKind::Text)
        || !lenses.contains_key(&WorldMemoryLensKind::Structured)
    {
        return invalid_world_memory("lenses", "text and structured lenses are required");
    }
    if source_kind == WorldMemorySourceKind::Script
        && !lenses.contains_key(&WorldMemoryLensKind::Code)
    {
        return invalid_world_memory("lenses.code", "script constellations require a code lens");
    }
    let slots = typed_slots_from_lenses(source_kind, &source_id, &lenses)?;
    Ok(WorldMemoryConstellation {
        constellation_id,
        world_id,
        source_kind,
        source_id,
        title,
        source_fingerprint,
        provenance_refs: normalize(provenance_refs),
        lenses,
        slots,
        reuse_prompt,
    })
}

pub(super) fn score_constellation(
    constellation: &WorldMemoryConstellation,
    query_terms: &BTreeMap<String, u32>,
) -> Option<WorldMemorySearchHit> {
    let mut lens_scores = BTreeMap::new();
    let mut score = 0;
    for slot in constellation
        .slots
        .iter()
        .filter(|slot| slot.is_searchable())
    {
        let Some(kind) = slot.lens_kind() else {
            continue;
        };
        let overlap = query_terms
            .keys()
            .filter(|term| slot.terms.contains_key(*term))
            .count() as u32;
        let weighted = overlap
            * match kind {
                WorldMemoryLensKind::Text => 30,
                WorldMemoryLensKind::Structured => 20,
                WorldMemoryLensKind::Code => 10,
            };
        if weighted > 0 {
            lens_scores.insert(kind, weighted);
            score += weighted;
        }
    }
    (score > 0).then(|| WorldMemorySearchHit {
        constellation_id: constellation.constellation_id.clone(),
        world_id: constellation.world_id.clone(),
        source_kind: constellation.source_kind,
        source_id: constellation.source_id.clone(),
        title: constellation.title.clone(),
        score,
        lens_scores,
        provenance_refs: constellation.provenance_refs.clone(),
        source_fingerprint: constellation.source_fingerprint.clone(),
    })
}

pub(super) fn detect_conflicts(
    constellations: &[WorldMemoryConstellation],
) -> Vec<WorldMemoryConflict> {
    let mut conflicts = Vec::new();
    for left_index in 0..constellations.len() {
        for right in constellations.iter().skip(left_index + 1) {
            let left = &constellations[left_index];
            if left.source_kind == WorldMemorySourceKind::Ability
                && right.source_kind == WorldMemorySourceKind::Ability
                && left.title == right.title
                && left.world_id != right.world_id
                && disjoint_non_empty(&left.provenance_refs, &right.provenance_refs)
            {
                conflicts.push(WorldMemoryConflict {
                    left_id: left.constellation_id.clone(),
                    right_id: right.constellation_id.clone(),
                    conflict_kind: "anchor-conflict".to_string(),
                    reason: "same ability title crosses worlds with disjoint provenance"
                        .to_string(),
                });
            }
        }
    }
    conflicts
}

pub(super) fn push_unique(
    constellations: &mut Vec<WorldMemoryConstellation>,
    seen: &mut BTreeSet<String>,
    constellation: WorldMemoryConstellation,
) -> Result<(), EngineError> {
    if !seen.insert(constellation.constellation_id.clone()) {
        return invalid_world_memory(
            "constellation.duplicate",
            format!("duplicate {}", constellation.constellation_id),
        );
    }
    constellations.push(constellation);
    Ok(())
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

pub(super) fn optional_property_values(object: &EditorObject, key: &str) -> Vec<String> {
    object
        .properties
        .get(key)
        .map(|value| vec![value.clone()])
        .unwrap_or_default()
}

pub(super) fn invalid_world_memory<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_world_memory_error(field, reason))
}

pub(super) fn invalid_world_memory_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_memory.{}", field.into()),
        reason: reason.into(),
    }
}

fn lens_from_text(kind: WorldMemoryLensKind, text: String) -> Result<WorldMemoryLens, EngineError> {
    let terms = token_counts(&text);
    if terms.is_empty() {
        return invalid_world_memory("lens.terms", format!("{kind:?} lens has no terms"));
    }
    Ok(WorldMemoryLens { text, terms })
}

fn normalize(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn disjoint_non_empty(left: &[String], right: &[String]) -> bool {
    !left.is_empty()
        && !right.is_empty()
        && left
            .iter()
            .collect::<BTreeSet<_>>()
            .is_disjoint(&right.iter().collect())
}
