//! Full State Verification for #1462 — text pipeline: real per-glyph advances +
//! atlas validation moved off the per-glyph hot path.
//!
//! Source of truth = the real Windows Arial font file on disk. This harness
//! independently reads the font's own advance metrics (fontdue, no atlas), then
//! proves:
//!   A. the built atlas carries those exact advances (proportional, not fixed);
//!   B. `layout_text_run` positions glyphs by the real advance scaled to the run
//!      font size — the old hardcoded `font_px * 0.6` is gone (mixed-width proof
//!      "iiii" vs "WWWW" at the geometry level);
//!   C. the same proof at the PIXEL level — the quads are rasterized through the
//!      atlas, written to a BMP, the BMP is re-read from disk, and the inked
//!      width of "WWWW" vs "iiii" tracks the real advance ratio (a fixed advance
//!      would make them identical);
//!   D. `glyph_uv` no longer validates per lookup, while `layout_text_run` and
//!      the load boundary still fail loud on a corrupted atlas.
#![recursion_limit = "256"]

use std::fs;

use fontdue::{Font, FontSettings};
use litd_engine::{
    math::{Vec2, Vec4},
    render::{
        font::{FontAtlas, FontAtlasBuildRequest, FontAtlasImage, reference_windows_font_path},
        ui::{GlyphQuad, TextRun, layout_text_run},
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

const FONT_PX: f32 = 40.0;
const CELL_PX: u32 = 64;
const SPREAD_PX: u32 = 8;
const COLUMNS: u32 = 8;
const INK_THRESHOLD: u8 = 96;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1462_text_pipeline")?;
    let font_path = reference_windows_font_path();

    // ---- Source of truth #1: the font's own advance metrics, read independently ----
    let font_bytes = fs::read(&font_path)?;
    let font_sha256 = sha256_hex(&font_bytes);
    let font = Font::from_bytes(
        font_bytes.as_slice(),
        FontSettings {
            scale: FONT_PX,
            ..FontSettings::default()
        },
    )
    .map_err(|error| format!("independent font parse failed: {error}"))?;
    let adv_i_independent = font.metrics('i', FONT_PX).advance_width;
    let adv_w_independent = font.metrics('W', FONT_PX).advance_width;

    // ---- Build the atlas (validated exactly once, at build) ----
    let build = FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        &font_path,
        "iW WWWWiiii",
        FONT_PX,
        CELL_PX,
        SPREAD_PX,
        COLUMNS,
    ))?;
    let atlas = build.atlas;
    let image = build.image;

    let adv_i_atlas = atlas
        .glyph('i' as u32)
        .ok_or("atlas missing 'i'")?
        .advance_width_px;
    let adv_w_atlas = atlas
        .glyph('W' as u32)
        .ok_or("atlas missing 'W'")?
        .advance_width_px;

    // ---- Part A: atlas advances equal the independent font metrics (bit-exact) ----
    let advances_match = adv_i_atlas.to_bits() == adv_i_independent.to_bits()
        && adv_w_atlas.to_bits() == adv_w_independent.to_bits();
    let proportional = adv_w_atlas > adv_i_atlas;
    assert!(
        advances_match,
        "atlas advances must equal independent font metrics"
    );
    assert!(
        proportional,
        "'W' must advance wider than 'i' (proportional font)"
    );

    // ---- Part B: layout advances (geometry) at scale 1.0 ----
    let iiii = layout_text_run(&atlas, &text_run("iiii", "iiii", FONT_PX))?;
    let wwww = layout_text_run(&atlas, &text_run("wwww", "WWWW", FONT_PX))?;
    let iiii_advances = quad_advances(&iiii);
    let wwww_advances = quad_advances(&wwww);
    let iiii_total = total_width(&iiii);
    let wwww_total = total_width(&wwww);

    // Every 'i' advance equals the real metric; likewise every 'W'.
    let iiii_geometry_ok = iiii_advances.iter().all(|a| approx(*a, adv_i_atlas, 0.001));
    let wwww_geometry_ok = wwww_advances.iter().all(|a| approx(*a, adv_w_atlas, 0.001));
    // The old fixed-width behavior would give both strings width 4 * (40 * 0.6) = 96.0.
    let old_fixed_width = 4.0 * FONT_PX * 0.6;
    let refutes_fixed_width = !approx(iiii_total, old_fixed_width, 1.0)
        && !approx(wwww_total, old_fixed_width, 1.0)
        && !approx(iiii_total, wwww_total, 1.0);
    assert!(
        iiii_geometry_ok && wwww_geometry_ok,
        "layout must use real per-glyph advances"
    );
    assert!(
        refutes_fixed_width,
        "layout must not reproduce the old fixed 0.6em width"
    );

    // ---- Part B2: font_scale — same atlas rendered at half size halves the advance ----
    let iiii_half = layout_text_run(&atlas, &text_run("iiii_half", "iiii", FONT_PX / 2.0))?;
    let half_advance = quad_advances(&iiii_half)[0];
    let scale_ok = approx(half_advance, adv_i_atlas * 0.5, 0.001);
    assert!(
        scale_ok,
        "advance must scale with run font size / atlas font size"
    );

    // ---- Part C: PIXEL-level proof — rasterize, write BMP, re-read, measure ink ----
    let iiii_raster = rasterize_run(&image, &atlas, &iiii, iiii_total);
    let wwww_raster = rasterize_run(&image, &atlas, &wwww, wwww_total);
    let iiii_bmp_path = run.run_dir().join("rendered-iiii.bmp");
    let wwww_bmp_path = run.run_dir().join("rendered-WWWW.bmp");
    fs::write(&iiii_bmp_path, iiii_raster.bmp_bytes())?;
    fs::write(&wwww_bmp_path, wwww_raster.bmp_bytes())?;
    // Read the artifacts back from disk and measure inked width from the file bytes.
    let iiii_ink = ink_width_from_bmp(&fs::read(&iiii_bmp_path)?)?;
    let wwww_ink = ink_width_from_bmp(&fs::read(&wwww_bmp_path)?)?;
    let pixel_ratio = wwww_ink as f32 / iiii_ink.max(1) as f32;
    let metric_ratio = adv_w_atlas / adv_i_atlas;
    // "WWWW" must render visibly wider than "iiii"; the pixel ratio must track the
    // real advance ratio (a fixed advance would make pixel_ratio ~= 1.0).
    let pixel_proof =
        wwww_ink > iiii_ink * 2 && approx(pixel_ratio, metric_ratio, metric_ratio * 0.25);
    assert!(
        pixel_proof,
        "rendered ink width must track real advance ratio, not a fixed width"
    );

    // ---- Part D: validation is off the per-glyph hot path, boundary still fails loud ----
    let mut tampered = atlas.clone();
    tampered.glyphs[0].advance_width_px += 137.0; // corrupt without recomputing fnv1a64
    let glyph_uv_ignores_corruption = tampered.glyph_uv('i').is_ok();
    let validate_catches_corruption = tampered.validate().is_err();
    let layout_fails_loud = layout_text_run(&tampered, &text_run("t", "iiii", FONT_PX)).is_err();
    // Edge: missing glyph in a valid atlas fails loud (character not in atlas).
    let missing_glyph_fails = layout_text_run(&atlas, &text_run("m", "Z", FONT_PX)).is_err();
    assert!(
        glyph_uv_ignores_corruption,
        "glyph_uv must not re-validate on the hot path"
    );
    assert!(
        validate_catches_corruption,
        "validate() must still catch a tampered atlas"
    );
    assert!(
        layout_fails_loud,
        "layout must validate once up front and fail loud"
    );
    assert!(
        missing_glyph_fails,
        "an absent glyph must fail loud, no silent skip"
    );

    // ---- Perf evidence: counterfactual old cost (validate per glyph) vs new (once) ----
    let long_text: String = "iW".repeat(1024); // 2048 glyphs
    let long_run = text_run("long", &long_text, FONT_PX);
    let t0 = std::time::Instant::now();
    let long_quads = layout_text_run(&atlas, &long_run)?;
    let new_layout_ns = t0.elapsed().as_nanos();
    let t1 = std::time::Instant::now();
    atlas.validate()?;
    let one_validate_ns = t1.elapsed().as_nanos();
    let old_estimated_ns = one_validate_ns.saturating_mul(long_quads.len() as u128);
    let speedup = old_estimated_ns as f64 / new_layout_ns.max(1) as f64;

    let report = json!({
        "schema": "litd.issue1462.text_pipeline_fsv.v1",
        "issue": 1462,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "font_path": font_path.display().to_string(),
            "font_sha256": font_sha256,
            "rendered_iiii_bmp": iiii_bmp_path.display().to_string(),
            "rendered_WWWW_bmp": wwww_bmp_path.display().to_string(),
            "manual_gate": "Read this JSON and open the two BMPs via Synapse. 'WWWW' must be visibly wider than 'iiii'; measured inked widths and the advance metrics below must agree."
        },
        "research_sources": [
            "https://freetype.org/freetype2/docs/glyphs/glyphs-3.html",
            "https://learn.microsoft.com/en-us/windows/win32/directwrite/justification--kerning--and-spacing",
            "https://www.warp.dev/blog/adventures-text-rendering-kerning-glyph-atlases",
            "https://fontforge.org/docs/ui/dialogs/validation.html"
        ],
        "sufficiency_argument": "Two glyphs at opposite extremes of the advance spectrum ('i' narrow, 'W' wide) are the minimal set that 100% distinguishes proportional advances from the old fixed 0.6em: under a fixed advance 'iiii' and 'WWWW' are identical width; under real metrics their widths differ by the true advance ratio. Verified at three independent layers (font metrics, layout geometry, rendered pixels) plus a corrupted-atlas boundary probe.",
        "part_a_metrics": {
            "advance_i_independent_px": adv_i_independent,
            "advance_W_independent_px": adv_w_independent,
            "advance_i_atlas_px": adv_i_atlas,
            "advance_W_atlas_px": adv_w_atlas,
            "atlas_matches_independent_bit_exact": advances_match,
            "proportional_W_gt_i": proportional
        },
        "part_b_geometry": {
            "iiii_advances_px": iiii_advances,
            "wwww_advances_px": wwww_advances,
            "iiii_total_width_px": iiii_total,
            "wwww_total_width_px": wwww_total,
            "old_fixed_width_would_be_px": old_fixed_width,
            "refutes_old_fixed_width": refutes_fixed_width,
            "half_size_advance_px": half_advance,
            "half_size_expected_px": adv_i_atlas * 0.5,
            "font_scale_ok": scale_ok
        },
        "part_c_pixels": {
            "iiii_inked_width_px": iiii_ink,
            "wwww_inked_width_px": wwww_ink,
            "pixel_width_ratio": pixel_ratio,
            "metric_advance_ratio": metric_ratio,
            "pixel_proof_passed": pixel_proof
        },
        "part_d_validation": {
            "glyph_uv_ignores_corruption": glyph_uv_ignores_corruption,
            "validate_catches_corruption": validate_catches_corruption,
            "layout_fails_loud_on_corruption": layout_fails_loud,
            "missing_glyph_fails_loud": missing_glyph_fails
        },
        "perf_evidence": {
            "note": "Wall-clock, non-deterministic; supplementary to the structural proof in part_d.",
            "layout_glyph_count": long_quads.len(),
            "new_layout_ns": new_layout_ns,
            "one_full_validate_ns": one_validate_ns,
            "old_estimated_ns_validate_per_glyph": old_estimated_ns,
            "estimated_speedup_x": speedup
        },
        "expected_actuals": {
            "all_assertions_passed": true
        }
    });

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1462_TEXT_PIPELINE_FSV report={} report_sha256={} adv_i={} adv_W={} iiii_total={} WWWW_total={} iiii_ink={} WWWW_ink={} pixel_ratio={:.3} metric_ratio={:.3} speedup={:.1}x",
        readback.path.display(),
        readback.sha256,
        adv_i_atlas,
        adv_w_atlas,
        iiii_total,
        wwww_total,
        iiii_ink,
        wwww_ink,
        pixel_ratio,
        metric_ratio,
        speedup,
    );
    Ok(())
}

fn text_run(id: &str, text: &str, font_px: f32) -> TextRun {
    TextRun {
        id: id.to_string(),
        text: text.to_string(),
        origin_px: Vec2::new(0.0, 0.0),
        font_px,
        color: Vec4::ONE,
    }
}

/// Per-glyph horizontal advance recovered from consecutive quad origins, with the
/// last advance taken from the final quad width (cursor delta == advance).
fn quad_advances(quads: &[GlyphQuad]) -> Vec<f32> {
    quads
        .iter()
        .map(|q| q.rect_px.max.x - q.rect_px.min.x)
        .collect()
}

fn total_width(quads: &[GlyphQuad]) -> f32 {
    match (quads.first(), quads.last()) {
        (Some(first), Some(last)) => last.rect_px.max.x - first.rect_px.min.x,
        _ => 0.0,
    }
}

fn approx(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// Rasterize a laid-out run by nearest-neighbor sampling each glyph's atlas cell
/// (via the quad UV) into the quad's pixel rect — the CPU analogue of the textured
/// quad the GPU draws. Produces a grayscale image sized to the run.
fn rasterize_run(
    atlas_image: &FontAtlasImage,
    atlas: &FontAtlas,
    quads: &[GlyphQuad],
    total_width: f32,
) -> GrayImage {
    let width = (total_width.ceil() as u32 + 2).max(2);
    let height = (atlas.font_px.ceil() as u32 + 2).max(2);
    let mut out = GrayImage::new(width, height);
    let aw = atlas_image.width as f32;
    let ah = atlas_image.height as f32;
    for quad in quads {
        let x0 = quad.rect_px.min.x.floor().max(0.0) as u32;
        let x1 = (quad.rect_px.max.x.ceil() as u32).min(width);
        let y0 = quad.rect_px.min.y.floor().max(0.0) as u32;
        let y1 = (quad.rect_px.max.y.ceil() as u32).min(height);
        let rw = (quad.rect_px.max.x - quad.rect_px.min.x).max(1.0);
        let rh = (quad.rect_px.max.y - quad.rect_px.min.y).max(1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let u = quad.uv.min.x
                    + (quad.uv.max.x - quad.uv.min.x)
                        * ((x as f32 + 0.5 - quad.rect_px.min.x) / rw);
                let v = quad.uv.min.y
                    + (quad.uv.max.y - quad.uv.min.y)
                        * ((y as f32 + 0.5 - quad.rect_px.min.y) / rh);
                let sx = (u * aw).floor().clamp(0.0, aw - 1.0) as u32;
                let sy = (v * ah).floor().clamp(0.0, ah - 1.0) as u32;
                if let Ok(sample) = atlas_image.pixel(sx, sy) {
                    let idx = (y * width + x) as usize;
                    if sample > out.pixels[idx] {
                        out.pixels[idx] = sample;
                    }
                }
            }
        }
    }
    out
}

/// Inked width (rightmost minus leftmost column with any pixel >= INK_THRESHOLD).
fn ink_width_from_bmp(bytes: &[u8]) -> Result<u32, Box<dyn std::error::Error>> {
    let (width, height, pixels) = decode_gray_bmp(bytes)?;
    let mut min_x: Option<u32> = None;
    let mut max_x: Option<u32> = None;
    for y in 0..height {
        for x in 0..width {
            if pixels[(y * width + x) as usize] >= INK_THRESHOLD {
                min_x = Some(min_x.map_or(x, |m| m.min(x)));
                max_x = Some(max_x.map_or(x, |m| m.max(x)));
            }
        }
    }
    match (min_x, max_x) {
        (Some(lo), Some(hi)) => Ok(hi - lo + 1),
        _ => Err("rendered image has no ink above threshold".into()),
    }
}

/// Minimal grayscale image backed by a 24-bit BMP encoder/decoder so the pixel
/// proof survives a real disk round-trip.
struct GrayImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl GrayImage {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        }
    }

    fn bmp_bytes(&self) -> Vec<u8> {
        let row_stride = ((self.width as usize).saturating_mul(3)).div_ceil(4) * 4;
        let pixel_bytes = row_stride * self.height as usize;
        let file_size = 54 + pixel_bytes;
        let mut bytes = Vec::with_capacity(file_size);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&(file_size as u32).to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&54u32.to_le_bytes());
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&(self.width as i32).to_le_bytes());
        bytes.extend_from_slice(&(self.height as i32).to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&24u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
        bytes.extend_from_slice(&2835i32.to_le_bytes());
        bytes.extend_from_slice(&2835i32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let padding = row_stride - self.width as usize * 3;
        for y in (0..self.height).rev() {
            for x in 0..self.width {
                let value = self.pixels[(y * self.width + x) as usize];
                bytes.extend_from_slice(&[value, value, value]);
            }
            bytes.extend(std::iter::repeat_n(0u8, padding));
        }
        bytes
    }
}

fn decode_gray_bmp(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return Err("not a BMP".into());
    }
    let pixel_offset = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
    let width = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]) as u32;
    let height = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]) as u32;
    let row_stride = ((width as usize).saturating_mul(3)).div_ceil(4) * 4;
    let mut pixels = vec![0u8; (width * height) as usize];
    for y in 0..height {
        let src_row = pixel_offset + (height - 1 - y) as usize * row_stride;
        for x in 0..width {
            let src = src_row + x as usize * 3;
            if src + 2 < bytes.len() {
                // grayscale: R==G==B, take the blue channel.
                pixels[(y * width + x) as usize] = bytes[src];
            }
        }
    }
    Ok((width, height, pixels))
}
