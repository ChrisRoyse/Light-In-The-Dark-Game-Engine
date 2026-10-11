use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
    render::font::reference_windows_font_path,
};

fn issue1326_unicode_text_shaping_manual_fsv() {
    let run = begin_issue_run("issue1326_text_shaping").unwrap();
    let font_path = reference_windows_font_path();
    let latin_path = run.run_dir().join("latin-la-sombra.json");
    let arabic_path = run.run_dir().join("arabic-rtl.json");
    let cluster_path = run.run_dir().join("grapheme-clusters.json");
    let a_path = run.run_dir().join("kerning-A.json");
    let v_path = run.run_dir().join("kerning-V.json");
    let av_path = run.run_dir().join("kerning-AV.json");
    let max_path = run.run_dir().join("edge-max-text.json");
    let before = json!({
        "font": font_file_state(&font_path),
        "latin": file_state(&latin_path),
        "arabic": file_state(&arabic_path),
        "cluster": file_state(&cluster_path),
        "kerning_av": file_state(&av_path),
        "max": file_state(&max_path),
    });

    let latin = shape_and_write(
        &font_path,
        "La sombra...",
        TextShapeDirection::Ltr,
        &latin_path,
    )
    .unwrap();
    let arabic = shape_and_write(&font_path, "سمح", TextShapeDirection::Rtl, &arabic_path).unwrap();
    let clusters = shape_and_write(
        &font_path,
        "Cafe\u{0301}",
        TextShapeDirection::Ltr,
        &cluster_path,
    )
    .unwrap();
    let a = shape_and_write(&font_path, "A", TextShapeDirection::Ltr, &a_path).unwrap();
    let v = shape_and_write(&font_path, "V", TextShapeDirection::Ltr, &v_path).unwrap();
    let av = shape_and_write(&font_path, "AV", TextShapeDirection::Ltr, &av_path).unwrap();
    let max_text = "A".repeat(MAX_TEXT_SHAPING_CHARS);
    let max = shape_and_write(&font_path, max_text, TextShapeDirection::Ltr, &max_path).unwrap();

    assert_eq!(latin.run.visual_text, "La sombra...");
    assert_eq!(latin.run.resolved_direction, TextShapeDirection::Ltr);
    assert!(latin.run.total_advance_x_px > 0.0);
    assert_eq!(arabic.run.visual_text, "حمس");
    assert_eq!(arabic.run.resolved_direction, TextShapeDirection::Rtl);
    let arabic_cluster_order = arabic
        .run
        .glyphs
        .iter()
        .map(|glyph| glyph.cluster_byte)
        .collect::<Vec<_>>();
    assert_eq!(arabic_cluster_order, vec![4, 2, 0]);
    assert!(
        arabic
            .run
            .glyphs
            .iter()
            .all(|glyph| glyph.x_advance_px > 0.0)
    );

    let grapheme_texts = clusters
        .run
        .grapheme_clusters
        .iter()
        .map(|cluster| cluster.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(grapheme_texts, vec!["C", "a", "f", "e\u{0301}"]);

    let unkerned_advance = a.run.total_advance_x_px + v.run.total_advance_x_px;
    let kerning_delta = unkerned_advance - av.run.total_advance_x_px;
    assert!(
        kerning_delta.abs() > 0.01,
        "AV kerning delta must be non-zero, got {kerning_delta}"
    );
    assert_eq!(max.run.glyphs.len(), MAX_TEXT_SHAPING_CHARS);
    assert_eq!(max.storage_schema, "litd.render.text_shaping.chunks.v1");
    assert!(max.chunk_count > 1);

    let empty_edge = failed_shape_edge(
        "empty_text",
        TextShapeRequest::new(&font_path, "", 40.0, TextShapeDirection::Ltr),
        run.run_dir(),
    );
    let invalid_font_path = run.run_dir().join("invalid-font.ttf");
    fs::write(&invalid_font_path, b"not a real font").unwrap();
    let invalid_font_edge = failed_shape_edge(
        "invalid_font",
        TextShapeRequest::new(&invalid_font_path, "A", 40.0, TextShapeDirection::Ltr),
        run.run_dir(),
    );
    let missing_glyph_edge = failed_shape_edge(
        "missing_glyph",
        TextShapeRequest::new(&font_path, "\u{10FFFF}", 40.0, TextShapeDirection::Ltr),
        run.run_dir(),
    );
    let over_limit_edge = failed_shape_edge(
        "over_limit",
        TextShapeRequest::new(
            &font_path,
            "A".repeat(MAX_TEXT_SHAPING_CHARS + 1),
            40.0,
            TextShapeDirection::Ltr,
        ),
        run.run_dir(),
    );
    let after = json!({
        "latin": file_state(&latin_path),
        "arabic": file_state(&arabic_path),
        "cluster": file_state(&cluster_path),
        "kerning_a": file_state(&a_path),
        "kerning_v": file_state(&v_path),
        "kerning_av": file_state(&av_path),
        "max": file_state(&max_path),
    });

    let report = json!({
        "schema_version": 1,
        "issue": 1326,
        "source_of_truth": {
            "font_path": font_path,
            "latin_json_path": latin.path,
            "latin_json_sha256": latin.sha256,
            "latin_fnv1a64": latin.fnv1a64,
            "arabic_json_path": arabic.path,
            "arabic_json_sha256": arabic.sha256,
            "arabic_fnv1a64": arabic.fnv1a64,
            "cluster_json_path": clusters.path,
            "cluster_json_sha256": clusters.sha256,
            "kerning_av_json_path": av.path,
            "kerning_av_json_sha256": av.sha256,
            "max_json_path": max.path,
            "max_json_sha256": max.sha256,
            "max_json_bytes": max.bytes,
            "max_storage_schema": max.storage_schema,
            "max_chunk_count": max.chunk_count,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "happy_path": {
            "before": before,
            "after": after,
            "expected": {
                "latin_visual_text": "La sombra...",
                "arabic_visual_text": "حمس",
                "arabic_cluster_order": [4, 2, 0],
                "grapheme_clusters": ["C", "a", "f", "e\u{0301}"],
                "kerning_delta_abs_min": 0.01,
                "max_glyph_count": MAX_TEXT_SHAPING_CHARS,
                "max_storage_schema": "litd.render.text_shaping.chunks.v1",
                "max_chunk_count_min": 2,
            },
            "actual": {
                "latin_visual_text": latin.run.visual_text,
                "latin_glyph_count": latin.run.glyphs.len(),
                "latin_total_advance_x_px": latin.run.total_advance_x_px,
                "arabic_visual_text": arabic.run.visual_text,
                "arabic_cluster_order": arabic_cluster_order,
                "arabic_total_advance_x_px": arabic.run.total_advance_x_px,
                "grapheme_clusters": grapheme_texts,
                "grapheme_count": clusters.run.grapheme_clusters.len(),
                "unkerned_advance_x_px": unkerned_advance,
                "kerned_av_advance_x_px": av.run.total_advance_x_px,
                "kerning_delta_x_px": kerning_delta,
                "max_glyph_count": max.run.glyphs.len(),
                "max_storage_schema": max.storage_schema,
                "max_chunk_count": max.chunk_count,
            }
        },
        "edges": {
            "empty_text": empty_edge,
            "invalid_font": invalid_font_edge,
            "missing_glyph": missing_glyph_edge,
            "over_limit": over_limit_edge,
        },
        "research": {
            "rustybuzz": "https://docs.rs/rustybuzz/latest/rustybuzz/",
            "unicode_bidi": "https://docs.rs/unicode-bidi/latest/unicode_bidi/",
            "unicode_segmentation": "https://docs.rs/unicode-segmentation/latest/unicode_segmentation/",
            "unicode_uax9": "https://unicode.org/reports/tr9/",
            "unicode_uax29": "https://www.unicode.org/reports/tr29/"
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["actual"]["arabic_cluster_order"],
        json!([4, 2, 0])
    );
    assert_eq!(
        report_readback.json["edges"]["invalid_font"]["error"]["code"],
        "LITD-RENDER-TEXT-SHAPING-INVALID-CONFIG"
    );
    assert!(
        report_readback.json["happy_path"]["actual"]["max_chunk_count"]
            .as_u64()
            .unwrap_or_default()
            > 1
    );
    println!(
        "ISSUE1326_TEXT_SHAPING_FSV report={} report_sha256={} latin={} latin_sha256={} latin_fnv={} arabic={} arabic_sha256={} arabic_fnv={} arabic_order={:?} cluster_sha256={} kerning_delta={} max_glyphs={} empty_code={} invalid_font_code={} missing_glyph_code={} over_limit_code={}",
        report_readback.path.display(),
        report_readback.sha256,
        latin.path.display(),
        latin.sha256,
        latin.fnv1a64,
        arabic.path.display(),
        arabic.sha256,
        arabic.fnv1a64,
        report_readback.json["happy_path"]["actual"]["arabic_cluster_order"],
        clusters.sha256,
        kerning_delta,
        max.run.glyphs.len(),
        report_readback.json["edges"]["empty_text"]["error"]["code"],
        report_readback.json["edges"]["invalid_font"]["error"]["code"],
        report_readback.json["edges"]["missing_glyph"]["error"]["code"],
        report_readback.json["edges"]["over_limit"]["error"]["code"],
    );
}

fn shape_and_write(
    font_path: &Path,
    text: impl Into<String>,
    direction: TextShapeDirection,
    path: &Path,
) -> Result<ShapedTextRunJsonReadback, EngineError> {
    let run = shape_text_run(TextShapeRequest::new(font_path, text, 40.0, direction))?;
    write_shaped_text_run_json(path, &run)
}

fn failed_shape_edge(label: &str, request: TextShapeRequest, root: &Path) -> Value {
    let path = root.join(format!("edge-{label}.json"));
    let before = file_state(&path);
    let error = attempt_shape_and_write(request, &path).unwrap_err();
    let after = file_state(&path);
    assert!(!path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn attempt_shape_and_write(request: TextShapeRequest, path: &Path) -> Result<(), EngineError> {
    let run = shape_text_run(request)?;
    write_shaped_text_run_json(path, &run).map(|_| ())
}

fn file_state(path: &Path) -> Value {
    let bytes = fs::read(path).ok();
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "bytes": bytes.as_ref().map(Vec::len),
        "sha256": bytes.as_deref().map(sha256_hex),
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
fn issue1326_latest_fsv_manifest_points_at_real_report() {
    issue1326_unicode_text_shaping_manual_fsv();
    let root = issue_root("issue1326_text_shaping");
    let manifest_path = root.join("latest-manifest.json");
    let bytes = fs::read(&manifest_path).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let report_path = Path::new(manifest["report_path"].as_str().unwrap());
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}
