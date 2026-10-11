use std::collections::BTreeSet;

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use serde_json::{Value, json};

use crate::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT, EngineCalyxSemanticError,
        build_engine_calyx_semantic_index, query_engine_calyx_semantic_index,
        read_engine_calyx_semantic_index, store_engine_calyx_semantic_index,
    },
};

#[test]
fn calyx_semantic_search_dedup_and_provenance_use_fifty_perspectives() {
    let (store, database, vault_dir) = content_store("happy");
    let readbacks = vec![
        store
            .put_content(&content(
                EngineContentKind::Ability,
                "ability-firestorm-alpha",
                "Firestorm Nuke",
                json!({
                    "ability_id": "firestorm-alpha",
                    "summary": "slow area fire nuke for siege control",
                    "damage_type": "fire",
                    "tags": ["slow", "area", "nuke", "burning", "siege"],
                    "damage_fp": 75
                }),
            ))
            .unwrap(),
        store
            .put_content(&content(
                EngineContentKind::Ability,
                "ability-firestorm-beta",
                "Firestorm Nuke",
                json!({
                    "ability_id": "firestorm-beta",
                    "summary": "slow area fire nuke for siege control",
                    "damage_type": "fire",
                    "tags": ["slow", "area", "nuke", "burning", "siege"],
                    "damage_fp": 75
                }),
            ))
            .unwrap(),
        store
            .put_content(&content(
                EngineContentKind::Unit,
                "unit-siege-mage",
                "Siege Mage",
                json!({
                    "unit_id": "siege-mage",
                    "role": "caster",
                    "tags": ["fire", "siege", "support"],
                    "hit_points_fp": 30
                }),
            ))
            .unwrap(),
        store
            .put_content(&content(
                EngineContentKind::Item,
                "item-frost-ward",
                "Frost Ward",
                json!({
                    "item_id": "frost-ward",
                    "summary": "defensive ice ward against burning",
                    "tags": ["ice", "defense", "ward"]
                }),
            ))
            .unwrap(),
    ];

    let index = build_engine_calyx_semantic_index("issue1119-semantic-index", &readbacks).unwrap();
    let persisted = store_engine_calyx_semantic_index(&database, &index).unwrap();
    let readback = read_engine_calyx_semantic_index(&database, "issue1119-semantic-index").unwrap();
    let query =
        query_engine_calyx_semantic_index(&readback.index, "slow area fire nuke", 3).unwrap();

    assert_eq!(persisted.index.index_fingerprint, index.index_fingerprint);
    assert_eq!(
        readback.index.perspective_count,
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
    );
    assert_eq!(
        query.perspective_roster.len(),
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
    );
    assert_eq!(
        query.per_perspective_weighted_terms.len(),
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
    );
    assert!(
        query
            .per_perspective_weighted_terms
            .values()
            .all(|weighted_terms| *weighted_terms > 0)
    );
    assert!(query.fused_result_fingerprint.len() >= 32);
    assert_eq!(query.hits[0].id, "ability-firestorm-alpha");
    assert!(query.hits[0].perspective_scores.len() >= 10);
    assert!(!query.hits[0].provenance_refs.is_empty());
    assert_eq!(
        query.hits[0].provenance_refs[0].source_path,
        "synthetic://issue1119/ability-firestorm-alpha"
    );
    assert_eq!(index.duplicate_groups.len(), 1);
    assert!(
        index.duplicate_groups[0]
            .record_keys
            .contains(&"ability/ability-firestorm-alpha".to_string())
    );
    assert!(
        index.duplicate_groups[0]
            .record_keys
            .contains(&"ability/ability-firestorm-beta".to_string())
    );

    let families = index
        .perspective_roster
        .iter()
        .map(|perspective| perspective.family)
        .collect::<BTreeSet<_>>();
    assert_eq!(families.len(), 10);

    println!(
        "issue1119_semantic perspectives={} families={} hits={} top={} dedup_groups={} index_sha={} vault={}",
        index.perspective_count,
        families.len(),
        query.hit_count,
        query.hits[0].key,
        index.duplicate_groups.len(),
        readback.database.sha256,
        vault_dir.display()
    );
}

#[test]
fn calyx_semantic_search_returns_grounded_zero_match_readback() {
    let (store, _database, _vault_dir) = content_store("zero");
    let readback = store
        .put_content(&content(
            EngineContentKind::Terrain,
            "terrain-ash-road",
            "Ash Road",
            json!({
                "terrain_id": "ash-road",
                "summary": "black volcanic road through ruins",
                "tags": ["road", "ash", "ruins"]
            }),
        ))
        .unwrap();
    let index = build_engine_calyx_semantic_index("issue1119-zero-index", &[readback]).unwrap();
    let query = query_engine_calyx_semantic_index(&index, "zzznomatch qqqtoken", 5).unwrap();

    assert_eq!(query.hit_count, 0);
    assert!(query.hits.is_empty());
    assert_eq!(
        query.perspective_roster.len(),
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
    );
    assert_eq!(
        query.per_perspective_weighted_terms.len(),
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
    );
    assert!(query.fused_result_fingerprint.len() >= 32);
}

#[test]
fn calyx_semantic_invalid_inputs_fail_loud() {
    let (store, _database, _vault_dir) = content_store("invalid");
    match build_engine_calyx_semantic_index("empty", &[]) {
        Err(EngineCalyxSemanticError::InvalidInput { field, reason }) => {
            assert_eq!(field, "readbacks");
            assert!(reason.contains("at least one"));
        }
        other => panic!("expected empty readback rejection, got {other:?}"),
    }

    let mut readback = store
        .put_content(&content(
            EngineContentKind::Story,
            "story-valid",
            "Opening Story",
            json!({"story_id": "opening", "summary": "dawn over the warded road"}),
        ))
        .unwrap();
    readback.source_path.clear();
    match build_engine_calyx_semantic_index("tampered", &[readback]) {
        Err(EngineCalyxSemanticError::InvalidInput { field, reason }) => {
            assert_eq!(field, "readback.source_path");
            assert!(reason.contains("non-empty"));
        }
        other => panic!("expected source path rejection, got {other:?}"),
    }

    let valid = store
        .put_content(&content(
            EngineContentKind::Lore,
            "lore-valid",
            "Ward Lore",
            json!({"lore_id": "ward", "summary": "ward stones remember fire"}),
        ))
        .unwrap();
    let index = build_engine_calyx_semantic_index("valid", &[valid]).unwrap();
    match query_engine_calyx_semantic_index(&index, " ", 5) {
        Err(EngineCalyxSemanticError::InvalidInput { field, reason }) => {
            assert_eq!(field, "query");
            assert!(reason.contains("searchable terms"));
        }
        other => panic!("expected query rejection, got {other:?}"),
    }
}

fn content_store(
    label: &str,
) -> (
    EngineCalyxContentStore,
    EngineCalyxDatabase,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database.clone(), label),
        database,
        vault_dir,
    )
}

fn content(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue1119/{id}"),
            author: "issue1119-test".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1119:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1119_semantic_search".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1119-{label}"))
}
