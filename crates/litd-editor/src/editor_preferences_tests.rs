use super::*;

use std::path::PathBuf;

use crate::editor_state_store::EditorStateStore;
use crate::history::{EditorDocument, EditorObject, stable_fingerprint};
use litd_engine::math::{Transform, Vec3};

#[test]
fn preferences_persist_across_restart_with_known_autosave_interval() {
    let (_temp_root, prefs_path, aster_dir) = test_paths("persist");
    let store = EditorPreferencesStore::new(prefs_path.clone(), aster_dir.clone());
    let mut prefs = EditorPreferences {
        units: EditorUnitsPreference::Meters,
        theme: EditorThemePreference::HighContrast,
        snap_grid_size: 0.5,
        text_scale: 1.5,
        ..Default::default()
    };
    prefs.autosave.interval_seconds = Some(37);
    let write = store.save_preferences(&prefs).unwrap();

    let restarted = EditorPreferencesStore::new(prefs_path, aster_dir);
    let loaded = restarted.load_preferences().unwrap();
    let outcome = restarted.load_or_default_loud();

    assert!(write.bytes > 64);
    assert_eq!(loaded, prefs);
    assert!(outcome.loaded_from_disk);
    assert_eq!(outcome.preferences.autosave.interval_seconds, Some(37));
    assert_eq!(
        outcome.preferences.theme,
        EditorThemePreference::HighContrast
    );
    assert_eq!(outcome.preferences.text_scale, 1.5);
}

#[test]
fn text_scale_defaults_for_old_records_and_rejects_invalid_values() {
    let mut value = serde_json::to_value(EditorPreferences::default()).unwrap();
    value.as_object_mut().unwrap().remove("text_scale");
    let old: EditorPreferences = serde_json::from_value(value).unwrap();
    assert_eq!(old.text_scale, 1.0);
    old.validate().unwrap();

    for invalid in [0.99, 2.01, f32::NAN, f32::INFINITY] {
        let preferences = EditorPreferences {
            text_scale: invalid,
            ..Default::default()
        };
        let error = preferences.validate().unwrap_err().to_string();
        assert!(error.contains("text_scale"), "error={error}");
    }
}

#[test]
fn autosave_crash_recovery_restores_last_document_from_aster() {
    let (_temp_root, prefs_path, aster_dir) = test_paths("recover");
    let store = EditorPreferencesStore::new(prefs_path, aster_dir);
    let prefs = EditorPreferences::default();
    let document = fixture_document();
    let expected = stable_fingerprint(&document).unwrap();

    let write = store.write_autosave(&prefs, &document).unwrap();
    let recovered = store
        .recover_autosave(&document.calyx_document_id, RecoveryChoice::Accept)
        .unwrap();

    assert_eq!(write.status, AutosaveWriteStatus::Written);
    assert!(write.seq > 0);
    assert!(write.graph_rows > 0);
    assert!(recovered.accepted);
    assert_eq!(recovered.recovered_fingerprint, expected);
    assert_eq!(recovered.document.unwrap(), document);
}

#[test]
fn corrupt_preferences_are_reported_while_ux_defaults_are_explicit() {
    let (_temp_root, prefs_path, aster_dir) = test_paths("corrupt");
    reset_state_store(&prefs_path);
    EditorStateStore::new(prefs_path.clone())
        .put_preferences(b"{ not valid json")
        .unwrap();
    let store = EditorPreferencesStore::new(prefs_path, aster_dir);

    let strict_error = store.load_preferences().unwrap_err();
    let outcome = store.load_or_default_loud();

    assert!(strict_error.to_string().contains("parse"));
    assert!(outcome.recovered_from_corruption);
    assert!(!outcome.loaded_from_disk);
    assert_eq!(outcome.preferences, EditorPreferences::default());
    assert!(outcome.error.contains("parse"));
}

#[test]
fn recovery_declined_and_interval_off_do_not_restore_or_write_snapshot() {
    let (_temp_root, prefs_path, aster_dir) = test_paths("decline-off");
    let store = EditorPreferencesStore::new(prefs_path, aster_dir);
    let document = fixture_document();
    let mut prefs = EditorPreferences::default();
    prefs.autosave.interval_seconds = None;

    let disabled = store.write_autosave(&prefs, &document).unwrap();
    let declined = store
        .recover_autosave(&document.calyx_document_id, RecoveryChoice::Decline)
        .unwrap();

    assert_eq!(disabled.status, AutosaveWriteStatus::Disabled);
    assert_eq!(disabled.seq, 0);
    assert!(!declined.accepted);
    assert!(declined.document.is_none());
    assert_eq!(declined.recovered_fingerprint, "declined");
}

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new("calyx://world/issue1221-prefs", "rev-1221").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_recovered",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(2.0, 0.0, 4.0)),
            )
            .with_property("name", "Recovered Unit"),
        )
        .unwrap();
    document
}

// #1535: own a `TestTempDir` guard (deletes on Drop) instead of leaking `%TEMP%/litd-editor-
// issue1221-*` via a raw `env::temp_dir().join(...)` with no cleanup. Callers MUST bind the guard.
fn test_paths(name: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf, PathBuf) {
    let root = litd_engine::test_temp::TestTempDir::new(&format!("editor-issue1221-{name}"));
    let prefs = root.path().join("preferences-aster");
    let autosave = root.path().join("autosave-aster");
    (root, prefs, autosave)
}

fn reset_state_store(path: &std::path::Path) {
    let _ = std::fs::remove_dir_all(path);
    let _ = std::fs::remove_file(path);
}
