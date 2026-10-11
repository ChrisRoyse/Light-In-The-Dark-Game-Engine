use litd_hash::sha256_hex;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    semantic_authoring_ops::{
        SemanticAuthoringOperation, SemanticAuthoringOpsLedger, SemanticAuthoringReport,
        SemanticAuthoringRequest, SemanticAuthoringStatus, build_semantic_authoring_ops_ledger,
        parse_semantic_authoring_ops_ledger, read_semantic_authoring_ops_ledger,
        run_semantic_authoring_action, write_semantic_authoring_ops_ledger,
    },
    world_memory::{
        WorldMemoryConstellation, WorldMemoryFeatureVector, WorldMemoryIndex, WorldMemoryLens,
        WorldMemoryLensKind, WorldMemorySlotKind, WorldMemorySourceKind, typed_slot,
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_SEMANTIC_AUTHORING_FSV",
        "issue1203_semantic_authoring_ops",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1203 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("semantic-authoring-ledger.json");
    let aster_dir = root.join("aster");
    let before = path_state(&output_path, &state_path, &aster_dir)?;

    let index = fixture_index();
    let reports = vec![
        run(
            &index,
            req_neighbors("issue1203-fsv-neighbors", "unit_anchor"),
        )?,
        run(&index, req_define("issue1203-fsv-define", "unit_anchor"))?,
        run(
            &index,
            req_consensus("issue1203-fsv-agree", SemanticAuthoringOperation::Agree),
        )?,
        run(
            &index,
            req_consensus(
                "issue1203-fsv-disagree",
                SemanticAuthoringOperation::Disagree,
            ),
        )?,
        run(
            &single_item_index(),
            req_neighbors("issue1203-fsv-no-neighbors", "unit_solo"),
        )?,
    ];
    let contradictory_error = contradictory_error(&index)?;
    let ood_error = ood_error(&index)?;
    let ledger = build_semantic_authoring_ops_ledger(
        "issue1203-semantic-authoring-ops-fsv",
        "issue1203-world-memory-fixture",
        reports,
    )?;
    verify_expected(&ledger, &contradictory_error, &ood_error)?;

    let writeback = write_semantic_authoring_ops_ledger(&state_path, &aster_dir, &ledger)?;
    let readback = read_semantic_authoring_ops_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing issue1203 semantic-authoring ledger")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err("state file bytes differ from Aster semantic-authoring ledger bytes".into());
    }
    let parsed = parse_semantic_authoring_ops_ledger(&readback.bytes)?;
    verify_expected(&parsed, &contradictory_error, &ood_error)?;
    let aster_stats = dir_stats(&aster_dir)?;

    let neighbors = report(&parsed, "issue1203-fsv-neighbors")?;
    let define = report(&parsed, "issue1203-fsv-define")?;
    let agree = report(&parsed, "issue1203-fsv-agree")?;
    let disagree = report(&parsed, "issue1203-fsv-disagree")?;
    let no_neighbors = report(&parsed, "issue1203-fsv-no-neighbors")?;
    let document = json!({
        "schema": "litd.issue1203.semantic_authoring_ops_fsv.v1",
        "issue": 1203,
        "source_of_truth": "Aster semantic-authoring ledger row and JSON mirror readback",
        "research_applied": [
            "Calyx sextant navigation primitives are the semantic source of truth",
            "Vector search should use compatible vector spaces and explicit cosine scoring",
            "Nearest-neighbor systems can return nearest-but-irrelevant rows, so no-neighbor and OOD states are explicit"
        ],
        "before": before,
        "happy_path": {
            "before": {
                "world_memory_rows": index.constellation_count,
                "anchor": "unit_anchor",
                "known_vectors": known_vectors()
            },
            "after": {
                "state_path": state_path,
                "state_sha256": sha256_hex(&disk_bytes),
                "aster_sha256": readback.sha256,
                "writeback_sha256": writeback.sha256,
                "state_matches_aster": disk_bytes == readback.bytes,
                "ledger_id": parsed.ledger_id,
                "report_count": parsed.report_count,
                "graph_rows": readback.graph_rows,
                "seq": readback.seq,
                "aster_file_count": aster_stats.0,
                "aster_file_bytes": aster_stats.1,
                "neighbors": summarize_report(neighbors),
                "define": summarize_report(define),
                "agree": summarize_report(agree),
                "disagree": summarize_report(disagree),
                "passed": true
            }
        },
        "edges": [
            {
                "edge": "no_neighbors_case",
                "before": {"world_memory_rows": 1, "anchor": "unit_solo"},
                "after": summarize_report(no_neighbors)
            },
            {
                "edge": "contradictory_request_shape",
                "before": {"operation": "agree", "lens": "Text"},
                "after": {"error": contradictory_error}
            },
            {
                "edge": "ood_subject_fails_closed",
                "before": {"subject_constellation_id": "unit_missing"},
                "after": {"error": ood_error}
            }
        ]
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1203 semantic authoring FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "ledger reports={} graph_rows={} state_matches_aster={} sha={}",
        parsed.report_count,
        readback.graph_rows,
        disk_bytes == readback.bytes,
        readback.sha256
    );
    println!(
        "tops neighbors={} agree={}:{:.3} disagree={}:{:.3} no_neighbors={}",
        neighbors.hits[0].constellation_id,
        agree.hits[0].constellation_id,
        agree.hits[0].score,
        disagree.hits[0].constellation_id,
        disagree.hits[0].score,
        no_neighbors.code
    );
    Ok(())
}

fn run(
    index: &WorldMemoryIndex,
    request: SemanticAuthoringRequest,
) -> Result<SemanticAuthoringReport, Box<dyn std::error::Error>> {
    Ok(run_semantic_authoring_action(
        "issue1203 semantic-authoring world-memory fixture",
        index,
        &request,
    )?)
}

fn verify_expected(
    ledger: &SemanticAuthoringOpsLedger,
    contradictory_error: &str,
    ood_error: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if report(ledger, "issue1203-fsv-neighbors")?.hits[0].constellation_id != "unit_contradictory" {
        return Err("neighbors did not return the same-text contradictory unit first".into());
    }
    if report(ledger, "issue1203-fsv-define")?
        .definition
        .as_ref()
        .map(|definition| definition.slots.len())
        != Some(2)
    {
        return Err("define did not persist two lens centroids".into());
    }
    let agree = report(ledger, "issue1203-fsv-agree")?;
    if agree.hits[0].constellation_id != "unit_agree" || agree.hits[0].score <= 0.99 {
        return Err("agree did not rank the all-lens agreeing unit first".into());
    }
    let disagree = report(ledger, "issue1203-fsv-disagree")?;
    if disagree.hits[0].constellation_id != "unit_contradictory"
        || (disagree.hits[0].score - 1.0).abs() > 1e-6
    {
        return Err("disagree did not rank the cross-lens spread unit first".into());
    }
    if report(ledger, "issue1203-fsv-no-neighbors")?.status != SemanticAuthoringStatus::NoNeighbors
    {
        return Err("single-row neighbor query did not persist no-neighbor status".into());
    }
    if !contradictory_error.contains("editor.semantic_authoring.request.contradictory") {
        return Err("contradictory request did not fail closed with request field".into());
    }
    if !ood_error.contains("editor.semantic_authoring.subject") {
        return Err("OOD subject did not fail closed with subject field".into());
    }
    Ok(())
}

fn fixture_index() -> WorldMemoryIndex {
    index(vec![
        constellation("unit_anchor", "Anchor Guardian", [10, 0], [10, 0]),
        constellation("unit_agree", "Agree Guardian", [9, 1], [9, 1]),
        constellation(
            "unit_contradictory",
            "Contradictory Guardian",
            [10, 0],
            [0, 10],
        ),
        constellation("unit_unrelated", "Unrelated Guardian", [0, 10], [0, 10]),
    ])
}

fn single_item_index() -> WorldMemoryIndex {
    index(vec![constellation(
        "unit_solo",
        "Solo Guardian",
        [10, 0],
        [10, 0],
    )])
}

fn index(constellations: Vec<WorldMemoryConstellation>) -> WorldMemoryIndex {
    WorldMemoryIndex {
        schema: "litd.authoring.world_memory.v2.no_flatten_slots".to_string(),
        source_of_truth: "issue1203 hand-computed world-memory vectors".to_string(),
        world_count: 1,
        constellation_count: constellations.len(),
        constellations,
        conflicts: Vec::new(),
    }
}

fn constellation(
    id: &str,
    title: &str,
    text_vector: [i32; 2],
    structured_vector: [i32; 2],
) -> WorldMemoryConstellation {
    let text = lens(format!("{title} text archetype"));
    let structured = lens(format!("{title} structured stats"));
    let slots = vec![
        typed_slot(WorldMemorySlotKind::Text, id, Some(&text), Vec::new(), []),
        typed_slot(
            WorldMemorySlotKind::Structured,
            id,
            Some(&structured),
            Vec::new(),
            [],
        ),
        typed_slot(
            WorldMemorySlotKind::VectorSet,
            id,
            None,
            vec![
                vector(id, WorldMemoryLensKind::Text, text_vector),
                vector(id, WorldMemoryLensKind::Structured, structured_vector),
            ],
            [("vector_source", "issue1203_hand_vectors".to_string())],
        ),
    ];
    let mut lenses = BTreeMap::new();
    lenses.insert(WorldMemoryLensKind::Text, text);
    lenses.insert(WorldMemoryLensKind::Structured, structured);
    WorldMemoryConstellation {
        constellation_id: id.to_string(),
        world_id: "issue1203-world".to_string(),
        source_kind: WorldMemorySourceKind::Unit,
        source_id: id.to_string(),
        title: title.to_string(),
        source_fingerprint: format!("fingerprint-{id}"),
        provenance_refs: vec![format!("Aster://issue1203/{id}")],
        lenses,
        slots,
        reuse_prompt: None,
    }
}

fn lens(text: String) -> WorldMemoryLens {
    let mut terms = BTreeMap::new();
    for token in text.split_whitespace() {
        *terms.entry(token.to_ascii_lowercase()).or_insert(0) += 1;
    }
    WorldMemoryLens { text, terms }
}

fn vector(
    source_id: &str,
    source_lens: WorldMemoryLensKind,
    values: [i32; 2],
) -> WorldMemoryFeatureVector {
    WorldMemoryFeatureVector {
        vector_id: format!("{source_id}::{source_lens:?}"),
        source_lens,
        dimension: 2,
        values: values.to_vec(),
    }
}

fn req_neighbors(query_id: &str, subject: &str) -> SemanticAuthoringRequest {
    request(
        query_id,
        SemanticAuthoringOperation::Neighbors,
        subject,
        Some(WorldMemoryLensKind::Text),
        Vec::new(),
        3,
    )
}

fn req_define(query_id: &str, subject: &str) -> SemanticAuthoringRequest {
    request(
        query_id,
        SemanticAuthoringOperation::Define,
        subject,
        Some(WorldMemoryLensKind::Text),
        Vec::new(),
        3,
    )
}

fn req_consensus(
    query_id: &str,
    operation: SemanticAuthoringOperation,
) -> SemanticAuthoringRequest {
    request(query_id, operation, "unit_anchor", None, Vec::new(), 3)
}

fn request(
    query_id: &str,
    operation: SemanticAuthoringOperation,
    subject: &str,
    lens: Option<WorldMemoryLensKind>,
    slot_filter: Vec<WorldMemoryLensKind>,
    k: usize,
) -> SemanticAuthoringRequest {
    SemanticAuthoringRequest {
        query_id: query_id.to_string(),
        operation,
        subject_constellation_id: subject.to_string(),
        lens,
        slot_filter,
        k,
        requested_by: "issue1203-fsv".to_string(),
    }
}

fn contradictory_error(index: &WorldMemoryIndex) -> Result<String, Box<dyn std::error::Error>> {
    Ok(run_semantic_authoring_action(
        "issue1203 semantic-authoring world-memory fixture",
        index,
        &request(
            "issue1203-fsv-contradictory",
            SemanticAuthoringOperation::Agree,
            "unit_anchor",
            Some(WorldMemoryLensKind::Text),
            Vec::new(),
            3,
        ),
    )
    .unwrap_err()
    .to_string())
}

fn ood_error(index: &WorldMemoryIndex) -> Result<String, Box<dyn std::error::Error>> {
    Ok(run_semantic_authoring_action(
        "issue1203 semantic-authoring world-memory fixture",
        index,
        &req_neighbors("issue1203-fsv-ood", "unit_missing"),
    )
    .unwrap_err()
    .to_string())
}

fn report<'a>(
    ledger: &'a SemanticAuthoringOpsLedger,
    query_id: &str,
) -> Result<&'a SemanticAuthoringReport, Box<dyn std::error::Error>> {
    ledger
        .reports
        .iter()
        .find(|report| report.query_id == query_id)
        .ok_or_else(|| format!("missing report {query_id}").into())
}

fn summarize_report(report: &SemanticAuthoringReport) -> serde_json::Value {
    json!({
        "query_id": report.query_id,
        "operation": report.operation,
        "status": report.status,
        "code": report.code,
        "subject": report.subject.constellation_id,
        "hits": report.hits.iter().map(|hit| json!({
            "rank": hit.rank,
            "constellation_id": hit.constellation_id,
            "score": hit.score,
            "mean_cosine": hit.mean_cosine,
            "min_cosine": hit.min_cosine,
            "max_cosine": hit.max_cosine,
            "spread": hit.spread,
            "per_slot": hit.per_slot
        })).collect::<Vec<_>>(),
        "definition_slots": report.definition.as_ref().map(|definition| &definition.slots),
        "skipped_insufficient_overlap": report.skipped_insufficient_overlap
    })
}

fn known_vectors() -> serde_json::Value {
    json!({
        "unit_anchor": {"text": [10, 0], "structured": [10, 0]},
        "unit_agree": {"text": [9, 1], "structured": [9, 1]},
        "unit_contradictory": {"text": [10, 0], "structured": [0, 10]},
        "unit_unrelated": {"text": [0, 10], "structured": [0, 10]}
    })
}

fn path_state(
    output_path: &Path,
    state_path: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let aster_stats = dir_stats(aster_dir)?;
    Ok(json!({
        "output_exists": output_path.exists(),
        "state_exists": state_path.exists(),
        "aster_exists": aster_dir.exists(),
        "aster_file_count": aster_stats.0,
        "aster_file_bytes": aster_stats.1
    }))
}

fn dir_stats(path: &Path) -> Result<(usize, u64), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let mut stack = vec![path.to_path_buf()];
    let mut count = 0;
    let mut bytes = 0;
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                count += 1;
                bytes += metadata.len();
            }
        }
    }
    Ok((count, bytes))
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    document["after_artifact_write"] = json!({"output_path": path});
    let bytes = serde_json::to_vec_pretty(&document)?;
    fs::write(path, &bytes)?;
    let readback = fs::read(path)?;
    if readback != bytes {
        return Err("manual FSV artifact bytes differed after readback".into());
    }
    Ok(())
}
