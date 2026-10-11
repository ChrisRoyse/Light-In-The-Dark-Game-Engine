use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, render::shader::shader_fingerprint};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{WorldMemoryLens, WorldMemoryLensKind, WorldMemorySourceKind};

const TOKEN_VECTOR_DIMENSION: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WorldMemorySlotKind {
    Text,
    Structured,
    Code,
    VectorSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldMemorySlotState {
    Populated,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemorySlotShape {
    pub text_bytes: usize,
    pub term_count: usize,
    pub vector_count: usize,
    pub vector_dimensions: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryFeatureVector {
    pub vector_id: String,
    pub source_lens: WorldMemoryLensKind,
    pub dimension: usize,
    pub values: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryTypedSlot {
    pub kind: WorldMemorySlotKind,
    pub state: WorldMemorySlotState,
    pub source_id: String,
    pub text: Option<String>,
    pub terms: BTreeMap<String, u32>,
    pub vectors: Vec<WorldMemoryFeatureVector>,
    pub shape: WorldMemorySlotShape,
    pub evidence: BTreeMap<String, String>,
    pub missing_reason: Option<String>,
    pub fingerprint: Option<String>,
}

impl WorldMemoryTypedSlot {
    pub fn lens_kind(&self) -> Option<WorldMemoryLensKind> {
        match self.kind {
            WorldMemorySlotKind::Text => Some(WorldMemoryLensKind::Text),
            WorldMemorySlotKind::Structured => Some(WorldMemoryLensKind::Structured),
            WorldMemorySlotKind::Code => Some(WorldMemoryLensKind::Code),
            WorldMemorySlotKind::VectorSet => None,
        }
    }

    pub fn is_searchable(&self) -> bool {
        self.state == WorldMemorySlotState::Populated
            && self.lens_kind().is_some()
            && !self.terms.is_empty()
    }
}

pub(super) fn typed_slots_from_lenses(
    source_kind: WorldMemorySourceKind,
    source_id: &str,
    lenses: &BTreeMap<WorldMemoryLensKind, WorldMemoryLens>,
) -> Result<Vec<WorldMemoryTypedSlot>, EngineError> {
    let mut slots = Vec::new();
    let text = required_lens(lenses, WorldMemoryLensKind::Text)?;
    slots.push(typed_slot(
        WorldMemorySlotKind::Text,
        source_id,
        Some(text),
        Vec::new(),
        [("source_lens", "Text".to_string())],
    ));
    let structured = required_lens(lenses, WorldMemoryLensKind::Structured)?;
    slots.push(typed_slot(
        WorldMemorySlotKind::Structured,
        source_id,
        Some(structured),
        Vec::new(),
        [("source_lens", "Structured".to_string())],
    ));

    if let Some(code) = lenses.get(&WorldMemoryLensKind::Code) {
        slots.push(typed_slot(
            WorldMemorySlotKind::Code,
            source_id,
            Some(code),
            Vec::new(),
            [("source_lens", "Code".to_string())],
        ));
    } else {
        slots.push(missing_slot(
            WorldMemorySlotKind::Code,
            source_id,
            missing_code_reason(source_kind),
        ));
    }

    let vectors = vec![
        token_vector(source_id, WorldMemoryLensKind::Text, &text.terms),
        token_vector(
            source_id,
            WorldMemoryLensKind::Structured,
            &structured.terms,
        ),
    ];
    slots.push(typed_slot(
        WorldMemorySlotKind::VectorSet,
        source_id,
        None,
        vectors,
        [("vector_source", "token_terms".to_string())],
    ));
    validate_typed_slots(source_kind, &slots)?;
    Ok(slots)
}

pub fn typed_slot(
    kind: WorldMemorySlotKind,
    source_id: &str,
    lens: Option<&WorldMemoryLens>,
    vectors: Vec<WorldMemoryFeatureVector>,
    evidence: impl IntoIterator<Item = (&'static str, String)>,
) -> WorldMemoryTypedSlot {
    let text = lens.map(|lens| lens.text.clone());
    let terms = lens.map(|lens| lens.terms.clone()).unwrap_or_default();
    let vector_dimensions = vectors
        .iter()
        .map(|vector| vector.dimension)
        .collect::<Vec<_>>();
    let shape = WorldMemorySlotShape {
        text_bytes: text.as_ref().map(|value| value.len()).unwrap_or(0),
        term_count: terms.len(),
        vector_count: vectors.len(),
        vector_dimensions,
    };
    let evidence = evidence
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect::<BTreeMap<_, _>>();
    let fingerprint = Some(shader_fingerprint(&format!(
        "{kind:?}|{source_id}|{shape:?}|{terms:?}|{vectors:?}|{evidence:?}"
    )));
    WorldMemoryTypedSlot {
        kind,
        state: WorldMemorySlotState::Populated,
        source_id: source_id.to_string(),
        text,
        terms,
        vectors,
        shape,
        evidence,
        missing_reason: None,
        fingerprint,
    }
}

pub fn validate_typed_slots(
    source_kind: WorldMemorySourceKind,
    slots: &[WorldMemoryTypedSlot],
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for slot in slots {
        if !seen.insert(slot.kind) {
            return invalid_slot("slot.duplicate", format!("duplicate {:?} slot", slot.kind));
        }
        validate_slot(slot)?;
    }
    require_populated(slots, WorldMemorySlotKind::Text)?;
    require_populated(slots, WorldMemorySlotKind::Structured)?;
    require_populated(slots, WorldMemorySlotKind::VectorSet)?;
    if matches!(
        source_kind,
        WorldMemorySourceKind::Ability | WorldMemorySourceKind::Script
    ) {
        require_populated(slots, WorldMemorySlotKind::Code)?;
    }
    Ok(())
}

fn validate_slot(slot: &WorldMemoryTypedSlot) -> Result<(), EngineError> {
    if slot.source_id.trim().is_empty() {
        return invalid_slot("slot.source_id", "slot source_id must be non-empty");
    }
    match slot.state {
        WorldMemorySlotState::Populated => validate_populated_slot(slot),
        WorldMemorySlotState::Missing => validate_missing_slot(slot),
    }
}

fn validate_populated_slot(slot: &WorldMemoryTypedSlot) -> Result<(), EngineError> {
    if slot
        .fingerprint
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return invalid_slot("slot.fingerprint", "populated slot fingerprint is empty");
    }
    match slot.kind {
        WorldMemorySlotKind::Text | WorldMemorySlotKind::Structured | WorldMemorySlotKind::Code => {
            if slot.text.as_deref().unwrap_or_default().trim().is_empty()
                || slot.terms.is_empty()
                || !slot.vectors.is_empty()
                || slot.shape.vector_count != 0
                || !slot.shape.vector_dimensions.is_empty()
                || slot.shape.text_bytes == 0
                || slot.shape.term_count != slot.terms.len()
            {
                return invalid_slot(
                    "slot.shape",
                    format!("{:?} populated text slot shape is inconsistent", slot.kind),
                );
            }
        }
        WorldMemorySlotKind::VectorSet => {
            if slot.text.is_some()
                || !slot.terms.is_empty()
                || slot.vectors.is_empty()
                || slot.shape.text_bytes != 0
                || slot.shape.term_count != 0
                || slot.shape.vector_count != slot.vectors.len()
                || slot.shape.vector_dimensions.len() != slot.vectors.len()
            {
                return invalid_slot("slot.shape", "vector-set slot shape is inconsistent");
            }
            for (index, vector) in slot.vectors.iter().enumerate() {
                if vector.vector_id.trim().is_empty()
                    || vector.dimension == 0
                    || vector.values.len() != vector.dimension
                    || slot.shape.vector_dimensions[index] != vector.dimension
                {
                    return invalid_slot(
                        "slot.shape.vector",
                        format!("vector {index} dimensions do not match shape"),
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_missing_slot(slot: &WorldMemoryTypedSlot) -> Result<(), EngineError> {
    if slot.text.is_some()
        || !slot.terms.is_empty()
        || !slot.vectors.is_empty()
        || slot.fingerprint.is_some()
        || slot
            .missing_reason
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return invalid_slot("slot.missing", "missing slot carried populated data");
    }
    if slot.shape.text_bytes != 0
        || slot.shape.term_count != 0
        || slot.shape.vector_count != 0
        || !slot.shape.vector_dimensions.is_empty()
    {
        return invalid_slot("slot.missing.shape", "missing slot shape must be empty");
    }
    Ok(())
}

fn require_populated(
    slots: &[WorldMemoryTypedSlot],
    kind: WorldMemorySlotKind,
) -> Result<(), EngineError> {
    let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
        return invalid_slot("slot.required", format!("missing {kind:?} slot"));
    };
    if slot.state != WorldMemorySlotState::Populated {
        return invalid_slot("slot.required", format!("{kind:?} slot is not populated"));
    }
    Ok(())
}

fn required_lens(
    lenses: &BTreeMap<WorldMemoryLensKind, WorldMemoryLens>,
    kind: WorldMemoryLensKind,
) -> Result<&WorldMemoryLens, EngineError> {
    lenses
        .get(&kind)
        .ok_or_else(|| slot_error("lens.required", format!("missing {kind:?} lens")))
}

fn missing_slot(
    kind: WorldMemorySlotKind,
    source_id: &str,
    reason: impl Into<String>,
) -> WorldMemoryTypedSlot {
    WorldMemoryTypedSlot {
        kind,
        state: WorldMemorySlotState::Missing,
        source_id: source_id.to_string(),
        text: None,
        terms: BTreeMap::new(),
        vectors: Vec::new(),
        shape: WorldMemorySlotShape {
            text_bytes: 0,
            term_count: 0,
            vector_count: 0,
            vector_dimensions: Vec::new(),
        },
        evidence: BTreeMap::new(),
        missing_reason: Some(reason.into()),
        fingerprint: None,
    }
}

fn token_vector(
    source_id: &str,
    source_lens: WorldMemoryLensKind,
    terms: &BTreeMap<String, u32>,
) -> WorldMemoryFeatureVector {
    let mut values = vec![0_i32; TOKEN_VECTOR_DIMENSION];
    for (term, count) in terms {
        let bucket = stable_bucket(term, TOKEN_VECTOR_DIMENSION);
        values[bucket] += i32::try_from(*count).unwrap_or(i32::MAX);
    }
    WorldMemoryFeatureVector {
        vector_id: format!("{source_id}::{source_lens:?}::token_terms"),
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

fn missing_code_reason(source_kind: WorldMemorySourceKind) -> String {
    format!("{source_kind:?} has no authored behavior/source-code slot")
}

fn invalid_slot<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(slot_error(field, reason))
}

fn slot_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_memory.{}", field.into()),
        reason: reason.into(),
    }
}
