//! Full State Verification for #882 - runtime text: world-anchored labels +
//! screen-space damage numbers over a cross-frame-cached SDF font atlas.
//!
//! The produced game (not the egui editor) needs text at runtime. This harness
//! proves the runtime-text vertical slice end to end, at two independent layers:
//!
//!   A. GLYPH-QUAD BUFFER readback - build a plan, serialize it to JSON, read the
//!      JSON back from disk, and re-verify its integrity hash + entry tallies. The
//!      three required edge cases each get an explicit, recorded outcome:
//!        - empty string        -> Empty      (zero quads, no error)
//!        - off-screen anchor    -> OffScreen  (zero quads, culled by projection)
//!        - very long string     -> Rendered   (quad count == character count)
//!   B. PIXEL readback - rasterize the on-screen label's quads through the cached
//!      SDF atlas into a BMP, write it, re-read the BMP from disk, and measure the
//!      inked width. Real ink proves the cached atlas + layout produce visible text.
//!
//! Atlas caching is proven directly: the atlas is built exactly once (misses == 1)
//! and reused across a second frame (hits >= 1).
#![recursion_limit = "256"]

use std::{fs, sync::Arc};

use litd_engine::{
    math::{Mat4, Vec3, Vec4},
    render::{
        font::{FontAtlas, FontAtlasBuild, FontAtlasBuildRequest, reference_windows_font_path},
        runtime_text::{
            DamageNumber, FontAtlasCache, RuntimeTextOutcome, RuntimeTextPlan, RuntimeTextScene,
            WorldLabel, build_runtime_text_plan, verify_runtime_text_plan_integrity,
        },
        ui::{GlyphQuad, UiRenderSettings},
    },
};
use serde_json::json;

const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 ";
const FONT_PX: f32 = 40.0;
const CELL_PX: u32 = 64;
const SPREAD_PX: u32 = 8;
const COLUMNS: u32 = 12;
const INK_THRESHOLD: u8 = 96;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue882_runtime_text")?;
    let request = FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        CHARSET,
        FONT_PX,
        CELL_PX,
        SPREAD_PX,
        COLUMNS,
    );

    // ---- Scene: on-screen label, empty label, off-screen label, long label,
    // on-screen damage number, empty number, off-screen number. ----
    let scene = RuntimeTextScene {
        settings: UiRenderSettings {
            viewport_width: 1280.0,
            viewport_height: 720.0,
        },
        view_projection: Mat4::IDENTITY,
        labels: vec![
            world_label("nameplate", "Ashen Reach", Vec3::ZERO),
            world_label("empty_label", "", Vec3::ZERO),
            world_label("offscreen_label", "Hidden", Vec3::new(100.0, 0.0, 0.0)),
            world_label("long_label", &"A".repeat(300), Vec3::ZERO),
        ],
        damage_numbers: vec![
            damage_number("crit", "245", Vec3::ZERO),
            damage_number("empty_hit", "", Vec3::ZERO),
            damage_number("far_hit", "999", Vec3::new(0.0, 100.0, 0.0)),
        ],
    };

    // ---- Build the plan; prove the atlas is built once and reused a 2nd frame. ----
    let mut cache = FontAtlasCache::new();
    let plan = build_runtime_text_plan(&mut cache, &request, &scene)?;
    let misses_after_frame1 = cache.misses();
    let _frame2 = build_runtime_text_plan(&mut cache, &request, &scene)?;
    let misses_after_frame2 = cache.misses();
    let hits_after_frame2 = cache.hits();
    let atlas_built_once = misses_after_frame1 == 1 && misses_after_frame2 == 1;
    let atlas_reused = hits_after_frame2 >= 1;
    assert!(
        atlas_built_once,
        "SDF atlas must be built exactly once across frames"
    );
    assert!(atlas_reused, "second frame must reuse the cached atlas");

    // ---- Layer A: glyph-quad buffer readback through a real disk round-trip. ----
    let plan_json_path = run.run_dir().join("runtime-text-plan.json");
    fs::write(&plan_json_path, serde_json::to_vec_pretty(&plan)?)?;
    let reloaded: RuntimeTextPlan = serde_json::from_slice(&fs::read(&plan_json_path)?)?;
    verify_runtime_text_plan_integrity(&reloaded)?;
    let integrity_ok = reloaded.fnv1a64 == plan.fnv1a64;
    assert!(
        integrity_ok,
        "reloaded plan hash must equal the built plan hash"
    );

    // Edge-case outcomes (read from the RELOADED plan, not the in-memory one).
    let nameplate_quads = rendered_count(&reloaded, "nameplate");
    let long_quads = rendered_count(&reloaded, "long_label");
    let crit_quads = rendered_count(&reloaded, "crit");
    let empty_label_ok = outcome_is_empty(&reloaded, "empty_label");
    let empty_hit_ok = outcome_is_empty(&reloaded, "empty_hit");
    let offscreen_label_ok = outcome_is_offscreen(&reloaded, "offscreen_label");
    let far_hit_ok = outcome_is_offscreen(&reloaded, "far_hit");

    assert_eq!(nameplate_quads, Some(11), "'Ashen Reach' is 11 glyphs");
    assert_eq!(
        long_quads,
        Some(300),
        "very long string lays out every glyph"
    );
    assert_eq!(crit_quads, Some(3), "'245' is 3 glyphs");
    assert!(
        empty_label_ok && empty_hit_ok,
        "empty strings must be Empty (0 quads)"
    );
    assert!(
        offscreen_label_ok && far_hit_ok,
        "off-screen anchors must be culled"
    );
    // Culling reaches the buffer: total quads == only the rendered characters.
    let phantom_free = reloaded.quads.len() == 11 + 300 + 3;
    assert!(
        phantom_free,
        "empty + off-screen items must contribute zero quads"
    );

    // ---- Layer B: pixel readback - rasterize the nameplate through the atlas. ----
    let build = cache.get_or_build(request.clone())?; // cache hit; gives us the image
    let (nameplate_start, nameplate_count) = rendered_range(&plan, "nameplate").unwrap();
    let nameplate_slice = &plan.quads[nameplate_start..nameplate_start + nameplate_count];
    let bmp_path = run.run_dir().join("rendered-nameplate.bmp");
    let raster = rasterize_quads(&build, nameplate_slice);
    fs::write(&bmp_path, raster.bmp_bytes())?;
    let ink_width = ink_width_from_bmp(&fs::read(&bmp_path)?)?;
    let pixel_proof = ink_width > (nameplate_count as u32); // wider than 1px/glyph
    assert!(
        pixel_proof,
        "rendered nameplate must have real ink, got {ink_width}px"
    );

    // ---- Combined machine verdict, computed (NOT a literal) from every check so
    // the reported pass/fail is derived from real state, not asserted true. ----
    let all_checks = atlas_built_once
        && atlas_reused
        && integrity_ok
        && nameplate_quads == Some(11)
        && long_quads == Some(300)
        && crit_quads == Some(3)
        && empty_label_ok
        && empty_hit_ok
        && offscreen_label_ok
        && far_hit_ok
        && phantom_free
        && pixel_proof;
    assert!(all_checks, "combined runtime-text verdict must hold");

    // ---- Edge-case before/after state (input -> recorded outcome), read from the
    // reloaded plan so the source of truth is the disk artifact, not memory. ----
    let edge_cases = json!([
        {
            "case": "empty_string",
            "before": {"id": "empty_label", "text": "", "anchor_on_screen": true},
            "after": {"outcome": "Empty", "quads": 0, "matched": empty_label_ok}
        },
        {
            "case": "off_screen_anchor",
            "before": {"id": "offscreen_label", "text": "Hidden", "ndc_x": 100.0, "anchor_on_screen": false},
            "after": {"outcome": "OffScreen", "quads": 0, "matched": offscreen_label_ok}
        },
        {
            "case": "very_long_string",
            "before": {"id": "long_label", "text_len": 300, "anchor_on_screen": true},
            "after": {"outcome": "Rendered", "quads": long_quads, "matched": long_quads == Some(300)}
        }
    ]);

    let report = json!({
        "schema": "litd.issue882.runtime_text_fsv.v1",
        "issue": 882,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "plan_json": plan_json_path.display().to_string(),
            "rendered_nameplate_bmp": bmp_path.display().to_string(),
            "font_path": reference_windows_font_path().display().to_string(),
            "manual_gate": "Read the plan JSON and open the nameplate BMP via Synapse. 'Ashen Reach' must be legible; the plan's entry outcomes below must match (empty->Empty, off-screen->OffScreen, long->300 quads)."
        },
        "license": {
            "rasterize_crate": "fontdue (MIT/Apache-2.0)",
            "shape_crate": "rustybuzz (MIT)",
            "note": "#1073-clean: both crates permissive; no font is vendored into the repo. build_runtime_text_plan takes the font path as a parameter, so shipped game code bakes in no proprietary font. reference_windows_font_path (arial.ttf) is a dev-box reference input to this harness and the pre-existing engine text tests only, never shipped."
        },
        "research_sources": [
            "https://github.com/grovesNL/glyphon",
            "https://www.redblobgames.com/x/2403-distance-field-fonts/",
            "https://aras-p.info/blog/2017/02/15/Font-Rendering-is-Getting-Interesting/",
            "https://libgdx.com/wiki/graphics/2d/fonts/distance-field-fonts"
        ],
        "design_decision": "Stayed on the existing fontdue(raster)+rustybuzz(shape) SDF-atlas stack instead of adding glyphon/cosmic-text. Both are permissive (#1073), but one SDF atlas is scale-free (same texture at any label distance / number scale) and avoids glyphon's larger tree + per-pixel-size re-raster - the right cost for the $500-PC game budget (#928). Atlas cached once and reused every frame.",
        "sufficiency_argument": "One frame containing all three required edge cases (empty, off-screen, very long) plus a normal label and a normal damage number is the minimal scene that exercises every branch of build_runtime_text_plan: projection cull, empty skip, full layout, and animation. Two frames prove the atlas cache (build once, reuse). Verified at the glyph-quad buffer layer (disk round-trip + integrity hash) and the pixel layer (rasterized BMP round-trip).",
        "atlas_cache": {
            "misses_after_frame1": misses_after_frame1,
            "misses_after_frame2": misses_after_frame2,
            "hits_after_frame2": hits_after_frame2,
            "built_once": atlas_built_once,
            "reused_next_frame": atlas_reused,
            "cached_atlas_count": cache.len()
        },
        "layer_a_glyph_quads": {
            "plan_fnv1a64": plan.fnv1a64,
            "reloaded_fnv1a64": reloaded.fnv1a64,
            "integrity_ok": integrity_ok,
            "total_quads": reloaded.quads.len(),
            "label_quads": reloaded.label_quad_count,
            "damage_quads": reloaded.damage_quad_count,
            "rendered_count": reloaded.rendered_count,
            "empty_count": reloaded.empty_count,
            "off_screen_count": reloaded.off_screen_count,
            "nameplate_quads": nameplate_quads,
            "long_string_quads": long_quads,
            "damage_number_quads": crit_quads,
            "empty_are_empty": empty_label_ok && empty_hit_ok,
            "offscreen_are_culled": offscreen_label_ok && far_hit_ok,
            "no_phantom_quads": phantom_free
        },
        "layer_b_pixels": {
            "rendered_nameplate_ink_width_px": ink_width,
            "glyph_count": nameplate_count,
            "pixel_proof_passed": pixel_proof
        },
        "edge_cases": edge_cases,
        "verdict": {
            "all_checks_passed": all_checks
        }
    });

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE882_RUNTIME_TEXT_FSV verdict={} report={} report_sha256={} atlas_misses={} atlas_hits={} total_quads={} rendered={} empty={} offscreen={} nameplate_ink_px={}",
        all_checks,
        readback.path.display(),
        readback.sha256,
        misses_after_frame2,
        hits_after_frame2,
        reloaded.quads.len(),
        reloaded.rendered_count,
        reloaded.empty_count,
        reloaded.off_screen_count,
        ink_width,
    );
    Ok(())
}

fn world_label(id: &str, text: &str, world: Vec3) -> WorldLabel {
    WorldLabel {
        id: id.to_string(),
        text: text.to_string(),
        world_position: world,
        world_offset: Vec3::ZERO,
        font_px: 32.0,
        color: Vec4::ONE,
    }
}

fn damage_number(id: &str, text: &str, world: Vec3) -> DamageNumber {
    DamageNumber {
        id: id.to_string(),
        text: text.to_string(),
        world_position: world,
        age_seconds: 0.5,
        lifetime_seconds: 1.0,
        rise_px_per_second: 30.0,
        font_px: 28.0,
        color: Vec4::ONE,
    }
}

fn rendered_range(plan: &RuntimeTextPlan, id: &str) -> Option<(usize, usize)> {
    plan.entries
        .iter()
        .find(|e| e.id == id)
        .and_then(|e| match e.outcome {
            RuntimeTextOutcome::Rendered {
                quad_start,
                quad_count,
                ..
            } => Some((quad_start, quad_count)),
            _ => None,
        })
}

fn rendered_count(plan: &RuntimeTextPlan, id: &str) -> Option<usize> {
    rendered_range(plan, id).map(|(_, count)| count)
}

fn outcome_is_empty(plan: &RuntimeTextPlan, id: &str) -> bool {
    plan.entries
        .iter()
        .find(|e| e.id == id)
        .is_some_and(|e| matches!(e.outcome, RuntimeTextOutcome::Empty))
}

fn outcome_is_offscreen(plan: &RuntimeTextPlan, id: &str) -> bool {
    plan.entries
        .iter()
        .find(|e| e.id == id)
        .is_some_and(|e| matches!(e.outcome, RuntimeTextOutcome::OffScreen))
}

/// Rasterize laid-out quads by nearest-neighbor sampling each glyph's atlas cell
/// (via its UV) into the quad's pixel rect - the CPU analogue of the textured quad
/// the GPU draws. Origin is normalized so the leftmost glyph starts at x=0.
fn rasterize_quads(build: &Arc<FontAtlasBuild>, quads: &[GlyphQuad]) -> GrayImage {
    let atlas: &FontAtlas = &build.atlas;
    let image = &build.image;
    let min_x = quads
        .iter()
        .map(|q| q.rect_px.min.x)
        .fold(f32::MAX, f32::min);
    let min_y = quads
        .iter()
        .map(|q| q.rect_px.min.y)
        .fold(f32::MAX, f32::min);
    let max_x = quads
        .iter()
        .map(|q| q.rect_px.max.x)
        .fold(f32::MIN, f32::max);
    let width = ((max_x - min_x).ceil() as u32 + 2).max(2);
    let height = (atlas.font_px.ceil() as u32 + 2).max(2);
    let mut out = GrayImage::new(width, height);
    let aw = image.width as f32;
    let ah = image.height as f32;
    for quad in quads {
        let rx0 = quad.rect_px.min.x - min_x;
        let ry0 = quad.rect_px.min.y - min_y;
        let rw = (quad.rect_px.max.x - quad.rect_px.min.x).max(1.0);
        let rh = (quad.rect_px.max.y - quad.rect_px.min.y).max(1.0);
        let x0 = rx0.floor().max(0.0) as u32;
        let x1 = ((rx0 + rw).ceil() as u32).min(width);
        let y0 = ry0.floor().max(0.0) as u32;
        let y1 = ((ry0 + rh).ceil() as u32).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                let u =
                    quad.uv.min.x + (quad.uv.max.x - quad.uv.min.x) * ((x as f32 + 0.5 - rx0) / rw);
                let v =
                    quad.uv.min.y + (quad.uv.max.y - quad.uv.min.y) * ((y as f32 + 0.5 - ry0) / rh);
                let sx = (u * aw).floor().clamp(0.0, aw - 1.0) as u32;
                let sy = (v * ah).floor().clamp(0.0, ah - 1.0) as u32;
                if let Ok(sample) = image.pixel(sx, sy) {
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
                pixels[(y * width + x) as usize] = bytes[src];
            }
        }
    }
    Ok((width, height, pixels))
}
