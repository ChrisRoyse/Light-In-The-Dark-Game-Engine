use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use image::{ImageBuffer, ImageFormat, Luma};
use litd_editor::{
    history::stable_fingerprint,
    terrain_heightmap::{
        HeightmapBitDepth, HeightmapByteOrder, HeightmapExportSpec, HeightmapImportSpec,
        export_heightmap, heightmap_samples_from_document, import_heightmap,
    },
    world_creation::{
        EditorWorldCreationSpec, EditorWorldRecord, WorldAsterReadback, parse_world_record,
        read_world_record, write_world_record,
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_TERRAIN_HEIGHTMAP_FSV",
        "issue1212_terrain_heightmap",
        "manual-fsv.json",
    )?;
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1212 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let aster_dir = root.join("aster");
    let source_dir = root.join("sources");
    fs::create_dir_all(&source_dir)?;

    let happy = happy_path(&root, &source_dir, &aster_dir)?;
    let wrong_size = wrong_size_edge(&source_dir)?;
    let raw8 = raw8_edge(&root, &source_dir, &aster_dir)?;
    let flat = flat_png_edge(&root, &source_dir, &aster_dir)?;

    let mut snapshot = json!({
        "schema": "litd.issue1212.terrain_heightmap_fsv.v1",
        "issue": 1212,
        "source_of_truth": "physical PNG/RAW source files, exported heightmap files, Aster world record readback, and EditorDocument terrain_cells",
        "artifact_path": path,
        "happy_path": happy,
        "edges": [wrong_size, raw8, flat],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote terrain heightmap FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!("happy {}", snapshot["happy_path"]["after"]);
    println!("wrong-size edge {}", snapshot["edges"][0]);
    println!("raw8 edge {}", snapshot["edges"][1]["after"]);
    println!("flat edge {}", snapshot["edges"][2]["after"]);
    Ok(())
}

struct StoredRecord {
    state_path: PathBuf,
    readback: WorldAsterReadback,
}

fn happy_path(
    root: &Path,
    source_dir: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let source_path = source_dir.join("known-png16-heightmap.png");
    let export_path = source_dir.join("roundtrip-raw16-le.raw");
    let samples = vec![
        0, 4096, 8192, 12288, 16384, 20480, 24576, 28672, 32768, 36864, 40960, 45056, 49152, 53248,
        57344, 65535,
    ];
    write_png16(&source_path, 4, 4, &samples)?;
    let mut record = test_world("png16-import");
    let before_samples = document_samples(&record)?;
    let import_report = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::png(&source_path, -8.0, 8.0),
    )?;
    sync_record(&mut record, "png16_import")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let readback_samples = document_samples(&parsed)?;
    assert_eq!(readback_samples, samples);

    let export_report = export_heightmap(
        &parsed.document,
        &HeightmapExportSpec::raw(
            &export_path,
            HeightmapBitDepth::U16,
            HeightmapByteOrder::LittleEndian,
            -8.0,
            8.0,
        ),
    )?;
    let exported_samples = read_raw16_le(&export_path)?;
    assert_eq!(exported_samples, samples);

    let mut roundtrip = test_world("raw16-roundtrip");
    let roundtrip_report = import_heightmap(
        &mut roundtrip.document,
        &HeightmapImportSpec::raw(
            &export_path,
            HeightmapBitDepth::U16,
            HeightmapByteOrder::LittleEndian,
            -8.0,
            8.0,
        ),
    )?;
    sync_record(&mut roundtrip, "raw16_roundtrip")?;
    let roundtrip_stored = write_read_record(root, aster_dir, &roundtrip)?;
    let roundtrip_parsed = parse_world_record(&roundtrip_stored.readback.bytes)?;
    let roundtrip_samples = document_samples(&roundtrip_parsed)?;
    assert_eq!(roundtrip_samples, samples);

    Ok(json!({
        "before": {
            "document_samples": before_samples,
            "source_path": source_path,
            "source_sha256": file_sha256(&source_path)?,
        },
        "after": {
            "import_report": import_report,
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "readback_samples": readback_samples,
            "export_report": export_report,
            "exported_path": export_path,
            "exported_sha256": file_sha256(&export_path)?,
            "exported_samples": exported_samples,
            "roundtrip_report": roundtrip_report,
            "roundtrip_state_path": roundtrip_stored.state_path,
            "roundtrip_state_sha256": file_sha256(&roundtrip_stored.state_path)?,
            "roundtrip_readback": readback_summary(&roundtrip_stored.readback),
            "roundtrip_samples": roundtrip_samples,
            "passed": true
        }
    }))
}

fn wrong_size_edge(source_dir: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let wrong_path = source_dir.join("wrong-size-2x2.png");
    write_png8(&wrong_path, 2, 2, &[0, 64, 128, 255])?;
    let mut record = test_world("wrong-size");
    let before = record.document.clone();
    let before_fingerprint = stable_fingerprint(&before)?;
    let error = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::png(&wrong_path, -8.0, 8.0),
    )
    .unwrap_err()
    .to_string();
    let after_fingerprint = stable_fingerprint(&record.document)?;
    assert_eq!(before, record.document);
    Ok(json!({
        "name": "wrong-size PNG rejected without mutation",
        "source_path": wrong_path,
        "source_sha256": file_sha256(&wrong_path)?,
        "before": {"fingerprint": before_fingerprint},
        "after": {
            "fingerprint": after_fingerprint,
            "document_unchanged": before == record.document,
            "error": error,
            "passed": error.contains("editor.terrain_heightmap.dimensions")
        }
    }))
}

fn raw8_edge(
    root: &Path,
    source_dir: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let raw_path = source_dir.join("known-raw8-heightmap.raw");
    let samples = (0_u8..16).map(|value| value * 11).collect::<Vec<_>>();
    fs::write(&raw_path, &samples)?;
    let mut record = test_world("raw8-import");
    let report = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::raw(
            &raw_path,
            HeightmapBitDepth::U8,
            HeightmapByteOrder::LittleEndian,
            -4.0,
            4.0,
        ),
    )?;
    sync_record(&mut record, "raw8_import")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let readback_samples = heightmap_samples_from_document(
        &parsed.document,
        &HeightmapExportSpec::raw(
            source_dir.join("raw8-readback.raw"),
            HeightmapBitDepth::U8,
            HeightmapByteOrder::LittleEndian,
            -4.0,
            4.0,
        ),
    )?;
    let readback_u8 = readback_samples
        .iter()
        .map(|sample| u8::try_from(*sample))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(readback_u8, samples);
    Ok(json!({
        "name": "RAW8 import persists exact samples",
        "before": {"source_path": raw_path, "source_sha256": file_sha256(&raw_path)?},
        "after": {
            "import_report": report,
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "readback_samples": readback_u8,
            "passed": true
        }
    }))
}

fn flat_png_edge(
    root: &Path,
    source_dir: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let flat_path = source_dir.join("flat-png8-heightmap.png");
    write_png8(&flat_path, 4, 4, &[255; 16])?;
    let mut record = test_world("flat-png8");
    let report = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::png(&flat_path, -2.0, 2.0),
    )?;
    sync_record(&mut record, "flat_png8")?;
    let stored = write_read_record(root, aster_dir, &record)?;
    let parsed = parse_world_record(&stored.readback.bytes)?;
    let heights = parsed
        .document
        .terrain_cells
        .values()
        .map(|cell| format!("{:.6}", cell.height))
        .collect::<BTreeSet<_>>();
    assert_eq!(heights.len(), 1);
    assert!(heights.contains("2.000000"));
    Ok(json!({
        "name": "flat PNG8 yields flat terrain",
        "before": {"source_path": flat_path, "source_sha256": file_sha256(&flat_path)?},
        "after": {
            "import_report": report,
            "state_path": stored.state_path,
            "state_sha256": file_sha256(&stored.state_path)?,
            "readback": readback_summary(&stored.readback),
            "unique_heights": heights,
            "passed": true
        }
    }))
}

fn write_read_record(
    root: &Path,
    aster_dir: &Path,
    record: &EditorWorldRecord,
) -> Result<StoredRecord, Box<dyn std::error::Error>> {
    let state_path = root.join(format!("{}.json", record.spec.revision_id));
    write_world_record(aster_dir, record)?;
    let readback = read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id))?;
    Ok(StoredRecord {
        state_path,
        readback,
    })
}

fn sync_record(
    record: &mut EditorWorldRecord,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    record.source_issue = 1212;
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.terrain_cell_count = record.document.terrain_cells.len();
    record.default_tile_count = record
        .document
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == record.spec.properties.initial_tile_id)
        .count();
    record.summary = format!(
        "world={} issue1212={} height_samples={} fingerprint={}",
        record.spec.properties.display_name,
        label,
        document_samples(record)?.len(),
        record.document_fingerprint
    );
    Ok(())
}

fn test_world(label: &str) -> EditorWorldRecord {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/issue1212-{label}");
    spec.revision_id = format!("issue1212-{label}-rev-1");
    spec.properties.display_name = format!("Issue 1212 {label}");
    spec.properties.description =
        format!("Known terrain heightmap settings for issue 1212 {label} FSV");
    spec.properties.width_tiles = 4;
    spec.properties.height_tiles = 4;
    spec.properties.playable_bounds.max_tile_x_exclusive = 4;
    spec.properties.playable_bounds.max_tile_z_exclusive = 4;
    let mut record = litd_editor::world_creation::create_editor_world_record(spec).unwrap();
    record.source_issue = 1212;
    record
}

fn document_samples(record: &EditorWorldRecord) -> Result<Vec<u16>, Box<dyn std::error::Error>> {
    Ok(heightmap_samples_from_document(
        &record.document,
        &HeightmapExportSpec::raw(
            "unused.raw",
            HeightmapBitDepth::U16,
            HeightmapByteOrder::LittleEndian,
            -8.0,
            8.0,
        ),
    )?)
}

fn write_png16(
    path: &PathBuf,
    width: u32,
    height: u32,
    samples: &[u16],
) -> Result<(), Box<dyn std::error::Error>> {
    let image = ImageBuffer::<Luma<u16>, Vec<u16>>::from_vec(width, height, samples.to_vec())
        .ok_or("invalid png16 buffer")?;
    image.save_with_format(path, ImageFormat::Png)?;
    Ok(())
}

fn write_png8(
    path: &PathBuf,
    width: u32,
    height: u32,
    samples: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let image = ImageBuffer::<Luma<u8>, Vec<u8>>::from_vec(width, height, samples.to_vec())
        .ok_or("invalid png8 buffer")?;
    image.save_with_format(path, ImageFormat::Png)?;
    Ok(())
}

fn read_raw16_le(path: &PathBuf) -> Result<Vec<u16>, Box<dyn std::error::Error>> {
    Ok(fs::read(path)?
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}

fn readback_summary(readback: &WorldAsterReadback) -> serde_json::Value {
    json!({
        "seq": readback.seq,
        "graph_rows": readback.graph_rows,
        "storage_schema": readback.storage_schema,
        "chunk_count": readback.chunk_count,
        "bytes": readback.bytes.len(),
        "sha256": readback.sha256,
        "document_fingerprint": readback.document_fingerprint
    })
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
            if fs::metadata(path)?.len() != expected_len {
                return Err("terrain heightmap FSV artifact byte readback mismatch".into());
            }
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err("terrain heightmap FSV artifact byte-count stabilization failed".into())
}

fn file_sha256(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
