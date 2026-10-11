use crate::{
    performance_quality::{
        GraphicsQualityTier, ISSUE1148_PERFORMANCE_SCHEMA, ISSUE1148_PERFORMANCE_SLUG,
        ULTRA_MAX_DRAW_CALLS, default_quality_scale, evaluate_draw_budget,
        run_issue1148_performance_quality_harness, validate_quality_scale,
        workstation_performance_target,
    },
    render::{FrameBudget, MAX_FRAME_ALLOCATIONS},
};

#[test]
fn quality_scale_defaults_to_ultra_and_is_monotonic() {
    let target = workstation_performance_target();
    let scale = default_quality_scale();
    let validation = validate_quality_scale(&scale).unwrap();
    let ultra = GraphicsQualityTier::Ultra.settings();

    assert_eq!(target.default_tier, GraphicsQualityTier::Ultra);
    assert!(validation.ultra_is_default);
    assert_eq!(validation.tier_count, 4);
    assert_eq!(ultra.target_width, 3840);
    assert_eq!(ultra.target_height, 2160);
    assert_eq!(ultra.target_fps, 120);
    assert_eq!(FrameBudget::default().max_draw_calls, ULTRA_MAX_DRAW_CALLS);
    assert_eq!(ultra.frame_budget().max_draw_calls, ULTRA_MAX_DRAW_CALLS);
}

#[test]
fn every_quality_tier_frame_budget_uses_the_real_allocation_ceiling() {
    // #1532 residual: `frame_budget()` hardcoded `max_allocations: 0` for every quality tier.
    // While `FrameStats.allocations` was itself hardcoded `0`, `0 <= 0` was a decorative
    // false-green; once #1532 fed the REAL per-frame allocation delta, a `0` ceiling here made
    // `budget_ok` FALSE on EVERY `--quality` game frame (measured ~163 allocs/frame at quality
    // low >> 0) — a false-RED regression on the shipping min-spec path. Every tier must now
    // share the same real, failable ceiling as `FrameBudget::default()`.
    for tier in GraphicsQualityTier::ordered() {
        let budget = tier.settings().frame_budget();
        assert_eq!(
            budget.max_allocations, MAX_FRAME_ALLOCATIONS,
            "{tier:?} frame_budget must use the real allocation ceiling, not 0"
        );
        // A realistic steady-state frame (quality-low measured ~163 allocs) stays green...
        assert!(
            budget.is_within(1, 163),
            "{tier:?} legit steady-state frame must pass the allocation budget"
        );
        // ...but a real per-frame allocation runaway above the ceiling trips it (failable).
        assert!(
            !budget.is_within(1, MAX_FRAME_ALLOCATIONS + 1),
            "{tier:?} allocation budget must be able to go red"
        );
    }
}

#[test]
fn draw_budget_rejects_over_budget_frames_loudly() {
    let settings = GraphicsQualityTier::Low.settings();
    let readback = evaluate_draw_budget(settings, settings.max_draw_calls_per_frame).unwrap();
    assert!(readback.draw_calls_within_budget);

    let error = evaluate_draw_budget(settings, settings.max_draw_calls_per_frame + 1).unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-BATCHING-DRAW-LIMIT");
}

#[test]
fn issue1148_harness_persists_ultra_quality_perf_readback() {
    let readback = pollster::block_on(run_issue1148_performance_quality_harness()).unwrap();
    let report = &readback.json;

    assert_eq!(report["schema"], ISSUE1148_PERFORMANCE_SCHEMA);
    assert_eq!(report["issue"], 1148);
    assert_eq!(
        report["happy_path"]["target"]["default_tier"],
        GraphicsQualityTier::Ultra.as_str()
    );
    assert_eq!(report["happy_path"]["validation"]["ultra_is_default"], true);
    assert_eq!(
        report["happy_path"]["ultra_render_probe"]["after"]["draw_calls"],
        2
    );
    assert_eq!(
        report["happy_path"]["ultra_render_probe"]["after"]["draw_budget"]["draw_calls_within_budget"],
        true
    );
    assert_eq!(
        report["edges"]["invalid_zero_fps"]["after"]["error_code"],
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["over_draw_budget"]["after"]["error_code"],
        "LITD-RENDER-BATCHING-DRAW-LIMIT"
    );
    assert_eq!(readback.manifest["issue_slug"], ISSUE1148_PERFORMANCE_SLUG);
    assert_eq!(readback.manifest["report_sha256"], readback.sha256);

    println!(
        "ISSUE1148_FSV_REPORT_READBACK path={} bytes={} sha256={} bmp_sha256={} wall_clock_ns={} draw_calls={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        report["happy_path"]["ultra_render_probe"]["after"]["bmp_readback"]["sha256"],
        report["happy_path"]["ultra_render_probe"]["after"]["wall_clock_ns"],
        report["happy_path"]["ultra_render_probe"]["after"]["draw_calls"],
    );
}
