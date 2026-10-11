use std::{collections::BTreeSet, fs, io, path::Path};

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{
        ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT, EngineCalyxSemanticError,
        build_engine_calyx_semantic_index, query_engine_calyx_semantic_index,
        read_engine_calyx_semantic_index, store_engine_calyx_semantic_index,
        validate_semantic_index,
    },
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1119_calyx_semantic")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database.clone());

    let records = content_records();
    let readbacks = records
        .iter()
        .map(|record| store.put_content(record))
        .collect::<Result<Vec<_>, _>>()?;
    let index = build_engine_calyx_semantic_index("issue1119-semantic-index", &readbacks)?;
    let persisted = store_engine_calyx_semantic_index(&database, &index)?;
    let stored = read_engine_calyx_semantic_index(&database, "issue1119-semantic-index")?;
    let query = query_engine_calyx_semantic_index(&stored.index, "slow area fire nuke", 3)?;
    let zero_match = query_engine_calyx_semantic_index(&stored.index, "zzznomatch qqqtoken", 5)?;
    let edges = edge_readbacks(&database, &readbacks, &stored.index)?;
    let vault_files = vault_file_readbacks(&vault_dir)?;
    let families = stored
        .index
        .perspective_roster
        .iter()
        .map(|perspective| perspective.family)
        .collect::<BTreeSet<_>>();

    let top_hit = query.hits.first().ok_or_else(|| {
        io::Error::other("semantic query produced no hits for issue1119 happy path")
    })?;
    let duplicate_keys = stored
        .index
        .duplicate_groups
        .first()
        .map(|group| group.record_keys.clone())
        .unwrap_or_default();
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let persisted_index_matches = persisted.index.index_fingerprint == index.index_fingerprint
        && stored.index.index_fingerprint == index.index_fingerprint
        && stored.database.sha256 == persisted.database.sha256;
    let perspective_acceptance = stored.index.perspective_count
        == ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
        && query.perspective_roster.len() == ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
        && query.per_perspective_weighted_terms.len() == ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
        && query
            .per_perspective_weighted_terms
            .values()
            .all(|weighted_terms| *weighted_terms > 0)
        && families.len() == 10;
    let search_acceptance = query.hit_count == 3
        && top_hit.key == "ability/ability-firestorm-alpha"
        && top_hit.perspective_scores.len() >= 10
        && !top_hit.provenance_refs.is_empty()
        && top_hit.provenance_refs[0].source_path
            == "synthetic://issue1119/ability-firestorm-alpha"
        && query.fused_result_fingerprint.len() >= 32;
    let dedup_acceptance = stored.index.duplicate_groups.len() == 1
        && duplicate_keys.contains(&"ability/ability-firestorm-alpha".to_string())
        && duplicate_keys.contains(&"ability/ability-firestorm-beta".to_string());
    let zero_match_acceptance = zero_match.hit_count == 0
        && zero_match.hits.is_empty()
        && zero_match.per_perspective_weighted_terms.len()
            == ENGINE_CALYX_KEYWORD_PERSPECTIVE_COUNT
        && zero_match.fused_result_fingerprint.len() >= 32;

    ensure(
        persisted_index_matches,
        "persisted semantic index did not match",
    )?;
    ensure(
        perspective_acceptance,
        "semantic perspective acceptance failed",
    )?;
    ensure(search_acceptance, "semantic search acceptance failed")?;
    ensure(dedup_acceptance, "semantic dedup acceptance failed")?;
    ensure(
        zero_match_acceptance,
        "semantic zero-match acceptance failed",
    )?;
    ensure(
        all_edges_matched,
        "semantic edge readback acceptance failed",
    )?;

    let report = json!({
        "schema": "litd.engine.issue1119.calyx_semantic_fsv.v1",
        "schema_version": 1,
        "issue": 1119,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, Aster vault rows, semantic index, query readbacks, provenance refs, and hashes"
        },
        "happy_path": {
            "content_count": readbacks.len(),
            "content_readbacks": readbacks.iter().map(|readback| json!({
                "kind": readback.kind,
                "id": readback.id,
                "cx_id": readback.cx_id,
                "input_sha256": readback.input_sha256,
                "source_path": readback.source_path,
                "database_seq": readback.database.seq,
                "database_sha256": readback.database.sha256,
                "row_key": readback.database.row_key,
                "graph_rows": readback.database.graph_rows
            })).collect::<Vec<_>>(),
            "persisted_database": persisted.database,
            "stored_database": stored.database,
            "index": stored.index,
            "query": query,
            "zero_match_query": zero_match,
            "vault_files": vault_files
        },
        "acceptance": {
            "persisted_index_matches": persisted_index_matches,
            "perspective_count": stored.index.perspective_count,
            "family_count": families.len(),
            "per_perspective_weighted_terms_count": query.per_perspective_weighted_terms.len(),
            "all_per_perspective_weighted_terms_nonzero": query.per_perspective_weighted_terms.values().all(|weighted_terms| *weighted_terms > 0),
            "query_hit_count": query.hit_count,
            "top_hit_key": top_hit.key,
            "top_hit_perspective_score_count": top_hit.perspective_scores.len(),
            "top_hit_provenance_source": top_hit.provenance_refs[0].source_path,
            "fused_result_fingerprint": query.fused_result_fingerprint,
            "duplicate_group_count": stored.index.duplicate_groups.len(),
            "duplicate_keys": duplicate_keys,
            "zero_match_hit_count": zero_match.hit_count,
            "zero_match_fingerprint": zero_match.fused_result_fingerprint,
            "semantic_index_fingerprint": stored.index.index_fingerprint,
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1119_CALYX_SEMANTIC_FSV report={} sha256={} manifest={} run_id={} perspectives={} families={} hits={} top={} dedup_groups={} edges={} index_fingerprint={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        stored.index.perspective_count,
        families.len(),
        query.hit_count,
        top_hit.key,
        stored.index.duplicate_groups.len(),
        edges.len(),
        stored.index.index_fingerprint
    );
    Ok(())
}

fn content_records() -> Vec<EngineContentRecord> {
    vec![
        content(
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
        ),
        content(
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
        ),
        content(
            EngineContentKind::Unit,
            "unit-siege-mage",
            "Siege Mage",
            json!({
                "unit_id": "siege-mage",
                "role": "caster",
                "tags": ["fire", "siege", "support"],
                "hit_points_fp": 30
            }),
        ),
        content(
            EngineContentKind::Item,
            "item-frost-ward",
            "Frost Ward",
            json!({
                "item_id": "frost-ward",
                "summary": "defensive ice ward against burning",
                "tags": ["ice", "defense", "ward"]
            }),
        ),
    ]
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
            author: "issue1119-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1119:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1119_calyx_semantic_fsv".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn edge_readbacks(
    database: &EngineCalyxDatabase,
    readbacks: &[litd_engine::calyx_content::EngineContentConstellationReadback],
    index: &litd_engine::calyx_semantic::EngineCalyxSemanticIndex,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut missing_source_path = readbacks[0].clone();
    missing_source_path.source_path.clear();
    let mut tampered_index = index.clone();
    tampered_index.index_fingerprint = "00000000".to_string();
    Ok(vec![
        expect_invalid_input(
            "empty_index_id_rejected",
            build_engine_calyx_semantic_index("", readbacks),
            "index_id",
            "non-empty",
        ),
        expect_invalid_input(
            "empty_readbacks_rejected",
            build_engine_calyx_semantic_index("empty", &[]),
            "readbacks",
            "at least one",
        ),
        expect_invalid_input(
            "missing_source_path_rejected",
            build_engine_calyx_semantic_index("tampered", &[missing_source_path]),
            "readback.source_path",
            "non-empty",
        ),
        expect_invalid_input(
            "empty_query_rejected",
            query_engine_calyx_semantic_index(index, " ", 5),
            "query",
            "searchable terms",
        ),
        expect_invalid_input(
            "zero_limit_rejected",
            query_engine_calyx_semantic_index(index, "fire", 0),
            "limit",
            "> 0",
        ),
        expect_invalid_input(
            "index_fingerprint_tamper_rejected",
            validate_semantic_index(&tampered_index),
            "index_fingerprint",
            "fingerprint mismatch",
        ),
        missing_index_edge(database),
    ])
}

fn expect_invalid_input<T>(
    label: &str,
    result: Result<T, EngineCalyxSemanticError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineCalyxSemanticError::InvalidInput { field, reason }) => json!({
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

fn missing_index_edge(database: &EngineCalyxDatabase) -> Value {
    match read_engine_calyx_semantic_index(database, "missing-semantic-index") {
        Err(EngineCalyxSemanticError::Persistence(error)) => json!({
            "label": "missing_persisted_index_rejected",
            "matched": error.to_string().contains("missing-semantic-index"),
            "error": error.to_string(),
        }),
        Err(error) => json!({
            "label": "missing_persisted_index_rejected",
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": "missing_persisted_index_rejected",
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
