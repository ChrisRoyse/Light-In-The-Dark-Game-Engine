use std::{fs, path::PathBuf};

use litd_editor::{
    editor_preferences::{
        AutosaveWriteStatus, EditorPreferences, EditorPreferencesStore, EditorThemePreference,
        EditorUnitsPreference, RecoveryChoice,
    },
    history::{EditorDocument, EditorObject, stable_fingerprint},
};
use litd_engine::math::{Transform, Vec3};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_EDITOR_PREFERENCES_FSV",
        "issue1221_preferences",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1221 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let prefs_path = root.join("preferences.json");
    let aster_dir = root.join("autosave-aster");
    let store = EditorPreferencesStore::new(prefs_path.clone(), aster_dir.clone());
    let mut prefs = EditorPreferences {
        units: EditorUnitsPreference::Meters,
        theme: EditorThemePreference::HighContrast,
        snap_grid_size: 0.5,
        ..Default::default()
    };
    prefs.autosave.interval_seconds = Some(37);
    let prefs_write = store.save_preferences(&prefs)?;
    let restarted = EditorPreferencesStore::new(prefs_path.clone(), aster_dir.clone());
    let loaded = restarted.load_preferences()?;
    let prefs_disk = serde_json::from_slice::<serde_json::Value>(&fs::read(&prefs_path)?)?;

    let document = fixture_document()?;
    let expected_document_fingerprint = stable_fingerprint(&document)?;
    let autosave_write = restarted.write_autosave(&loaded, &document)?;
    let recovered =
        restarted.recover_autosave(&document.calyx_document_id, RecoveryChoice::Accept)?;
    let recovered_document = recovered
        .document
        .as_ref()
        .ok_or("accepted recovery returned no document")?;

    let corrupt_edge = corrupt_preferences_edge(&root)?;
    let declined_edge = declined_recovery_edge(&restarted, &document)?;
    let off_edge = interval_off_edge(&root, &document)?;

    let mut snapshot = json!({
        "schema": "litd.issue1221.preferences_fsv.v1",
        "issue": 1221,
        "source_of_truth": "preferences.json readback plus Aster autosave snapshot recovery",
        "artifact_path": path,
        "before": {"artifact": before_artifact},
        "happy_path": {
            "before": {
                "preferences_path": prefs_path,
                "autosave_aster_dir": aster_dir,
                "document_fingerprint": expected_document_fingerprint
            },
            "after": {
                "preferences_write": prefs_write,
                "preferences_disk": prefs_disk,
                "loaded_after_restart": loaded,
                "autosave_write": autosave_write,
                "recovered": {
                    "accepted": recovered.accepted,
                    "fingerprint": recovered.recovered_fingerprint,
                    "object_count": recovered_document.objects.len(),
                    "unit_name": recovered_document.objects["unit_recovered"].properties["name"]
                }
            }
        },
        "edges": [corrupt_edge, declined_edge, off_edge],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote preferences FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "recovered fingerprint={} autosave_status={:?}",
        recovered.recovered_fingerprint, autosave_write.status
    );
    Ok(())
}

fn corrupt_preferences_edge(
    root: &std::path::Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let prefs_path = root.join("corrupt-preferences.json");
    fs::write(&prefs_path, b"{ not json")?;
    let store = EditorPreferencesStore::new(prefs_path, root.join("corrupt-aster"));
    let strict_error = store.load_preferences().unwrap_err().to_string();
    let outcome = store.load_or_default_loud();
    Ok(json!({
        "edge": "corrupt_preferences_loud_defaults",
        "before": {"file": "{ not json"},
        "after": {
            "strict_error": strict_error,
            "recovered_from_corruption": outcome.recovered_from_corruption,
            "loaded_from_disk": outcome.loaded_from_disk,
            "theme": format!("{:?}", outcome.preferences.theme),
            "error": outcome.error
        }
    }))
}

fn declined_recovery_edge(
    store: &EditorPreferencesStore,
    document: &EditorDocument,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let declined = store.recover_autosave(&document.calyx_document_id, RecoveryChoice::Decline)?;
    Ok(json!({
        "edge": "recovery_declined",
        "before": {"document_id": document.calyx_document_id},
        "after": {
            "accepted": declined.accepted,
            "document_present": declined.document.is_some(),
            "fingerprint": declined.recovered_fingerprint
        }
    }))
}

fn interval_off_edge(
    root: &std::path::Path,
    document: &EditorDocument,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let store = EditorPreferencesStore::new(
        root.join("off-preferences.json"),
        root.join("off-autosave-aster"),
    );
    let mut prefs = EditorPreferences::default();
    prefs.autosave.interval_seconds = None;
    let report = store.write_autosave(&prefs, document)?;
    Ok(json!({
        "edge": "autosave_interval_off",
        "before": {"interval_seconds": null},
        "after": {
            "status": format!("{:?}", report.status),
            "seq": report.seq,
            "graph_rows": report.graph_rows,
            "disabled": report.status == AutosaveWriteStatus::Disabled
        }
    }))
}

fn fixture_document() -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new("calyx://world/issue1221-prefs", "rev-1221")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_recovered",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(2.0, 0.0, 4.0)),
        )
        .with_property("name", "Recovered Unit"),
    )?;
    Ok(document)
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
