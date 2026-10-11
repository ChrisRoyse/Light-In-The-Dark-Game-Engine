use std::{fs, io, path::Path};

use litd_engine::{
    calyx_asset_intelligence::{find_similar_calyx_assets, search_calyx_assets},
    calyx_asset_pipeline::{
        EngineAssetIngestRequest, EngineAssetMediaType, EngineCalyxAssetPipelineError,
        EngineCalyxAssetPipelineReadback, ingest_assets_into_calyx, read_calyx_asset_pipeline,
        validate_pipeline,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{query_engine_calyx_semantic_index, read_engine_calyx_semantic_index},
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1122_calyx_asset_pipeline")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let requests = asset_requests();

    let stored = ingest_assets_into_calyx(&database, "issue1122-assets", &requests)?;
    let readback = read_calyx_asset_pipeline(&database, "issue1122-assets")?;
    let semantic =
        read_engine_calyx_semantic_index(&database, "issue1122-assets/asset-semantic-index")?;
    let semantic_query =
        query_engine_calyx_semantic_index(&semantic.index, "fire explosion vfx", 2)?;
    let asset_search = search_calyx_assets(&database, "issue1122-assets", "ice ward texture", 3)?;
    let similar = find_similar_calyx_assets(&database, "issue1122-assets", "asset-fire-vfx", 2)?;
    let edges = edge_readbacks(&database, &readback.pipeline)?;
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let dedup_group = readback
        .pipeline
        .dedup_groups
        .first()
        .ok_or_else(|| io::Error::other("expected issue1122 dedup group"))?;
    let semantic_top = semantic_query
        .hits
        .first()
        .ok_or_else(|| io::Error::other("expected semantic asset hit"))?;
    let search_top = asset_search
        .hits
        .first()
        .ok_or_else(|| io::Error::other("expected editor asset search hit"))?;
    let similar_top = similar
        .hits
        .first()
        .ok_or_else(|| io::Error::other("expected similar asset hit"))?;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let pipeline_acceptance = stored.pipeline.pipeline_fingerprint
        == readback.pipeline.pipeline_fingerprint
        && stored.database.sha256 == readback.database.sha256
        && readback.pipeline.asset_count == 3
        && readback.pipeline.deduplicated_count == 1
        && readback.pipeline.dedup_groups.len() == 1
        && dedup_group.canonical_asset_id == "asset-fire-vfx"
        && dedup_group
            .duplicate_asset_ids
            .contains(&"asset-fire-vfx-copy".to_string())
        && !readback.pipeline.semantic_index_fingerprint.is_empty();
    let intelligence_acceptance = semantic_top.id == "asset-fire-vfx"
        && !semantic_top.provenance_refs.is_empty()
        && search_top.asset_id == "asset-ice-texture"
        && !search_top.provenance_refs.is_empty()
        && similar.source_asset_id.as_deref() == Some("asset-fire-vfx")
        && similar_top.asset_id == "asset-fire-vfx-copy"
        && similar_top.match_reason == "dedup+semantic"
        && similar_top.canonical_asset_id == "asset-fire-vfx";

    ensure(pipeline_acceptance, "asset pipeline acceptance failed")?;
    ensure(
        intelligence_acceptance,
        "asset intelligence acceptance failed",
    )?;
    ensure(
        all_edges_matched,
        "asset pipeline edge readback acceptance failed",
    )?;

    let report = json!({
        "schema": "litd.engine.issue1122.calyx_asset_pipeline_fsv.v1",
        "schema_version": 1,
        "issue": 1122,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, Aster vault rows, pipeline ledger, semantic index, editor search/similarity readbacks, provenance refs, and hashes"
        },
        "happy_path": {
            "pipeline": readback.pipeline,
            "pipeline_database": readback.database,
            "semantic_database": semantic.database,
            "semantic_query": semantic_query,
            "asset_search": asset_search,
            "similar_assets": similar,
            "vault_files": vault_files
        },
        "acceptance": {
            "pipeline_acceptance": pipeline_acceptance,
            "asset_count": readback.pipeline.asset_count,
            "deduplicated_count": readback.pipeline.deduplicated_count,
            "dedup_group_count": readback.pipeline.dedup_groups.len(),
            "canonical_asset_id": dedup_group.canonical_asset_id,
            "duplicate_asset_ids": dedup_group.duplicate_asset_ids,
            "pipeline_fingerprint": readback.pipeline.pipeline_fingerprint,
            "pipeline_database_sha256": readback.database.sha256,
            "semantic_index_id": readback.pipeline.semantic_index_id,
            "semantic_index_fingerprint": readback.pipeline.semantic_index_fingerprint,
            "semantic_top_hit": semantic_top.key,
            "asset_search_top": search_top.asset_id,
            "asset_search_fingerprint": asset_search.fused_result_fingerprint,
            "similar_source_asset": similar.source_asset_id,
            "similar_top_asset": similar_top.asset_id,
            "similar_match_reason": similar_top.match_reason,
            "similar_fingerprint": similar.fused_result_fingerprint,
            "intelligence_acceptance": intelligence_acceptance,
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let readback_report = run.write_report(&report)?;
    println!(
        "ISSUE1122_CALYX_ASSET_PIPELINE_FSV report={} sha256={} manifest={} run_id={} assets={} deduped={} top={} similar={} edges={} pipeline_fingerprint={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.latest_manifest_path.display(),
        run.run_id(),
        readback.pipeline.asset_count,
        readback.pipeline.deduplicated_count,
        semantic_top.key,
        similar_top.asset_id,
        edges.len(),
        readback.pipeline.pipeline_fingerprint
    );
    Ok(())
}

fn asset_requests() -> Vec<EngineAssetIngestRequest> {
    vec![
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
    ]
}

fn edge_readbacks(
    database: &EngineCalyxDatabase,
    pipeline: &EngineCalyxAssetPipelineReadback,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut missing_provenance = asset(
        "asset-missing-provenance",
        "Missing Provenance",
        EngineAssetMediaType::Mesh,
        "import://mesh/missing.glb",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        &["mesh"],
    );
    missing_provenance.provenance_refs.clear();
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
    let mut bad_counts = pipeline.clone();
    bad_counts.asset_count += 1;
    let mut bad_fingerprint = pipeline.clone();
    bad_fingerprint.pipeline_fingerprint = "00000000".to_string();

    Ok(vec![
        expect_invalid_input(
            "empty_pipeline_id_rejected",
            ingest_assets_into_calyx(database, "", &asset_requests()),
            "pipeline_id",
            "non-empty",
        ),
        expect_invalid_input(
            "empty_requests_rejected",
            ingest_assets_into_calyx(database, "empty", &[]),
            "requests",
            "at least one",
        ),
        expect_invalid_input(
            "missing_provenance_rejected",
            ingest_assets_into_calyx(database, "missing-provenance", &[missing_provenance]),
            "list",
            "provenance",
        ),
        expect_invalid_input(
            "duplicate_asset_id_rejected",
            ingest_assets_into_calyx(database, "duplicate-id", &[duplicate_a, duplicate_b]),
            "asset_id",
            "unique",
        ),
        expect_invalid_input(
            "missing_similar_asset_rejected",
            find_similar_calyx_assets(database, "issue1122-assets", "missing-asset", 1),
            "asset_id",
            "missing-asset",
        ),
        expect_invalid_input(
            "zero_search_limit_rejected",
            search_calyx_assets(database, "issue1122-assets", "fire", 0),
            "limit",
            "> 0",
        ),
        expect_invalid_input(
            "zero_similarity_limit_rejected",
            find_similar_calyx_assets(database, "issue1122-assets", "asset-fire-vfx", 0),
            "limit",
            "> 0",
        ),
        expect_invalid_input(
            "pipeline_count_tamper_rejected",
            validate_pipeline(&bad_counts),
            "counts",
            "must match",
        ),
        expect_invalid_input(
            "pipeline_fingerprint_tamper_rejected",
            validate_pipeline(&bad_fingerprint),
            "fingerprint",
            "valid",
        ),
        missing_pipeline_edge(database),
    ])
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

fn expect_invalid_input<T>(
    label: &str,
    result: Result<T, EngineCalyxAssetPipelineError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineCalyxAssetPipelineError::InvalidInput { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "field": field,
            "reason": reason,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
        }),
    }
}

fn missing_pipeline_edge(database: &EngineCalyxDatabase) -> Value {
    match read_calyx_asset_pipeline(database, "missing-asset-pipeline") {
        Err(EngineCalyxAssetPipelineError::Persistence(error)) => json!({
            "label": "missing_persisted_pipeline_rejected",
            "matched": error.to_string().contains("missing-asset-pipeline"),
            "error": error.to_string(),
        }),
        Err(error) => json!({
            "label": "missing_persisted_pipeline_rejected",
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": "missing_persisted_pipeline_rejected",
            "matched": false,
            "unexpected": "read succeeded",
        }),
    }
}

fn vault_file_readbacks(vault_dir: &Path) -> io::Result<Vec<Value>> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(vault_dir)? {
        collect_file_readbacks(&entry?.path(), &mut rows)?;
    }
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(rows)
}

fn collect_file_readbacks(path: &Path, rows: &mut Vec<Value>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_file_readbacks(&entry?.path(), rows)?;
        }
    } else {
        let bytes = fs::read(path)?;
        rows.push(json!({
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": litd_hash::sha256_hex(&bytes),
        }));
    }
    Ok(())
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
