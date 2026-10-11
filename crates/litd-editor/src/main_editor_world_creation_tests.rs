use std::path::PathBuf;

use litd_editor::world_creation::{parse_world_record, read_world_record};

use super::*;

#[test]
fn create_world_command_records_evidence_and_persists_aster() {
    let (_temp_root, state_path, aster_dir) = test_paths("runtime-happy");
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.world_creation = WorldCreationRuntime::new(aster_dir.clone()).unwrap();
    runtime.world_creation.draft.display_name = "Issue 1210 Runtime World".to_string();
    runtime.world_creation.draft.width_tiles = 7;
    runtime.world_creation.draft.height_tiles = 5;
    runtime.world_creation.draft.random_height_seed = 777;

    let before = runtime.evidence();
    println!(
        "issue1210 runtime before status={} creates={} store={}",
        before.world_creation_status,
        before.world_creation_create_count,
        before.world_creation_store
    );

    runtime.create_world_from_draft_command().unwrap();

    let after = runtime.evidence();
    let readback = read_world_record(&aster_dir, "calyx://world/issue1210-new-world")
        .unwrap()
        .unwrap();
    let parsed = parse_world_record(&readback.bytes).unwrap();
    println!(
        "issue1210 runtime after status={} creates={} cells={} default_tiles={} seq={} graph_rows={} summary={}",
        after.world_creation_status,
        after.world_creation_create_count,
        parsed.terrain_cell_count,
        parsed.default_tile_count,
        readback.seq,
        readback.graph_rows,
        parsed.summary
    );

    assert_eq!(
        parsed.spec.properties.display_name,
        "Issue 1210 Runtime World"
    );
    assert_eq!(parsed.terrain_cell_count, 35);
    assert_eq!(parsed.default_tile_count, 35);
    assert_eq!(
        after.world_creation_create_count,
        before.world_creation_create_count + 1
    );
    assert_eq!(after.world_creation_terrain_cell_count, 35);
    assert_eq!(after.world_creation_default_tile_count, 35);
    assert!(!state_path.exists());
    assert_eq!(readback.bytes, serde_json::to_vec_pretty(&parsed).unwrap());
}

#[test]
fn invalid_runtime_world_settings_do_not_write_state() {
    let (_temp_root, state_path, aster_dir) = test_paths("runtime-invalid");
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.world_creation = WorldCreationRuntime::new(aster_dir.clone()).unwrap();
    runtime.world_creation.draft.width_tiles = 3;

    let before = runtime.evidence();
    let error = runtime
        .create_world_from_draft_command()
        .unwrap_err()
        .to_string();
    let after = runtime.evidence();
    println!(
        "issue1210 runtime_invalid before status={} creates={} after status={} creates={} state_exists={} aster_exists={} error={}",
        before.world_creation_status,
        before.world_creation_create_count,
        after.world_creation_status,
        after.world_creation_create_count,
        state_path.exists(),
        aster_dir.exists(),
        error
    );

    assert!(error.contains("editor.world_creation.properties.width_tiles"));
    assert_eq!(
        after.world_creation_create_count,
        before.world_creation_create_count
    );
    assert!(after.world_creation_status.starts_with("error:"));
    assert!(!state_path.exists());
    assert!(!aster_dir.exists());
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn test_paths(label: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf, PathBuf) {
    let root = litd_engine::test_temp::TestTempDir::new(&format!("editor-world-runtime-{label}"));
    let state_path = root.path().join("world.json");
    let aster_dir = root.path().join("aster");
    (root, state_path, aster_dir)
}
