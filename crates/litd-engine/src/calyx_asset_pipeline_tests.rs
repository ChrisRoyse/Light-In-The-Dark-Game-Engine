use crate::{
    calyx_asset_intelligence::{find_similar_calyx_assets, search_calyx_assets},
    calyx_asset_pipeline::{
        EngineAssetIngestRequest, EngineAssetMediaType, EngineCalyxAssetPipelineError,
        ingest_assets_into_calyx, read_calyx_asset_pipeline,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{query_engine_calyx_semantic_index, read_engine_calyx_semantic_index},
};

#[test]
fn calyx_asset_pipeline_ingests_dedups_and_indexes_assets() {
    let (database, vault_dir) = database("happy");
    let requests = vec![
        asset(
            "asset-fire-vfx",
            "Fire Explosion VFX",
            EngineAssetMediaType::Vfx,
            "generated://vfx/fire-explosion",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &["fire", "explosion", "vfx"],
        ),
        asset(
            "asset-fire-vfx-copy",
            "Fire Explosion VFX Copy",
            EngineAssetMediaType::Vfx,
            "generated://vfx/fire-explosion-copy",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &["fire", "explosion", "duplicate"],
        ),
        asset(
            "asset-ice-texture",
            "Ice Ward Texture",
            EngineAssetMediaType::Texture,
            "import://textures/ice-ward.png",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &["ice", "ward", "texture"],
        ),
    ];

    let stored = ingest_assets_into_calyx(&database, "issue1122-assets", &requests).unwrap();
    let readback = read_calyx_asset_pipeline(&database, "issue1122-assets").unwrap();
    let semantic =
        read_engine_calyx_semantic_index(&database, "issue1122-assets/asset-semantic-index")
            .unwrap();
    let query =
        query_engine_calyx_semantic_index(&semantic.index, "fire explosion vfx", 2).unwrap();
    let asset_search =
        search_calyx_assets(&database, "issue1122-assets", "ice ward texture", 3).unwrap();
    let similar =
        find_similar_calyx_assets(&database, "issue1122-assets", "asset-fire-vfx", 2).unwrap();

    assert_eq!(stored.pipeline.asset_count, 3);
    assert_eq!(stored.pipeline.deduplicated_count, 1);
    assert_eq!(stored.pipeline.dedup_groups.len(), 1);
    assert_eq!(
        stored.pipeline.dedup_groups[0].canonical_asset_id,
        "asset-fire-vfx"
    );
    assert!(
        stored.pipeline.dedup_groups[0]
            .duplicate_asset_ids
            .contains(&"asset-fire-vfx-copy".to_string())
    );
    assert_eq!(
        readback.pipeline.pipeline_fingerprint,
        stored.pipeline.pipeline_fingerprint
    );
    assert_eq!(query.hits[0].id, "asset-fire-vfx");
    assert!(!query.hits[0].provenance_refs.is_empty());
    assert_eq!(asset_search.hits[0].asset_id, "asset-ice-texture");
    assert_eq!(similar.source_asset_id.as_deref(), Some("asset-fire-vfx"));
    assert_eq!(similar.hits[0].asset_id, "asset-fire-vfx-copy");
    assert_eq!(similar.hits[0].match_reason, "dedup+semantic");
    assert_eq!(similar.hits[0].canonical_asset_id, "asset-fire-vfx");

    println!(
        "issue1122_asset_pipeline assets={} deduped={} top={} similar={} pipeline_sha={} vault={}",
        readback.pipeline.asset_count,
        readback.pipeline.deduplicated_count,
        query.hits[0].key,
        similar.hits[0].asset_id,
        readback.database.sha256,
        vault_dir.display()
    );
}

#[test]
fn calyx_asset_pipeline_invalid_inputs_fail_loud() {
    let (database, _vault_dir) = database("invalid");
    match ingest_assets_into_calyx(&database, "empty", &[]) {
        Err(EngineCalyxAssetPipelineError::InvalidInput { field, reason }) => {
            assert_eq!(field, "requests");
            assert!(reason.contains("at least one"));
        }
        other => panic!("expected empty request rejection, got {other:?}"),
    }

    let mut missing_provenance = asset(
        "asset-missing-provenance",
        "Missing Provenance",
        EngineAssetMediaType::Mesh,
        "import://mesh/missing.glb",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        &["mesh"],
    );
    missing_provenance.provenance_refs.clear();
    match ingest_assets_into_calyx(&database, "missing-provenance", &[missing_provenance]) {
        Err(EngineCalyxAssetPipelineError::InvalidInput { field, reason }) => {
            assert_eq!(field, "list");
            assert!(reason.contains("provenance"));
        }
        other => panic!("expected provenance rejection, got {other:?}"),
    }

    let duplicate_a = asset(
        "asset-duplicate-id",
        "Duplicate A",
        EngineAssetMediaType::Material,
        "generated://material/a",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        &["material"],
    );
    let duplicate_b = asset(
        "asset-duplicate-id",
        "Duplicate B",
        EngineAssetMediaType::Material,
        "generated://material/b",
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        &["material"],
    );
    match ingest_assets_into_calyx(&database, "duplicate-id", &[duplicate_a, duplicate_b]) {
        Err(EngineCalyxAssetPipelineError::InvalidInput { field, reason }) => {
            assert_eq!(field, "asset_id");
            assert!(reason.contains("unique"));
        }
        other => panic!("expected duplicate id rejection, got {other:?}"),
    }

    let valid = asset(
        "asset-valid",
        "Valid Asset",
        EngineAssetMediaType::Document,
        "generated://docs/valid",
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        &["document"],
    );
    ingest_assets_into_calyx(&database, "valid-pipeline", &[valid]).unwrap();
    match find_similar_calyx_assets(&database, "valid-pipeline", "missing-asset", 1) {
        Err(EngineCalyxAssetPipelineError::InvalidInput { field, reason }) => {
            assert_eq!(field, "asset_id");
            assert!(reason.contains("missing-asset"));
        }
        other => panic!("expected missing asset rejection, got {other:?}"),
    }
}

fn asset(
    asset_id: &str,
    display_name: &str,
    media_type: EngineAssetMediaType,
    source_uri: &str,
    bytes_sha256: &str,
    tags: &[&str],
) -> EngineAssetIngestRequest {
    EngineAssetIngestRequest {
        asset_id: asset_id.to_string(),
        display_name: display_name.to_string(),
        media_type,
        source_uri: source_uri.to_string(),
        bytes_sha256: bytes_sha256.to_string(),
        byte_len: 4096,
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        provenance_refs: vec![
            format!("provenance://issue1122/{asset_id}"),
            format!("source://issue1122/{asset_id}"),
        ],
    }
}

fn database(label: &str) -> (EngineCalyxDatabase, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    (
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap(),
        vault_dir,
    )
}

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1122-{label}"))
}
