use std::{
    fs,
    path::{Path, PathBuf},
};

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_editor::{
    editor_preferences::{
        AutosaveWriteStatus, EditorAutosaveSnapshot, EditorPreferences, EditorPreferencesStore,
        RecoveryChoice,
    },
    history::{EditorDocument, EditorObject, UndoRedoStack, stable_fingerprint, to_stable_ron},
};
use litd_engine::math::{Transform, Vec3};
use litd_hash::sha256_hex;
use serde_json::json;

const AUTOSAVE_COLLECTION: &str = "litd_editor_autosave";
const AUTOSAVE_COLUMN: &[u8] = b"autosave_snapshot_json";
const AUTOSAVE_ROW_PREFIX: &str = "autosave/";
const PREFS_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
const PREFS_VAULT_PURPOSE: &[u8] = b"litd-editor-preferences-issue1221";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, artifact_dir) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE739_FSV_DIR",
        "issue739_editor_undo_autosave_source",
        "manual",
    )?;

    let mut document = fixture_document()?;
    let mut history = UndoRedoStack::new(8)?;
    let base_fingerprint = stable_fingerprint(&document)?;

    let after_move = record_edit(&mut history, &mut document, "edit1.move_unit", |doc| {
        doc.revision_id = "rev-739-edit-1".to_string();
        doc.objects
            .get_mut("unit_knight")
            .ok_or("unit_knight missing")?
            .transform
            .translation = Vec3::new(8.0, 0.0, -3.0);
        Ok(())
    })?;
    let after_rename = record_edit(&mut history, &mut document, "edit2.rename_unit", |doc| {
        doc.revision_id = "rev-739-edit-2".to_string();
        doc.objects
            .get_mut("unit_knight")
            .ok_or("unit_knight missing")?
            .properties
            .insert("name".to_string(), "Undo Redo Captain".to_string());
        Ok(())
    })?;
    let after_vfx = record_edit(&mut history, &mut document, "edit3.add_vfx", |doc| {
        doc.revision_id = "rev-739-edit-3".to_string();
        doc.upsert_object(EditorObject::new(
            "vfx_undo_probe",
            "Vfx",
            Some("unit_knight".to_string()),
            Transform::from_translation(Vec3::new(8.0, 2.0, -3.0)),
        ))?;
        Ok(())
    })?;

    let undo_first = history
        .undo(&mut document)?
        .ok_or("first undo returned no history entry")?;
    let undo_second = history
        .undo(&mut document)?
        .ok_or("second undo returned no history entry")?;
    let redo_first = history
        .redo(&mut document)?
        .ok_or("redo returned no history entry")?;
    assert_eq!(document, after_rename);
    assert_eq!(undo_first, "edit3.add_vfx");
    assert_eq!(undo_second, "edit2.rename_unit");
    assert_eq!(redo_first, "edit2.rename_unit");
    assert!(!document.objects.contains_key("vfx_undo_probe"));
    assert_eq!(
        document.objects["unit_knight"].properties["name"],
        "Undo Redo Captain"
    );
    assert_eq!(
        document.objects["unit_knight"].transform.translation,
        Vec3::new(8.0, 0.0, -3.0)
    );

    let expected_source_text = to_stable_ron(&after_rename)?;
    let source_path = artifact_dir.join("document_after_undo_undo_redo.ron");
    write_text(&source_path, &expected_source_text)?;
    let source_readback_text = fs::read_to_string(&source_path)?;
    if source_readback_text != expected_source_text {
        return Err("source readback did not match expected post-redo state".into());
    }
    let source_readback_doc: EditorDocument = ron::from_str(&source_readback_text)?;
    if source_readback_doc != after_rename {
        return Err("source readback document did not match expected post-redo state".into());
    }

    let resave_text = to_stable_ron(&source_readback_doc)?;
    let resave_path = artifact_dir.join("document_resave.ron");
    write_text(&resave_path, &resave_text)?;
    if fs::read(&resave_path)? != fs::read(&source_path)? {
        return Err("stable source re-save produced a byte diff".into());
    }

    let prefs_path = artifact_dir.join("preferences-aster");
    let autosave_aster_dir = artifact_dir.join("autosave-aster");
    let store = EditorPreferencesStore::new(prefs_path.clone(), autosave_aster_dir.clone());
    let mut prefs = EditorPreferences::default();
    prefs.autosave.interval_seconds = Some(5);
    let autosave_write = store.write_autosave(&prefs, &document)?;
    if autosave_write.status != AutosaveWriteStatus::Written {
        return Err("autosave write did not persist a snapshot".into());
    }
    let raw_autosave = read_autosave_snapshot(&autosave_aster_dir, &document.calyx_document_id)?;
    let autosave_snapshot: EditorAutosaveSnapshot = serde_json::from_slice(&raw_autosave.bytes)?;
    if autosave_snapshot.document != document {
        return Err("raw autosave snapshot document did not match current document".into());
    }
    if autosave_snapshot.autosave.stable_ron != expected_source_text {
        return Err("raw autosave source text did not match canonical source text".into());
    }
    let autosave_source_path = artifact_dir.join("autosave_sourceform.ron");
    write_text(
        &autosave_source_path,
        &autosave_snapshot.autosave.stable_ron,
    )?;

    let restarted_store = EditorPreferencesStore::new(prefs_path, autosave_aster_dir);
    let recovered =
        restarted_store.recover_autosave(&document.calyx_document_id, RecoveryChoice::Accept)?;
    let recovered_document = recovered
        .document
        .as_ref()
        .ok_or("accepted autosave recovery returned no document")?;
    if recovered_document != &document {
        return Err("crash recovery did not return the autosaved document".into());
    }

    let undo_empty_edge = undo_past_start_edge()?;
    let source_bytes = fs::read(&source_path)?;
    let resave_bytes = fs::read(&resave_path)?;
    let autosave_bytes = fs::read(&autosave_source_path)?;
    let report = json!({
        "schema": "litd.issue739.editor_sourceform_fsv.v1",
        "issue": 739,
        "source_of_truth": "UndoRedoStack state plus canonical stable RON files and raw Aster autosave snapshot readback",
        "run_id": fsv_run.run_id(),
        "sourceform_kind": "stable_ron_editor_document",
        "paths": {
            "source_after_undo_undo_redo": source_path,
            "resave": resave_path,
            "autosave_sourceform": autosave_source_path
        },
        "three_edit_sequence": {
            "base_fingerprint": base_fingerprint,
            "after_move_fingerprint": stable_fingerprint(&after_move)?,
            "after_rename_fingerprint": stable_fingerprint(&after_rename)?,
            "after_vfx_fingerprint": stable_fingerprint(&after_vfx)?,
            "undo_labels": [undo_first, undo_second],
            "redo_labels": [redo_first],
            "final_fingerprint": stable_fingerprint(&document)?,
            "expected_final_fingerprint": stable_fingerprint(&after_rename)?,
            "final_equals_expected": document == after_rename,
            "vfx_removed_after_undo_undo_redo": !document.objects.contains_key("vfx_undo_probe"),
            "unit_translation": {
                "x": document.objects["unit_knight"].transform.translation.x,
                "y": document.objects["unit_knight"].transform.translation.y,
                "z": document.objects["unit_knight"].transform.translation.z
            },
            "unit_name": document.objects["unit_knight"].properties["name"]
        },
        "source_readback": {
            "bytes": source_bytes.len(),
            "sha256": sha256_hex(&source_bytes),
            "matches_expected_text": source_readback_text == expected_source_text,
            "parsed_equals_expected": source_readback_doc == after_rename
        },
        "autosave": {
            "write_report": autosave_write,
            "raw_seq": raw_autosave.seq,
            "raw_graph_rows": raw_autosave.graph_rows,
            "snapshot_bytes": raw_autosave.bytes.len(),
            "snapshot_sha256": sha256_hex(&raw_autosave.bytes),
            "stable_ron_bytes": autosave_bytes.len(),
            "stable_ron_sha256": sha256_hex(&autosave_bytes),
            "stable_ron_matches_source": autosave_snapshot.autosave.stable_ron == expected_source_text,
            "recovery_accepted": recovered.accepted,
            "recovered_fingerprint": recovered.recovered_fingerprint,
            "recovered_equals_final": recovered_document == &document
        },
        "edges": [
            undo_empty_edge,
            json!({
                "edge": "crash_recover_from_autosave",
                "before": {"store_restarted": true},
                "after": {
                    "accepted": recovered.accepted,
                    "document_present": recovered.document.is_some(),
                    "document_matches_final": recovered_document == &document
                }
            }),
            json!({
                "edge": "resave_byte_stable_no_spurious_diff",
                "before": {
                    "source_sha256": sha256_hex(&source_bytes),
                    "source_bytes": source_bytes.len()
                },
                "after": {
                    "resave_sha256": sha256_hex(&resave_bytes),
                    "resave_bytes": resave_bytes.len(),
                    "byte_stable": source_bytes == resave_bytes
                }
            })
        ]
    });
    let readback = fsv_run.write_json_artifact("manual/issue739-sourceform-fsv.json", &report)?;
    println!(
        "wrote issue739 FSV report to {} sha256={}",
        readback.path.display(),
        readback.sha256
    );
    println!(
        "final_fingerprint={} source_sha256={} autosave_sha256={}",
        stable_fingerprint(&document)?,
        sha256_hex(&source_bytes),
        sha256_hex(&raw_autosave.bytes)
    );
    Ok(())
}

fn record_edit<F>(
    history: &mut UndoRedoStack,
    document: &mut EditorDocument,
    label: &str,
    edit: F,
) -> Result<EditorDocument, Box<dyn std::error::Error>>
where
    F: FnOnce(&mut EditorDocument) -> Result<(), Box<dyn std::error::Error>>,
{
    let before = document.clone();
    edit(document)?;
    document.validate()?;
    let after = document.clone();
    history.record(label, before, after.clone())?;
    Ok(after)
}

fn fixture_document() -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new("calyx://world/issue739-sourceform", "rev-739-base")?;
    document.upsert_object(
        EditorObject::new("world", "World", None, Transform::IDENTITY)
            .with_property("name", "Issue 739 Sourceform World"),
    )?;
    document.upsert_object(
        EditorObject::new(
            "unit_knight",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(2.0, 0.0, -3.0)),
        )
        .with_property("name", "Knight")
        .with_property("faction", "Dawnward"),
    )?;
    document.validate()?;
    Ok(document)
}

fn undo_past_start_edge() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut stack = UndoRedoStack::new(2)?;
    let mut document = fixture_document()?;
    let before = stable_fingerprint(&document)?;
    let label = stack.undo(&mut document)?;
    let after = stable_fingerprint(&document)?;
    Ok(json!({
        "edge": "undo_past_start_no_op",
        "before": {"fingerprint": before},
        "after": {
            "label": label,
            "fingerprint": after,
            "unchanged": before == after
        }
    }))
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text.as_bytes())?;
    Ok(())
}

#[derive(Debug)]
struct RawAutosaveSnapshot {
    bytes: Vec<u8>,
    seq: u64,
    graph_rows: usize,
}

fn read_autosave_snapshot(
    dir: &PathBuf,
    document_id: &str,
) -> Result<RawAutosaveSnapshot, Box<dyn std::error::Error>> {
    let vault_id = PREFS_VAULT_ID.parse::<VaultId>()?;
    let vault = AsterVault::<SystemClock>::open(
        dir,
        vault_id,
        PREFS_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )?;
    let column = PlainColumn::new(&vault, AUTOSAVE_COLLECTION)?;
    let seq = vault.latest_seq();
    let row = format!("{AUTOSAVE_ROW_PREFIX}{document_id}").into_bytes();
    let bytes = column
        .get(seq, &row, AUTOSAVE_COLUMN)?
        .ok_or("autosave snapshot missing from Aster readback")?;
    let graph_rows = vault.scan_cf_at(seq, ColumnFamily::Graph)?.len();
    Ok(RawAutosaveSnapshot {
        bytes,
        seq,
        graph_rows,
    })
}
