use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
};

#[test]
fn issue1325_ttf_sdf_font_atlas_manual_fsv() {
    let run = begin_issue_run("issue1325_ttf_sdf_font_atlas").unwrap();
    let font_path = reference_windows_font_path();
    let atlas_path = run.run_dir().join("arial-sdf-atlas.json");
    let bmp_path = run.run_dir().join("arial-sdf-atlas.bmp");
    let repeat_bmp_path = run.run_dir().join("arial-sdf-atlas-repeat.bmp");
    let before = json!({
        "font": font_file_state(&font_path),
        "atlas_json": file_state(&atlas_path),
        "atlas_bmp": file_state(&bmp_path),
    });

    let request = FontAtlasBuildRequest::new(
        &font_path,
        vec!['A', 'g', '\u{00E9}', '\u{03A9}'],
        40.0,
        64,
        8,
        4,
    );
    let build = build_sdf_font_atlas(request.clone()).unwrap();
    let json_readback = write_font_atlas_json(&atlas_path, &build.atlas).unwrap();
    let bmp_readback = write_font_atlas_bmp(&bmp_path, &build.image).unwrap();
    let loaded_atlas = load_font_atlas_json(&atlas_path).unwrap().atlas;
    let disk_bmp_bytes = fs::read(&bmp_path).unwrap();
    assert_eq!(sha256_hex(&disk_bmp_bytes), bmp_readback.sha256);
    assert_eq!(loaded_atlas.fnv1a64, build.atlas.fnv1a64);

    let repeat = build_sdf_font_atlas(request).unwrap();
    let repeat_bmp = write_font_atlas_bmp(&repeat_bmp_path, &repeat.image).unwrap();
    assert_eq!(repeat.atlas.fnv1a64, build.atlas.fnv1a64);
    assert_eq!(repeat_bmp.sha256, bmp_readback.sha256);

    let glyph_a = loaded_atlas.glyph('A' as u32).unwrap();
    let glyph_omega = loaded_atlas.glyph('\u{03A9}' as u32).unwrap();
    assert_eq!((glyph_a.cell_x, glyph_a.cell_y), (0, 0));
    assert_eq!((glyph_omega.cell_x, glyph_omega.cell_y), (3, 0));
    let a_high = bmp_cell_high_pixels(&bmp_path, bmp_readback.width, 64, glyph_a, 128);
    let omega_high = bmp_cell_high_pixels(&bmp_path, bmp_readback.width, 64, glyph_omega, 128);
    assert!(a_high > 0);
    assert!(omega_high > 0);

    let empty_edge = failed_build_edge(
        "empty_glyphs",
        FontAtlasBuildRequest::new(&font_path, Vec::new(), 40.0, 64, 8, 4),
        run.run_dir(),
    );
    let missing_font_edge = failed_build_edge(
        "missing_font",
        FontAtlasBuildRequest::for_text(
            r"C:\Windows\Fonts\litd_missing_font.ttf",
            "A",
            40.0,
            64,
            8,
            4,
        ),
        run.run_dir(),
    );
    let missing_glyph_edge = failed_build_edge(
        "missing_glyph",
        FontAtlasBuildRequest::new(&font_path, vec!['\u{10FFFF}'], 40.0, 64, 8, 4),
        run.run_dir(),
    );
    let max_glyphs = max_glyphs_edge(&font_path, run.run_dir());
    let after = json!({
        "atlas_json": file_state(&atlas_path),
        "atlas_bmp": file_state(&bmp_path),
        "repeat_bmp": file_state(&repeat_bmp_path),
    });

    let report = json!({
        "schema_version": 1,
        "issue": 1325,
        "source_of_truth": {
            "font_path": font_path,
            "atlas_json_path": json_readback.path,
            "atlas_json_sha256": json_readback.sha256,
            "atlas_fnv1a64": json_readback.fnv1a64,
            "atlas_bmp_path": bmp_readback.path,
            "atlas_bmp_sha256": bmp_readback.sha256,
            "repeat_bmp_sha256": repeat_bmp.sha256,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "happy_path": {
            "before": before,
            "after": after,
            "expected": {
                "glyph_count": 4,
                "atlas_size": [256, 64],
                "glyph_A_cell": [0, 0],
                "glyph_omega_cell": [3, 0],
                "stable_bmp_sha256": bmp_readback.sha256,
                "repeat_bmp_sha256": bmp_readback.sha256,
            },
            "actual": {
                "glyph_count": loaded_atlas.glyphs.len(),
                "atlas_size": [bmp_readback.width, bmp_readback.height],
                "bmp_magic": bmp_readback.magic,
                "glyph_A_cell": [glyph_a.cell_x, glyph_a.cell_y],
                "glyph_A_high_pixels": a_high,
                "glyph_omega_cell": [glyph_omega.cell_x, glyph_omega.cell_y],
                "glyph_omega_high_pixels": omega_high,
                "atlas_fnv1a64": loaded_atlas.fnv1a64,
                "atlas_bmp_sha256": bmp_readback.sha256,
                "repeat_bmp_sha256": repeat_bmp.sha256,
            }
        },
        "edges": {
            "empty_glyphs": empty_edge,
            "missing_font": missing_font_edge,
            "missing_glyph": missing_glyph_edge,
            "max_glyphs": max_glyphs,
        },
        "research": {
            "fontdue": "https://docs.rs/fontdue/latest/fontdue/struct.Font.html",
            "sdf": "https://cdn.akamai.steamstatic.com/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf",
            "distance_transform": "https://www.cs.cornell.edu/dph/papers/dt.pdf",
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["actual"]["glyph_count"],
        4
    );
    assert_eq!(
        report_readback.json["edges"]["missing_glyph"]["error"]["code"],
        "LITD-RENDER-UI-INVALID-CONFIG"
    );
    println!(
        "ISSUE1325_FONT_ATLAS_FSV report={} report_sha256={} atlas_json={} atlas_sha256={} atlas_fnv1a64={} bmp={} bmp_sha256={} repeat_bmp_sha256={} glyph_A_high={} glyph_omega_high={} empty_code={} missing_font_code={} missing_glyph_code={} max_glyphs={} max_bmp_sha256={}",
        report_readback.path.display(),
        report_readback.sha256,
        json_readback.path.display(),
        json_readback.sha256,
        json_readback.fnv1a64,
        bmp_readback.path.display(),
        bmp_readback.sha256,
        repeat_bmp.sha256,
        a_high,
        omega_high,
        report_readback.json["edges"]["empty_glyphs"]["error"]["code"],
        report_readback.json["edges"]["missing_font"]["error"]["code"],
        report_readback.json["edges"]["missing_glyph"]["error"]["code"],
        report_readback.json["edges"]["max_glyphs"]["after"]["glyph_count"],
        report_readback.json["edges"]["max_glyphs"]["after"]["bmp_sha256"],
    );
}

fn failed_build_edge(label: &str, request: FontAtlasBuildRequest, root: &Path) -> Value {
    let path = root.join(format!("edge-{label}-atlas.json"));
    let before = file_state(&path);
    let error = attempt_build_and_write(request, &path).unwrap_err();
    let after = file_state(&path);
    assert!(!path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn attempt_build_and_write(request: FontAtlasBuildRequest, path: &Path) -> Result<(), EngineError> {
    let build = build_sdf_font_atlas(request)?;
    write_font_atlas_json(path, &build.atlas).map(|_| ())
}

fn max_glyphs_edge(font_path: &Path, root: &Path) -> Value {
    let json_path = root.join("edge-max-glyphs-atlas.json");
    let bmp_path = root.join("edge-max-glyphs-atlas.bmp");
    let before = json!({
        "json": file_state(&json_path),
        "bmp": file_state(&bmp_path),
    });
    let glyphs = (33_u8..=126).map(char::from).collect::<Vec<_>>();
    let build = build_sdf_font_atlas(FontAtlasBuildRequest::new(
        font_path, glyphs, 40.0, 64, 8, 16,
    ))
    .unwrap();
    let json_readback = write_font_atlas_json(&json_path, &build.atlas).unwrap();
    let bmp_readback = write_font_atlas_bmp(&bmp_path, &build.image).unwrap();
    let loaded = load_font_atlas_json(&json_path).unwrap().atlas;
    let glyph_z = loaded.glyph('Z' as u32).unwrap();
    let z_high = bmp_cell_high_pixels(&bmp_path, bmp_readback.width, 64, glyph_z, 128);
    assert_eq!(loaded.glyphs.len(), 94);
    assert!(z_high > 0);
    json!({
        "before": before,
        "after": {
            "json": file_state(&json_path),
            "bmp": file_state(&bmp_path),
            "glyph_count": loaded.glyphs.len(),
            "rows": loaded.rows,
            "atlas_fnv1a64": loaded.fnv1a64,
            "json_sha256": json_readback.sha256,
            "bmp_sha256": bmp_readback.sha256,
            "glyph_Z_cell": [glyph_z.cell_x, glyph_z.cell_y],
            "glyph_Z_high_pixels": z_high,
        },
        "expected": {
            "glyph_count": 94,
            "glyph_Z_high_pixels_min": 1,
        }
    })
}

fn bmp_cell_high_pixels(
    path: &Path,
    width: u32,
    cell_px: u32,
    glyph: &FontGlyph,
    threshold: u8,
) -> u32 {
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[0..2], b"BM");
    let height = u32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let row_stride = (width as usize * 3).div_ceil(4) * 4;
    let mut count = 0_u32;
    for y in glyph.cell_y * cell_px..(glyph.cell_y + 1) * cell_px {
        let bmp_y = height - 1 - y;
        for x in glyph.cell_x * cell_px..(glyph.cell_x + 1) * cell_px {
            let offset = 54 + bmp_y as usize * row_stride + x as usize * 3;
            if bytes[offset] >= threshold {
                count += 1;
            }
        }
    }
    count
}

fn file_state(path: &Path) -> Value {
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "bytes": fs::metadata(path).map(|meta| meta.len()).ok(),
    })
}

fn font_file_state(path: &Path) -> Value {
    file_state(path)
}

fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}

#[test]
fn issue1325_latest_fsv_manifest_points_at_real_report() {
    let root = issue_root("issue1325_ttf_sdf_font_atlas");
    let manifest_path = root.join("latest-manifest.json");
    if !manifest_path.exists() {
        return;
    }
    let bytes = fs::read(&manifest_path).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let report_path = Path::new(manifest["report_path"].as_str().unwrap());
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}
