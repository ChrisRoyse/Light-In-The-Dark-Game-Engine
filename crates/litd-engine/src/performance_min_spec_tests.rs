use crate::{
    performance_min_spec::{
        ISSUE1153_MIN_SPEC_SCHEMA, ISSUE1153_MIN_SPEC_SLUG, low_player_min_spec_profile,
        run_issue1153_player_min_spec_harness, validate_player_min_spec_profile,
    },
    performance_quality::{GraphicsQualityTier, evaluate_draw_budget},
};

#[test]
fn player_min_spec_profile_targets_1080p60_low_with_upscaling() {
    let profile = low_player_min_spec_profile().unwrap();

    assert_eq!(profile.target.output_width, 1920);
    assert_eq!(profile.target.output_height, 1080);
    assert_eq!(profile.target.target_fps, 60);
    assert_eq!(profile.target.gpu_vram_floor_gb, 6);
    assert_eq!(profile.target.ram_gb, 16);
    assert_eq!(profile.settings.tier, GraphicsQualityTier::Low);
    assert_eq!(profile.settings.render_scale_percent, 50);
    assert_eq!(profile.internal_render_width, 960);
    assert_eq!(profile.internal_render_height, 540);
    assert!(profile.internal_pixels < profile.output_pixels);
    assert!(profile.upscaler.open_source_required);
    assert!(profile.upscaler.required_when_render_scale_below_100);
    assert!(!profile.upscaler.frame_generation_required_for_min_spec);
}

#[test]
fn player_min_spec_profile_rejects_missing_upscaler_and_over_budget() {
    let mut profile = low_player_min_spec_profile().unwrap();
    profile.upscaler.open_source_required = false;
    profile.upscaler.required_when_render_scale_below_100 = false;
    profile.upscaler.technology.clear();

    let error = validate_player_min_spec_profile(&profile).unwrap_err();
    assert_eq!(error.code(), "LITD-GAME-PACKAGE-INVALID-CONFIG");

    let low = GraphicsQualityTier::Low.settings();
    let error = evaluate_draw_budget(low, low.max_draw_calls_per_frame + 1).unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-BATCHING-DRAW-LIMIT");
}

#[test]
fn issue1153_harness_persists_player_min_spec_readback() {
    let readback = pollster::block_on(run_issue1153_player_min_spec_harness()).unwrap();
    let report = &readback.json;

    assert_eq!(report["schema"], ISSUE1153_MIN_SPEC_SCHEMA);
    assert_eq!(report["issue"], 1153);
    assert_eq!(report["happy_path"]["profile"]["target"]["target_fps"], 60);
    assert_eq!(
        report["happy_path"]["profile"]["settings"]["tier"],
        GraphicsQualityTier::Low.as_str()
    );
    assert_eq!(
        report["happy_path"]["profile"]["upscaler"]["open_source_required"],
        true
    );
    assert_eq!(
        report["happy_path"]["low_visual_floor_probe"]["after"]["comparison"]["pass"],
        true
    );
    assert_eq!(
        report["happy_path"]["low_render_probe"]["after"]["draw_budget"]["draw_calls_within_budget"],
        true
    );
    assert_eq!(
        report["edges"]["invalid_ram_floor"]["after"]["error_code"],
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["missing_upscaler_for_scaled_low"]["after"]["error_code"],
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["over_draw_budget"]["after"]["error_code"],
        "LITD-RENDER-BATCHING-DRAW-LIMIT"
    );
    assert_eq!(readback.manifest["issue_slug"], ISSUE1153_MIN_SPEC_SLUG);
    assert_eq!(readback.manifest["report_sha256"], readback.sha256);

    println!(
        "ISSUE1153_FSV_REPORT_READBACK path={} bytes={} sha256={} visual_bmp_sha256={} gpu_bmp_sha256={} draw_calls={} internal={}x{}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        report["happy_path"]["low_visual_floor_probe"]["after"]["bmp_readback"]["sha256"],
        report["happy_path"]["low_render_probe"]["after"]["bmp_readback"]["sha256"],
        report["happy_path"]["low_render_probe"]["after"]["draw_calls"],
        report["happy_path"]["profile"]["internal_render_width"],
        report["happy_path"]["profile"]["internal_render_height"],
    );
}
