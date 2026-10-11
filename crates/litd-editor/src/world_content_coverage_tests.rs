use std::{collections::BTreeMap, fs};

use crate::world_memory::{
    WorldMemoryConstellation, WorldMemoryFeatureVector, WorldMemoryIndex, WorldMemoryLens,
    WorldMemoryLensKind, WorldMemorySlotKind, WorldMemorySourceKind, typed_slot,
};

use super::*;

#[test]
fn reports_blind_spot_overrepresentation_and_lens_mismatch() {
    let index = problem_index();
    let report = analyze_world_content_coverage(
        "issue1202 known world-memory coverage fixture",
        &index,
        &coverage_spec("issue1202-problem"),
    )
    .unwrap();

    assert_eq!(report.status, WorldCoverageStatus::Findings);
    assert_eq!(report.abundance.n_lenses, 2);
    assert_eq!(report.abundance.n_constellations, 8);
    assert_eq!(report.measured_count, 16);
    assert_eq!(report.materialized_count, 8);
    assert_eq!(report.agreement_edges.len(), 1);
    assert_eq!(report.agreement_edges[0].n, 8);
    let mid_water = bucket(&report, "mid_water_place");
    assert_eq!(mid_water.eligible_count, 3);
    assert_eq!(mid_water.match_count, 0);
    assert_eq!(mid_water.status, WorldCoverageBucketStatus::BlindSpot);
    let sword = bucket(&report, "sword_item");
    assert_eq!(sword.eligible_count, 5);
    assert_eq!(sword.match_count, 4);
    assert_eq!(sword.share_milli, 800);
    assert_eq!(sword.status, WorldCoverageBucketStatus::Overrepresented);
    assert!(report.findings.iter().any(|finding| {
        finding.kind == WorldCoverageFindingKind::BlindSpot
            && finding.bucket_id == "mid_water_place"
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.kind == WorldCoverageFindingKind::Overrepresented
            && finding.bucket_id == "sword_item"
    }));
    assert_eq!(report.lens_blind_spots.len(), 1);
    assert_eq!(
        report.lens_blind_spots[0].constellation_id,
        "issue1202-world::Place::water-mid-rumor"
    );
    assert_eq!(report.lens_blind_spots[0].delta, 1.0);
    println!(
        "issue1202 problem findings={} blind_spots={} mid_water={} sword_share={} materialized={}",
        report.findings.len(),
        report.lens_blind_spots.len(),
        mid_water.match_count,
        sword.share_milli,
        report.materialized_count
    );
}

#[test]
fn balanced_single_item_and_threshold_boundary_report_no_findings() {
    let balanced = analyze_world_content_coverage(
        "issue1202 balanced fixture",
        &balanced_index(),
        &coverage_spec("issue1202-balanced"),
    )
    .unwrap();
    assert_eq!(balanced.status, WorldCoverageStatus::Balanced);
    assert!(balanced.findings.is_empty());
    assert!(balanced.lens_blind_spots.is_empty());

    let single = analyze_world_content_coverage(
        "issue1202 single item fixture",
        &single_item_index(),
        &single_item_spec(),
    )
    .unwrap();
    assert_eq!(single.status, WorldCoverageStatus::Balanced);
    assert_eq!(single.abundance.n_constellations, 1);
    assert_eq!(single.measured_count, 2);
    assert_eq!(single.materialized_count, 1);
    assert!(single.findings.is_empty());

    let boundary = analyze_world_content_coverage(
        "issue1202 threshold boundary fixture",
        &threshold_boundary_index(),
        &threshold_boundary_spec(),
    )
    .unwrap();
    assert_eq!(boundary.status, WorldCoverageStatus::Balanced);
    assert_eq!(bucket(&boundary, "sword_item").share_milli, 500);
    assert!(boundary.findings.is_empty());
    println!(
        "issue1202 edges balanced_findings={} single_materialized={} boundary_share={}",
        balanced.findings.len(),
        single.materialized_count,
        bucket(&boundary, "sword_item").share_milli
    );
}

#[test]
fn invalid_specs_and_vectors_fail_closed() {
    let mut spec = coverage_spec("issue1202-invalid");
    spec.buckets.clear();
    let err = analyze_world_content_coverage("issue1202 invalid spec", &problem_index(), &spec)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("editor.world_content_coverage.spec")
    );

    let mut bad_threshold = coverage_spec("issue1202-invalid-threshold");
    bad_threshold.buckets[0].max_count = Some(0);
    let err = analyze_world_content_coverage(
        "issue1202 invalid threshold",
        &problem_index(),
        &bad_threshold,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("editor.world_content_coverage.spec.bucket")
    );

    let mut zero = problem_index();
    zero.constellations[0].slots[2].vectors[0].values = vec![0, 0];
    let err =
        analyze_world_content_coverage("issue1202 zero vector", &zero, &coverage_spec("zero"))
            .unwrap_err();
    assert!(
        err.to_string()
            .contains("editor.world_content_coverage.index.vector.zero")
    );
    println!(
        "issue1202 invalids spec=editor.world_content_coverage.spec \
         threshold=editor.world_content_coverage.spec.bucket \
         zero=editor.world_content_coverage.index.vector.zero"
    );
}

#[test]
fn coverage_ledger_persists_to_aster_and_reads_back() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1202_world_content_coverage", "unit").unwrap();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("coverage-ledger.json");
    let aster_dir = root.join("aster");
    let report = analyze_world_content_coverage(
        "issue1202 persistence fixture",
        &problem_index(),
        &coverage_spec("issue1202-persist"),
    )
    .unwrap();
    let ledger = build_world_content_coverage_ledger(
        "issue1202-world-content-coverage-ledger",
        vec![report],
    )
    .unwrap();
    let writeback = write_world_content_coverage_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_world_content_coverage_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    assert_eq!(writeback.bytes, readback.bytes);
    assert_eq!(writeback.sha256, readback.sha256);
    assert_eq!(readback.graph_rows, 2);
    assert_eq!(readback.report_count, 1);
    assert_eq!(readback.finding_count, 3);
    assert!(!state_path.exists());
    let parsed = parse_world_content_coverage_ledger(&readback.bytes).unwrap();
    assert_eq!(parsed.reports[0].status, WorldCoverageStatus::Findings);
    println!(
        "issue1202 aster reports={} findings={} graph_rows={} sha={} state_bytes={}",
        readback.report_count,
        readback.finding_count,
        readback.graph_rows,
        readback.sha256,
        readback.bytes.len()
    );
}

fn bucket<'a>(
    report: &'a WorldContentCoverageReport,
    bucket_id: &str,
) -> &'a WorldCoverageBucketReport {
    report
        .bucket_reports
        .iter()
        .find(|bucket| bucket.bucket_id == bucket_id)
        .unwrap()
}

fn coverage_spec(analysis_id: &str) -> WorldContentCoverageSpec {
    WorldContentCoverageSpec {
        analysis_id: analysis_id.to_string(),
        source_index_id: "issue1202-world-memory".to_string(),
        requested_by: "issue1202-test".to_string(),
        buckets: vec![
            WorldCoverageBucketSpec {
                bucket_id: "mid_water_place".to_string(),
                label: "Mid-level water zones".to_string(),
                lens: WorldMemoryLensKind::Structured,
                source_kinds: vec![WorldMemorySourceKind::Place],
                required_terms: vec!["water".to_string(), "mid".to_string()],
                min_count: 1,
                max_count: None,
                min_share_milli: 1,
                max_share_milli: None,
            },
            WorldCoverageBucketSpec {
                bucket_id: "sword_item".to_string(),
                label: "Sword items".to_string(),
                lens: WorldMemoryLensKind::Structured,
                source_kinds: vec![WorldMemorySourceKind::Item],
                required_terms: vec!["sword".to_string()],
                min_count: 0,
                max_count: Some(2),
                min_share_milli: 0,
                max_share_milli: Some(500),
            },
        ],
    }
}

fn single_item_spec() -> WorldContentCoverageSpec {
    WorldContentCoverageSpec {
        analysis_id: "issue1202-single".to_string(),
        source_index_id: "issue1202-world-memory".to_string(),
        requested_by: "issue1202-test".to_string(),
        buckets: vec![WorldCoverageBucketSpec {
            bucket_id: "mid_water_place".to_string(),
            label: "Mid-level water zones".to_string(),
            lens: WorldMemoryLensKind::Structured,
            source_kinds: vec![WorldMemorySourceKind::Place],
            required_terms: vec!["water".to_string(), "mid".to_string()],
            min_count: 1,
            max_count: Some(1),
            min_share_milli: 1000,
            max_share_milli: Some(1000),
        }],
    }
}

fn threshold_boundary_spec() -> WorldContentCoverageSpec {
    WorldContentCoverageSpec {
        analysis_id: "issue1202-boundary".to_string(),
        source_index_id: "issue1202-world-memory".to_string(),
        requested_by: "issue1202-test".to_string(),
        buckets: vec![WorldCoverageBucketSpec {
            bucket_id: "sword_item".to_string(),
            label: "Sword items".to_string(),
            lens: WorldMemoryLensKind::Structured,
            source_kinds: vec![WorldMemorySourceKind::Item],
            required_terms: vec!["sword".to_string()],
            min_count: 1,
            max_count: Some(1),
            min_share_milli: 500,
            max_share_milli: Some(500),
        }],
    }
}

fn problem_index() -> WorldMemoryIndex {
    index(vec![
        content(
            WorldMemorySourceKind::Place,
            "forest-low",
            "Low Forest",
            "low forest zone",
            "biome forest tier low zone",
            [10, 1],
            [9, 1],
        ),
        content(
            WorldMemorySourceKind::Place,
            "forest-mid",
            "Mid Forest",
            "mid forest zone",
            "biome forest tier mid zone",
            [9, 2],
            [8, 2],
        ),
        content(
            WorldMemorySourceKind::Place,
            "water-mid-rumor",
            "Water Rumor",
            "water mid hidden grotto",
            "biome desert tier high zone",
            [1, 10],
            [10, 0],
        ),
        sword("sword-1"),
        sword("sword-2"),
        sword("sword-3"),
        sword("sword-4"),
        content(
            WorldMemorySourceKind::Item,
            "shield-1",
            "Shield",
            "iron shield",
            "category armor shield tier low",
            [4, 2],
            [4, 2],
        ),
    ])
}

fn balanced_index() -> WorldMemoryIndex {
    index(vec![
        content(
            WorldMemorySourceKind::Place,
            "water-mid",
            "Mid Water",
            "water mid cove",
            "biome water tier mid zone",
            [1, 10],
            [1, 9],
        ),
        content(
            WorldMemorySourceKind::Item,
            "sword-1",
            "Sword",
            "iron sword",
            "category weapon sword tier low",
            [5, 0],
            [5, 0],
        ),
        content(
            WorldMemorySourceKind::Item,
            "shield-1",
            "Shield",
            "iron shield",
            "category armor shield tier low",
            [4, 2],
            [4, 2],
        ),
    ])
}

fn single_item_index() -> WorldMemoryIndex {
    index(vec![content(
        WorldMemorySourceKind::Place,
        "water-mid",
        "Mid Water",
        "water mid cove",
        "biome water tier mid zone",
        [1, 10],
        [1, 9],
    )])
}

fn threshold_boundary_index() -> WorldMemoryIndex {
    index(vec![
        content(
            WorldMemorySourceKind::Item,
            "sword-1",
            "Sword",
            "iron sword",
            "category weapon sword tier low",
            [5, 0],
            [5, 0],
        ),
        content(
            WorldMemorySourceKind::Item,
            "shield-1",
            "Shield",
            "iron shield",
            "category armor shield tier low",
            [4, 2],
            [4, 2],
        ),
    ])
}

fn sword(id: &str) -> WorldMemoryConstellation {
    content(
        WorldMemorySourceKind::Item,
        id,
        "Sword",
        "iron sword",
        "category weapon sword tier low",
        [5, 0],
        [5, 0],
    )
}

fn index(constellations: Vec<WorldMemoryConstellation>) -> WorldMemoryIndex {
    WorldMemoryIndex {
        schema: "litd.authoring.world_memory.v2.no_flatten_slots".to_string(),
        source_of_truth: "issue1202 hand-authored world-memory coverage fixture".to_string(),
        world_count: 1,
        constellation_count: constellations.len(),
        constellations,
        conflicts: Vec::new(),
    }
}

fn content(
    kind: WorldMemorySourceKind,
    id: &str,
    title: &str,
    text: &str,
    structured: &str,
    text_vector: [i32; 2],
    structured_vector: [i32; 2],
) -> WorldMemoryConstellation {
    let text_lens = lens(text);
    let structured_lens = lens(structured);
    let slots = vec![
        typed_slot(
            WorldMemorySlotKind::Text,
            id,
            Some(&text_lens),
            Vec::new(),
            [],
        ),
        typed_slot(
            WorldMemorySlotKind::Structured,
            id,
            Some(&structured_lens),
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
            [("vector_source", "issue1202_hand_vectors".to_string())],
        ),
    ];
    let mut lenses = BTreeMap::new();
    lenses.insert(WorldMemoryLensKind::Text, text_lens);
    lenses.insert(WorldMemoryLensKind::Structured, structured_lens);
    WorldMemoryConstellation {
        constellation_id: format!("issue1202-world::{kind:?}::{id}"),
        world_id: "issue1202-world".to_string(),
        source_kind: kind,
        source_id: id.to_string(),
        title: title.to_string(),
        source_fingerprint: format!("fingerprint-{id}"),
        provenance_refs: vec![format!("Aster://issue1202/{id}")],
        lenses,
        slots,
        reuse_prompt: None,
    }
}

fn lens(text: &str) -> WorldMemoryLens {
    let mut terms = BTreeMap::new();
    for token in text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        *terms.entry(token.to_ascii_lowercase()).or_insert(0) += 1;
    }
    WorldMemoryLens {
        text: text.to_string(),
        terms,
    }
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
