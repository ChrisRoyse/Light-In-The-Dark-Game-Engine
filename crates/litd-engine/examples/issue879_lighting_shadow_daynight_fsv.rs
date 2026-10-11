use std::{fs, path::Path};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    fsv_artifacts::begin_issue_run,
    gpu::GpuContext,
    math::Vec3,
    render::{
        lighting::{
            ClusterGrid, DayNightLightingInput, DayNightLightingPlan, ForwardPlusGpuSettings,
            RenderLight, SpotCone, WeatherLightingState, build_day_night_lighting_plan,
            cluster_lights, render_forward_plus_lighting_gpu_with_context,
        },
        shadow::{
            ShadowCamera, ShadowPlan, ShadowSettings, build_shadow_plan,
            render_shadow_maps_gpu_with_context,
        },
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let run = begin_issue_run("issue879_lighting_shadow_daynight")?;
    let report_path = run.report_path();
    let day_forward_bmp = run.artifact_path("day-forward-plus.bmp")?;
    let night_forward_bmp = run.artifact_path("night-weather-forward-plus.bmp")?;
    let shadow_bmp = run.artifact_path("day-shadow-atlas.bmp")?;
    let context = GpuContext::new_headless().await?;
    let grid = cluster_grid();
    let camera = shadow_camera();
    let settings = shadow_settings();

    let day_plan =
        build_day_night_lighting_plan(DayNightLightingInput::default(), &local_lights())?;
    let day_assignment = cluster_lights(grid, &day_plan.lights)?;
    let day_shadow = build_shadow_plan(settings, camera, &day_plan.lights)?;
    let day_forward = render_forward_plus_lighting_gpu_with_context(
        &context,
        forward_settings(),
        grid,
        &day_plan.lights,
    )
    .await?;
    day_forward.image.write_bmp(&day_forward_bmp)?;
    let day_shadow_gpu =
        render_shadow_maps_gpu_with_context(&context, settings, camera, &day_plan.lights).await?;
    day_shadow_gpu.image.write_bmp(&shadow_bmp)?;

    let night_input = DayNightLightingInput {
        time_of_day_hours: 22.0,
        weather: WeatherLightingState {
            cloud_cover: 0.82,
            precipitation: 0.55,
            fog_density: 0.64,
        },
        ..DayNightLightingInput::default()
    };
    let night_plan = build_day_night_lighting_plan(night_input, &local_lights())?;
    let night_assignment = cluster_lights(grid, &night_plan.lights)?;
    let night_shadow = build_shadow_plan(settings, camera, &night_plan.lights)?;
    let night_forward = render_forward_plus_lighting_gpu_with_context(
        &context,
        forward_settings(),
        grid,
        &night_plan.lights,
    )
    .await?;
    night_forward.image.write_bmp(&night_forward_bmp)?;

    let report = json!({
        "schema_version": 1,
        "issue": 879,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "day_forward_bmp": day_forward_bmp.display().to_string(),
            "night_forward_bmp": night_forward_bmp.display().to_string(),
            "shadow_bmp": shadow_bmp.display().to_string(),
            "manual_fsv_readback": "read persisted JSON, BMP hashes, GPU adapter, and Synapse screenshot/DOM for this exact report"
        },
        "sufficiency_argument": "The report drives the public day/night weather planner into the real clustered lighting assignment, real forward+ GPU compute/shading path, real shadow planner, and real shadow GPU atlas pass. Day verifies a directional sun plus point and spot locals; stormy night verifies moon primary, lower primary intensity, weather attenuation, and thicker fog. Edges prove invalid time, invalid weather, local directional misuse, and shadow atlas overflow fail loud.",
        "gpu_context": context_state(&context),
        "day": {
            "lighting_plan": lighting_state(&day_plan),
            "cluster_assignment": assignment_state(&day_assignment),
            "shadow_plan": shadow_state(&day_shadow)?,
            "forward_plus_gpu": {
                "stats": day_forward.stats,
                "bmp_readback": image_state(&day_forward_bmp),
            },
            "shadow_gpu": {
                "stats": day_shadow_gpu.stats,
                "bmp_readback": image_state(&shadow_bmp),
            }
        },
        "stormy_night": {
            "lighting_plan": lighting_state(&night_plan),
            "cluster_assignment": assignment_state(&night_assignment),
            "shadow_plan": shadow_state(&night_shadow)?,
            "forward_plus_gpu": {
                "stats": night_forward.stats,
                "bmp_readback": image_state(&night_forward_bmp),
            }
        },
        "edges": [
            edge_invalid_time(),
            edge_invalid_weather(),
            edge_local_directional(),
            edge_shadow_atlas_overflow(settings, camera),
        ],
        "expected_actuals": {
            "day_primary": "sun",
            "night_primary": "moon",
            "local_light_count": 2,
            "directional_indices": [0],
            "cluster_refs_summary": "0:[1]|1:[2]",
            "shadow_entries": 9,
            "directional_cascades": 2,
            "point_faces": 6,
            "spot_maps": 1,
            "edge_count": 4
        }
    });
    let readback = run.write_report(&report)?;
    assert_report(&readback.json)?;
    println!(
        "issue879 lighting_shadow_daynight_fsv report={} bytes={} sha256={} adapter={} day_primary={} night_primary={} day_refs={} shadow_entries={} day_bmp_sha256={} shadow_bmp_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["gpu_context"]["adapter_name"],
        readback.json["day"]["lighting_plan"]["primary_light_name"],
        readback.json["stormy_night"]["lighting_plan"]["primary_light_name"],
        readback.json["day"]["cluster_assignment"]["refs_summary"],
        readback.json["day"]["shadow_plan"]["entry_count"],
        readback.json["day"]["forward_plus_gpu"]["bmp_readback"]["sha256"],
        readback.json["day"]["shadow_gpu"]["bmp_readback"]["sha256"],
    );
    Ok(())
}

fn local_lights() -> Vec<RenderLight> {
    vec![
        RenderLight::point(
            "left_torch",
            Vec3::new(-1.0, 0.0, 1.0),
            0.25,
            Vec3::new(1.0, 0.38, 0.08),
            6.0,
        ),
        RenderLight::spot(
            "right_gate_spot",
            Vec3::new(1.0, 0.0, 1.0),
            -Vec3::Z,
            0.25,
            SpotCone::new(0.2, 0.5),
            Vec3::new(0.25, 0.75, 1.0),
            5.0,
        ),
    ]
}

fn cluster_grid() -> ClusterGrid {
    ClusterGrid::new(
        2,
        1,
        1,
        Vec3::new(-2.0, -1.0, 0.0),
        Vec3::new(2.0, 1.0, 2.0),
        4,
    )
}

fn forward_settings() -> ForwardPlusGpuSettings {
    ForwardPlusGpuSettings {
        width: 640,
        height: 360,
        shading_z_slice: 0,
    }
}

fn shadow_settings() -> ShadowSettings {
    ShadowSettings {
        atlas_size: 512,
        tile_size: 128,
        cascade_count: 2,
        max_shadowed_point_lights: 1,
        max_shadowed_spot_lights: 1,
        pcf_radius: 2,
        depth_bias: 0.0015,
        normal_bias: 0.04,
        ..ShadowSettings::default()
    }
}

fn shadow_camera() -> ShadowCamera {
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

fn lighting_state(plan: &DayNightLightingPlan) -> Value {
    json!({
        "time_of_day_hours": plan.input.time_of_day_hours,
        "phase": plan.phase.label(),
        "weather": plan.input.weather,
        "weather_summary": plan.input.weather.summary(),
        "weather_attenuation": plan.weather_attenuation,
        "solar_elevation": plan.solar_elevation,
        "day_factor": plan.day_factor,
        "twilight_factor": plan.twilight_factor,
        "primary_light_name": plan.primary_light_name,
        "primary_direction_to_light": plan.primary_direction_to_light,
        "primary_direction_label": plan.primary_direction_label(),
        "primary_intensity": plan.primary_intensity,
        "primary_color": plan.primary_color,
        "ambient_color": plan.ambient_color,
        "ambient_intensity": plan.ambient_intensity,
        "fog_color": plan.fog_color,
        "fog_max": plan.fog_max,
        "light_names": plan.lights.iter().map(|light| light.name.clone()).collect::<Vec<_>>(),
        "light_count": plan.lights.len(),
    })
}

fn assignment_state(assignment: &litd_engine::render::lighting::ClusteredLightAssignment) -> Value {
    json!({
        "cluster_count": assignment.cluster_count(),
        "directional_count": assignment.directional_count(),
        "directional_light_indices": assignment.directional_light_indices,
        "total_light_refs": assignment.total_light_refs(),
        "non_empty_clusters": assignment.non_empty_cluster_count(),
        "max_cluster_occupancy": assignment.max_cluster_occupancy(),
        "refs_summary": assignment.refs_summary(),
    })
}

fn shadow_state(plan: &ShadowPlan) -> Result<Value, EngineError> {
    let first = plan.entries.first().expect("fixture has shadow entries");
    let last = plan.entries.last().expect("fixture has shadow entries");
    Ok(json!({
        "entry_count": plan.entries.len(),
        "directional_cascades": plan.directional_cascade_count(),
        "point_faces": plan.point_face_count(),
        "spot_maps": plan.spot_map_count(),
        "atlas_tile_capacity": plan.settings.tile_capacity()?,
        "first_light": first.light_name,
        "last_light": last.light_name,
        "first_tile": first.tile,
        "last_tile": last.tile,
        "pcf_radius": plan.settings.pcf_radius,
    }))
}

fn edge_invalid_time() -> Value {
    let input = DayNightLightingInput {
        time_of_day_hours: 24.0,
        ..DayNightLightingInput::default()
    };
    edge_result(
        "invalid_time_of_day",
        json!({"time_of_day_hours": 24.0}),
        build_day_night_lighting_plan(input, &[]),
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    )
}

fn edge_invalid_weather() -> Value {
    let input = DayNightLightingInput {
        weather: WeatherLightingState {
            cloud_cover: -0.1,
            precipitation: 0.0,
            fog_density: 0.0,
        },
        ..DayNightLightingInput::default()
    };
    edge_result(
        "invalid_weather_strength",
        json!({"cloud_cover": -0.1}),
        build_day_night_lighting_plan(input, &[]),
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    )
}

fn edge_local_directional() -> Value {
    let locals = vec![RenderLight::directional(
        "rogue_sun",
        Vec3::Y,
        Vec3::ONE,
        1.0,
    )];
    edge_result(
        "local_directional_rejected",
        json!({"local_light": "rogue_sun"}),
        build_day_night_lighting_plan(DayNightLightingInput::default(), &locals),
        "LITD-RENDER-CLUSTERED-LIGHTING-INVALID-CONFIG",
    )
}

fn edge_shadow_atlas_overflow(settings: ShadowSettings, camera: ShadowCamera) -> Value {
    let plan = build_day_night_lighting_plan(DayNightLightingInput::default(), &local_lights())
        .expect("fixture plan");
    let tiny = ShadowSettings {
        atlas_size: 128,
        tile_size: 128,
        ..settings
    };
    edge_result(
        "shadow_atlas_overflow",
        json!({"atlas_size": tiny.atlas_size, "tile_size": tiny.tile_size}),
        build_shadow_plan(tiny, camera, &plan.lights),
        "LITD-RENDER-SHADOW-ATLAS-OVERFLOW",
    )
}

fn edge_result<T>(
    name: &'static str,
    before: Value,
    result: Result<T, EngineError>,
    expected_code: &'static str,
) -> Value {
    match result {
        Ok(_) => json!({
            "name": name,
            "before": before,
            "after": {"status": "accepted"},
            "expected_code": expected_code,
            "matched_expected_code": false,
        }),
        Err(error) => {
            let report: ErrorReport = error.report();
            json!({
                "name": name,
                "before": before,
                "after": {
                    "status": "rejected",
                    "error": {
                        "code": report.code,
                        "message": report.message,
                        "remediation": report.remediation,
                        "source_error": report.source_error,
                    }
                },
                "expected_code": expected_code,
                "matched_expected_code": report.code == expected_code,
            })
        }
    }
}

fn context_state(context: &GpuContext) -> Value {
    let report = context.report();
    json!({
        "context_id": report.context_id,
        "mode": format!("{:?}", report.mode),
        "adapter_name": report.adapter.name,
        "backend": report.adapter.backend,
        "device_type": report.adapter.device_type,
        "driver": report.adapter.driver,
        "driver_info": report.adapter.driver_info,
        "device_lost_report": context.device_lost_report(),
    })
}

fn image_state(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap();
    json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
        "bmp_signature": String::from_utf8_lossy(&bytes[0..2]).to_string(),
        "bmp_width": u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
        "bmp_height": u32::from_le_bytes(bytes[22..26].try_into().unwrap()),
    })
}

fn assert_report(report: &Value) -> Result<(), EngineError> {
    let day_intensity = report["day"]["lighting_plan"]["primary_intensity"]
        .as_f64()
        .unwrap_or_default();
    let night_intensity = report["stormy_night"]["lighting_plan"]["primary_intensity"]
        .as_f64()
        .unwrap_or_default();
    let day_fog = report["day"]["lighting_plan"]["fog_max"]
        .as_f64()
        .unwrap_or_default();
    let night_fog = report["stormy_night"]["lighting_plan"]["fog_max"]
        .as_f64()
        .unwrap_or_default();
    let ok = report["issue"] == 879
        && report["day"]["lighting_plan"]["primary_light_name"] == "sun"
        && report["stormy_night"]["lighting_plan"]["primary_light_name"] == "moon"
        && day_intensity > night_intensity
        && night_fog > day_fog
        && report["day"]["cluster_assignment"]["directional_light_indices"] == json!([0])
        && report["day"]["cluster_assignment"]["refs_summary"] == "0:[1]|1:[2]"
        && report["day"]["shadow_plan"]["entry_count"] == 9
        && report["day"]["shadow_gpu"]["stats"]["entry_count"] == 9
        && report["day"]["forward_plus_gpu"]["stats"]["gpu_refs_summary"] == "0:[1]|1:[2]"
        && report["day"]["forward_plus_gpu"]["bmp_readback"]["bmp_signature"] == "BM"
        && report["stormy_night"]["forward_plus_gpu"]["bmp_readback"]["bmp_signature"] == "BM"
        && report["day"]["shadow_gpu"]["bmp_readback"]["bmp_signature"] == "BM"
        && report["edges"].as_array().is_some_and(|edges| {
            edges.len() == 4
                && edges
                    .iter()
                    .all(|edge| edge["matched_expected_code"] == true)
        });
    if ok {
        Ok(())
    } else {
        Err(EngineError::SerializedFingerprintMismatch {
            expected: "issue879 report with day sun, stormy night moon, forward+ GPU, shadow GPU, and four matched edges".to_string(),
            actual: report.to_string(),
        })
    }
}
