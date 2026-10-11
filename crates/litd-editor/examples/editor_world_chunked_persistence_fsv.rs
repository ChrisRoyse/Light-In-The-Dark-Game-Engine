use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::world_creation::{
    EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
    MAX_WORLD_DIMENSION_TILES, WORLD_ASTER_CHUNK_SCHEMA, WORLD_ASTER_CHUNK_SIZE,
    WorldAsterReadback, create_editor_world_record, parse_world_record, read_world_record,
    write_world_record,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_WORLD_CHUNKED_FSV",
        "issue1286_chunked_world_persistence",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1286 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");
    let before_aster = dir_state(&aster_dir)?;

    let large_spec = large_world_spec("calyx://world/issue1286-large-chunked");
    let large_record = create_editor_world_record(large_spec.clone())?;
    let serialized = serde_json::to_vec_pretty(&large_record)?;
    let large_state = root.join(format!("{}.json", large_record.spec.revision_id));
    let writeback = write_world_record(&aster_dir, &large_record)?;
    let readback = read_world_record(&aster_dir, &large_spec.calyx_document_id)?
        .ok_or("missing large chunked world readback")?;
    let parsed = parse_world_record(&readback.bytes)?;
    if readback.bytes != serialized {
        return Err("large world readback bytes did not match serialized source".into());
    }
    if writeback.sha256 != readback.sha256 || writeback.chunk_count != readback.chunk_count {
        return Err("write receipt and independent readback disagreed".into());
    }
    if readback.storage_schema != WORLD_ASTER_CHUNK_SCHEMA || readback.chunk_count <= 1 {
        return Err("large world did not use chunked Aster storage".into());
    }

    let small_edge = small_world_edge(&root, &aster_dir)?;
    let missing_edge = missing_world_edge(&aster_dir)?;
    let invalid_edge = invalid_document_id_edge(&aster_dir)?;
    let mut snapshot = json!({
        "schema": "litd.issue1286.chunked_world_persistence_fsv.v1",
        "issue": 1286,
        "source_of_truth": "Aster manifest/chunk readback plus physical world JSON and WAL files",
        "artifact_path": path,
        "before": {
            "artifact": before_artifact,
            "aster_dir": before_aster,
            "large_world": {
                "document_id": large_spec.calyx_document_id,
                "expected_serialized_bytes": serialized.len(),
                "single_cell_cap_bytes": 1_048_576,
                "chunk_size": WORLD_ASTER_CHUNK_SIZE
            }
        },
        "happy_path": {
            "after": {
                "state_path": large_state,
                "state_sha256": file_sha256(&large_state)?,
                "write_receipt": readback_summary(&writeback),
                "readback": readback_summary(&readback),
                "parsed": {
                    "document_id": parsed.spec.calyx_document_id,
                    "terrain_cell_count": parsed.terrain_cell_count,
                    "default_tile_count": parsed.default_tile_count,
                    "document_fingerprint": parsed.document_fingerprint,
                    "summary": parsed.summary
                },
                "passed": true
            }
        },
        "edges": [
            small_edge,
            missing_edge,
            invalid_edge
        ],
        "after": {
            "artifact": {"exists": false, "bytes": 0},
            "aster_dir": dir_state(&aster_dir)?
        }
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote chunked world persistence FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "large readback {}",
        snapshot["happy_path"]["after"]["readback"]
    );
    println!("small edge {}", snapshot["edges"][0]["after"]);
    println!("missing edge {}", snapshot["edges"][1]["after"]);
    println!("invalid edge {}", snapshot["edges"][2]["after"]);
    Ok(())
}

fn small_world_edge(
    root: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let spec = small_world_spec("calyx://world/issue1286-small-chunked");
    let record = create_editor_world_record(spec.clone())?;
    let serialized = serde_json::to_vec_pretty(&record)?;
    let state = root.join(format!("{}.json", record.spec.revision_id));
    let before = dir_state(aster_dir)?;
    write_world_record(aster_dir, &record)?;
    let readback = read_world_record(aster_dir, &spec.calyx_document_id)?
        .ok_or("missing small chunked world readback")?;
    Ok(json!({
        "name": "under-cap world still uses manifest contract",
        "before": {
            "serialized_bytes": serialized.len(),
            "aster_dir": before
        },
        "after": {
            "state_path": state,
            "state_sha256": file_sha256(&state)?,
            "readback": readback_summary(&readback),
            "uses_chunk_manifest": readback.storage_schema == WORLD_ASTER_CHUNK_SCHEMA,
            "single_chunk": readback.chunk_count == 1
        }
    }))
}

fn missing_world_edge(aster_dir: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let before = dir_state(aster_dir)?;
    let missing_id = "calyx://world/issue1286-missing";
    let result = read_world_record(aster_dir, missing_id)?;
    let after = dir_state(aster_dir)?;
    Ok(json!({
        "name": "missing world stays absent without synthetic success",
        "before": {
            "document_id": missing_id,
            "aster_dir": before
        },
        "after": {
            "is_none": result.is_none(),
            "aster_dir": after
        }
    }))
}

fn invalid_document_id_edge(
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let before = dir_state(aster_dir)?;
    let error = read_world_record(aster_dir, "  ")
        .expect_err("empty document id must fail")
        .to_string();
    let after = dir_state(aster_dir)?;
    Ok(json!({
        "name": "invalid document id fails loud",
        "before": {
            "document_id": "  ",
            "aster_dir": before
        },
        "after": {
            "error": error,
            "aster_dir": after
        }
    }))
}

fn large_world_spec(document_id: &str) -> EditorWorldCreationSpec {
    world_spec(
        document_id,
        "issue1286-large-chunked-rev-1",
        MAX_WORLD_DIMENSION_TILES,
        MAX_WORLD_DIMENSION_TILES,
    )
}

fn small_world_spec(document_id: &str) -> EditorWorldCreationSpec {
    world_spec(document_id, "issue1286-small-chunked-rev-1", 16, 16)
}

fn world_spec(
    document_id: &str,
    revision_id: &str,
    width: u32,
    height: u32,
) -> EditorWorldCreationSpec {
    EditorWorldCreationSpec {
        calyx_document_id: document_id.to_string(),
        revision_id: revision_id.to_string(),
        properties: EditorWorldProperties {
            display_name: format!("Issue 1286 Chunked {width}x{height}"),
            description: "Known world settings for chunked Aster persistence FSV".to_string(),
            width_tiles: width,
            height_tiles: height,
            random_height_seed: Some(1286),
            playable_bounds: EditorPlayableBounds {
                min_tile_x: 0,
                min_tile_z: 0,
                max_tile_x_exclusive: width as i32,
                max_tile_z_exclusive: height as i32,
            },
            ..EditorWorldProperties::default()
        },
    }
}

fn readback_summary(readback: &WorldAsterReadback) -> serde_json::Value {
    json!({
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "storage_schema": readback.storage_schema,
        "chunk_count": readback.chunk_count,
        "manifest_bytes": readback.manifest_bytes,
        "bytes": readback.bytes.len(),
        "sha256": readback.sha256,
        "document_fingerprint": readback.document_fingerprint,
        "cell_key": readback.cell_key,
        "index_key": readback.index_key,
        "first_chunk_column": readback.chunk_columns.first(),
        "last_chunk_column": readback.chunk_columns.last()
    })
}

fn dir_state(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(json!({"exists": false, "files": 0, "bytes": 0}));
    }
    let mut files = 0_usize;
    let mut bytes = 0_u64;
    let mut wal_bytes = 0_u64;
    for entry in walkdir(path)? {
        let metadata = fs::metadata(&entry)?;
        if metadata.is_file() {
            files += 1;
            bytes += metadata.len();
            if entry.extension().and_then(|ext| ext.to_str()) == Some("wal") {
                wal_bytes += metadata.len();
            }
        }
    }
    Ok(json!({"exists": true, "files": files, "bytes": bytes, "wal_bytes": wal_bytes}))
}

fn walkdir(root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn write_document_with_final_state(
    path: &PathBuf,
    document: &mut serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"]["artifact"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            if disk_len != expected_len {
                return Err(format!(
                    "FSV artifact write verification failed for {}: embedded after.bytes={}, disk bytes={}",
                    path.display(),
                    expected_len,
                    disk_len
                )
                .into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err(format!(
        "FSV artifact byte-count stabilization failed for {}",
        path.display()
    )
    .into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}

fn file_sha256(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
