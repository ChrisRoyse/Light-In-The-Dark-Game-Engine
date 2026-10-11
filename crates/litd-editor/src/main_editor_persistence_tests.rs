use super::*;
use litd_editor::shell::{DockPanelKind, build_editor_shell_plan, pop_out_panel};
use litd_engine::math::Vec2;
use litd_engine::render::ui::UiRect;
use std::fs;

#[path = "main_editor_persistence_tests_part2.rs"]
mod part2;

#[test]
fn shell_state_store_writes_and_restores_real_aster_state() {
    let path = test_state_path("roundtrip");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let switched = switch_editor_mode(&base, EditorMode::Vfx).unwrap();
    let floating = UiRect::new(Vec2::new(1200.0, 120.0), Vec2::new(1800.0, 840.0)).unwrap();
    let popped = pop_out_panel(&switched, DockPanelKind::RightInspector, floating).unwrap();
    let mut writer = ShellStateStore::new(path.clone());

    writer.save_plan(&popped).unwrap();

    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"active_mode\": \"Vfx\""));
    assert!(text.contains("\"world_layers\""));
    assert!(text.contains("\"command_keybindings\""));
    assert!(text.contains("\"foliage\": true"));
    assert!(text.contains("\"fog_effects\": true"));
    assert!(text.contains("\"letterbox_mode\": false"));
    assert!(text.contains("\"docked\": false"));

    let mut reader = ShellStateStore::new(path.clone());
    let restored = reader.load_or_initialize(settings, base).unwrap();
    let inspector = restored.panel(DockPanelKind::RightInspector).unwrap();

    assert!(reader.loaded_from_disk());
    assert_eq!(restored.active_mode, EditorMode::Vfx);
    assert!(!inspector.docked);
    assert_eq!(inspector.floating_rect_px, Some(floating));
    assert_eq!(reader.save_count(), 1);
    reset_state_store(&path);
}

#[test]
fn shell_state_store_persists_world_layer_visibility() {
    let path = test_state_path("world-layer-roundtrip");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut writer = ShellStateStore::new(path.clone());
    let visibility = WorldLayerVisibility {
        foliage: false,
        grid: false,
        letterbox_mode: true,
        ..Default::default()
    };

    writer
        .save_plan_with_world_layers(&base, visibility)
        .unwrap();

    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"foliage\": false"));
    assert!(text.contains("\"grid\": false"));
    assert!(text.contains("\"letterbox_mode\": true"));

    let mut reader = ShellStateStore::new(path.clone());
    let _ = reader.load_or_initialize(settings, base).unwrap();

    assert!(reader.loaded_from_disk());
    assert_eq!(reader.world_layers(), visibility);
    let text = read_state_text(&path);
    assert!(text.contains("\"foliage\": false"));
    assert!(text.contains("\"grid\": false"));
    reset_state_store(&path);
}

#[test]
fn schema_six_foliage_only_world_layers_upgrade_with_visible_defaults() {
    let path = test_state_path("schema-six-world-layer-upgrade");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    write_schema_six_foliage_only_state_record(&path, &base);

    let mut store = ShellStateStore::new(path.clone());
    let _ = store.load_or_initialize(settings, base).unwrap();

    assert!(store.loaded_from_disk());
    let layers = store.world_layers();
    assert!(!layers.foliage);
    assert!(layers.terrain);
    assert!(layers.doodads);
    assert!(layers.fog_effects);
    assert!(!layers.grid);
    assert!(!layers.letterbox_mode);
    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"foliage\": false"));
    assert!(text.contains("\"terrain\": true"));
    reset_state_store(&path);
}

#[test]
fn schema_eight_stock_camera_migrates_to_current_bird_eye_profile() {
    let path = test_state_path("schema-eight-camera-migration");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut previous_plan = base.clone();
    previous_plan.viewport.camera = previous_schema_eight_stock_camera();
    write_test_state_record(&path, SCHEMA_EIGHT_VERSION, &previous_plan);

    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base.clone()).unwrap();

    assert!(store.loaded_from_disk());
    assert_eq!(restored.viewport.camera, base.viewport.camera);
    let text = read_state_text(&path);
    println!("schema8_camera_migration after_state={text}");
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 150.0"));
    assert!(text.contains("\"yaw_degrees\": 38.0"));
    assert!(text.contains("\"pitch_degrees\": -42.0"));
    assert!(!text.contains("\"distance\": 165.0"));
    reset_state_store(&path);
}

#[test]
fn schema_eleven_stock_camera_migrates_to_current_visual_preset() {
    let path = test_state_path("schema-eleven-camera-migration");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut previous_plan = base.clone();
    previous_plan.viewport.camera = schema_eleven_top_down_default_camera();
    write_test_state_record(&path, SCHEMA_ELEVEN_VERSION, &previous_plan);

    println!(
        "issue1164 schema11_camera before path={} camera={:?}",
        path.display(),
        previous_plan.viewport.camera
    );
    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base.clone()).unwrap();

    assert!(store.loaded_from_disk());
    assert_eq!(restored.viewport.camera, base.viewport.camera);
    let text = read_state_text(&path);
    println!("issue1164 schema11_camera after_state={text}");
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 150.0"));
    assert!(text.contains("\"yaw_degrees\": 38.0"));
    assert!(text.contains("\"pitch_degrees\": -42.0"));
    assert!(!text.contains("\"pitch_degrees\": -60.0"));
    reset_state_store(&path);
}

#[test]
fn schema_one_legacy_default_camera_migrates_to_current_default() {
    let path = test_state_path("schema-one-camera-migration");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut legacy_plan = base.clone();
    legacy_plan.viewport.camera = legacy_top_down_default_camera();
    write_test_state_record(&path, LEGACY_SCHEMA_VERSION, &legacy_plan);

    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base.clone()).unwrap();

    assert!(store.loaded_from_disk());
    assert_eq!(restored.viewport.camera, base.viewport.camera);
    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 150.0"));
    assert!(text.contains("\"yaw_degrees\": 38.0"));
    assert!(text.contains("\"pitch_degrees\": -42.0"));
    assert!(!text.contains("\"distance\": 165.0"));
    reset_state_store(&path);
}

#[test]
fn schema_two_previous_default_camera_migrates_to_current_vista() {
    let path = test_state_path("schema-two-camera-migration");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut previous_plan = base.clone();
    previous_plan.viewport.camera = previous_schema_two_camera();
    write_test_state_record(&path, SCHEMA_TWO_VERSION, &previous_plan);

    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base.clone()).unwrap();

    assert!(store.loaded_from_disk());
    assert_eq!(restored.viewport.camera, base.viewport.camera);
    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 150.0"));
    assert!(!text.contains("\"distance\": 128.0"));
    reset_state_store(&path);
}

#[test]
fn schema_three_previous_default_camera_migrates_to_current_vista() {
    let path = test_state_path("schema-three-camera-migration");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut previous_plan = base.clone();
    previous_plan.viewport.camera = previous_schema_three_camera();
    write_test_state_record(&path, SCHEMA_THREE_VERSION, &previous_plan);

    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base.clone()).unwrap();

    assert!(store.loaded_from_disk());
    assert_eq!(restored.viewport.camera, base.viewport.camera);
    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 150.0"));
    assert!(!text.contains("\"distance\": 190.0"));
    reset_state_store(&path);
}

#[test]
fn schema_four_previous_default_camera_migrates_to_current_vista() {
    let path = test_state_path("schema-four-camera-migration");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut previous_plan = base.clone();
    previous_plan.viewport.camera = previous_schema_four_camera();
    write_test_state_record(&path, SCHEMA_FOUR_VERSION, &previous_plan);

    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base.clone()).unwrap();

    assert!(store.loaded_from_disk());
    assert_eq!(restored.viewport.camera, base.viewport.camera);
    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 150.0"));
    assert!(text.contains("\"pitch_degrees\": -42.0"));
    reset_state_store(&path);
}

#[test]
fn schema_one_custom_camera_is_preserved_while_file_is_upgraded() {
    let path = test_state_path("schema-one-custom-camera");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut custom_plan = base.clone();
    custom_plan.viewport.camera.distance = 140.0;
    custom_plan.viewport.camera.yaw_degrees = 22.0;
    write_test_state_record(&path, LEGACY_SCHEMA_VERSION, &custom_plan);

    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base).unwrap();

    assert_eq!(restored.viewport.camera, custom_plan.viewport.camera);
    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 140.0"));
    assert!(text.contains("\"yaw_degrees\": 22.0"));
    reset_state_store(&path);
}

#[test]
fn schema_three_custom_camera_is_preserved_while_file_is_upgraded() {
    let path = test_state_path("schema-three-custom-camera");
    reset_state_store(&path);
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut custom_plan = base.clone();
    custom_plan.viewport.camera = previous_schema_three_camera();
    custom_plan.viewport.camera.distance = 136.0;
    custom_plan.viewport.camera.target_world.z = -32.0;
    write_test_state_record(&path, SCHEMA_THREE_VERSION, &custom_plan);

    let mut store = ShellStateStore::new(path.clone());
    let restored = store.load_or_initialize(settings, base).unwrap();

    assert_eq!(restored.viewport.camera, custom_plan.viewport.camera);
    let text = read_state_text(&path);
    assert!(text.contains("\"schema_version\": 12"));
    assert!(text.contains("\"distance\": 136.0"));
    assert!(text.contains("-32.0"));
    reset_state_store(&path);
}

#[test]
fn shell_state_store_persists_camera_bookmark_with_aster_readback() {
    let path = test_state_path("camera-bookmark-persistence");
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let mut plan = build_editor_shell_plan(settings).unwrap();
    let mut store = ShellStateStore::new(path.clone());
    store.load_or_initialize(settings, plan.clone()).unwrap();
    plan.viewport.camera.target_world = Vec3::new(32.0, 0.0, -48.0);
    plan.viewport.camera.distance = 96.0;

    println!(
        "issue1216 bookmark_persistence before path={} bookmarks={}",
        path.display(),
        store.camera_bookmark_summary()
    );
    store.save_camera_bookmark(1, plan.viewport.camera).unwrap();
    store.save_plan(&plan).unwrap();

    let text = read_state_text(&path);
    assert!(text.contains("\"camera_bookmarks\""));
    assert!(text.contains("\"slot\": 1"));
    println!("issue1216 bookmark_persistence after_file={text}");

    let mut reloaded = ShellStateStore::new(path.clone());
    let _ = reloaded.load_or_initialize(settings, build_editor_shell_plan(settings).unwrap());
    let restored = reloaded.restore_camera_bookmark(1).unwrap();
    assert_eq!(restored, plan.viewport.camera);
    println!(
        "issue1216 bookmark_persistence after_restore bookmarks={} restored={:?}",
        reloaded.camera_bookmark_summary(),
        restored
    );
    reset_state_store(&path);
}

#[test]
fn invalid_shell_state_aster_row_fails_loud() {
    let path = test_state_path("invalid");
    reset_state_store(&path);
    write_state_bytes(&path, b"{not-json}\n");
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let mut store = ShellStateStore::new(path.clone());

    match store.load_or_initialize(settings, base) {
        Err(EngineError::SerializationFailed { format, reason }) => {
            assert_eq!(format, "json");
            assert!(reason.contains("parse"));
            assert!(reason.contains("key must be a string"));
        }
        other => panic!("expected JSON parse failure, got {other:?}"),
    }
    reset_state_store(&path);
}

fn test_state_path(name: &str) -> PathBuf {
    workspace_root().join("tmp").join(format!(
        "litd-editor-shell-state-test-{name}-{}-aster",
        std::process::id()
    ))
}

fn reset_state_store(path: &Path) {
    let _ = fs::remove_dir_all(path);
    let _ = fs::remove_file(path);
}

fn read_state_text(path: &Path) -> String {
    let bytes = EditorStateStore::new(path.to_path_buf())
        .read_shell_state()
        .unwrap()
        .expect("shell state Aster row");
    String::from_utf8(bytes).unwrap()
}

fn write_state_bytes(path: &Path, bytes: &[u8]) {
    EditorStateStore::new(path.to_path_buf())
        .put_shell_state(bytes)
        .unwrap();
}

fn write_test_state_record(path: &Path, schema_version: u32, plan: &EditorShellPlan) {
    let record = ShellStateFile {
        schema_version,
        record_kind: STATE_RECORD_KIND.to_string(),
        persistence: persist_shell_state(plan).unwrap(),
        world_layers: None,
        play_camera_rig: None,
        camera_bookmarks: None,
        command_keybindings: None,
    };
    let json = serde_json::to_string_pretty(&record).unwrap();
    write_state_bytes(path, json.as_bytes());
}

fn write_schema_six_foliage_only_state_record(path: &Path, plan: &EditorShellPlan) {
    let persistence = serde_json::to_value(persist_shell_state(plan).unwrap()).unwrap();
    let record = serde_json::json!({
        "schema_version": SCHEMA_SIX_VERSION,
        "record_kind": STATE_RECORD_KIND,
        "persistence": persistence,
        "world_layers": {
            "foliage": false
        }
    });
    let json = serde_json::to_string_pretty(&record).unwrap();
    write_state_bytes(path, json.as_bytes());
}
