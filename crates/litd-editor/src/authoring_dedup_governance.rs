use std::collections::{BTreeMap, BTreeSet};

#[path = "authoring_dedup_support.rs"]
mod authoring_dedup_support;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    authoring::LoweredAbilityDocument,
    history::{EditorObject, stable_fingerprint},
    world_creation::{EditorWorldRecord, validate_world_record},
};
use authoring_dedup_support::{
    anchor_conflicts, cosine_milli, decision_rank, invalid_dedup, merge_suggestion,
    normalize_values, optional_property_values, token_counts,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CatalogEntityKind {
    Ability,
    Unit,
    Item,
    World,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DedupSlotKind {
    Identity,
    Mechanics,
    Semantics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DedupDecisionKind {
    ExactDuplicate,
    NearDuplicate,
    AnchorConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupPolicy {
    pub required_slots: Vec<DedupSlotKind>,
    pub tau_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupTextSlot {
    pub text: String,
    pub terms: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogDedupRecord {
    pub record_id: String,
    pub kind: CatalogEntityKind,
    pub world_id: String,
    pub stable_fingerprint: String,
    pub anchor_ids: Vec<String>,
    pub source_refs: Vec<String>,
    pub slots: BTreeMap<DedupSlotKind, DedupTextSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogDedupReport {
    pub schema: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub candidate_count: usize,
    pub candidates: Vec<DedupCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupCandidate {
    pub left_id: String,
    pub right_id: String,
    pub kind: CatalogEntityKind,
    pub decision: DedupDecisionKind,
    pub slot_scores: BTreeMap<DedupSlotKind, u16>,
    pub minimum_score_milli: u16,
    pub exact_fingerprint: bool,
    pub anchor_conflicts: Vec<String>,
    pub merge_suggestion: String,
}

impl Default for DedupPolicy {
    fn default() -> Self {
        Self {
            required_slots: vec![
                DedupSlotKind::Identity,
                DedupSlotKind::Mechanics,
                DedupSlotKind::Semantics,
            ],
            tau_milli: 850,
        }
    }
}

impl CatalogDedupRecord {
    pub fn from_lowered_ability(document: &LoweredAbilityDocument) -> Result<Self, EngineError> {
        let ability = &document.ability;
        let provenance = &document.provenance;
        Self::from_slot_texts(
            format!(
                "{}::{}::{}",
                ability.world_id, ability.id, provenance.request_id
            ),
            CatalogEntityKind::Ability,
            ability.world_id.clone(),
            provenance.matched_terms.clone(),
            provenance.source_refs.clone(),
            stable_fingerprint(document)?,
            [
                (
                    DedupSlotKind::Identity,
                    format!(
                        "{} {} {} {}",
                        ability.id, ability.name, ability.faction, ability.damage_type
                    ),
                ),
                (
                    DedupSlotKind::Mechanics,
                    format!(
                        "{} damage {} cooldown {} mana {} tags {} balance {} {} {}",
                        ability.damage_type,
                        ability.damage,
                        ability.cooldown_seconds,
                        ability.mana_cost,
                        ability.tags.join(" "),
                        document.balance.profile_id,
                        document.balance.damage_min,
                        document.balance.damage_max
                    ),
                ),
                (
                    DedupSlotKind::Semantics,
                    format!(
                        "{} {} {} {} {} {}",
                        ability.world_id,
                        ability.region,
                        ability.faction,
                        ability.summary,
                        ability.tags.join(" "),
                        provenance.matched_terms.join(" ")
                    ),
                ),
            ],
        )
    }

    pub fn from_editor_object(
        world_id: impl Into<String>,
        object: &EditorObject,
    ) -> Result<Self, EngineError> {
        let kind = match object.kind.as_str() {
            "Unit" => CatalogEntityKind::Unit,
            "Item" => CatalogEntityKind::Item,
            other => {
                return invalid_dedup(
                    "object.kind",
                    format!("expected Unit or Item object, got {other}"),
                );
            }
        };
        let property_text = object
            .properties
            .iter()
            .map(|(key, value)| format!("{key} {value}"))
            .collect::<Vec<_>>()
            .join(" ");
        let source_refs = optional_property_values(object, "source_ref");
        let anchor_ids = optional_property_values(object, "anchor_id");
        Self::from_slot_texts(
            format!("{}::{}", object.kind, object.id),
            kind,
            world_id.into(),
            anchor_ids,
            source_refs,
            stable_fingerprint(object)?,
            [
                (
                    DedupSlotKind::Identity,
                    format!(
                        "{} {} {} {} {}",
                        object.id,
                        object.kind,
                        object.properties.get("name").cloned().unwrap_or_default(),
                        object
                            .properties
                            .get("unit_type")
                            .or_else(|| object.properties.get("item_type"))
                            .cloned()
                            .unwrap_or_default(),
                        object
                            .properties
                            .get("display_name")
                            .cloned()
                            .unwrap_or_default()
                    ),
                ),
                (
                    DedupSlotKind::Mechanics,
                    format!(
                        "{} {} {}",
                        object.kind,
                        property_text,
                        object.properties.get("stats").cloned().unwrap_or_default()
                    ),
                ),
                (
                    DedupSlotKind::Semantics,
                    format!(
                        "{} {} {}",
                        object.parent.clone().unwrap_or_default(),
                        object.properties.get("role").cloned().unwrap_or_default(),
                        object
                            .properties
                            .get("description")
                            .cloned()
                            .unwrap_or_default()
                    ),
                ),
            ],
        )
    }

    pub fn from_world_record(record: &EditorWorldRecord) -> Result<Self, EngineError> {
        validate_world_record(record)?;
        let properties = &record.spec.properties;
        Self::from_slot_texts(
            record.spec.calyx_document_id.clone(),
            CatalogEntityKind::World,
            record.spec.calyx_document_id.clone(),
            vec![record.playable_region_id.clone()],
            vec![record.summary.clone()],
            record.document_fingerprint.clone(),
            [
                (
                    DedupSlotKind::Identity,
                    format!(
                        "{} {} {}",
                        properties.display_name, properties.tileset_id, properties.initial_tile_id
                    ),
                ),
                (
                    DedupSlotKind::Mechanics,
                    format!(
                        "{} {} {} {} {} {}",
                        properties.width_tiles,
                        properties.height_tiles,
                        properties.initial_cliff_type,
                        properties.initial_cliff_level,
                        record.terrain_cell_count,
                        record.default_tile_count
                    ),
                ),
                (
                    DedupSlotKind::Semantics,
                    format!(
                        "{} {} {} {}",
                        properties.description,
                        properties.loading_screen_asset_ref,
                        properties.preview_asset_ref,
                        record.summary
                    ),
                ),
            ],
        )
    }

    pub fn from_slot_texts(
        record_id: impl Into<String>,
        kind: CatalogEntityKind,
        world_id: impl Into<String>,
        anchor_ids: Vec<String>,
        source_refs: Vec<String>,
        stable_fingerprint: impl Into<String>,
        slots: impl IntoIterator<Item = (DedupSlotKind, String)>,
    ) -> Result<Self, EngineError> {
        let record = Self {
            record_id: record_id.into(),
            kind,
            world_id: world_id.into(),
            stable_fingerprint: stable_fingerprint.into(),
            anchor_ids: normalize_values(anchor_ids),
            source_refs: normalize_values(source_refs),
            slots: slots
                .into_iter()
                .map(|(slot, text)| Ok((slot, DedupTextSlot::from_text(slot, text)?)))
                .collect::<Result<BTreeMap<_, _>, EngineError>>()?,
        };
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.record_id.trim().is_empty()
            || self.world_id.trim().is_empty()
            || self.stable_fingerprint.trim().is_empty()
        {
            return invalid_dedup(
                "record.identity",
                "record_id, world_id, and stable_fingerprint are required",
            );
        }
        if self.slots.is_empty() {
            return invalid_dedup("record.slots", "at least one dedup slot is required");
        }
        Ok(())
    }
}

impl DedupTextSlot {
    fn from_text(kind: DedupSlotKind, text: String) -> Result<Self, EngineError> {
        let terms = token_counts(&text);
        if terms.is_empty() {
            return invalid_dedup(
                "slot.terms",
                format!("{kind:?} slot must contain at least one token"),
            );
        }
        Ok(Self { text, terms })
    }
}

pub fn build_catalog_dedup_report(
    records: &[CatalogDedupRecord],
    policy: &DedupPolicy,
) -> Result<CatalogDedupReport, EngineError> {
    validate_policy(policy)?;
    if records.is_empty() {
        return invalid_dedup("records", "catalog must contain at least one record");
    }
    validate_records(records, policy)?;
    let mut candidates = Vec::new();
    for left_index in 0..records.len() {
        for right in records.iter().skip(left_index + 1) {
            let left = &records[left_index];
            if left.kind != right.kind {
                continue;
            }
            if let Some(candidate) = compare_pair(left, right, policy) {
                candidates.push(candidate);
            }
        }
    }
    candidates.sort_by(|left, right| {
        decision_rank(left.decision)
            .cmp(&decision_rank(right.decision))
            .then_with(|| right.minimum_score_milli.cmp(&left.minimum_score_milli))
            .then_with(|| left.left_id.cmp(&right.left_id))
            .then_with(|| left.right_id.cmp(&right.right_id))
    });
    Ok(CatalogDedupReport {
        schema: "litd.authoring.catalog_dedup_governance.v1".to_string(),
        source_of_truth: "canonical lowered ability documents + editor objects + world records"
            .to_string(),
        record_count: records.len(),
        candidate_count: candidates.len(),
        candidates,
    })
}

fn compare_pair(
    left: &CatalogDedupRecord,
    right: &CatalogDedupRecord,
    policy: &DedupPolicy,
) -> Option<DedupCandidate> {
    let slot_scores = policy
        .required_slots
        .iter()
        .map(|slot| {
            let score = cosine_milli(&left.slots[slot].terms, &right.slots[slot].terms);
            (*slot, score)
        })
        .collect::<BTreeMap<_, _>>();
    let minimum_score_milli = slot_scores.values().copied().min().unwrap_or(0);
    let exact_fingerprint = left.stable_fingerprint == right.stable_fingerprint;
    let passes_tau = slot_scores.values().all(|score| *score >= policy.tau_milli);
    if !exact_fingerprint && !passes_tau {
        return None;
    }
    let anchor_conflicts = anchor_conflicts(left, right);
    let decision = if exact_fingerprint {
        DedupDecisionKind::ExactDuplicate
    } else if anchor_conflicts.is_empty() {
        DedupDecisionKind::NearDuplicate
    } else {
        DedupDecisionKind::AnchorConflict
    };
    Some(DedupCandidate {
        left_id: left.record_id.clone(),
        right_id: right.record_id.clone(),
        kind: left.kind,
        decision,
        slot_scores,
        minimum_score_milli,
        exact_fingerprint,
        anchor_conflicts,
        merge_suggestion: merge_suggestion(decision, left, right),
    })
}

fn validate_policy(policy: &DedupPolicy) -> Result<(), EngineError> {
    if policy.required_slots.is_empty() || policy.tau_milli > 1000 {
        return invalid_dedup(
            "policy",
            "required_slots must be non-empty and tau_milli must be 0..=1000",
        );
    }
    let unique = policy.required_slots.iter().collect::<BTreeSet<_>>();
    if unique.len() != policy.required_slots.len() {
        return invalid_dedup(
            "policy.required_slots",
            "slot list must not contain duplicates",
        );
    }
    Ok(())
}

fn validate_records(
    records: &[CatalogDedupRecord],
    policy: &DedupPolicy,
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for record in records {
        record.validate()?;
        if !seen.insert(&record.record_id) {
            return invalid_dedup(
                "records.duplicate",
                format!("duplicate {}", record.record_id),
            );
        }
        for slot in &policy.required_slots {
            if !record.slots.contains_key(slot) {
                return invalid_dedup(
                    "records.required_slot",
                    format!("{} missing {slot:?}", record.record_id),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "authoring_dedup_governance_tests.rs"]
mod tests;
