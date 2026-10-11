use std::collections::BTreeMap;

use calyx_core::{
    Constellation, CxFlags, CxId, InputRef, LedgerRef, Modality, SlotId, SlotVector, VaultId,
};
use calyx_sextant::{
    ConsensusHit, ConsensusMode, ConsensusReport, Hit, HnswIndex, SearchEngine, SextantIndex,
    SlotIndexMap, agree, define, disagree, neighbors,
};
use litd_engine::EngineError;

use crate::world_memory::{
    WorldMemoryConstellation, WorldMemoryFeatureVector, WorldMemoryIndex, WorldMemoryLensKind,
    WorldMemorySlotKind, WorldMemorySlotState, validate_world_memory_index,
};

use super::{
    SEMANTIC_AUTHORING_OPS_SCHEMA, SemanticAuthoringDefinition, SemanticAuthoringDefinitionSlot,
    SemanticAuthoringHit, SemanticAuthoringOperation, SemanticAuthoringReport,
    SemanticAuthoringRequest, SemanticAuthoringSlotEvidence, SemanticAuthoringStatus,
    SemanticAuthoringSubject,
};

#[path = "semantic_authoring_ops_engine_report.rs"]
mod semantic_authoring_ops_engine_report;
#[path = "semantic_authoring_ops_engine_support.rs"]
mod semantic_authoring_ops_engine_support;

pub(in crate::semantic_authoring_ops) use semantic_authoring_ops_engine_report::validate_report;
pub(super) use semantic_authoring_ops_engine_support::invalid_semantic_authoring;
use semantic_authoring_ops_engine_support::{
    calyx_error, cx_for_constellation, mark_no_neighbors_if_empty, semantic_authoring_error, sha32,
};

const SEMANTIC_AUTHORING_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB3";

struct EngineContext {
    engine: SearchEngine,
    subjects: BTreeMap<CxId, SemanticAuthoringSubject>,
    cx_by_constellation: BTreeMap<String, CxId>,
}

pub(super) fn run_semantic_authoring_request(
    source_of_truth: String,
    index: &WorldMemoryIndex,
    request: &SemanticAuthoringRequest,
) -> Result<SemanticAuthoringReport, EngineError> {
    let context = build_engine(index)?;
    let subject_cx = context
        .cx_by_constellation
        .get(&request.subject_constellation_id)
        .copied()
        .ok_or_else(|| {
            semantic_authoring_error(
                "subject",
                format!(
                    "subject constellation {} is not present in the world-memory index",
                    request.subject_constellation_id
                ),
            )
        })?;
    let subject = subject_for(&context, subject_cx)?.clone();
    let mut report = SemanticAuthoringReport {
        schema: SEMANTIC_AUTHORING_OPS_SCHEMA.to_string(),
        query_id: request.query_id.clone(),
        source_of_truth,
        operation: request.operation,
        status: SemanticAuthoringStatus::Returned,
        code: "LITD-SEMANTIC-RETURNED".to_string(),
        reasons: Vec::new(),
        requested_by: request.requested_by.clone(),
        k: request.k,
        lens: request.lens,
        slot_filter: request.slot_filter.clone(),
        subject,
        hits: Vec::new(),
        definition: None,
        skipped_insufficient_overlap: Vec::new(),
    };

    match request.operation {
        SemanticAuthoringOperation::Neighbors => {
            let slot = lens_slot(request.lens.expect("validated lens"));
            let raw = neighbors(&context.engine, subject_cx, slot, request.k + 1)
                .map_err(calyx_error("neighbors"))?;
            report.hits = raw
                .into_iter()
                .filter(|hit| hit.cx_id != subject_cx)
                .take(request.k)
                .map(|hit| hit_to_report_hit(&context, hit))
                .collect::<Result<Vec<_>, _>>()?;
            mark_no_neighbors_if_empty(&mut report, "neighbors returned no related content");
        }
        SemanticAuthoringOperation::Define => {
            let slot = lens_slot(request.lens.expect("validated lens"));
            let definition = define(&context.engine, subject_cx, slot, request.k)
                .map_err(calyx_error("define"))?;
            report.definition = Some(definition_to_report(subject_cx, definition)?);
        }
        SemanticAuthoringOperation::Agree => {
            let slot_filter = request_slots(&request.slot_filter);
            let consensus = agree(
                &context.engine,
                subject_cx,
                request.k,
                slot_filter.as_deref(),
            )
            .map_err(calyx_error("agree"))?;
            apply_consensus(&context, &mut report, consensus)?;
            mark_no_neighbors_if_empty(&mut report, "agree returned no consensus matches");
        }
        SemanticAuthoringOperation::Disagree => {
            let slot_filter = request_slots(&request.slot_filter);
            let consensus = disagree(
                &context.engine,
                subject_cx,
                request.k,
                slot_filter.as_deref(),
            )
            .map_err(calyx_error("disagree"))?;
            apply_consensus(&context, &mut report, consensus)?;
            mark_no_neighbors_if_empty(&mut report, "disagree returned no contradiction matches");
        }
    }
    Ok(report)
}

fn build_engine(index: &WorldMemoryIndex) -> Result<EngineContext, EngineError> {
    validate_world_memory_index(index)?;
    if index.constellations.is_empty() {
        return invalid_semantic_authoring("index.empty", "world-memory index has no content");
    }
    let mut subjects = BTreeMap::new();
    let mut cx_by_constellation = BTreeMap::new();
    let mut entries = BTreeMap::<SlotId, Vec<(CxId, Vec<f32>, u64)>>::new();
    for (seq, constellation) in index.constellations.iter().enumerate() {
        let cx_id = cx_for_constellation(&constellation.constellation_id);
        if cx_by_constellation
            .insert(constellation.constellation_id.clone(), cx_id)
            .is_some()
        {
            return invalid_semantic_authoring(
                "index.duplicate",
                format!("duplicate {}", constellation.constellation_id),
            );
        }
        subjects.insert(cx_id, subject_from_constellation(cx_id, constellation));
        for vector in feature_vectors(constellation)? {
            let slot = lens_slot(vector.source_lens);
            entries.entry(slot).or_default().push((
                cx_id,
                feature_vector_values(vector)?,
                seq as u64 + 1,
            ));
        }
    }
    if entries.is_empty() {
        return invalid_semantic_authoring(
            "index.vectors",
            "world-memory index has no populated semantic vectors",
        );
    }

    let indexes = SlotIndexMap::new();
    for (slot, rows) in entries {
        let dim = rows
            .first()
            .map(|(_, vector, _)| vector.len())
            .ok_or_else(|| semantic_authoring_error("index.vectors", "empty slot vector set"))?;
        let mut index = HnswIndex::new(slot, dim as u32, 1_203 + u64::from(slot.get()));
        for (cx_id, vector, seq) in rows {
            if vector.len() != dim {
                return invalid_semantic_authoring(
                    "index.vector.dim",
                    format!("slot {} has mixed vector dimensions", slot.get()),
                );
            }
            index
                .insert(
                    cx_id,
                    SlotVector::Dense {
                        dim: dim as u32,
                        data: vector,
                    },
                    seq,
                )
                .map_err(calyx_error("index.insert"))?;
        }
        indexes
            .register(index)
            .map_err(calyx_error("index.register"))?;
    }

    let mut engine = SearchEngine::new(indexes);
    let vault_id = SEMANTIC_AUTHORING_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| semantic_authoring_error("vault_id", error))?;
    for (seq, (cx_id, subject)) in subjects.iter().enumerate() {
        engine.put_constellation(calyx_doc(*cx_id, subject, vault_id, seq as u64 + 1));
    }
    Ok(EngineContext {
        engine,
        subjects,
        cx_by_constellation,
    })
}

fn feature_vectors(
    constellation: &WorldMemoryConstellation,
) -> Result<Vec<&WorldMemoryFeatureVector>, EngineError> {
    let Some(vector_slot) = constellation
        .slots
        .iter()
        .find(|slot| slot.kind == WorldMemorySlotKind::VectorSet)
    else {
        return invalid_semantic_authoring(
            "index.vector_slot",
            format!("{} has no vector-set slot", constellation.constellation_id),
        );
    };
    if vector_slot.state != WorldMemorySlotState::Populated || vector_slot.vectors.is_empty() {
        return invalid_semantic_authoring(
            "index.vector_slot",
            format!(
                "{} vector-set slot is not populated",
                constellation.constellation_id
            ),
        );
    }
    Ok(vector_slot.vectors.iter().collect())
}

fn feature_vector_values(vector: &WorldMemoryFeatureVector) -> Result<Vec<f32>, EngineError> {
    if vector.dimension == 0 || vector.values.len() != vector.dimension {
        return invalid_semantic_authoring(
            "index.vector.shape",
            format!("{} has invalid dimensions", vector.vector_id),
        );
    }
    if vector.values.iter().all(|value| *value == 0) {
        return invalid_semantic_authoring(
            "index.vector.zero",
            format!("{} has zero norm", vector.vector_id),
        );
    }
    Ok(vector.values.iter().map(|value| *value as f32).collect())
}

fn apply_consensus(
    context: &EngineContext,
    report: &mut SemanticAuthoringReport,
    consensus: ConsensusReport,
) -> Result<(), EngineError> {
    report.hits = consensus
        .hits
        .into_iter()
        .map(|hit| consensus_hit_to_report_hit(context, hit, consensus.mode))
        .collect::<Result<Vec<_>, _>>()?;
    report.skipped_insufficient_overlap = consensus
        .skipped_insufficient_overlap
        .into_iter()
        .map(|cx_id| subject_for(context, cx_id).cloned())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

fn hit_to_report_hit(
    context: &EngineContext,
    hit: Hit,
) -> Result<SemanticAuthoringHit, EngineError> {
    let subject = subject_for(context, hit.cx_id)?;
    Ok(SemanticAuthoringHit {
        rank: hit.rank,
        constellation_id: subject.constellation_id.clone(),
        calyx_cx_id: hit.cx_id.to_string(),
        world_id: subject.world_id.clone(),
        source_kind: subject.source_kind,
        source_id: subject.source_id.clone(),
        title: subject.title.clone(),
        score: hit.score,
        mean_cosine: None,
        min_cosine: None,
        max_cosine: None,
        spread: None,
        per_slot: Vec::new(),
        provenance_refs: subject.provenance_refs.clone(),
    })
}

fn consensus_hit_to_report_hit(
    context: &EngineContext,
    hit: ConsensusHit,
    mode: ConsensusMode,
) -> Result<SemanticAuthoringHit, EngineError> {
    let subject = subject_for(context, hit.cx_id)?;
    let per_slot = hit
        .per_slot
        .into_iter()
        .map(|entry| {
            Ok(SemanticAuthoringSlotEvidence {
                lens: slot_lens(entry.slot)?,
                slot_id: entry.slot.get(),
                cosine: entry.cosine,
            })
        })
        .collect::<Result<Vec<_>, EngineError>>()?;
    let score = match mode {
        ConsensusMode::Agree => hit.min_cosine,
        ConsensusMode::Disagree => hit.spread,
    };
    Ok(SemanticAuthoringHit {
        rank: hit.rank,
        constellation_id: subject.constellation_id.clone(),
        calyx_cx_id: hit.cx_id.to_string(),
        world_id: subject.world_id.clone(),
        source_kind: subject.source_kind,
        source_id: subject.source_id.clone(),
        title: subject.title.clone(),
        score,
        mean_cosine: Some(hit.mean_cosine),
        min_cosine: Some(hit.min_cosine),
        max_cosine: Some(hit.max_cosine),
        spread: Some(hit.spread),
        per_slot,
        provenance_refs: subject.provenance_refs.clone(),
    })
}

fn definition_to_report(
    subject_cx: CxId,
    definition: Constellation,
) -> Result<SemanticAuthoringDefinition, EngineError> {
    let mut slots = Vec::new();
    for (slot, vector) in definition.slots {
        let SlotVector::Dense { dim, data } = vector else {
            continue;
        };
        slots.push(SemanticAuthoringDefinitionSlot {
            lens: slot_lens(slot)?,
            slot_id: slot.get(),
            dimension: dim as usize,
            centroid: data,
        });
    }
    slots.sort_by_key(|slot| slot.slot_id);
    Ok(SemanticAuthoringDefinition {
        subject_constellation_id: definition
            .metadata
            .get("world_memory_constellation_id")
            .cloned()
            .unwrap_or_else(|| subject_cx.to_string()),
        calyx_cx_id: subject_cx.to_string(),
        slots,
    })
}

fn request_slots(lenses: &[WorldMemoryLensKind]) -> Option<Vec<SlotId>> {
    (!lenses.is_empty()).then(|| lenses.iter().copied().map(lens_slot).collect())
}

fn lens_slot(lens: WorldMemoryLensKind) -> SlotId {
    match lens {
        WorldMemoryLensKind::Text => SlotId::new(1),
        WorldMemoryLensKind::Structured => SlotId::new(2),
        WorldMemoryLensKind::Code => SlotId::new(3),
    }
}

fn slot_lens(slot: SlotId) -> Result<WorldMemoryLensKind, EngineError> {
    match slot.get() {
        1 => Ok(WorldMemoryLensKind::Text),
        2 => Ok(WorldMemoryLensKind::Structured),
        3 => Ok(WorldMemoryLensKind::Code),
        other => invalid_semantic_authoring("slot", format!("unknown semantic slot {other}")),
    }
}

fn subject_for(
    context: &EngineContext,
    cx_id: CxId,
) -> Result<&SemanticAuthoringSubject, EngineError> {
    context
        .subjects
        .get(&cx_id)
        .ok_or_else(|| semantic_authoring_error("cx", format!("missing subject for {cx_id}")))
}

fn subject_from_constellation(
    cx_id: CxId,
    constellation: &WorldMemoryConstellation,
) -> SemanticAuthoringSubject {
    SemanticAuthoringSubject {
        constellation_id: constellation.constellation_id.clone(),
        calyx_cx_id: cx_id.to_string(),
        world_id: constellation.world_id.clone(),
        source_kind: constellation.source_kind,
        source_id: constellation.source_id.clone(),
        title: constellation.title.clone(),
        provenance_refs: constellation.provenance_refs.clone(),
    }
}

fn calyx_doc(
    cx_id: CxId,
    subject: &SemanticAuthoringSubject,
    vault_id: VaultId,
    seq: u64,
) -> Constellation {
    let hash = sha32(subject.constellation_id.as_bytes());
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "world_memory_constellation_id".to_string(),
        subject.constellation_id.clone(),
    );
    metadata.insert("world_id".to_string(), subject.world_id.clone());
    metadata.insert("source_id".to_string(), subject.source_id.clone());
    Constellation {
        cx_id,
        vault_id,
        panel_version: 1,
        created_at: seq,
        input_ref: InputRef {
            hash,
            pointer: Some(subject.constellation_id.clone()),
            redacted: false,
        },
        modality: Modality::Text,
        slots: BTreeMap::new(),
        scalars: BTreeMap::new(),
        metadata,
        anchors: Vec::new(),
        provenance: LedgerRef { seq, hash },
        flags: CxFlags::default(),
    }
}
