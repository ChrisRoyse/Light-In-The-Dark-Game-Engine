use super::*;
use crate::{
    math::{Mat4, Vec3, Vec4},
    render::{
        font::{FontAtlasBuildRequest, reference_windows_font_path},
        ui::UiRenderSettings,
    },
};

const CHARSET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 ";

fn atlas_request() -> FontAtlasBuildRequest {
    FontAtlasBuildRequest::for_text(reference_windows_font_path(), CHARSET, 40.0, 64, 8, 12)
}

fn settings() -> UiRenderSettings {
    UiRenderSettings {
        viewport_width: 1280.0,
        viewport_height: 720.0,
    }
}

fn label(id: &str, text: &str, world: Vec3) -> WorldLabel {
    WorldLabel {
        id: id.to_string(),
        text: text.to_string(),
        world_position: world,
        world_offset: Vec3::ZERO,
        font_px: 32.0,
        color: Vec4::ONE,
    }
}

fn damage(id: &str, text: &str, world: Vec3) -> DamageNumber {
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

#[test]
fn atlas_cache_builds_once_and_reuses() {
    let mut cache = FontAtlasCache::new();
    let first = cache.get_or_build(atlas_request()).unwrap();
    assert_eq!(cache.misses(), 1);
    assert_eq!(cache.hits(), 0);
    let second = cache.get_or_build(atlas_request()).unwrap();
    assert_eq!(cache.misses(), 1, "identical request must not rebuild");
    assert_eq!(cache.hits(), 1);
    assert_eq!(cache.len(), 1);
    // Same underlying atlas, shared (not re-cloned).
    assert!(std::sync::Arc::ptr_eq(&first, &second));
}

#[test]
fn plan_covers_all_edge_cases_and_verifies() {
    let mut cache = FontAtlasCache::new();
    let scene = RuntimeTextScene {
        settings: settings(),
        view_projection: Mat4::IDENTITY,
        labels: vec![
            label("onscreen", "Ashen Reach", Vec3::ZERO),
            label("empty", "", Vec3::ZERO),
            // NDC.x = 100 -> outside the clip volume -> culled.
            label("offscreen", "Hidden", Vec3::new(100.0, 0.0, 0.0)),
            label("long", &"A".repeat(300), Vec3::ZERO),
        ],
        damage_numbers: vec![
            damage("hit", "245", Vec3::ZERO),
            damage("empty_hit", "", Vec3::ZERO),
            damage("far_hit", "999", Vec3::new(0.0, 100.0, 0.0)),
        ],
    };
    let plan = build_runtime_text_plan(&mut cache, &atlas_request(), &scene).unwrap();

    // First frame: exactly one atlas build.
    assert_eq!(cache.misses(), 1);

    let outcome = |id: &str| {
        plan.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.outcome)
            .unwrap()
    };

    // On-screen label rendered with 11 glyph quads ("Ashen Reach").
    match outcome("onscreen") {
        RuntimeTextOutcome::Rendered { quad_count, .. } => assert_eq!(quad_count, 11),
        other => panic!("onscreen label not rendered: {other:?}"),
    }
    // Empty string -> Empty, zero quads.
    assert_eq!(outcome("empty"), RuntimeTextOutcome::Empty);
    assert_eq!(outcome("empty_hit"), RuntimeTextOutcome::Empty);
    // Off-screen anchor -> OffScreen, zero quads.
    assert_eq!(outcome("offscreen"), RuntimeTextOutcome::OffScreen);
    assert_eq!(outcome("far_hit"), RuntimeTextOutcome::OffScreen);
    // Very long string -> all 300 glyphs laid out.
    match outcome("long") {
        RuntimeTextOutcome::Rendered { quad_count, .. } => assert_eq!(quad_count, 300),
        other => panic!("long label not rendered: {other:?}"),
    }

    assert_eq!(plan.empty_count, 2);
    assert_eq!(plan.off_screen_count, 2);
    assert_eq!(plan.rendered_count, 3);
    assert_eq!(plan.label_quad_count, 11 + 300);
    assert_eq!(plan.damage_quad_count, 3); // "245"
    assert_eq!(
        plan.quads.len(),
        plan.label_quad_count + plan.damage_quad_count
    );

    // Independent integrity re-derivation passes.
    verify_runtime_text_plan_integrity(&plan).unwrap();

    // Second frame reuses the cached atlas (no rebuild).
    let _ = build_runtime_text_plan(&mut cache, &atlas_request(), &scene).unwrap();
    assert_eq!(cache.misses(), 1);
    assert!(cache.hits() >= 1);
}

#[test]
fn damage_number_fades_alpha_over_lifetime() {
    let mut cache = FontAtlasCache::new();
    let scene = RuntimeTextScene {
        settings: settings(),
        view_projection: Mat4::IDENTITY,
        labels: Vec::new(),
        damage_numbers: vec![damage("hit", "5", Vec3::ZERO)],
    };
    let plan = build_runtime_text_plan(&mut cache, &atlas_request(), &scene).unwrap();
    // age/lifetime = 0.5 -> alpha halved from 1.0.
    let quad = plan.quads.first().unwrap();
    assert!(
        (quad.color.w - 0.5).abs() < 1e-6,
        "alpha faded to {}",
        quad.color.w
    );
}

#[test]
fn tampered_plan_fails_verification() {
    let mut cache = FontAtlasCache::new();
    let scene = RuntimeTextScene {
        settings: settings(),
        view_projection: Mat4::IDENTITY,
        labels: vec![label("onscreen", "Hi", Vec3::ZERO)],
        damage_numbers: Vec::new(),
    };
    let mut plan = build_runtime_text_plan(&mut cache, &atlas_request(), &scene).unwrap();
    // Corrupt a quad without recomputing the hash.
    plan.quads[0].color.w = 0.123;
    assert!(verify_runtime_text_plan_integrity(&plan).is_err());
}

#[test]
fn missing_glyph_fails_loud() {
    let mut cache = FontAtlasCache::new();
    // Charset lacks the ampersand.
    let scene = RuntimeTextScene {
        settings: settings(),
        view_projection: Mat4::IDENTITY,
        labels: vec![label("bad", "A&B", Vec3::ZERO)],
        damage_numbers: Vec::new(),
    };
    assert!(build_runtime_text_plan(&mut cache, &atlas_request(), &scene).is_err());
}
