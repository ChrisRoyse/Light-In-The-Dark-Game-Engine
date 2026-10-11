use std::{fs, path::PathBuf};

use litd_editor::{
    asset_placement::{
        AssetDropRequest, ContentBrowserAssetRef, PLACED_ASSET_RELATIVE_PATH_PROPERTY,
        PLACED_ASSET_SHA256_PROPERTY, place_assets_in_world_record,
    },
    asset_vault::AssetVaultCatalog,
    world_creation::{
        EditorWorldCreationSpec, WorldAsterReadback, create_editor_world_record,
        parse_world_record, read_world_record, write_world_record,
    },
};
use litd_engine::math::Vec3;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_ASSET_DRAG_DROP_FSV",
        "issue1223_asset_drag_drop",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1223 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");

    let mut record = create_editor_world_record(EditorWorldCreationSpec::issue1210_default())?;
    record.spec.calyx_document_id = "calyx://world/issue1223-drag-drop-fsv".to_string();
    record.document.calyx_document_id = record.spec.calyx_document_id.clone();
    let assets = model_assets(2)?;
    let happy_request = AssetDropRequest {
        request_id: "issue1223_happy".to_string(),
        assets: vec![assets[0].clone()],
        drop_world: Vec3::new(4.35, 0.0, 6.6),
        surface_valid: true,
        snap_grid: Some(1.0),
        clamp_to_playable_bounds: true,
        multi_asset_spacing: 2.0,
    };
    let happy_report = place_assets_in_world_record(&mut record, &happy_request)?;
    let happy_aster = write_and_read_world(&root, &aster_dir, &record)?;
    let happy_readback = parse_world_record(&happy_aster.bytes)?;
    let happy_object = object_snapshot(&happy_readback, &happy_report.placed_object_ids[0])?;

    let invalid_edge = invalid_surface_edge(&record, &assets[0])?;
    let clamp_edge = clamp_edge(&root, &aster_dir, &assets)?;
    let multi_edge = multi_asset_edge(&root, &aster_dir, &assets)?;

    let mut snapshot = json!({
        "schema": "litd.issue1223.asset_drag_drop_fsv.v1",
        "issue": 1223,
        "source_of_truth": "Aster world record readback plus EditorDocument.objects object snapshots",
        "artifact_path": path,
        "before": {
            "artifact": before_artifact,
            "world_document_id": record.spec.calyx_document_id,
            "asset_paths": assets.iter().map(|asset| asset.relative_path.clone()).collect::<Vec<_>>()
        },
        "happy_path": {
            "before": {
                "drop_world": [4.35, 0.0, 6.6],
                "snap_grid": 1.0,
                "asset": assets[0]
            },
            "after": {
                "report": happy_report,
                "aster": readback_summary(&happy_aster),
                "readback_object": happy_object,
                "expected_translation": [4.0, 0.0, 7.0]
            }
        },
        "edges": [invalid_edge, clamp_edge, multi_edge],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote asset drag/drop FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "happy readback object={} translation={:?} asset={}",
        happy_report.placed_object_ids[0],
        happy_report.readback[0].translation,
        happy_report.readback[0].asset_relative_path
    );
    Ok(())
}

fn invalid_surface_edge(
    source_record: &litd_editor::world_creation::EditorWorldRecord,
    asset: &ContentBrowserAssetRef,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut probe = source_record.clone();
    let before = probe.document_fingerprint.clone();
    let request = AssetDropRequest {
        request_id: "issue1223_invalid_surface".to_string(),
        assets: vec![asset.clone()],
        drop_world: Vec3::new(5.0, 0.0, 5.0),
        surface_valid: false,
        snap_grid: Some(1.0),
        clamp_to_playable_bounds: true,
        multi_asset_spacing: 2.0,
    };
    let error = place_assets_in_world_record(&mut probe, &request)
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "invalid_surface_rejected",
        "before": {"fingerprint": before, "object_count": source_record.document.objects.len()},
        "after": {
            "fingerprint": probe.document_fingerprint,
            "object_count": probe.document.objects.len(),
            "unchanged": before == probe.document_fingerprint,
            "error": error
        }
    }))
}

fn clamp_edge(
    root: &std::path::Path,
    aster_dir: &std::path::Path,
    assets: &[ContentBrowserAssetRef],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = create_editor_world_record(EditorWorldCreationSpec::issue1210_default())?;
    record.spec.calyx_document_id = "calyx://world/issue1223-clamp-fsv".to_string();
    record.document.calyx_document_id = record.spec.calyx_document_id.clone();
    let request = AssetDropRequest {
        request_id: "issue1223_clamp".to_string(),
        assets: vec![assets[0].clone()],
        drop_world: Vec3::new(999.0, 0.0, -12.0),
        surface_valid: true,
        snap_grid: Some(1.0),
        clamp_to_playable_bounds: true,
        multi_asset_spacing: 2.0,
    };
    let report = place_assets_in_world_record(&mut record, &request)?;
    let readback = parse_world_record(&write_and_read_world(root, aster_dir, &record)?.bytes)?;
    Ok(json!({
        "edge": "outside_bounds_clamped",
        "before": {"drop_world": [999.0, 0.0, -12.0], "playable_bounds": "0,0..16,12"},
        "after": {
            "clamped": report.clamped,
            "object": object_snapshot(&readback, &report.placed_object_ids[0])?,
            "expected_translation": [16.0, 0.0, 0.0]
        }
    }))
}

fn multi_asset_edge(
    root: &std::path::Path,
    aster_dir: &std::path::Path,
    assets: &[ContentBrowserAssetRef],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = create_editor_world_record(EditorWorldCreationSpec::issue1210_default())?;
    record.spec.calyx_document_id = "calyx://world/issue1223-multi-fsv".to_string();
    record.document.calyx_document_id = record.spec.calyx_document_id.clone();
    let request = AssetDropRequest {
        request_id: "issue1223_multi".to_string(),
        assets: assets.to_vec(),
        drop_world: Vec3::new(2.0, 0.0, 3.0),
        surface_valid: true,
        snap_grid: Some(1.0),
        clamp_to_playable_bounds: true,
        multi_asset_spacing: 2.0,
    };
    let report = place_assets_in_world_record(&mut record, &request)?;
    let readback = parse_world_record(&write_and_read_world(root, aster_dir, &record)?.bytes)?;
    let objects = report
        .placed_object_ids
        .iter()
        .map(|id| object_snapshot(&readback, id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "edge": "multi_asset_drag",
        "before": {"asset_count": assets.len(), "drop_world": [2.0, 0.0, 3.0]},
        "after": {"placed_object_ids": report.placed_object_ids, "objects": objects}
    }))
}

fn write_and_read_world(
    _root: &std::path::Path,
    aster_dir: &std::path::Path,
    record: &litd_editor::world_creation::EditorWorldRecord,
) -> Result<WorldAsterReadback, Box<dyn std::error::Error>> {
    write_world_record(aster_dir, record)?;
    read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id).into())
}

fn object_snapshot(
    record: &litd_editor::world_creation::EditorWorldRecord,
    object_id: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let object = record
        .document
        .objects
        .get(object_id)
        .ok_or_else(|| format!("missing object {object_id} in Aster readback"))?;
    Ok(json!({
        "object_id": object.id,
        "kind": object.kind,
        "translation": object.transform.translation.to_array(),
        "asset_relative_path": object.properties.get(PLACED_ASSET_RELATIVE_PATH_PROPERTY),
        "asset_sha256": object.properties.get(PLACED_ASSET_SHA256_PROPERTY)
    }))
}

fn model_assets(count: usize) -> Result<Vec<ContentBrowserAssetRef>, Box<dyn std::error::Error>> {
    let catalog = AssetVaultCatalog::load_default()?;
    let assets = catalog
        .records()
        .iter()
        .filter(|record| {
            let path = record.relative_path.to_ascii_lowercase();
            (path.ends_with(".gltf") || path.ends_with(".glb")) && record.bytes_on_disk > 0
        })
        .take(count)
        .map(ContentBrowserAssetRef::from_vault_record)
        .collect::<Vec<_>>();
    if assets.len() != count {
        return Err(format!("need {count} real glTF/GLB assets, found {}", assets.len()).into());
    }
    Ok(assets)
}

fn readback_summary(readback: &WorldAsterReadback) -> serde_json::Value {
    json!({"seq": readback.seq, "graph_rows": readback.graph_rows, "bytes": readback.bytes.len()})
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
