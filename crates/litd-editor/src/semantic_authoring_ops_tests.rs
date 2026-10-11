use std::{collections::BTreeMap, fs};

use calyx_core::CxId;

use super::*;
use crate::world_memory::{
    WorldMemoryConstellation, WorldMemoryFeatureVector, WorldMemoryIndex, WorldMemoryLens,
    WorldMemorySlotKind, typed_slot,
};

#[test]
fn define_agree_disagree_and_neighbors_match_known_vectors() {
    let index = fixture_index();
    let neighbors = run_semantic_authoring_action(
        "issue1203 fixture world-memory index",
        &index,
        &request(
            "issue1203-neighbors",
            SemanticAuthoringOperation::Neighbors,
            "unit_anchor",
            Some(WorldMemoryLensKind::Text),
            Vec::new(),
            3,
        ),
    )
    .unwrap();
    let define = run_semantic_authoring_action(
        "issue1203 fixture world-memory index",
        &index,
        &request(
            "issue1203-define",
            SemanticAuthoringOperation::Define,
            "unit_anchor",
            Some(WorldMemoryLensKind::Text),
            Vec::new(),
            3,
        ),
    )
    .unwrap();
    let agree = run_semantic_authoring_action(
        "issue1203 fixture world-memory index",
        &index,
        &request(
            "issue1203-agree",
            SemanticAuthoringOperation::Agree,
            "unit_anchor",
            None,
            Vec::new(),
            3,
        ),
    )
    .unwrap();
    let disagree = run_semantic_authoring_action(
        "issue1203 fixture world-memory index",
        &index,
        &request(
            "issue1203-disagree",
            SemanticAuthoringOperation::Disagree,
            "unit_anchor",
            None,
            Vec::new(),
            3,
        ),
    )
    .unwrap();

    assert_eq!(neighbors.hits[0].constellation_id, "unit_contradictory");
    assert_eq!(define.definition.as_ref().unwrap().slots.len(), 2);
    assert_eq!(agree.hits[0].constellation_id, "unit_agree");
    assert!(agree.hits[0].score > 0.99);
    assert_eq!(disagree.hits[0].constellation_id, "unit_contradictory");
    assert_eq!(disagree.hits[0].score, 1.0);
    println!(
        "issue1203 happy neighbors_top={} agree_top={} agree_score={:.3} disagree_top={} disagree_score={:.3} define_slots={}",
        neighbors.hits[0].constellation_id,
        agree.hits[0].constellation_id,
        agree.hits[0].score,
        disagree.hits[0].constellation_id,
        disagree.hits[0].score,
        define.definition.as_ref().unwrap().slots.len()
    );
}

#[test]
fn no_neighbors_contradictory_request_and_ood_subject_are_explicit() {
    let no_neighbors = run_semantic_authoring_action(
        "issue1203 single-row world-memory index",
        &single_item_index(),
        &request(
            "issue1203-no-neighbors",
            SemanticAuthoringOperation::Neighbors,
            "unit_solo",
            Some(WorldMemoryLensKind::Text),
            Vec::new(),
            3,
        ),
    )
    .unwrap();
    assert_eq!(no_neighbors.status, SemanticAuthoringStatus::NoNeighbors);
    assert_eq!(no_neighbors.code, "LITD-SEMANTIC-NO-NEIGHBORS");
    assert!(no_neighbors.hits.is_empty());

    let contradictory = run_semantic_authoring_action(
        "issue1203 fixture world-memory index",
        &fixture_index(),
        &request(
            "issue1203-contradictory",
            SemanticAuthoringOperation::Agree,
            "unit_anchor",
            Some(WorldMemoryLensKind::Text),
            Vec::new(),
            3,
        ),
    )
    .unwrap_err();
    assert!(
        contradictory
            .to_string()
            .contains("editor.semantic_authoring.request.contradictory")
    );

    let ood = run_semantic_authoring_action(
        "issue1203 fixture world-memory index",
        &fixture_index(),
        &request(
            "issue1203-ood",
            SemanticAuthoringOperation::Neighbors,
            "unit_missing",
            Some(WorldMemoryLensKind::Text),
            Vec::new(),
            3,
        ),
    )
    .unwrap_err();
    assert!(
        ood.to_string()
            .contains("editor.semantic_authoring.subject")
    );
    println!(
        "issue1203 edges no_neighbors={} contradictory={} ood={}",
        no_neighbors.code, contradictory, ood
    );
}

#[test]
fn ledger_persists_to_aster_and_reads_back() {
    let index = fixture_index();
    let reports = vec![
        run_semantic_authoring_action(
            "issue1203 fixture world-memory index",
            &index,
            &request(
                "issue1203-ledger-neighbors",
                SemanticAuthoringOperation::Neighbors,
                "unit_anchor",
                Some(WorldMemoryLensKind::Text),
                Vec::new(),
                3,
            ),
        )
        .unwrap(),
        run_semantic_authoring_action(
            "issue1203 fixture world-memory index",
            &index,
            &request(
                "issue1203-ledger-agree",
                SemanticAuthoringOperation::Agree,
                "unit_anchor",
                None,
                Vec::new(),
                3,
            ),
        )
        .unwrap(),
    ];
    let ledger =
        build_semantic_authoring_ops_ledger("issue1203-ledger-test", "issue1203-index", reports)
            .unwrap();
    let root = test_root("semantic-authoring");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("semantic-authoring-ledger.json");
    let aster_dir = root.join("aster");

    let writeback = write_semantic_authoring_ops_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_semantic_authoring_ops_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_semantic_authoring_ops_ledger(&readback.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(readback.bytes, writeback.bytes);
    assert_eq!(parsed.report_count, 2);
    assert_eq!(
        parsed.reports[0].hits[0].constellation_id,
        "unit_contradictory"
    );
    assert_eq!(parsed.reports[1].hits[0].constellation_id, "unit_agree");
    println!(
        "issue1203 aster reports={} graph_rows={} sha={} state_bytes={}",
        parsed.report_count,
        readback.graph_rows,
        readback.sha256,
        readback.bytes.len()
    );
}

#[test]
fn structured_calyx_authoring_ops_are_exported_from_editor_surface() {
    let request = SemanticAuthoringRequest {
        query_id: "issue1414-editor-surface".to_string(),
        operation: SemanticAuthoringOperation::Neighbors,
        subject_constellation_id: CxId::from_bytes([0x14; 16]).to_string(),
        lens: Some(WorldMemoryLensKind::Text),
        slot_filter: Vec::new(),
        k: 3,
        requested_by: "litd-editor-issue1414-test".to_string(),
    };

    assert_eq!(request.operation, SemanticAuthoringOperation::Neighbors);
    assert_eq!(request.lens, Some(WorldMemoryLensKind::Text));
    assert_eq!(request.k, 3);
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
    let mut lenses = BTreeMap::new();
    lenses.insert(
        WorldMemoryLensKind::Text,
        lens(format!("{title} text archetype")),
    );
    lenses.insert(
        WorldMemoryLensKind::Structured,
        lens(format!("{title} structured stats")),
    );
    let text = lenses.get(&WorldMemoryLensKind::Text).unwrap();
    let structured = lenses.get(&WorldMemoryLensKind::Structured).unwrap();
    let slots = vec![
        typed_slot(WorldMemorySlotKind::Text, id, Some(text), Vec::new(), []),
        typed_slot(
            WorldMemorySlotKind::Structured,
            id,
            Some(structured),
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

fn request(
    query_id: &str,
    operation: SemanticAuthoringOperation,
    subject_constellation_id: &str,
    lens: Option<WorldMemoryLensKind>,
    slot_filter: Vec<WorldMemoryLensKind>,
    k: usize,
) -> SemanticAuthoringRequest {
    SemanticAuthoringRequest {
        query_id: query_id.to_string(),
        operation,
        subject_constellation_id: subject_constellation_id.to_string(),
        lens,
        slot_filter,
        k,
        requested_by: "issue1203-test".to_string(),
    }
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn test_root(name: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("editor-{name}"))
}
