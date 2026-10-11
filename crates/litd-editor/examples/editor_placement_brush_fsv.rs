use std::{fs, path::PathBuf};

use litd_editor::{
    asset_placement::{ContentBrowserAssetRef, PLACED_ASSET_RELATIVE_PATH_PROPERTY},
    asset_vault::AssetVaultCatalog,
    placement_brush::{
        BRUSH_CONTENT_KIND_PROPERTY, BRUSH_SCALE_PROPERTY, BRUSH_STAMP_ID_PROPERTY,
        BRUSH_TERRAIN_CELL_PROPERTY, BRUSH_TERRAIN_HEIGHT_PROPERTY, BRUSH_YAW_DEGREES_PROPERTY,
        BrushRange, PlacementBrushAsset, PlacementBrushContent, PlacementBrushReadback,
        PlacementBrushReport, PlacementBrushRequest, PlacementBrushSnap,
        stamp_placement_brush_in_world_record,
    },
    prefabs::{PrefabDefinition, PrefabLibrary, PrefabObjectTemplate},
    world_creation::{
        EditorWorldCreationSpec, WorldAsterReadback, create_editor_world_record,
        parse_world_record, read_world_record, write_world_record,
    },
};
use litd_engine::math::{Transform, Vec3};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_PLACEMENT_BRUSH_FSV",
        "issue1220_placement_brush",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1220 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");
    let assets = model_assets(3)?;

    let mut record = issue1220_world("happy")?;
    let happy_request = asset_request("issue1220_happy", &assets, 8, 8, 1220);
    let happy_report = stamp_placement_brush_in_world_record(&mut record, &happy_request)?;
    let happy_aster = write_and_read_world(&root, &aster_dir, &record)?;
    let happy_readback = parse_world_record(&happy_aster.bytes)?;
    let happy_objects = report_objects(&happy_readback, &happy_report)?;
    let happy_summary = variation_summary(&happy_report);
    ensure(
        happy_summary["yaw_unique"].as_u64().unwrap_or_default() > 1
            && happy_summary["scale_unique"].as_u64().unwrap_or_default() > 1
            && happy_summary["variant_unique"].as_u64().unwrap_or_default() > 1
            && happy_summary["terrain_height_unique"]
                .as_u64()
                .unwrap_or_default()
                > 1,
        "happy path did not produce expected rotation/scale/variant/terrain variation",
    )?;
    ensure_grid_and_terrain(&happy_readback, &happy_report)?;

    let zero_jitter = zero_jitter_edge(&root, &aster_dir, &assets)?;
    let density_cap = density_cap_edge(&root, &aster_dir, &assets)?;
    let uneven_terrain = uneven_terrain_edge(&root, &aster_dir, &assets)?;
    let prefab_brush = prefab_brush_edge(&root, &aster_dir, &assets[0])?;

    let mut snapshot = json!({
        "schema": "litd.issue1220.placement_brush_fsv.v1",
        "issue": 1220,
        "source_of_truth": "Aster world record readback plus EditorDocument.objects brush placement snapshots",
        "artifact_path": path,
        "before": {
            "artifact": before_artifact,
            "asset_paths": assets.iter().map(|asset| asset.relative_path.clone()).collect::<Vec<_>>()
        },
        "happy_path": {
            "before": {
                "stamp_id": happy_request.stamp_id,
                "requested_count": happy_request.requested_count,
                "density_cap": happy_request.density_cap,
                "snap": "grid_and_terrain",
                "rotation_range": [happy_request.rotation_y_degrees.min, happy_request.rotation_y_degrees.max],
                "scale_range": [happy_request.uniform_scale.min, happy_request.uniform_scale.max]
            },
            "after": {
                "report": happy_report,
                "variation": happy_summary,
                "aster": readback_summary(&happy_aster),
                "objects": happy_objects
            }
        },
        "edges": [zero_jitter, density_cap, uneven_terrain, prefab_brush],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote placement brush FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "happy placed={} yaw_unique={} scale_unique={} variants={} terrain_heights={}",
        snapshot["happy_path"]["after"]["report"]["placed_count"],
        snapshot["happy_path"]["after"]["variation"]["yaw_unique"],
        snapshot["happy_path"]["after"]["variation"]["scale_unique"],
        snapshot["happy_path"]["after"]["variation"]["variant_unique"],
        snapshot["happy_path"]["after"]["variation"]["terrain_height_unique"]
    );
    Ok(())
}

fn zero_jitter_edge(
    root: &std::path::Path,
    aster_dir: &std::path::Path,
    assets: &[ContentBrowserAssetRef],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = issue1220_world("zero-jitter")?;
    let mut request = asset_request("issue1220_zero", assets, 6, 6, 77);
    request.rotation_y_degrees = BrushRange::constant(45.0);
    request.uniform_scale = BrushRange::constant(1.0);
    let before_count = record.document.objects.len();
    let report = stamp_placement_brush_in_world_record(&mut record, &request)?;
    let readback = parse_world_record(&write_and_read_world(root, aster_dir, &record)?.bytes)?;
    ensure(
        report
            .readback
            .iter()
            .all(|item| item.yaw_degrees == 45.0 && item.uniform_scale == 1.0),
        "zero jitter edge produced non-constant rotation or scale",
    )?;
    Ok(json!({
        "edge": "zero_jitter_deterministic",
        "before": {"object_count": before_count, "rotation": [45.0, 45.0], "scale": [1.0, 1.0]},
        "after": {
            "report": report,
            "objects": report_objects(&readback, &report)?
        }
    }))
}

fn density_cap_edge(
    root: &std::path::Path,
    aster_dir: &std::path::Path,
    assets: &[ContentBrowserAssetRef],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = issue1220_world("density-cap")?;
    let request = asset_request("issue1220_density_cap", assets, 20, 5, 122_005);
    let before_count = record.document.objects.len();
    let report = stamp_placement_brush_in_world_record(&mut record, &request)?;
    let readback = parse_world_record(&write_and_read_world(root, aster_dir, &record)?.bytes)?;
    ensure(
        report.capped && report.placed_count == 5,
        "density cap edge did not cap placement at 5",
    )?;
    Ok(json!({
        "edge": "density_cap",
        "before": {"object_count": before_count, "requested_count": 20, "density_cap": 5},
        "after": {
            "capped": report.capped,
            "placed_count": report.placed_count,
            "objects": report_objects(&readback, &report)?
        }
    }))
}

fn uneven_terrain_edge(
    root: &std::path::Path,
    aster_dir: &std::path::Path,
    assets: &[ContentBrowserAssetRef],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = issue1220_world("uneven-terrain")?;
    let request = asset_request("issue1220_uneven", assets, 7, 7, 122_007);
    let before_heights = terrain_height_set(&record);
    let report = stamp_placement_brush_in_world_record(&mut record, &request)?;
    let readback = parse_world_record(&write_and_read_world(root, aster_dir, &record)?.bytes)?;
    ensure_grid_and_terrain(&readback, &report)?;
    let placed_heights = unique_strings(
        report
            .readback
            .iter()
            .map(|item| format!("{:.3}", item.translation[1])),
    );
    ensure(
        placed_heights.len() > 1,
        "uneven terrain edge did not place across multiple snapped heights",
    )?;
    Ok(json!({
        "edge": "snap_to_uneven_terrain",
        "before": {"world_height_values": before_heights},
        "after": {
            "placed_height_values": placed_heights,
            "objects": report_objects(&readback, &report)?
        }
    }))
}

fn prefab_brush_edge(
    root: &std::path::Path,
    aster_dir: &std::path::Path,
    asset: &ContentBrowserAssetRef,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut record = issue1220_world("prefab")?;
    let request = prefab_request(asset)?;
    let before_count = record.document.objects.len();
    let report = stamp_placement_brush_in_world_record(&mut record, &request)?;
    let readback = parse_world_record(&write_and_read_world(root, aster_dir, &record)?.bytes)?;
    ensure(
        report.placed_count == 2
            && report
                .readback
                .iter()
                .all(|item| item.content_kind == "prefab"),
        "prefab brush did not expand two prefab instances",
    )?;
    Ok(json!({
        "edge": "prefab_brush",
        "before": {"object_count": before_count, "prefab_asset": asset.relative_path},
        "after": {
            "report": report,
            "objects": report_objects(&readback, &report)?
        }
    }))
}

fn asset_request(
    stamp_id: &str,
    assets: &[ContentBrowserAssetRef],
    requested_count: usize,
    density_cap: usize,
    seed: u64,
) -> PlacementBrushRequest {
    let variants = assets
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, asset)| PlacementBrushAsset::new(asset, (index + 1) as u32))
        .collect();
    PlacementBrushRequest {
        stamp_id: stamp_id.to_string(),
        center: Vec3::new(7.5, 0.0, 5.5),
        radius: 3.0,
        requested_count,
        density_cap,
        seed,
        rotation_y_degrees: BrushRange {
            min: 0.0,
            max: 270.0,
        },
        uniform_scale: BrushRange { min: 0.8, max: 1.2 },
        snap: PlacementBrushSnap::GridAndTerrain { grid_size: 1.0 },
        clamp_to_playable_bounds: true,
        content: PlacementBrushContent::Assets { variants },
    }
}

fn prefab_request(
    asset: &ContentBrowserAssetRef,
) -> Result<PlacementBrushRequest, Box<dyn std::error::Error>> {
    let template = PrefabObjectTemplate::new("asset_doodad", "Doodad", None, Transform::IDENTITY)
        .with_property(
            PLACED_ASSET_RELATIVE_PATH_PROPERTY,
            asset.relative_path.clone(),
        );
    let definition = PrefabDefinition::new(
        "issue1220_brush_prefab",
        "Issue 1220 Brush Prefab",
        "1220",
        vec![template],
        Vec::new(),
    )?;
    let mut library = PrefabLibrary::new(4)?;
    library.insert_definition(definition)?;
    Ok(PlacementBrushRequest {
        stamp_id: "issue1220_prefab".to_string(),
        center: Vec3::new(5.0, 0.0, 4.0),
        radius: 1.0,
        requested_count: 2,
        density_cap: 2,
        seed: 99,
        rotation_y_degrees: BrushRange::constant(0.0),
        uniform_scale: BrushRange::constant(1.0),
        snap: PlacementBrushSnap::GridAndTerrain { grid_size: 1.0 },
        clamp_to_playable_bounds: true,
        content: PlacementBrushContent::Prefab {
            prefab_id: "issue1220_brush_prefab".to_string(),
            library,
            overrides: Vec::new(),
        },
    })
}

fn issue1220_world(
    label: &str,
) -> Result<litd_editor::world_creation::EditorWorldRecord, Box<dyn std::error::Error>> {
    let mut record = create_editor_world_record(EditorWorldCreationSpec::issue1210_default())?;
    record.spec.calyx_document_id = format!("calyx://world/issue1220-placement-brush-{label}");
    record.document.calyx_document_id = record.spec.calyx_document_id.clone();
    Ok(record)
}

fn report_objects(
    record: &litd_editor::world_creation::EditorWorldRecord,
    report: &PlacementBrushReport,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let mut objects = Vec::new();
    for item in &report.readback {
        for object_id in &item.object_ids {
            let object = record
                .document
                .objects
                .get(object_id)
                .ok_or_else(|| format!("missing object {object_id} in Aster readback"))?;
            objects.push(json!({
                "object_id": object.id,
                "kind": object.kind,
                "translation": object.transform.translation.to_array(),
                "scale": object.transform.scale.to_array(),
                "stamp_id": object.properties.get(BRUSH_STAMP_ID_PROPERTY),
                "content_kind": object.properties.get(BRUSH_CONTENT_KIND_PROPERTY),
                "yaw_degrees": object.properties.get(BRUSH_YAW_DEGREES_PROPERTY),
                "uniform_scale": object.properties.get(BRUSH_SCALE_PROPERTY),
                "terrain_cell": object.properties.get(BRUSH_TERRAIN_CELL_PROPERTY),
                "terrain_height": object.properties.get(BRUSH_TERRAIN_HEIGHT_PROPERTY),
                "asset_relative_path": object.properties.get(PLACED_ASSET_RELATIVE_PATH_PROPERTY),
            }));
        }
    }
    Ok(objects)
}

fn ensure_grid_and_terrain(
    record: &litd_editor::world_creation::EditorWorldRecord,
    report: &PlacementBrushReport,
) -> Result<(), Box<dyn std::error::Error>> {
    for item in &report.readback {
        ensure(
            item.translation[0].fract() == 0.0,
            "x coordinate was not grid-snapped",
        )?;
        ensure(
            item.translation[2].fract() == 0.0,
            "z coordinate was not grid-snapped",
        )?;
        let cell_id = item
            .terrain_cell_id
            .as_ref()
            .ok_or("missing terrain cell id")?;
        let cell = record
            .document
            .terrain_cells
            .get(cell_id)
            .ok_or_else(|| format!("missing terrain cell {cell_id}"))?;
        ensure(
            item.translation[1] == cell.height,
            "y coordinate did not equal terrain height",
        )?;
    }
    Ok(())
}

fn variation_summary(report: &PlacementBrushReport) -> serde_json::Value {
    json!({
        "yaw_unique": unique_strings(report.readback.iter().map(|item| format!("{:.3}", item.yaw_degrees))).len(),
        "scale_unique": unique_strings(report.readback.iter().map(|item| format!("{:.3}", item.uniform_scale))).len(),
        "variant_unique": unique_variants(&report.readback),
        "terrain_height_unique": unique_strings(report.readback.iter().map(|item| format!("{:.3}", item.translation[1]))).len(),
        "within_rotation_range": report.readback.iter().all(|item| (0.0..=270.0).contains(&item.yaw_degrees)),
        "within_scale_range": report.readback.iter().all(|item| (0.8..=1.2).contains(&item.uniform_scale)),
        "grid_snapped": report.readback.iter().all(|item| item.translation[0].fract() == 0.0 && item.translation[2].fract() == 0.0),
        "terrain_snapped": report.readback.iter().all(|item| item.terrain_height == Some(item.translation[1])),
    })
}

fn unique_strings(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut values = values.collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn unique_variants(readback: &[PlacementBrushReadback]) -> usize {
    let mut values = readback
        .iter()
        .filter_map(|item| item.variant_index)
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values.len()
}

fn terrain_height_set(record: &litd_editor::world_creation::EditorWorldRecord) -> Vec<String> {
    unique_strings(
        record
            .document
            .terrain_cells
            .values()
            .map(|cell| format!("{:.3}", cell.height)),
    )
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
            ensure(
                disk_len == expected_len,
                "FSV artifact byte readback mismatch",
            )?;
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

fn ensure(condition: bool, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string().into())
    }
}
