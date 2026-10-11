use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use image::{ImageBuffer, ImageFormat, Luma};

use crate::world_creation::{EditorWorldCreationSpec, create_editor_world_record};

use super::*;

#[test]
fn png16_import_and_raw16_export_round_trip_stably() {
    let root = test_dir("png16_round_trip");
    let source = root.join("source-heightmap.png");
    let exported = root.join("exported-heightmap.raw");
    let samples = vec![
        0, 4096, 8192, 12288, 16384, 20480, 24576, 28672, 32768, 36864, 40960, 45056, 49152, 53248,
        57344, 65535,
    ];
    write_png16(&source, 4, 4, &samples);
    let mut record = test_world("png16-round-trip");

    let import_report = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::png(&source, -8.0, 8.0),
    )
    .unwrap();
    println!(
        "issue1212 png16 import changed={} sha={} after={}",
        import_report.changed_count, import_report.source_sha256, import_report.after_summary
    );

    assert_eq!(import_report.bit_depth, HeightmapBitDepth::U16);
    assert_eq!(import_report.width, 4);
    assert_eq!(import_report.height, 4);
    assert_eq!(
        sample_heights(&record.document),
        expected_heights(&samples, -8.0, 8.0)
    );

    let export_report = export_heightmap(
        &record.document,
        &HeightmapExportSpec::raw(
            &exported,
            HeightmapBitDepth::U16,
            HeightmapByteOrder::LittleEndian,
            -8.0,
            8.0,
        ),
    )
    .unwrap();
    let exported_samples = read_raw16_le(&exported);
    println!(
        "issue1212 raw16 export samples={} sha={} first={} last={}",
        export_report.sample_count,
        export_report.output_sha256,
        exported_samples[0],
        exported_samples[15]
    );

    assert_eq!(export_report.sample_count, 16);
    assert_eq!(exported_samples, samples);

    let mut round_trip = test_world("raw16-round-trip");
    import_heightmap(
        &mut round_trip.document,
        &HeightmapImportSpec::raw(
            &exported,
            HeightmapBitDepth::U16,
            HeightmapByteOrder::LittleEndian,
            -8.0,
            8.0,
        ),
    )
    .unwrap();
    assert_eq!(
        sample_heights(&round_trip.document),
        sample_heights(&record.document)
    );
}

#[test]
fn raw8_import_reads_real_bytes_and_wrong_size_png_fails_without_mutation() {
    let root = test_dir("raw8_wrong_size");
    let raw = root.join("source-heightmap.raw");
    let wrong_png = root.join("wrong-size.png");
    let raw_samples = (0_u8..16).map(|value| value * 11).collect::<Vec<_>>();
    fs::write(&raw, &raw_samples).unwrap();
    write_png8(&wrong_png, 2, 2, &[0, 64, 128, 255]);
    let mut record = test_world("raw8");

    let report = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::raw(
            &raw,
            HeightmapBitDepth::U8,
            HeightmapByteOrder::LittleEndian,
            -4.0,
            4.0,
        ),
    )
    .unwrap();
    println!(
        "issue1212 raw8 import changed={} sha={} height_0_0={:.6}",
        report.changed_count,
        report.source_sha256,
        record.document.terrain_cells["terrain_cell_0_0"].height
    );

    assert_eq!(report.bit_depth, HeightmapBitDepth::U8);
    assert_approx(
        record.document.terrain_cells["terrain_cell_3_3"].height,
        -4.0 + (165.0 / 255.0) * 8.0,
    );

    let before = record.document.clone();
    let error = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::png(&wrong_png, -4.0, 4.0),
    )
    .unwrap_err()
    .to_string();
    println!(
        "issue1212 wrong-size before_eq_after={} error={}",
        before == record.document,
        error
    );

    assert_eq!(before, record.document);
    assert!(error.contains("editor.terrain_heightmap.dimensions"));
}

#[test]
fn flat_png8_yields_flat_terrain_and_export_range_errors_loudly() {
    let root = test_dir("flat_png8");
    let source = root.join("flat.png");
    write_png8(&source, 4, 4, &[255; 16]);
    let mut record = test_world("flat");

    let report = import_heightmap(
        &mut record.document,
        &HeightmapImportSpec::png(&source, -2.0, 2.0),
    )
    .unwrap();
    let heights = sample_heights(&record.document);
    println!(
        "issue1212 flat import changed={} unique_heights={:?}",
        report.changed_count, heights
    );

    assert_eq!(report.bit_depth, HeightmapBitDepth::U8);
    assert!(heights.iter().all(|height| *height == "2.000000"));

    let error = export_heightmap(
        &record.document,
        &HeightmapExportSpec::png(
            root.join("too-small-range.png"),
            HeightmapBitDepth::U8,
            -1.0,
            1.0,
        ),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("editor.terrain_heightmap.export.range"));
}

fn test_world(label: &str) -> crate::world_creation::EditorWorldRecord {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/issue1212-{label}");
    spec.revision_id = format!("issue1212-{label}-rev-1");
    spec.properties.display_name = format!("Issue 1212 {label}");
    spec.properties.width_tiles = 4;
    spec.properties.height_tiles = 4;
    spec.properties.playable_bounds.max_tile_x_exclusive = 4;
    spec.properties.playable_bounds.max_tile_z_exclusive = 4;
    create_editor_world_record(spec).unwrap()
}

fn test_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("litd-editor crate lives under workspace crates/");
    let path = workspace_root
        .join("target/test-data/issue1212")
        .join(label)
        .join(format!("{}-{stamp}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_png16(path: &Path, width: u32, height: u32, samples: &[u16]) {
    let image = ImageBuffer::<Luma<u16>, Vec<u16>>::from_vec(width, height, samples.to_vec())
        .expect("valid png16 buffer");
    image.save_with_format(path, ImageFormat::Png).unwrap();
}

fn write_png8(path: &Path, width: u32, height: u32, samples: &[u8]) {
    let image = ImageBuffer::<Luma<u8>, Vec<u8>>::from_vec(width, height, samples.to_vec())
        .expect("valid png8 buffer");
    image.save_with_format(path, ImageFormat::Png).unwrap();
}

fn read_raw16_le(path: &Path) -> Vec<u16> {
    fs::read(path)
        .unwrap()
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

fn expected_heights(samples: &[u16], min_height: f32, max_height: f32) -> Vec<String> {
    samples
        .iter()
        .map(|sample| {
            format!(
                "{:.6}",
                min_height + (*sample as f32 / u16::MAX as f32) * (max_height - min_height)
            )
        })
        .collect()
}

fn sample_heights(document: &EditorDocument) -> Vec<String> {
    (0..4)
        .flat_map(|z| {
            (0..4).map(move |x| {
                format!(
                    "{:.6}",
                    document.terrain_cells[&format!("terrain_cell_{x}_{z}")].height
                )
            })
        })
        .collect()
}

fn assert_approx(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.000_1,
        "actual {actual} expected {expected}"
    );
}
