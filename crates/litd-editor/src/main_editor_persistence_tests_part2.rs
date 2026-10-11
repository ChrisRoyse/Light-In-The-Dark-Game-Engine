use super::*;
use litd_editor::{
    editor_preferences::{
        EditorPreferences, EditorPreferencesStore, EditorThemePreference, EditorUnitsPreference,
    },
    history::{EditorDocument, EditorObject, UndoRedoStack},
    shell::{DockPanelKind, build_editor_shell_plan, pop_out_panel},
};
use litd_engine::math::{Transform, Vec2};
use litd_engine::render::ui::UiRect;
use serde_json::json;
use std::fs;

#[test]
fn issue1357_editor_state_lives_in_calyx_aster_fsv() {
    let root = fsv_root().join("issue1357_editor_state_calyx");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let happy = issue1357_happy(&root.join("happy"));
    let empty_payload = issue1357_edge_empty_payload(&root.join("edge-empty-payload"));
    let corrupt_shell = issue1357_edge_corrupt_shell(&root.join("edge-corrupt-shell"));
    let corrupt_preferences =
        issue1357_edge_corrupt_preferences(&root.join("edge-corrupt-preferences"));
    let missing_history_mvcc =
        issue1357_edge_missing_history_mvcc(&root.join("edge-missing-history-mvcc"));

    let readback = json!({
        "issue": 1357,
        "source_of_truth": "Aster Graph CF rows and MVCC sequence readback for shell state, preferences, and history undo snapshots",
        "happy": happy,
        "edges": {
            "empty_payload": empty_payload,
            "corrupt_shell": corrupt_shell,
            "corrupt_preferences": corrupt_preferences,
            "missing_history_mvcc": missing_history_mvcc,
        },
    });
    let readback_path = root.join("issue1357-editor-state-calyx-readback.json");
    fs::write(
        &readback_path,
        serde_json::to_vec_pretty(&readback).unwrap(),
    )
    .unwrap();
    println!("ISSUE1357_FSV_ROOT={}", root.display());
    println!("ISSUE1357_READBACK={}", readback_path.display());
    println!("{}", serde_json::to_string_pretty(&readback).unwrap());

    assert_eq!(readback["happy"]["shell"]["graph_rows"], 1);
    assert_eq!(readback["happy"]["preferences"]["graph_rows"], 1);
    assert_eq!(readback["happy"]["history"]["graph_rows"], 1);
    assert_eq!(readback["happy"]["shell"]["loaded_mode"], "Vfx");
    assert_eq!(
        readback["happy"]["preferences"]["loaded_theme"],
        "HighContrast"
    );
    assert_eq!(
        readback["happy"]["history"]["undo_restored_revision"],
        "calyx-rev-1"
    );
    assert_eq!(
        readback["happy"]["history"]["redo_restored_revision"],
        "calyx-rev-2"
    );
    assert_eq!(readback["happy"]["legacy_json_side_store_paths"], json!([]));
    assert_eq!(
        readback["edges"]["empty_payload"]["error_field"],
        "editor.state.payload"
    );
    assert!(
        readback["edges"]["corrupt_shell"]["error"]
            .as_str()
            .unwrap()
            .contains("parse")
    );
    assert!(
        readback["edges"]["corrupt_preferences"]["error"]
            .as_str()
            .unwrap()
            .contains("parse")
    );
    assert!(
        readback["edges"]["missing_history_mvcc"]["error"]
            .as_str()
            .unwrap()
            .contains("mvcc")
    );
}

fn issue1357_happy(root: &Path) -> serde_json::Value {
    fs::create_dir_all(root).unwrap();
    let shell_dir = root.join("shell-aster");
    let preferences_dir = root.join("preferences-aster");
    let history_dir = root.join("history-aster");
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let base = build_editor_shell_plan(settings).unwrap();
    let switched = switch_editor_mode(&base, EditorMode::Vfx).unwrap();
    let floating = UiRect::new(Vec2::new(1200.0, 120.0), Vec2::new(1800.0, 840.0)).unwrap();
    let popped = pop_out_panel(&switched, DockPanelKind::RightInspector, floating).unwrap();
    let mut shell_store = ShellStateStore::new(shell_dir.clone());
    shell_store.save_plan(&popped).unwrap();
    let shell_bytes = EditorStateStore::new(shell_dir.clone())
        .read_shell_state()
        .unwrap()
        .unwrap();
    let mut shell_reader = ShellStateStore::new(shell_dir.clone());
    let loaded_shell = shell_reader
        .load_or_initialize(settings, base.clone())
        .unwrap();

    let preferences_store =
        EditorPreferencesStore::new(preferences_dir.clone(), root.join("autosave-aster"));
    let mut preferences = EditorPreferences {
        units: EditorUnitsPreference::Meters,
        theme: EditorThemePreference::HighContrast,
        ..Default::default()
    };
    preferences.autosave.interval_seconds = Some(37);
    let preferences_write = preferences_store.save_preferences(&preferences).unwrap();
    let loaded_preferences = preferences_store.load_preferences().unwrap();
    let preferences_bytes = EditorStateStore::new(preferences_dir.clone())
        .read_preferences()
        .unwrap()
        .unwrap();

    let history_store = EditorStateStore::new(history_dir.clone());
    let before = issue1357_document("calyx-rev-1", 1.0);
    let after = issue1357_document("calyx-rev-2", 8.0);
    let mut document = after.clone();
    let mut history = UndoRedoStack::new(8).unwrap();
    history
        .record_with_mvcc(&history_store, "move unit", before.clone(), after.clone())
        .unwrap();
    let before_seq = history.undo[0].before_snapshot_seq.unwrap();
    let after_seq = history.undo[0].after_snapshot_seq.unwrap();
    let before_bytes = history_store
        .read_history_bytes_at(&before.calyx_document_id, before_seq)
        .unwrap()
        .unwrap();
    let after_bytes = history_store
        .read_history_bytes_at(&after.calyx_document_id, after_seq)
        .unwrap()
        .unwrap();
    history
        .undo_from_mvcc(&history_store, &mut document)
        .unwrap();
    let undo_revision = document.revision_id.clone();
    history
        .redo_from_mvcc(&history_store, &mut document)
        .unwrap();
    let redo_revision = document.revision_id.clone();

    json!({
        "shell": {
            "vault_dir": shell_dir.display().to_string(),
            "graph_rows": EditorStateStore::new(shell_dir.clone()).read_graph_rows().unwrap().len(),
            "latest_seq": EditorStateStore::new(shell_dir.clone()).latest_seq().unwrap(),
            "bytes": shell_bytes.len(),
            "sha256": litd_hash::sha256_hex(&shell_bytes),
            "loaded_mode": format!("{:?}", loaded_shell.active_mode),
            "loaded_from_aster": shell_reader.loaded_from_disk(),
        },
        "preferences": {
            "vault_dir": preferences_dir.display().to_string(),
            "graph_rows": EditorStateStore::new(preferences_dir.clone()).read_graph_rows().unwrap().len(),
            "latest_seq": EditorStateStore::new(preferences_dir.clone()).latest_seq().unwrap(),
            "bytes": preferences_bytes.len(),
            "sha256": litd_hash::sha256_hex(&preferences_bytes),
            "write_readback_bytes": preferences_write.bytes,
            "loaded_theme": format!("{:?}", loaded_preferences.theme),
            "loaded_units": format!("{:?}", loaded_preferences.units),
        },
        "history": {
            "vault_dir": history_dir.display().to_string(),
            "graph_rows": history_store.read_graph_rows().unwrap().len(),
            "latest_seq": history_store.latest_seq().unwrap(),
            "before_seq": before_seq,
            "after_seq": after_seq,
            "before_sha256": litd_hash::sha256_hex(&before_bytes),
            "after_sha256": litd_hash::sha256_hex(&after_bytes),
            "undo_restored_revision": undo_revision,
            "redo_restored_revision": redo_revision,
        },
        "legacy_json_side_store_paths": legacy_json_side_store_paths(root),
    })
}

fn issue1357_edge_empty_payload(root: &Path) -> serde_json::Value {
    let store = EditorStateStore::new(root.join("state-aster"));
    let before = store.read_graph_rows().unwrap_or_default();
    let error = store.put_shell_state(b"").unwrap_err();
    let after = store.read_graph_rows().unwrap_or_default();
    json!({
        "before_rows": before.len(),
        "error": error.to_string(),
        "error_field": engine_error_field(&error),
        "after_rows": after.len(),
    })
}

fn issue1357_edge_corrupt_shell(root: &Path) -> serde_json::Value {
    let path = root.join("shell-aster");
    let store = EditorStateStore::new(path.clone());
    store.put_shell_state(b"{not-json}\n").unwrap();
    let before = store.read_graph_rows().unwrap();
    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let mut shell = ShellStateStore::new(path);
    let error = shell
        .load_or_initialize(settings, build_editor_shell_plan(settings).unwrap())
        .unwrap_err();
    let after = store.read_graph_rows().unwrap();
    json!({
        "before_rows": before.len(),
        "error": error.to_string(),
        "after_rows": after.len(),
        "after_sha256": after[0].sha256,
    })
}

fn issue1357_edge_corrupt_preferences(root: &Path) -> serde_json::Value {
    let path = root.join("preferences-aster");
    let store = EditorStateStore::new(path.clone());
    store.put_preferences(b"{not-json}\n").unwrap();
    let before = store.read_graph_rows().unwrap();
    let preferences = EditorPreferencesStore::new(path, root.join("autosave-aster"));
    let error = preferences.load_preferences().unwrap_err();
    let after = store.read_graph_rows().unwrap();
    json!({
        "before_rows": before.len(),
        "error": error.to_string(),
        "after_rows": after.len(),
        "after_sha256": after[0].sha256,
    })
}

fn issue1357_edge_missing_history_mvcc(root: &Path) -> serde_json::Value {
    let store = EditorStateStore::new(root.join("history-aster"));
    let before = issue1357_document("calyx-rev-1", 1.0);
    let after = issue1357_document("calyx-rev-2", 8.0);
    let mut document = after.clone();
    let mut history = UndoRedoStack::new(8).unwrap();
    history.record("move unit", before, after).unwrap();
    let error = history.undo_from_mvcc(&store, &mut document).unwrap_err();
    json!({
        "graph_rows": store.read_graph_rows().unwrap_or_default().len(),
        "error": error.to_string(),
        "error_field": engine_error_field(&error),
    })
}

fn issue1357_document(revision: &str, x: f32) -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1357-editor-state", revision).unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(EditorObject::new(
            "unit_1357",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(x, 0.0, 4.0)),
        ))
        .unwrap();
    document
}

fn legacy_json_side_store_paths(root: &Path) -> Vec<String> {
    let mut paths = Vec::new();
    collect_legacy_json_side_store_paths(root, &mut paths);
    paths.sort();
    paths
}

fn collect_legacy_json_side_store_paths(path: &Path, out: &mut Vec<String>) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_file() {
        if is_legacy_editor_json_side_store(path) {
            out.push(path.display().to_string());
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect_legacy_json_side_store_paths(&entry.path(), out);
    }
}

fn is_legacy_editor_json_side_store(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    matches!(
        name,
        "litd-editor-shell-state.json"
            | "litd-editor-preferences.json"
            | "preferences.json"
            | "shell-state.json"
    ) || (name.starts_with("litd-editor-shell-state-") && name.ends_with(".json"))
}

fn engine_error_field(error: &EngineError) -> String {
    match error {
        EngineError::UiRenderInvalidConfig { field, .. } => field.clone(),
        other => format!("{other:?}"),
    }
}

fn fsv_root() -> PathBuf {
    std::env::var_os("CALYX_FSV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target").join("fsv"))
}
