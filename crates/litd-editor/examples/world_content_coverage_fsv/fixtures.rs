use std::collections::BTreeMap;

use litd_editor::{
    world_content_coverage::{WorldContentCoverageSpec, WorldCoverageBucketSpec},
    world_memory::{
        WorldMemoryConstellation, WorldMemoryFeatureVector, WorldMemoryIndex, WorldMemoryLens,
        WorldMemoryLensKind, WorldMemorySlotKind, WorldMemorySourceKind, typed_slot,
    },
};

pub(crate) fn coverage_spec(analysis_id: &str) -> WorldContentCoverageSpec {
    WorldContentCoverageSpec {
        analysis_id: analysis_id.to_string(),
        source_index_id: "issue1202-world-memory".to_string(),
        requested_by: "issue1202-fsv".to_string(),
        buckets: vec![
            bucket_spec(
                "mid_water_place",
                WorldMemorySourceKind::Place,
                "water mid",
                1,
                None,
                1,
                None,
            ),
            bucket_spec(
                "sword_item",
                WorldMemorySourceKind::Item,
                "sword",
                0,
                Some(1),
                0,
                None,
            ),
        ],
    }
}

pub(crate) fn single_item_spec() -> WorldContentCoverageSpec {
    WorldContentCoverageSpec {
        analysis_id: "issue1202-fsv-single".to_string(),
        source_index_id: "issue1202-world-memory".to_string(),
        requested_by: "issue1202-fsv".to_string(),
        buckets: vec![bucket_spec(
            "mid_water_place",
            WorldMemorySourceKind::Place,
            "water mid",
            1,
            Some(1),
            1000,
            Some(1000),
        )],
    }
}

pub(crate) fn threshold_boundary_spec() -> WorldContentCoverageSpec {
    WorldContentCoverageSpec {
        analysis_id: "issue1202-fsv-boundary".to_string(),
        source_index_id: "issue1202-world-memory".to_string(),
        requested_by: "issue1202-fsv".to_string(),
        buckets: vec![bucket_spec(
            "sword_item",
            WorldMemorySourceKind::Item,
            "sword",
            1,
            Some(1),
            500,
            Some(500),
        )],
    }
}

pub(crate) fn problem_index() -> WorldMemoryIndex {
    index(vec![
        content(
            WorldMemorySourceKind::Place,
            "forest",
            "forest zone",
            "biome forest tier low",
            [10, 1],
            [9, 1],
        ),
        content(
            WorldMemorySourceKind::Place,
            "water-rumor",
            "water mid grotto",
            "biome desert tier high",
            [1, 10],
            [10, 0],
        ),
        sword("sword-1"),
        sword("sword-2"),
        sword("sword-3"),
    ])
}

pub(crate) fn balanced_index() -> WorldMemoryIndex {
    index(vec![
        content(
            WorldMemorySourceKind::Place,
            "water-mid",
            "water mid cove",
            "biome water tier mid",
            [1, 10],
            [1, 9],
        ),
        sword("sword-1"),
    ])
}

pub(crate) fn single_item_index() -> WorldMemoryIndex {
    index(vec![content(
        WorldMemorySourceKind::Place,
        "water-mid",
        "water mid cove",
        "biome water tier mid",
        [1, 10],
        [1, 9],
    )])
}

pub(crate) fn threshold_boundary_index() -> WorldMemoryIndex {
    index(vec![
        sword("sword-1"),
        content(
            WorldMemorySourceKind::Item,
            "shield",
            "iron shield",
            "category armor shield",
            [4, 2],
            [4, 2],
        ),
    ])
}

fn bucket_spec(
    id: &str,
    source_kind: WorldMemorySourceKind,
    terms: &str,
    min_count: usize,
    max_count: Option<usize>,
    min_share_milli: u16,
    max_share_milli: Option<u16>,
) -> WorldCoverageBucketSpec {
    WorldCoverageBucketSpec {
        bucket_id: id.to_string(),
        label: id.replace('_', " "),
        lens: WorldMemoryLensKind::Structured,
        source_kinds: vec![source_kind],
        required_terms: terms.split_whitespace().map(str::to_string).collect(),
        min_count,
        max_count,
        min_share_milli,
        max_share_milli,
    }
}

fn sword(id: &str) -> WorldMemoryConstellation {
    content(
        WorldMemorySourceKind::Item,
        id,
        "iron sword",
        "category weapon sword",
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
        title: id.to_string(),
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
