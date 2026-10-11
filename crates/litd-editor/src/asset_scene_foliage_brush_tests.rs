use super::*;
use crate::asset_scene_foliage_config::load_default_foliage_config;
use std::fs;

fn test_state_path(name: &str) -> PathBuf {
    workspace_root()
        .join("target")
        .join("tmp")
        .join(format!("{name}-{}-aster", std::process::id()))
}

#[test]
fn add_and_erase_brush_stamps_mutate_effective_config_from_disk() {
    let path = test_state_path("foliage-brush-add-erase");
    let _ = fs::remove_dir_all(&path);
    let store = FoliageBrushStore { path: path.clone() };
    let base_config = load_default_foliage_config().unwrap();

    let added = store
        .add_density_brush(&base_config, [0.0, -24.0], 8.0, 50.0)
        .unwrap();
    assert!(added.present_on_disk);
    assert_eq!(added.stamps.len(), 1);
    let with_add = apply_foliage_brush_state(base_config.clone(), &added).unwrap();
    assert_eq!(with_add.brush.added_patch_count, 1);
    assert_eq!(with_add.brush.erased_patch_count, 0);
    assert_eq!(with_add.patches.len(), base_config.patches.len() + 1);
    assert!(with_add.target_clump_count() > base_config.target_clump_count());

    let erased = store.erase_density_brush([0.0, -24.0], 9.0).unwrap();
    assert_eq!(erased.stamps.len(), 2);
    let with_erase = apply_foliage_brush_state(base_config.clone(), &erased).unwrap();
    assert_eq!(with_erase.brush.added_patch_count, 1);
    assert_eq!(with_erase.brush.erased_patch_count, 1);
    assert_eq!(with_erase.patches.len(), base_config.patches.len());
    assert_eq!(
        with_erase.target_clump_count(),
        base_config.target_clump_count()
    );

    let reloaded = load_foliage_brush_state(&path).unwrap();
    assert!(reloaded.present_on_disk);
    assert_eq!(reloaded.stamps.len(), 2);
    assert_ne!(reloaded.source_sha256, "missing");
    let _ = fs::remove_dir_all(&path);
}

#[test]
fn invalid_brush_state_fails_loud() {
    let path = test_state_path("foliage-brush-invalid");
    let _ = fs::remove_dir_all(&path);
    litd_engine::calyx_json_store::write_calyx_json_bytes(
        &brush_spec(&path),
        br#"{"schema_version":1,"record_kind":"wrong","source_issue":1271,"stamps":[]}"#,
    )
    .unwrap();

    match load_foliage_brush_state(&path) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.asset_scene.foliage_brush.record_kind");
            assert!(reason.contains("expected"));
        }
        other => panic!("expected record-kind failure, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&path);
}
