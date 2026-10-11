use crate::world_creation::{
    EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
    create_editor_world_record, read_world_record,
};

use super::*;

#[test]
fn legacy_single_cell_read_remains_supported_for_existing_worlds() {
    let root = temp_root("legacy-single-cell");
    let aster_dir = root.join("aster");
    let record = create_editor_world_record(known_spec("calyx://world/issue1286-legacy")).unwrap();
    let bytes = serde_json::to_vec_pretty(&record).unwrap();
    let vault = open_aster(&aster_dir).unwrap();
    let column = PlainColumn::new(&vault, WORLD_ASTER_COLLECTION).unwrap();
    let row = row_key(&record.spec.calyx_document_id).unwrap();
    let commit = column.put(&row, WORLD_ASTER_COLUMN, &bytes).unwrap();
    println!(
        "issue1286 legacy before manifest_absent=true bytes={} seq={}",
        bytes.len(),
        commit.seq
    );

    let readback = read_world_record(&aster_dir, &record.spec.calyx_document_id)
        .unwrap()
        .unwrap();
    println!(
        "issue1286 legacy after schema={} chunks={} bytes={} sha256={} fingerprint={}",
        readback.storage_schema,
        readback.chunk_count,
        readback.bytes.len(),
        readback.sha256,
        readback.document_fingerprint
    );

    assert_eq!(readback.storage_schema, LEGACY_SCHEMA);
    assert_eq!(readback.chunk_count, 1);
    assert_eq!(readback.bytes, bytes);
    assert_eq!(readback.document_fingerprint, record.document_fingerprint);
}

#[test]
fn manifest_missing_chunk_fails_loud_without_legacy_fallback() {
    let root = temp_root("missing-chunk");
    let aster_dir = root.join("aster");
    let record =
        create_editor_world_record(known_spec("calyx://world/issue1286-missing-chunk")).unwrap();
    let bytes = serde_json::to_vec_pretty(&record).unwrap();
    let manifest = build_chunk_manifest(&record, &bytes).unwrap();
    let manifest_bytes = manifest_to_bytes(&manifest).unwrap();
    let vault = open_aster(&aster_dir).unwrap();
    let column = PlainColumn::new(&vault, WORLD_ASTER_COLLECTION).unwrap();
    let row = row_key(&record.spec.calyx_document_id).unwrap();
    column.put(&row, WORLD_ASTER_COLUMN, &bytes).unwrap();
    column
        .put(&row, WORLD_ASTER_MANIFEST_COLUMN, &manifest_bytes)
        .unwrap();
    println!(
        "issue1286 missing_chunk before legacy_bytes={} manifest_chunks={} stored_chunks=0",
        bytes.len(),
        manifest.chunk_count
    );

    let error = read_world_record(&aster_dir, &record.spec.calyx_document_id)
        .unwrap_err()
        .to_string();
    println!("issue1286 missing_chunk after error={error}");

    assert!(error.contains("editor.world_creation.aster.chunk.missing"));
}

#[test]
fn chunk_sha_mismatch_fails_loud_after_reassembly() {
    let root = temp_root("sha-mismatch");
    let aster_dir = root.join("aster");
    let mut spec = known_spec("calyx://world/issue1286-sha-mismatch");
    spec.properties.description = "large description ".repeat(20_000);
    let record = create_editor_world_record(spec).unwrap();
    let writeback = write_world_record(&aster_dir, &record).unwrap();
    let vault = open_aster(&aster_dir).unwrap();
    let column = PlainColumn::new(&vault, WORLD_ASTER_COLLECTION).unwrap();
    let row = row_key(&record.spec.calyx_document_id).unwrap();
    let manifest_bytes = column
        .get(vault.latest_seq(), &row, WORLD_ASTER_MANIFEST_COLUMN)
        .unwrap()
        .unwrap();
    let manifest = parse_chunk_manifest(&manifest_bytes, &record.spec.calyx_document_id).unwrap();
    let corrupt = vec![b'z'; manifest.chunk_size];
    column
        .put(&row, manifest.chunk_columns[0].as_bytes(), &corrupt)
        .unwrap();
    println!(
        "issue1286 sha_mismatch before chunks={} expected_sha={} corrupt_chunk_bytes={}",
        writeback.chunk_count,
        writeback.sha256,
        corrupt.len()
    );

    let error = read_world_record(&aster_dir, &record.spec.calyx_document_id)
        .unwrap_err()
        .to_string();
    println!("issue1286 sha_mismatch after error={error}");

    assert!(error.contains("editor.world_creation.aster.chunk.sha256"));
}

fn known_spec(document_id: &str) -> EditorWorldCreationSpec {
    EditorWorldCreationSpec {
        calyx_document_id: document_id.to_string(),
        revision_id: "issue1286-store-test-rev-1".to_string(),
        properties: EditorWorldProperties {
            display_name: "Issue 1286 Store Test".to_string(),
            description: "Known world settings for chunked Aster persistence".to_string(),
            width_tiles: 16,
            height_tiles: 16,
            random_height_seed: Some(1286),
            playable_bounds: EditorPlayableBounds {
                min_tile_x: 0,
                min_tile_z: 0,
                max_tile_x_exclusive: 16,
                max_tile_z_exclusive: 16,
            },
            ..EditorWorldProperties::default()
        },
    }
}

fn temp_root(label: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("world-store-{label}"))
}
