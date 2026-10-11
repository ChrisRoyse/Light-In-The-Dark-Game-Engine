use super::*;

#[test]
fn clustered_assignment_separates_directional_from_local_lights() {
    let grid = ClusterGrid::new(
        2,
        1,
        1,
        Vec3::new(-2.0, -1.0, 0.0),
        Vec3::new(2.0, 1.0, 2.0),
        4,
    );
    let lights = vec![
        RenderLight::directional("sun", Vec3::Z, Vec3::ONE, 2.0),
        RenderLight::point("left", Vec3::new(-1.0, 0.0, 1.0), 0.25, Vec3::ONE, 5.0),
        RenderLight::spot(
            "right",
            Vec3::new(1.0, 0.0, 1.0),
            -Vec3::Z,
            0.25,
            SpotCone::new(0.2, 0.5),
            Vec3::ONE,
            5.0,
        ),
    ];

    let assignment = cluster_lights(grid, &lights).unwrap();
    assert_eq!(assignment.directional_light_indices, vec![0]);
    assert_eq!(assignment.cluster_light_indices[0], vec![1]);
    assert_eq!(assignment.cluster_light_indices[1], vec![2]);
    assert_eq!(assignment.total_light_refs(), 2);
}

#[test]
fn clustered_assignment_fails_loud_on_overflow() {
    let grid = ClusterGrid::new(1, 1, 1, Vec3::ZERO, Vec3::splat(2.0), 1);
    let lights = vec![
        RenderLight::point("a", Vec3::ONE, 2.0, Vec3::ONE, 1.0),
        RenderLight::point("b", Vec3::ONE, 2.0, Vec3::ONE, 1.0),
    ];

    match cluster_lights(grid, &lights) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::ClusteredLightingClusterOverflow {
                cluster_index,
                max_lights,
            },
        )) => {
            assert_eq!(cluster_index, 0);
            assert_eq!(max_lights, 1);
        }
        other => panic!("expected cluster overflow, got {other:?}"),
    }
}

#[test]
fn invalid_cluster_grid_is_rejected() {
    let grid = ClusterGrid::new(0, 1, 1, Vec3::ZERO, Vec3::ONE, 4);
    match grid.validate() {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::ClusteredLightingInvalidConfig { field, .. },
        )) => {
            assert_eq!(field, "slices");
        }
        other => panic!("expected invalid config, got {other:?}"),
    }
}

#[test]
fn wgsl_contract_contains_compute_kernel_and_culling() {
    assert!(FORWARD_PLUS_WGSL.contains("@compute"));
    assert!(FORWARD_PLUS_WGSL.contains("litd_sphere_intersects_cluster"));
}

#[test]
fn day_night_lighting_outputs_primary_sun_and_local_point_spot() {
    let locals = vec![
        RenderLight::point("torch", Vec3::new(-1.0, 0.0, 1.0), 0.5, Vec3::ONE, 3.0),
        RenderLight::spot(
            "gate",
            Vec3::new(1.0, 0.0, 1.0),
            -Vec3::Z,
            0.5,
            SpotCone::new(0.2, 0.6),
            Vec3::ONE,
            4.0,
        ),
    ];
    let plan = build_day_night_lighting_plan(DayNightLightingInput::default(), &locals).unwrap();

    assert_eq!(plan.phase, TimeOfDayPhase::Day);
    assert_eq!(plan.primary_light_name, "sun");
    assert_eq!(plan.lights.len(), 3);
    assert!(matches!(plan.lights[0].kind, LightKind::Directional { .. }));
    assert!(matches!(plan.lights[1].kind, LightKind::Point { .. }));
    assert!(matches!(plan.lights[2].kind, LightKind::Spot { .. }));
    assert!(plan.primary_intensity > 0.5);
    assert_eq!(plan.ambient_light.name, "ambient_day");
    assert_eq!(plan.lighting_rig().unwrap().ambient, plan.ambient_light);
    assert!(plan.fog_max >= 0.58);
}

#[test]
fn stormy_night_uses_moon_and_thickens_fog() {
    let input = DayNightLightingInput {
        time_of_day_hours: 22.0,
        weather: WeatherLightingState {
            cloud_cover: 0.8,
            precipitation: 0.5,
            fog_density: 0.7,
        },
        ..DayNightLightingInput::default()
    };
    let plan = build_day_night_lighting_plan(input, &[]).unwrap();

    assert_eq!(plan.phase, TimeOfDayPhase::Night);
    assert_eq!(plan.primary_light_name, "moon");
    assert!(plan.primary_intensity < 0.3);
    assert!(plan.weather_attenuation < 0.7);
    assert!(plan.fog_max > DayNightLightingInput::default().fog_max_base);
}

#[test]
fn day_night_lighting_fails_loud_on_bad_inputs() {
    let bad_time = DayNightLightingInput {
        time_of_day_hours: 24.0,
        ..DayNightLightingInput::default()
    };
    match build_day_night_lighting_plan(bad_time, &[]) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::ClusteredLightingInvalidConfig { field, .. },
        )) => {
            assert_eq!(field, "day_night.time_of_day_hours");
        }
        other => panic!("expected invalid time, got {other:?}"),
    }

    let bad_weather = DayNightLightingInput {
        weather: WeatherLightingState {
            cloud_cover: 1.2,
            precipitation: 0.0,
            fog_density: 0.0,
        },
        ..DayNightLightingInput::default()
    };
    match build_day_night_lighting_plan(bad_weather, &[]) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::ClusteredLightingInvalidConfig { field, .. },
        )) => {
            assert_eq!(field, "weather.cloud_cover");
        }
        other => panic!("expected invalid weather, got {other:?}"),
    }

    let local_directional = vec![RenderLight::directional("bad", Vec3::Y, Vec3::ONE, 1.0)];
    match build_day_night_lighting_plan(DayNightLightingInput::default(), &local_directional) {
        Err(EngineError::Rendering(
            litd_engine::error::RenderingError::ClusteredLightingInvalidConfig { field, .. },
        )) => {
            assert_eq!(field, "local_lights[0].kind");
        }
        other => panic!("expected local directional rejection, got {other:?}"),
    }
}

#[test]
fn ambient_light_fails_loud_on_invalid_fields() {
    for (ambient, field) in [
        (AmbientLight::new("", Vec3::ONE, 1.0), "ambient.name"),
        (
            AmbientLight::new("bad_color", Vec3::new(-0.1, 0.0, 0.0), 1.0),
            "ambient.color",
        ),
        (
            AmbientLight::new("bad_intensity", Vec3::ONE, -0.1),
            "ambient.intensity",
        ),
    ] {
        match LightingRig::new(ambient, Vec::new()) {
            Err(EngineError::Rendering(
                litd_engine::error::RenderingError::ClusteredLightingInvalidConfig {
                    field: actual,
                    ..
                },
            )) => {
                assert_eq!(actual, field);
            }
            other => panic!("expected invalid ambient {field}, got {other:?}"),
        }
    }
}
