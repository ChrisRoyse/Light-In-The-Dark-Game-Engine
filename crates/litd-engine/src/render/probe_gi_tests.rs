//! Unit coverage for the probe-grid GI primitives: octahedral mapping, grid indexing,
//! bake determinism, quantization round-trip, Chebyshev visibility, and cage sampling.

use super::*;
use crate::math::{Aabb, Vec3};

fn test_spec() -> ProbeGridSpec {
    ProbeGridSpec {
        origin: Vec3::new(0.0, 0.0, 0.0),
        spacing: Vec3::splat(1.0),
        counts: [3, 3, 3],
        irradiance_dim: 6,
        visibility_dim: 6,
    }
}

fn red_panel_scene(state: &str, sky: Vec3) -> BakeScene {
    BakeScene::new(
        state,
        vec![EmissivePanel::new(
            PanelAxis::X,
            -0.5,
            [-1.0, -1.0],
            [3.0, 3.0],
            Vec3::new(4.0, 0.1, 0.1),
        )],
        vec![],
        SkyLighting::new(sky, sky * 0.5),
    )
}

#[test]
fn octahedral_roundtrip_recovers_axis_and_diagonal_directions() {
    let dirs = [
        Vec3::X,
        Vec3::Y,
        Vec3::Z,
        -Vec3::X,
        -Vec3::Y,
        -Vec3::Z,
        Vec3::new(1.0, 1.0, 1.0).normalize(),
        Vec3::new(-1.0, 0.5, -0.3).normalize(),
    ];
    for dir in dirs {
        let uv = oct_encode(dir);
        assert!((0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y));
        let back = oct_decode(uv);
        assert!(
            dir.dot(back) > 0.999,
            "octahedral roundtrip failed for {dir:?}: got {back:?}"
        );
    }
}

#[test]
fn grid_indexing_is_x_fastest_and_positions_follow_spacing() {
    let spec = test_spec();
    assert_eq!(spec.probe_count().unwrap(), 27);
    assert_eq!(spec.linear_index(0, 0, 0).unwrap(), 0);
    assert_eq!(spec.linear_index(1, 0, 0).unwrap(), 1);
    assert_eq!(spec.linear_index(0, 1, 0).unwrap(), 3);
    assert_eq!(spec.linear_index(0, 0, 1).unwrap(), 9);
    assert_eq!(spec.probe_position(2, 1, 0), Vec3::new(2.0, 1.0, 0.0));
    assert_eq!(spec.max_corner(), Vec3::new(2.0, 2.0, 2.0));
}

#[test]
fn spec_validation_rejects_degenerate_grids() {
    let mut spec = test_spec();
    spec.counts = [1, 3, 3];
    assert!(matches!(
        spec.validate(),
        Err(ProbeGiError::InvalidConfig { .. })
    ));
    let mut spec = test_spec();
    spec.spacing = Vec3::new(0.0, 1.0, 1.0);
    assert!(matches!(
        spec.validate(),
        Err(ProbeGiError::InvalidConfig { .. })
    ));
}

#[test]
fn bake_is_deterministic_and_fingerprint_is_stable() {
    let spec = test_spec();
    let scene = red_panel_scene("day", Vec3::new(0.1, 0.12, 0.16));
    let a = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let b = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    assert_eq!(a, b, "bake must be bit-identical across runs");

    let low_a = lower_probe_volume(&a).unwrap();
    let low_b = lower_probe_volume(&b).unwrap();
    assert_eq!(
        low_a.fingerprint, low_b.fingerprint,
        "lowered fingerprint must be reproducible"
    );
    // Independent verification recomputes the hash from the stored integers.
    let readback = verify_lowered_probe_volume(&low_a).unwrap();
    assert_eq!(readback.fingerprint, low_a.fingerprint);
    assert_eq!(readback.probe_count, 27);
}

#[test]
fn fingerprint_verification_fails_on_tampered_artifact() {
    let spec = test_spec();
    let scene = red_panel_scene("day", Vec3::new(0.1, 0.12, 0.16));
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let mut lowered = lower_probe_volume(&volume).unwrap();
    lowered.irradiance_q[0] = lowered.irradiance_q[0].wrapping_add(7);
    assert!(matches!(
        verify_lowered_probe_volume(&lowered),
        Err(ProbeGiError::FingerprintMismatch { .. })
    ));
}

#[test]
fn decode_round_trips_within_quantization_tolerance() {
    let spec = test_spec();
    let scene = red_panel_scene("day", Vec3::new(0.1, 0.12, 0.16));
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let lowered = lower_probe_volume(&volume).unwrap();
    let decoded = lowered.decode().unwrap();
    assert_eq!(decoded.spec, volume.spec);
    for (a, b) in volume.probes.iter().zip(&decoded.probes) {
        for (ta, tb) in a.irradiance.iter().zip(&b.irradiance) {
            assert!((ta.x - tb.x).abs() < 1.0e-3);
            assert!((ta.y - tb.y).abs() < 1.0e-3);
        }
    }
}

#[test]
fn irradiance_is_stronger_toward_the_emissive_panel() {
    let spec = test_spec();
    let scene = red_panel_scene("day", Vec3::new(0.05, 0.06, 0.08));
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let config = ProbeSampleConfig::default();
    // Facing the panel (-x) should collect far more red than facing away (+x).
    let toward = sample_irradiance(&volume, Vec3::new(0.5, 1.0, 1.0), -Vec3::X, config).unwrap();
    let away = sample_irradiance(&volume, Vec3::new(1.5, 1.0, 1.0), Vec3::X, config).unwrap();
    assert!(
        toward.x > away.x + 0.05,
        "toward-panel red {} should exceed away red {}",
        toward.x,
        away.x
    );
}

#[test]
fn light_level_decreases_with_distance_from_panel() {
    let spec = ProbeGridSpec {
        counts: [5, 3, 3],
        ..test_spec()
    };
    let scene = red_panel_scene("day", Vec3::new(0.02, 0.024, 0.03));
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let config = ProbeSampleConfig::default();
    let near = light_level_at(&volume, Vec3::new(0.5, 1.0, 1.0), config)
        .unwrap()
        .light_level;
    let mid = light_level_at(&volume, Vec3::new(2.0, 1.0, 1.0), config)
        .unwrap()
        .light_level;
    let far = light_level_at(&volume, Vec3::new(3.5, 1.0, 1.0), config)
        .unwrap()
        .light_level;
    assert!(near > mid, "near {near} should exceed mid {mid}");
    assert!(mid > far, "mid {mid} should exceed far {far}");
    assert!((0.0..=1.0).contains(&near));
}

#[test]
fn occluder_blocks_light_leak_behind_a_wall() {
    // Panel on -x, a wall at x=2.5 spanning the whole cross-section, shadow region at x>2.5.
    let spec = ProbeGridSpec {
        counts: [6, 3, 3],
        ..test_spec()
    };
    let scene = BakeScene::new(
        "day",
        vec![EmissivePanel::new(
            PanelAxis::X,
            -0.5,
            [-1.0, -1.0],
            [3.0, 3.0],
            Vec3::new(4.0, 0.1, 0.1),
        )],
        vec![Aabb::new(Vec3::new(2.4, -1.0, -1.0), Vec3::new(2.6, 3.0, 3.0)).unwrap()],
        SkyLighting::new(Vec3::new(0.02, 0.024, 0.03), Vec3::new(0.01, 0.01, 0.01)),
    );
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let config = ProbeSampleConfig::default();
    let lit = light_level_at(&volume, Vec3::new(0.5, 1.0, 1.0), config)
        .unwrap()
        .light_level;
    let shadow = light_level_at(&volume, Vec3::new(4.5, 1.0, 1.0), config)
        .unwrap()
        .light_level;
    assert!(
        lit > shadow + 0.02,
        "lit {lit} must clearly exceed shadowed {shadow} (no leak)"
    );
}

#[test]
fn day_state_is_brighter_than_night_state() {
    let spec = ProbeGridSpec {
        counts: [5, 3, 3],
        ..test_spec()
    };
    let day = bake_probe_volume(
        spec,
        &red_panel_scene("day", Vec3::new(0.30, 0.34, 0.45)),
        BakeConfig::default(),
    )
    .unwrap();
    let night = bake_probe_volume(
        spec,
        &red_panel_scene("night", Vec3::new(0.01, 0.012, 0.02)),
        BakeConfig::default(),
    )
    .unwrap();
    let config = ProbeSampleConfig::default();
    let pos = Vec3::new(2.0, 1.5, 1.0);
    let day_ll = light_level_at(&day, pos, config).unwrap().light_level;
    let night_ll = light_level_at(&night, pos, config).unwrap().light_level;
    assert!(
        day_ll > night_ll + 0.01,
        "day {day_ll} should exceed night {night_ll}"
    );
}

#[test]
fn sampling_far_outside_the_volume_fails_loud() {
    let spec = test_spec();
    let scene = red_panel_scene("day", Vec3::new(0.1, 0.1, 0.1));
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let result = light_level_at(
        &volume,
        Vec3::new(50.0, 1.0, 1.0),
        ProbeSampleConfig::default(),
    );
    assert!(matches!(
        result,
        Err(ProbeGiError::SampleOutOfBounds { .. })
    ));
}

#[test]
fn half_res_field_upsamples_close_to_full_resolution_query() {
    let spec = ProbeGridSpec {
        counts: [5, 3, 5],
        ..test_spec()
    };
    let scene = red_panel_scene("day", Vec3::new(0.05, 0.06, 0.08));
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default()).unwrap();
    let config = ProbeSampleConfig::default();
    // Half-resolution evaluation lattice (Overwatch 2 low-tier path).
    let half = light_level_field(&volume, config, [3, 2, 3]).unwrap();
    let pos = Vec3::new(1.7, 1.0, 2.3);
    let full = light_level_at(&volume, pos, config).unwrap().light_level;
    let upsampled = half.sample(pos);
    assert!(
        (full - upsampled).abs() < 0.06,
        "half-res upsample {upsampled} should track full-res {full}"
    );
}
