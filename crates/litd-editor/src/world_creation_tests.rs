use std::path::PathBuf;

use super::*;

#[test]
fn known_world_persists_to_aster_without_json_mirror() {
    let (_temp_root, legacy_state_path, aster_dir) = test_paths("known");
    let spec = known_spec("calyx://world/issue1210-known", 6, 4);
    println!(
        "issue1210 known before legacy_exists={} aster_exists={} dims={}x{}",
        legacy_state_path.exists(),
        aster_dir.exists(),
        spec.properties.width_tiles,
        spec.properties.height_tiles
    );

    let record = create_editor_world_record(spec.clone()).unwrap();
    let writeback = write_world_record(&aster_dir, &record).unwrap();
    let readback = read_world_record(&aster_dir, &spec.calyx_document_id)
        .unwrap()
        .unwrap();
    let parsed = parse_world_record(&readback.bytes).unwrap();
    println!(
        "issue1210 known after seq={} graph_rows={} cells={} default_tiles={} bounds={:?} summary={}",
        readback.seq,
        readback.graph_rows,
        parsed.terrain_cell_count,
        parsed.default_tile_count,
        parsed.spec.properties.playable_bounds,
        parsed.summary
    );

    assert_eq!(writeback.bytes, readback.bytes);
    assert_eq!(writeback.storage_schema, WORLD_ASTER_CHUNK_SCHEMA);
    assert_eq!(readback.storage_schema, WORLD_ASTER_CHUNK_SCHEMA);
    assert_eq!(writeback.chunk_count, 1);
    assert!(!legacy_state_path.exists());
    assert_eq!(
        parsed.spec.properties.display_name,
        "Issue 1210 Known World"
    );
    assert_eq!(parsed.spec.properties.width_tiles, 6);
    assert_eq!(parsed.spec.properties.height_tiles, 4);
    assert_eq!(parsed.terrain_cell_count, 24);
    assert_eq!(parsed.default_tile_count, 24);
    assert_eq!(
        parsed.document.regions["playable_bounds"].name,
        "Playable Bounds"
    );
    assert!(parsed.document.objects.contains_key("world"));
}

#[test]
fn max_world_persists_as_chunked_aster_manifest() {
    let (_temp_root, legacy_state_path, aster_dir) = test_paths("chunked-large");
    let spec = known_spec(
        "calyx://world/issue1286-large-chunked",
        MAX_WORLD_DIMENSION_TILES,
        MAX_WORLD_DIMENSION_TILES,
    );
    let record = create_editor_world_record(spec.clone()).unwrap();
    let serialized = serde_json::to_vec_pretty(&record).unwrap();
    println!(
        "issue1286 chunked before legacy_exists={} aster_exists={} serialized_bytes={} chunk_size={}",
        legacy_state_path.exists(),
        aster_dir.exists(),
        serialized.len(),
        WORLD_ASTER_CHUNK_SIZE
    );
    assert!(serialized.len() > 1_048_576);

    let writeback = write_world_record(&aster_dir, &record).unwrap();
    let readback = read_world_record(&aster_dir, &spec.calyx_document_id)
        .unwrap()
        .unwrap();
    let parsed = parse_world_record(&readback.bytes).unwrap();
    println!(
        "issue1286 chunked after seq={} graph_rows={} chunks={} bytes={} sha256={} cells={} fingerprint={}",
        readback.seq,
        readback.graph_rows,
        readback.chunk_count,
        readback.bytes.len(),
        readback.sha256,
        parsed.terrain_cell_count,
        readback.document_fingerprint
    );

    assert_eq!(writeback.storage_schema, WORLD_ASTER_CHUNK_SCHEMA);
    assert_eq!(readback.storage_schema, WORLD_ASTER_CHUNK_SCHEMA);
    assert!(readback.chunk_count > 1);
    assert_eq!(readback.bytes, serialized);
    assert!(!legacy_state_path.exists());
    assert_eq!(parsed.terrain_cell_count, (128 * 128) as usize);
    assert_eq!(parsed.default_tile_count, parsed.terrain_cell_count);
    assert_eq!(parsed.spec.calyx_document_id, spec.calyx_document_id);
}

#[test]
fn min_and_max_dimensions_create_expected_field_sizes() {
    for (label, width, height) in [
        ("min", MIN_WORLD_DIMENSION_TILES, MIN_WORLD_DIMENSION_TILES),
        ("max", MAX_WORLD_DIMENSION_TILES, MAX_WORLD_DIMENSION_TILES),
    ] {
        let spec = known_spec(&format!("calyx://world/issue1210-{label}"), width, height);
        let before = "not-created";
        let record = create_editor_world_record(spec).unwrap();
        println!(
            "issue1210 {label} before={before} after cells={} default_tiles={} summary={}",
            record.terrain_cell_count, record.default_tile_count, record.summary
        );
        assert_eq!(record.terrain_cell_count, (width * height) as usize);
        assert_eq!(record.default_tile_count, record.terrain_cell_count);
    }
}

#[test]
fn invalid_world_settings_fail_loud_before_persistence() {
    let (_temp_root, legacy_state_path, aster_dir) = test_paths("invalid");
    let mut spec = known_spec("calyx://world/issue1210-invalid", 8, 8);
    spec.properties.playable_bounds.max_tile_x_exclusive = 99;
    let before = (legacy_state_path.exists(), aster_dir.exists());

    let error = create_editor_world_record(spec).unwrap_err().to_string();
    println!(
        "issue1210 invalid before state_exists={} aster_exists={} after state_exists={} aster_exists={} error={}",
        before.0,
        before.1,
        legacy_state_path.exists(),
        aster_dir.exists(),
        error
    );

    assert!(error.contains("editor.world_creation.properties.playable_bounds.world"));
    assert!(!legacy_state_path.exists());
    assert!(!aster_dir.exists());
}

#[test]
fn default_tile_is_applied_to_every_cell_without_height_seed() {
    let mut spec = known_spec("calyx://world/issue1210-default-tile", 5, 5);
    spec.properties.random_height_seed = None;
    spec.properties.initial_cliff_level = 3;
    let before = "terrain empty";
    let record = create_editor_world_record(spec).unwrap();
    let all_default = record
        .document
        .terrain_cells
        .values()
        .all(|cell| cell.tile_id == DEFAULT_WORLD_TILE_ID && cell.height == 3.0);
    println!(
        "issue1210 default_tile before={before} after cells={} default_tiles={} all_default={}",
        record.terrain_cell_count, record.default_tile_count, all_default
    );
    assert!(all_default);
    assert_eq!(record.default_tile_count, 25);
}

fn known_spec(document_id: &str, width: u32, height: u32) -> EditorWorldCreationSpec {
    EditorWorldCreationSpec {
        calyx_document_id: document_id.to_string(),
        revision_id: "issue1210-known-rev-1".to_string(),
        properties: EditorWorldProperties {
            display_name: "Issue 1210 Known World".to_string(),
            description: "Synthetic known settings with expected Aster readback".to_string(),
            width_tiles: width,
            height_tiles: height,
            random_height_seed: Some(4242),
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

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn test_paths(label: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf, PathBuf) {
    let root = litd_engine::test_temp::TestTempDir::new(&format!("world-creation-{label}"));
    let world = root.path().join("world.json");
    let aster = root.path().join("aster");
    (root, world, aster)
}
