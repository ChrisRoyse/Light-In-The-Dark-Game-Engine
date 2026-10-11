//! Unit coverage for the world-environment authoring surfaces (#808/#814/#815/#816).

use super::*;
use crate::world_environment::{
    lighting::{
        BakedGiStateRef, DayNightCyclePolicy, LightingEnvironmentAuthoring, TimeOfDayKeyframe,
    },
    minimap::{LoadingScreen, MinimapAuthoring, MinimapSource, PreviewImageRef, generate_minimap},
    record::EnvironmentPayload,
    uniform::{EnvironmentUniform, cpu_fog_factor, cpu_water_tint},
    water::{BoundaryKind, MapBoundary, SkyboxPreset, WaterBody, WaterEnvironmentAuthoring},
    weather::{GlobalFog, PrecipitationKind, WeatherEnvironmentAuthoring},
};

fn hex64(seed: &str) -> String {
    litd_hash::sha256_hex(seed.as_bytes())
}

fn water_fixture() -> WaterEnvironmentAuthoring {
    WaterEnvironmentAuthoring {
        id: "map-shore".to_string(),
        display_name: "Shore".to_string(),
        author: "tester".to_string(),
        water: WaterBody {
            shallow_depth_mm: 800,
            deep_depth_mm: 4000,
            shore_blend_mm: 500,
            shallow_color: Rgba8::new(80, 140, 170, 90),
            deep_color: Rgba8::new(10, 30, 70, 220),
            waves_cliff_shores: true,
            waves_rolling_shores: false,
        },
        boundary: MapBoundary {
            width_tiles: 128,
            height_tiles: 96,
            playable_inset_tiles: 6,
            kind: BoundaryKind::WaterExtent,
        },
        skybox: SkyboxPreset::ClearDay,
    }
}

#[test]
fn water_payload_is_lowerable_and_ordered() {
    let payload = water_fixture().to_payload().expect("valid water payload");
    assert_eq!(payload.surface_kind, "water_boundary_environment");
    // schema + surface_kind + id are stamped.
    assert_eq!(payload.body["schema"], WORLD_ENVIRONMENT_SCHEMA);
    assert_eq!(payload.body["surface_kind"], "water_boundary_environment");
    assert_eq!(payload.body["water_deep_depth_mm"], 4000);
}

#[test]
fn water_rejects_deep_shallower_than_shallow() {
    let mut water = water_fixture();
    water.water.deep_depth_mm = 800;
    let err = water.validate().expect_err("deep must exceed shallow");
    assert_eq!(err.code(), "LITD_ENV_WATER");
}

#[test]
fn water_rejects_deep_more_transparent_than_shallow() {
    let mut water = water_fixture();
    water.water.deep_color = Rgba8::new(10, 30, 70, 50);
    assert!(water.validate().is_err());
}

#[test]
fn boundary_rejects_inset_consuming_map() {
    let mut water = water_fixture();
    water.boundary.playable_inset_tiles = 48; // 2*48 == 96 == height
    assert!(water.validate().is_err());
}

#[test]
fn float_field_is_rejected_before_lowering() {
    let err = EnvironmentPayload::new("x", "id", serde_json::json!({ "density": 0.5 }))
        .expect_err("floats are forbidden");
    assert_eq!(err.code(), "LITD_ENV_PAYLOAD");
}

#[test]
fn lighting_requires_increasing_keyframes() {
    let mut lighting = lighting_fixture();
    lighting.keyframes[1].time_permille = lighting.keyframes[0].time_permille;
    assert!(lighting.validate().is_err());
}

fn lighting_fixture() -> LightingEnvironmentAuthoring {
    LightingEnvironmentAuthoring {
        id: "light-cycle".to_string(),
        display_name: "Cycle".to_string(),
        author: "tester".to_string(),
        cycle: DayNightCyclePolicy::Enabled,
        cycle_seconds: 600,
        keyframes: vec![
            TimeOfDayKeyframe {
                time_permille: 250,
                sun_color: Rgb8::new(255, 240, 210),
                sun_intensity_milli: 900,
                ambient_color: Rgb8::new(120, 130, 150),
                sun_azimuth_deg: 90,
                sun_elevation_deg: 30,
            },
            TimeOfDayKeyframe {
                time_permille: 750,
                sun_color: Rgb8::new(40, 40, 90),
                sun_intensity_milli: 100,
                ambient_color: Rgb8::new(20, 22, 40),
                sun_azimuth_deg: 270,
                sun_elevation_deg: -20,
            },
        ],
        baked_gi_states: vec![BakedGiStateRef {
            state_label: "day".to_string(),
            time_permille: 250,
            probe_volume_fingerprint: hex64("day"),
        }],
    }
}

#[test]
fn lighting_payload_records_baked_states() {
    let payload = lighting_fixture().to_payload().expect("valid lighting");
    assert_eq!(payload.body["keyframe_count"], 2);
    assert_eq!(payload.body["baked_gi_states"][0]["state_label"], "day");
}

#[test]
fn lighting_rejects_short_probe_fingerprint() {
    let mut lighting = lighting_fixture();
    lighting.baked_gi_states[0].probe_volume_fingerprint = "abcd".to_string();
    assert!(lighting.validate().is_err());
}

fn weather_fixture() -> WeatherEnvironmentAuthoring {
    WeatherEnvironmentAuthoring {
        id: "storm".to_string(),
        display_name: "Storm".to_string(),
        author: "tester".to_string(),
        precipitation: PrecipitationKind::Rain,
        intensity_permille: 700,
        wind_bearing_deg: 45,
        fog: GlobalFog {
            enabled: true,
            color: Rgb8::new(150, 155, 165),
            density_milli: 120,
            visibility_distance_mm: 80_000,
            start_distance_mm: 5_000,
        },
    }
}

#[test]
fn weather_rejects_clear_with_intensity() {
    let mut weather = weather_fixture();
    weather.precipitation = PrecipitationKind::Clear;
    assert!(weather.validate().is_err());
}

#[test]
fn weather_rejects_start_past_visibility() {
    let mut weather = weather_fixture();
    weather.fog.start_distance_mm = 90_000;
    assert!(weather.validate().is_err());
}

#[test]
fn minimap_generation_is_deterministic_and_box_averages() {
    // 2x2 source: top row red/green, bottom row blue/white -> 1x1 minimap averages.
    let source = vec![
        Rgba8::new(255, 0, 0, 255),
        Rgba8::new(0, 255, 0, 255),
        Rgba8::new(0, 0, 255, 255),
        Rgba8::new(255, 255, 255, 255),
    ];
    let a = generate_minimap(&source, 2, 2, 1).expect("gen");
    let b = generate_minimap(&source, 2, 2, 1).expect("gen");
    assert_eq!(a.fingerprint, b.fingerprint, "deterministic");
    // Average of (255,0,0),(0,255,0),(0,0,255),(255,255,255) = (127,127,127,255).
    let packed = a.pixels[0];
    let r = (packed >> 24) & 0xff;
    assert_eq!(r, 127);
}

#[test]
fn minimap_rejects_bad_source_length() {
    let source = vec![Rgba8::new(0, 0, 0, 255)];
    assert!(generate_minimap(&source, 2, 2, 1).is_err());
}

fn minimap_fixture() -> MinimapAuthoring {
    MinimapAuthoring {
        id: "map-preview".to_string(),
        display_name: "Preview".to_string(),
        author: "tester".to_string(),
        source: MinimapSource::TerrainAlbedo,
        resolution_px: 128,
        rts_shipped_as_data: true,
        loading_screen: LoadingScreen {
            title: "Shore Assault".to_string(),
            subtitle: "2v2 melee".to_string(),
            preview_image: PreviewImageRef {
                blob_sha256: hex64("preview"),
                width_px: 1024,
                height_px: 576,
            },
        },
    }
}

#[test]
fn minimap_payload_marks_no_rpg_hud() {
    let payload = minimap_fixture().to_payload().expect("valid minimap");
    assert_eq!(payload.body["rpg_runtime_hud"], false);
    assert_eq!(payload.body["rts_shipped_as_data"], true);
}

#[test]
fn fog_factor_matches_expected_curve() {
    let uniform = EnvironmentUniform {
        fog_color: [0.5, 0.5, 0.5, 0.05],
        fog_range: [10.0, 100.0, 0.0, 0.0],
        ..EnvironmentUniform::default()
    };
    // At/before start: no fog.
    assert_eq!(cpu_fog_factor(&uniform, 10.0), 0.0);
    // At the visibility horizon: fully fogged.
    assert_eq!(cpu_fog_factor(&uniform, 100.0), 1.0);
    // Monotonic increase in between.
    assert!(cpu_fog_factor(&uniform, 40.0) < cpu_fog_factor(&uniform, 60.0));
}

#[test]
fn water_tint_blends_shallow_to_deep() {
    let uniform = EnvironmentUniform {
        water_shallow: [0.3, 0.5, 0.6, 0.35],
        water_deep: [0.04, 0.12, 0.27, 0.86],
        water_depths: [0.8, 4.0, 0.5, 0.0],
        ..EnvironmentUniform::default()
    };
    // Shallow end.
    assert_eq!(cpu_water_tint(&uniform, 0.8)[3], 0.35);
    // Deep end.
    assert!((cpu_water_tint(&uniform, 4.0)[3] - 0.86).abs() < 1.0e-6);
    // Midpoint alpha strictly between.
    let mid = cpu_water_tint(&uniform, 2.4)[3];
    assert!(mid > 0.35 && mid < 0.86);
}
