use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    history::EditorObject,
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
        create_editor_world_record,
    },
    world_memory::{
        AuthoredWorldMemoryInput, GuardedWorldMemoryQuery, GuardedWorldMemorySearchLedger,
        GuardedWorldMemorySearchStatus, WorldMemoryIndex, WorldMemorySearchAudience,
        WorldMemorySearchCalibration, build_guarded_world_memory_search_ledger,
        build_world_memory_index, generated_world_script, guarded_query_world_memory,
        parse_guarded_world_memory_search_ledger, read_guarded_world_memory_search_ledger,
        write_guarded_world_memory_search_ledger,
    },
};
use litd_engine::math::Transform;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_GUARDED_SEARCH_FSV",
        "issue1207_guarded_search",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1207 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("guarded-search-ledger.json");
    let aster_dir = root.join("aster");
    let before = path_state(&output_path, &state_path, &aster_dir);

    let index = build_world_memory_index(&[memory_input("issue1207-world")])?;
    let ward = ward();
    let calibration_profile = calibration("issue1207-search-calibration", 300, 50);
    let happy = guarded_query_world_memory(
        &index,
        &query("issue1207-happy", "poison stacking archer", 3),
        &ward,
        Some(&calibration_profile),
    )?;
    let unsafe_query = guarded_query_world_memory(
        &index,
        &query("issue1207-unsafe", "spaceship laser exploit", 3),
        &ward,
        Some(&calibration_profile),
    )?;
    let ood = guarded_query_world_memory(
        &index,
        &query("issue1207-ood", "mortgage bakery account", 3),
        &ward,
        Some(&calibration_profile),
    )?;
    let borderline = guarded_query_world_memory(
        &index,
        &query("issue1207-borderline", "antidote", 3),
        &ward,
        Some(&calibration("issue1207-borderline-calibration", 800, 50)),
    )?;
    let empty_index = guarded_query_world_memory(
        &empty_index_from(&index),
        &query("issue1207-empty-index", "poison", 3),
        &ward,
        Some(&calibration_profile),
    )?;
    let missing_calibration = guarded_query_world_memory(
        &index,
        &query("issue1207-missing-calibration", "poison", 3),
        &ward,
        None,
    )?;
    let ledger = build_guarded_world_memory_search_ledger(
        "issue1207-guarded-search-fsv",
        vec![
            happy.clone(),
            unsafe_query.clone(),
            ood.clone(),
            borderline.clone(),
            empty_index.clone(),
            missing_calibration.clone(),
        ],
    )?;
    verify_expected_verdicts(&ledger)?;

    let writeback = write_guarded_world_memory_search_ledger(&state_path, &aster_dir, &ledger)?;
    let readback = read_guarded_world_memory_search_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing issue1207 guarded-search ledger")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err("state file bytes differ from Aster guarded-search ledger bytes".into());
    }
    let parsed = parse_guarded_world_memory_search_ledger(&readback.bytes)?;
    verify_expected_verdicts(&parsed)?;

    let document = json!({
        "schema": "litd.issue1207.guarded_search_fsv.v1",
        "issue": 1207,
        "source_of_truth": "Aster guarded-search ledger row and JSON mirror readback",
        "before": before,
        "happy_path": {
            "before": {
                "query": "poison stacking archer",
                "limit": 3,
                "tau_milli": calibration_profile.tau_milli,
                "borderline_margin_milli": calibration_profile.borderline_margin_milli
            },
            "after": {
                "state_path": state_path,
                "state_sha256": sha256_hex(&disk_bytes),
                "aster_sha256": readback.sha256,
                "writeback_sha256": writeback.sha256,
                "state_matches_aster": disk_bytes == readback.bytes,
                "verdict_count": parsed.verdict_count,
                "graph_rows": readback.graph_rows,
                "seq": readback.seq,
                "cell_key": writeback.cell_key,
                "index_key": writeback.index_key,
                "verdict": summarize(&happy),
                "passed": true
            }
        },
        "edges": [
            {
                "edge": "unsafe_query_refused",
                "before": {"query": unsafe_query.query},
                "after": summarize(&unsafe_query)
            },
            {
                "edge": "ood_query_refused",
                "before": {"query": ood.query},
                "after": summarize(&ood)
            },
            {
                "edge": "borderline_tau_refused",
                "before": {"query": borderline.query, "tau_milli": 800, "margin_milli": 50},
                "after": summarize(&borderline)
            },
            {
                "edge": "empty_index_refused",
                "before": {"world_count": 0, "constellation_count": 0},
                "after": summarize(&empty_index)
            },
            {
                "edge": "calibration_missing_refused",
                "before": {"calibration": null},
                "after": summarize(&missing_calibration)
            }
        ]
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1207 guarded search FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "ledger verdicts={} graph_rows={} state_matches_aster={} sha={}",
        parsed.verdict_count,
        readback.graph_rows,
        disk_bytes == readback.bytes,
        readback.sha256
    );
    println!(
        "verdicts happy={} unsafe={} ood={} borderline={} empty={} calibration={}",
        happy.code,
        unsafe_query.code,
        ood.code,
        borderline.code,
        empty_index.code,
        missing_calibration.code
    );
    Ok(())
}

fn verify_expected_verdicts(
    ledger: &GuardedWorldMemorySearchLedger,
) -> Result<(), Box<dyn std::error::Error>> {
    expect(
        ledger,
        "issue1207-happy",
        GuardedWorldMemorySearchStatus::Admitted,
        "LITD-SEARCH-ADMITTED",
    )?;
    expect(
        ledger,
        "issue1207-unsafe",
        GuardedWorldMemorySearchStatus::Refused,
        "LITD-SEARCH-REFUSED-UNSAFE",
    )?;
    expect(
        ledger,
        "issue1207-ood",
        GuardedWorldMemorySearchStatus::Refused,
        "LITD-SEARCH-REFUSED-OOD",
    )?;
    expect(
        ledger,
        "issue1207-borderline",
        GuardedWorldMemorySearchStatus::Refused,
        "LITD-SEARCH-REFUSED-BORDERLINE",
    )?;
    expect(
        ledger,
        "issue1207-empty-index",
        GuardedWorldMemorySearchStatus::Refused,
        "LITD-SEARCH-EMPTY-INDEX",
    )?;
    expect(
        ledger,
        "issue1207-missing-calibration",
        GuardedWorldMemorySearchStatus::Refused,
        "LITD-SEARCH-CALIBRATION-MISSING",
    )?;
    Ok(())
}

fn expect(
    ledger: &GuardedWorldMemorySearchLedger,
    query_id: &str,
    status: GuardedWorldMemorySearchStatus,
    code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let verdict = ledger
        .verdicts
        .iter()
        .find(|item| item.query_id == query_id)
        .ok_or_else(|| format!("missing guarded-search verdict {query_id}"))?;
    if verdict.status != status || verdict.code != code {
        return Err(format!(
            "{query_id} expected {status:?}/{code}, got {:?}/{}",
            verdict.status, verdict.code
        )
        .into());
    }
    Ok(())
}

fn summarize(
    verdict: &litd_editor::world_memory::GuardedWorldMemorySearchVerdict,
) -> serde_json::Value {
    json!({
        "query_id": verdict.query_id,
        "status": format!("{:?}", verdict.status),
        "code": verdict.code,
        "hit_count": verdict.hit_count,
        "candidate_hit_count": verdict.candidate_hit_count,
        "top_score": verdict.top_score,
        "top_score_milli": verdict.top_score_milli,
        "tau_milli": verdict.tau_milli,
        "matched_terms": verdict.matched_terms,
        "blocked_terms": verdict.blocked_terms,
        "source_refs": verdict.source_refs,
        "hits": verdict.hits.iter().map(|hit| json!({
            "constellation_id": hit.constellation_id,
            "source_kind": format!("{:?}", hit.source_kind),
            "title": hit.title,
            "score": hit.score,
            "provenance_refs": hit.provenance_refs
        })).collect::<Vec<_>>()
    })
}

fn memory_input(world_id: &str) -> AuthoredWorldMemoryInput {
    let world = world_record(world_id);
    AuthoredWorldMemoryInput {
        scripts: vec![generated_world_script(&world).unwrap()],
        world,
        abilities: Vec::new(),
        objects: vec![
            EditorObject::new("unit_venom_archer", "Unit", None, Transform::IDENTITY)
                .with_property("name", "Venom Archer")
                .with_property("role", "poison stacking ranged unit")
                .with_property("source_ref", "Aster://issue1207/units/venom-archer"),
            EditorObject::new("item_antidote", "Item", None, Transform::IDENTITY)
                .with_property("name", "Antidote")
                .with_property("description", "cleanses poison stacks")
                .with_property("source_ref", "Aster://issue1207/items/antidote"),
        ],
    }
}

fn world_record(world_id: &str) -> litd_editor::world_creation::EditorWorldRecord {
    let properties = EditorWorldProperties {
        display_name: "Issue 1207 Guarded Search".to_string(),
        description: "Poison search fixture for guarded player and agent queries".to_string(),
        width_tiles: 8,
        height_tiles: 8,
        playable_bounds: EditorPlayableBounds {
            min_tile_x: 0,
            min_tile_z: 0,
            max_tile_x_exclusive: 8,
            max_tile_z_exclusive: 8,
        },
        ..Default::default()
    };
    create_editor_world_record(EditorWorldCreationSpec {
        calyx_document_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        properties,
    })
    .unwrap()
}

fn ward() -> litd_editor::authoring::WardPolicy {
    litd_editor::authoring::WardPolicy {
        policy_id: "ward-search-issue1207".to_string(),
        min_source_anchors: 1,
        min_grounded_terms: 1,
        blocked_terms: vec![
            "spaceship".to_string(),
            "laser".to_string(),
            "stock market".to_string(),
        ],
    }
}

fn calibration(id: &str, tau_milli: u16, margin_milli: u16) -> WorldMemorySearchCalibration {
    WorldMemorySearchCalibration {
        calibration_id: id.to_string(),
        tau_milli,
        borderline_margin_milli: margin_milli,
    }
}

fn query(query_id: &str, query: &str, limit: usize) -> GuardedWorldMemoryQuery {
    GuardedWorldMemoryQuery {
        query_id: query_id.to_string(),
        audience: WorldMemorySearchAudience::Agent,
        query: query.to_string(),
        limit,
        requested_by: "issue1207-fsv".to_string(),
    }
}

fn empty_index_from(index: &WorldMemoryIndex) -> WorldMemoryIndex {
    let mut empty = index.clone();
    empty.world_count = 0;
    empty.constellation_count = 0;
    empty.constellations.clear();
    empty
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(&document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            if fs::metadata(path)?.len() != expected_len {
                return Err("issue1207 FSV artifact byte readback mismatch".into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("issue1207 FSV artifact byte-count stabilization failed".into())
}

fn path_state(output_path: &PathBuf, state_path: &PathBuf, aster_dir: &Path) -> serde_json::Value {
    json!({
        "artifact_exists": output_path.exists(),
        "artifact_bytes": fs::metadata(output_path).map(|metadata| metadata.len()).unwrap_or(0),
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "aster_dir_exists": aster_dir.exists()
    })
}
