use super::*;

#[test]
fn quality_mode_720p_maps_to_1080p() {
    let (w, h) = render_dimensions(UpscaleQualityMode::Quality, 1920, 1080).unwrap();
    assert_eq!((w, h), (1280, 720));
}

#[test]
fn quality_mode_scale_factors_match_fsr_table() {
    assert_eq!(UpscaleQualityMode::Native.scale_factor(), 1.0);
    assert_eq!(UpscaleQualityMode::Quality.scale_factor(), 1.5);
    assert_eq!(UpscaleQualityMode::Balanced.scale_factor(), 1.7);
    assert_eq!(UpscaleQualityMode::Performance.scale_factor(), 2.0);
    assert_eq!(UpscaleQualityMode::UltraPerformance.scale_factor(), 3.0);
}

#[test]
fn native_plan_is_1x_and_not_upscaled() {
    let settings = UpscaleSettings {
        mode: UpscaleQualityMode::Native,
        ..UpscaleSettings::default()
    };
    let plan = build_upscale_plan(settings, 1920, 1080).unwrap();
    assert!(plan.is_native());
    assert!((plan.pixel_ratio() - 1.0).abs() < 1.0e-6);
    assert_eq!(plan.render_width, 1920);
    assert_eq!(plan.render_height, 1080);
}

#[test]
fn ultra_performance_is_max_ratio() {
    let settings = UpscaleSettings {
        mode: UpscaleQualityMode::UltraPerformance,
        ..UpscaleSettings::default()
    };
    let plan = build_upscale_plan(settings, 1920, 1080).unwrap();
    assert_eq!((plan.render_width, plan.render_height), (640, 360));
    assert!(plan.pixel_ratio() > 8.9 && plan.pixel_ratio() < 9.1);
}

#[test]
fn default_settings_are_enabled_and_quality() {
    let settings = UpscaleSettings::default();
    assert!(settings.enabled);
    assert_eq!(settings.mode, UpscaleQualityMode::Quality);
    settings.validate().unwrap();
}

#[test]
fn invalid_sharpness_fails_loud() {
    let settings = UpscaleSettings {
        sharpness: 2.0,
        ..UpscaleSettings::default()
    };
    match settings.validate() {
        Err(EngineError::UpscaleInvalidConfig { field, .. }) => assert_eq!(field, "sharpness"),
        other => panic!("expected UpscaleInvalidConfig, got {other:?}"),
    }
}

#[test]
fn zero_output_fails_loud() {
    match render_dimensions(UpscaleQualityMode::Quality, 0, 1080) {
        Err(EngineError::UpscaleInvalidConfig { field, .. }) => assert_eq!(field, "output"),
        other => panic!("expected UpscaleInvalidConfig, got {other:?}"),
    }
}

#[test]
fn reproject_walks_history_backward() {
    let uv = reproject_uv(Vec2::new(0.5, 0.5), Vec2::new(0.1, -0.2)).unwrap();
    assert!((uv.x - 0.4).abs() < 1.0e-6);
    assert!((uv.y - 0.7).abs() < 1.0e-6);
}

#[test]
fn first_frame_and_out_of_bounds_reject_history() {
    assert_eq!(
        disocclusion_factor(true, Vec2::new(0.5, 0.5), Vec2::ZERO, 0.1),
        1.0
    );
    assert_eq!(
        disocclusion_factor(false, Vec2::new(-0.1, 0.5), Vec2::new(0.6, 0.0), 0.1),
        1.0
    );
}

#[test]
fn static_pixel_keeps_history() {
    let disocc = disocclusion_factor(false, Vec2::new(0.5, 0.5), Vec2::ZERO, 0.1);
    assert_eq!(disocc, 0.0);
    let accum = accumulate(
        Vec3::new(0.2, 0.2, 0.2),
        Vec3::new(0.8, 0.8, 0.8),
        0.9,
        disocc,
    )
    .unwrap();
    // history-weighted toward the accumulated value.
    assert!(accum.x > 0.6);
}

#[test]
fn neighborhood_clamp_rejects_ghost() {
    let clamped = neighborhood_clamp(
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.2, 0.2, 0.2),
        Vec3::new(0.4, 0.4, 0.4),
    );
    assert!(clamped.x <= 0.4 && clamped.x >= 0.2);
}

#[test]
fn rcas_preserves_flat_region_without_nan() {
    // Flat neighborhood at full sharpness must return the center exactly (the
    // old division-based form produced 0/0 = NaN here).
    let c = Vec3::splat(0.5);
    let out = rcas(c, c, c, c, c, 1.0);
    assert!(out.is_finite());
    assert!((out - c).length() < 1.0e-6);
}

#[test]
fn rcas_sharpens_toward_local_contrast_within_bounds() {
    // Center sits below a bright western neighbor; sharpening pushes it away
    // from the bright side (down), increasing local contrast, but never below
    // the neighborhood minimum.
    let center = Vec3::splat(0.55);
    let out = rcas(
        center,
        Vec3::splat(0.5),
        Vec3::splat(0.5),
        Vec3::splat(0.5),
        Vec3::splat(0.9),
        1.0,
    );
    assert!(out.is_finite());
    assert!(out.x < center.x);
    assert!(out.x >= 0.5 - 1.0e-6);
}
