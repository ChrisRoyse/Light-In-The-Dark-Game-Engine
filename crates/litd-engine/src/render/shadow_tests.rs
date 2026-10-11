use super::*;
use crate::render::lighting::SpotCone;

#[test]
fn cascade_ranges_are_monotonic_and_reach_camera_far() {
    let camera = test_camera();
    let ranges = cascade_ranges(ShadowSettings::default(), camera).unwrap();
    assert_eq!(ranges.len(), 4);
    assert!((ranges[0].0 - camera.near).abs() < 0.0001);
    assert!((ranges[3].1 - camera.far).abs() < 0.0001);
    assert!(
        ranges
            .windows(2)
            .all(|pair| pair[0].1 <= pair[1].0 + 0.0001)
    );
}

#[test]
fn shadow_plan_covers_directional_point_and_spot_lights() {
    let plan = build_shadow_plan(ShadowSettings::default(), test_camera(), &test_lights()).unwrap();
    assert_eq!(plan.directional_cascade_count(), 4);
    assert_eq!(plan.point_face_count(), 6);
    assert_eq!(plan.spot_map_count(), 1);
    assert_eq!(plan.entries.len(), 11);
    assert_eq!(plan.entries[10].tile.x, 2048);
}

#[test]
fn shadow_atlas_overflow_fails_loud() {
    let settings = ShadowSettings {
        atlas_size: 1024,
        tile_size: 1024,
        ..ShadowSettings::default()
    };
    match build_shadow_plan(settings, test_camera(), &test_lights()) {
        Err(EngineError::ShadowAtlasOverflow {
            required_tiles,
            available_tiles,
        }) => {
            assert_eq!(required_tiles, 11);
            assert_eq!(available_tiles, 1);
        }
        other => panic!("expected ShadowAtlasOverflow, got {other:?}"),
    }
}

#[test]
fn wgsl_contract_contains_pcf_sampling() {
    assert!(SHADOW_WGSL.contains("litd_shadow_pcf"));
    assert!(SHADOW_WGSL.contains("textureSampleCompare"));
}

fn test_camera() -> ShadowCamera {
    ShadowCamera {
        position: Vec3::new(0.0, 8.0, -12.0),
        forward: Vec3::new(0.0, -0.35, 1.0),
        up: Vec3::Y,
        fov_y_rad: 60.0_f32.to_radians(),
        aspect: 16.0 / 9.0,
        near: 0.5,
        far: 80.0,
    }
}

fn test_lights() -> Vec<RenderLight> {
    vec![
        RenderLight::directional("sun", Vec3::new(0.2, 0.6, 1.0), Vec3::ONE, 3.0),
        RenderLight::point("torch", Vec3::new(2.0, 2.0, 3.0), 10.0, Vec3::ONE, 6.0),
        RenderLight::spot(
            "gate",
            Vec3::new(-3.0, 4.0, 1.0),
            Vec3::new(0.3, -1.0, 0.6),
            18.0,
            SpotCone::new(0.35, 0.8),
            Vec3::ONE,
            5.0,
        ),
    ]
}
