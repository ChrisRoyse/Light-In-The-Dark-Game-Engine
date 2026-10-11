use std::{fs, path::Path};

use litd_engine::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
    math::{Aabb, Mat4, Vec2, Vec3, Vec4},
    render::{
        font::{FontAtlas, FontAtlasBuildRequest, reference_windows_font_path},
        text_shaping::{
            TextShapeDirection, TextShapeRequest, shape_text_run, write_shaped_text_run_json,
        },
        ui::{
            FogOfWarGrid, MapOverlayPolicy, MinimapConfig, ShapedTextRunPlacement, UiRect,
            UiRenderInputs, UiRenderSettings, VisionSource, build_shaped_text_atlas,
            build_ui_render_plan, measure_shaped_text_layout, render_shaped_text_preview_image,
            write_shaped_text_preview_bmp,
        },
    },
};
use litd_hash::sha256_hex;
use serde::Serialize;
use serde_json::{Value, json};

use super::{LocalizedStringTable, TextDirection, measure_text_layout};

#[test]
fn issue1327_localized_shaped_text_reaches_ui_bmp_manual_fsv() {
    let run = begin_issue_run("issue1327_shaped_ui_localization").unwrap();
    let font_path = reference_windows_font_path();
    let shaped_path = run.run_dir().join("localized-arabic-shaped-run.json");
    let atlas_path = run.run_dir().join("localized-arabic-shaped-atlas.json");
    let bmp_path = run.run_dir().join("localized-arabic-ui-preview.bmp");
    let before = json!({
        "font": file_state(&font_path),
        "shaped_run": file_state(&shaped_path),
        "atlas": file_state(&atlas_path),
        "bmp": file_state(&bmp_path),
    });

    let mut table = LocalizedStringTable::new("en-US").unwrap();
    table
        .upsert_default("hud.quest.line", "Light the gate", "quest HUD line")
        .unwrap();
    table
        .set_translation("hud.quest.line", "ar", "سمح نور")
        .unwrap();
    let localized = table.render_locale("hud.quest.line", "ar").unwrap();
    let editor_probe = measure_text_layout(&localized, 4).unwrap();
    let shape_direction = to_shape_direction(localized.direction);
    let shaped = shape_text_run(TextShapeRequest::new(
        &font_path,
        localized.text.clone(),
        40.0,
        shape_direction,
    ))
    .unwrap();
    let engine_probe = measure_shaped_text_layout(&shaped, 4).unwrap();
    assert_eq!(editor_probe.line_count, engine_probe.line_count);
    assert_eq!(editor_probe.overflow, engine_probe.overflow);
    assert_eq!(shape_direction, engine_probe.direction);

    let shaped_readback = write_shaped_text_run_json(&shaped_path, &shaped).unwrap();
    let atlas_build = build_shaped_text_atlas(&shaped, 96, 10, 8).unwrap();
    let atlas_json = write_json_readback(&atlas_path, &atlas_build.atlas).unwrap();
    let settings = UiRenderSettings {
        viewport_width: 420.0,
        viewport_height: 160.0,
    };
    let legacy_atlas = FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        &font_path, "A", 40.0, 64, 8, 4,
    ))
    .unwrap()
    .atlas;
    let placement = ShapedTextRunPlacement {
        id: "hud.quest.line.ar".to_string(),
        run: shaped.clone(),
        atlas: atlas_build.atlas.clone(),
        origin_px: Vec2::new(24.0, 24.0),
        color: Vec4::new(0.94, 0.88, 0.64, 1.0),
    };
    let plan = build_ui_render_plan(
        settings,
        &legacy_atlas,
        UiRenderInputs {
            text_runs: Vec::new(),
            shaped_text_runs: vec![placement],
            widgets: Vec::new(),
            floating_texts: Vec::new(),
            minimap: MinimapConfig {
                world_bounds: world_bounds(),
                rect_px: UiRect::new(Vec2::new(320.0, 24.0), Vec2::new(396.0, 100.0)).unwrap(),
            },
            overlay_policy: MapOverlayPolicy::runtime_rpg_no_map_access(),
            markers: Vec::new(),
            viewport_world_bounds: world_bounds(),
            fog: FogOfWarGrid::new(2, 2, world_bounds()).unwrap(),
            vision_sources: vec![VisionSource {
                world_position: Vec3::ZERO,
                radius: 8.0,
            }],
            view_projection: Mat4::IDENTITY,
        },
    )
    .unwrap();
    assert_eq!(plan.glyph_quads.len(), shaped.glyphs.len());
    assert_eq!(
        plan.glyph_quads
            .iter()
            .map(|quad| quad.glyph_id)
            .collect::<Vec<_>>(),
        shaped
            .glyphs
            .iter()
            .map(|glyph| glyph.glyph_id)
            .collect::<Vec<_>>()
    );
    let image =
        render_shaped_text_preview_image(420, 160, &plan.glyph_quads, &atlas_build.image).unwrap();
    let bmp = write_shaped_text_preview_bmp(&bmp_path, &image, &plan.glyph_quads).unwrap();
    let ink_glyph_ids = atlas_build
        .atlas
        .glyphs
        .iter()
        .filter(|glyph| glyph.ink_pixels > 0)
        .map(|glyph| glyph.glyph_id)
        .collect::<Vec<_>>();
    for glyph_id in &ink_glyph_ids {
        assert!(
            bmp.glyph_rect_hits
                .iter()
                .any(|hit| hit.glyph_id == *glyph_id && hit.non_background_pixels > 0),
            "glyph {glyph_id} must have non-background pixels in its rendered rect"
        );
    }
    assert!(bmp.non_background_pixels > 0);

    let empty_edge = empty_text_edge(run.run_dir(), &font_path);
    let zero_width_edge = zero_layout_width_edge(run.run_dir(), &shaped);
    let hash_mismatch_edge = hash_mismatch_edge(run.run_dir(), &shaped);
    let after = json!({
        "shaped_run": file_state(&shaped_path),
        "atlas": file_state(&atlas_path),
        "bmp": file_state(&bmp_path),
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1327,
        "source_of_truth": {
            "localized_text": localized.text,
            "localized_direction": format!("{:?}", localized.direction),
            "shaped_run_json": shaped_readback.path,
            "shaped_run_sha256": shaped_readback.sha256,
            "atlas_json": atlas_path,
            "atlas_json_sha256": atlas_json["sha256"],
            "atlas_fnv1a64": atlas_build.atlas.fnv1a64,
            "bmp_path": bmp.bmp_path,
            "bmp_sha256": bmp.bmp_sha256,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "happy_path": {
            "before": before,
            "after": after,
            "expected": {
                "localized_text": "سمح نور",
                "editor_line_count": 2,
                "engine_line_count": 2,
                "overflow": false,
                "glyph_quads": shaped.glyphs.len(),
                "non_background_pixels_min": 1,
                "wgsl_sdf_function": "litd_ui_sdf_alpha",
            },
            "actual": {
                "editor_probe": editor_probe,
                "engine_probe": engine_probe,
                "visual_text": shaped.visual_text,
                "glyph_quads": plan.glyph_quads.len(),
                "glyph_ids": plan.glyph_quads.iter().map(|quad| quad.glyph_id).collect::<Vec<_>>(),
                "ink_glyph_ids": ink_glyph_ids,
                "bmp_bytes": bmp.bmp_bytes,
                "bmp_sha256": bmp.bmp_sha256,
                "bmp_non_background_pixels": bmp.non_background_pixels,
                "glyph_rect_hits": bmp.glyph_rect_hits,
                "wgsl_has_sdf": litd_engine::render::ui::UI_RENDER_WGSL
                    .contains("litd_ui_sdf_alpha"),
            }
        },
        "edges": {
            "empty_text": empty_edge,
            "zero_layout_width": zero_width_edge,
            "font_hash_mismatch": hash_mismatch_edge,
        },
        "research": {
            "harfbuzz_glyphs_rendering": "https://harfbuzz.github.io/glyphs-and-rendering.html",
            "fontdue_indexed_raster": "https://docs.rs/fontdue/latest/fontdue/struct.Font.html"
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["actual"]["wgsl_has_sdf"],
        true
    );
    assert_eq!(
        report_readback.json["edges"]["font_hash_mismatch"]["error"]["code"],
        "LITD-RENDER-TEXT-SHAPING-READBACK-MISMATCH"
    );
    println!(
        "ISSUE1327_SHAPED_UI_LOCALIZATION_FSV report={} report_sha256={} shaped_run={} shaped_sha256={} atlas_json={} atlas_sha256={} atlas_fnv={} bmp={} bmp_sha256={} non_background={} line_count={} overflow={} empty_code={} zero_width_code={} hash_mismatch_code={}",
        report_readback.path.display(),
        report_readback.sha256,
        shaped_readback.path.display(),
        shaped_readback.sha256,
        atlas_path.display(),
        atlas_json["sha256"],
        atlas_build.atlas.fnv1a64,
        bmp.bmp_path.display(),
        bmp.bmp_sha256,
        bmp.non_background_pixels,
        report_readback.json["happy_path"]["actual"]["engine_probe"]["line_count"],
        report_readback.json["happy_path"]["actual"]["engine_probe"]["overflow"],
        report_readback.json["edges"]["empty_text"]["error"]["code"],
        report_readback.json["edges"]["zero_layout_width"]["error"]["code"],
        report_readback.json["edges"]["font_hash_mismatch"]["error"]["code"],
    );
}

#[test]
fn issue1327_latest_fsv_manifest_points_at_real_report() {
    let root = issue_root("issue1327_shaped_ui_localization");
    let manifest_path = root.join("latest-manifest.json");
    if !manifest_path.exists() {
        return;
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).expect("manifest json");
    let report_path = manifest["report_path"].as_str().expect("report path");
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}

fn empty_text_edge(root: &Path, font_path: &Path) -> Value {
    let path = root.join("edge-empty-text.json");
    let before = file_state(&path);
    let error = shape_text_run(TextShapeRequest::new(
        font_path,
        "",
        40.0,
        TextShapeDirection::Rtl,
    ))
    .unwrap_err();
    let after = file_state(&path);
    json!({ "before": before, "after": after, "error": error_report(&error) })
}

fn zero_layout_width_edge(
    root: &Path,
    shaped: &litd_engine::render::text_shaping::ShapedTextRun,
) -> Value {
    let path = root.join("edge-zero-layout-width.json");
    let before = file_state(&path);
    let error = measure_shaped_text_layout(shaped, 0).unwrap_err();
    let after = file_state(&path);
    json!({ "before": before, "after": after, "error": error_report(&error) })
}

fn hash_mismatch_edge(
    root: &Path,
    shaped: &litd_engine::render::text_shaping::ShapedTextRun,
) -> Value {
    let path = root.join("edge-font-hash-mismatch.json");
    let before = file_state(&path);
    let mut bad = shaped.clone();
    bad.source_font_sha256 = "0".repeat(64);
    let error = build_shaped_text_atlas(&bad, 96, 10, 8).unwrap_err();
    let after = file_state(&path);
    json!({ "before": before, "after": after, "error": error_report(&error) })
}

fn to_shape_direction(direction: TextDirection) -> TextShapeDirection {
    match direction {
        TextDirection::Ltr => TextShapeDirection::Ltr,
        TextDirection::Rtl => TextShapeDirection::Rtl,
    }
}

fn world_bounds() -> Aabb {
    Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(16.0)).unwrap()
}

fn write_json_readback(path: &Path, value: &impl Serialize) -> Result<Value, EngineError> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: error.to_string(),
        })?;
    fs::write(path, &bytes).map_err(|error| EngineError::UiRenderPersistenceFailed {
        op: "write_shaped_text_atlas_json".to_string(),
        reason: error.to_string(),
    })?;
    let readback = fs::read(path).map_err(|error| EngineError::UiRenderPersistenceFailed {
        op: "read_shaped_text_atlas_json".to_string(),
        reason: error.to_string(),
    })?;
    if readback != bytes {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "shaped_text_atlas_json.bytes".to_string(),
            expected: sha256_hex(&bytes),
            actual: sha256_hex(&readback),
        });
    }
    Ok(json!({
        "path": path.display().to_string(),
        "bytes": readback.len(),
        "sha256": sha256_hex(&readback),
    }))
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

fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}
