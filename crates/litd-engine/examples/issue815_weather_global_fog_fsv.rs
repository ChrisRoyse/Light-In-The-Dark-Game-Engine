//! FSV for #815: weather effects + global fog / visibility distance.
//!
//! Author a rain weather layer with global exponential fog + an explicit visibility
//! horizon, persist through the real content path (pending_measurement, no ORT),
//! lower to a frozen package, read the row back with a fresh vault handle. The fog
//! renderer consumption is proven by building the GPU fog uniform FROM the lowered
//! fields, naga-validating `world_environment.wgsl`, and EXECUTING its real fog helper
//! on the GPU (wgpu compute readback) to confirm the shader output equals the CPU
//! oracle (the visibility cutoff produces full fog at the horizon).
//! Three edges prove the fog/precipitation invariants fail loud.

use std::error::Error;

use litd_engine::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING, EngineCalyxContentStore, EngineContentKind,
        EngineContentMeasurementConfig,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    world_environment::{
        GlobalFog, PrecipitationKind, Rgb8, WeatherEnvironmentAuthoring, WorldEnvironmentError,
        record::persist_and_lower_environment,
        uniform::{EnvironmentUniform, apply_weather_fog, cpu_fog_factor},
    },
};
use serde_json::json;

#[path = "world_environment_fsv_common/fixture.rs"]
mod fixture;
#[path = "world_environment_fsv_common/wgsl_verify.rs"]
mod wgsl_verify;

fn weather_fixture(id: &str) -> WeatherEnvironmentAuthoring {
    WeatherEnvironmentAuthoring {
        id: id.to_string(),
        display_name: "Driving Rain".to_string(),
        author: "issue815-fsv".to_string(),
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

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue815_weather_global_fog")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue815",
            fixture::unavailable_panel_state(),
        ),
    );

    let authoring = weather_fixture("weather-driving-rain-issue815");
    let payload = authoring.to_payload()?;
    let readback = persist_and_lower_environment(
        &store,
        &authoring.author,
        "editor://worlds/moor/weather",
        &authoring.display_name,
        &payload,
    )?;

    // Renderer consumption: build the fog uniform from the LOWERED fields.
    let mut uniform = EnvironmentUniform::default();
    apply_weather_fog(&mut uniform, &readback.lowered)?;
    // Visibility distance (m) == 80.0; before start there is no fog; at the
    // horizon fog is fully saturated; the interior is monotone increasing.
    let start = uniform.fog_range[0];
    let horizon = uniform.fog_range[1];
    let near_clear = cpu_fog_factor(&uniform, start) == 0.0;
    let horizon_full = cpu_fog_factor(&uniform, horizon) == 1.0;
    let monotone = cpu_fog_factor(&uniform, start + 10.0) < cpu_fog_factor(&uniform, start + 30.0);
    let render = json!({
        "fog_start_m": start,
        "fog_visibility_m": horizon,
        "near_clear": near_clear,
        "horizon_full": horizon_full,
        "interior_monotone": monotone,
    });
    let wgsl = wgsl_verify::verify_world_environment_wgsl()?;

    let fresh = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let fresh_store = EngineCalyxContentStore::with_measurement_config(
        fresh,
        EngineContentMeasurementConfig::from_preloaded_state(
            "separate-readback:issue815",
            fixture::unavailable_panel_state(),
        ),
    );
    let on_disk = fresh_store.get_content_for_editor(EngineContentKind::Terrain, &authoring.id)?;

    let edges = json!({
        "clear_with_intensity_rejected": reject_clear_intensity().is_err(),
        "fog_start_past_visibility_rejected": reject_start_past_visibility().is_err(),
        "enabled_fog_zero_visibility_rejected": reject_zero_visibility().is_err(),
    });

    let acceptance = json!({
        "persisted_pending_without_ort":
            readback.row.measurement.measurement_state == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        "declared_gate_binding": readback.row.measurement.declared_learned_lenses >= 10,
        "lowered_field_count_positive": !readback.lowered.fields.is_empty(),
        "on_disk_sha_matches_writer": on_disk.database.sha256 == readback.row.database.sha256,
        "on_disk_cx_matches_writer": on_disk.cx_id == readback.row.cx_id,
        "fog_near_clear": near_clear,
        "fog_horizon_full": horizon_full,
        "fog_interior_monotone": monotone,
        "wgsl_naga_valid": wgsl["naga_validate_ok"] == true,
        "wgsl_gpu_execution_verified": wgsl["gpu_execution"]["verified"] == true,
        "wgsl_fog_matches_gpu": wgsl["gpu_execution"]["fog"]["within_tolerance"] == true,
        "edge_clear_intensity_rejected": edges["clear_with_intensity_rejected"],
        "edge_start_past_visibility_rejected": edges["fog_start_past_visibility_rejected"],
        "edge_zero_visibility_rejected": edges["enabled_fog_zero_visibility_rejected"],
    });

    let report = json!({
        "schema": "litd.engine.issue815.weather_global_fog_fsv.v1",
        "issue": 815,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "environment": { "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some() },
            "manual_gate": "Read this report + the persisted Aster row; command success alone is not proof."
        },
        "sufficiency_argument": "One rain+fog weather surface is the smallest input that exercises the whole slice: explicit persist under an addon-absent panel (pending_measurement), a durable readback, a frozen lowered package whose integer fog fields build the GPU uniform, and a naga-validated WGSL whose exponential-fog-with-visibility-cutoff math mirrors the CPU exactly (clear before start, full at the horizon, monotone between). Three edges prove the clear-intensity, start<visibility, and positive-visibility invariants fail loud.",
        "persist": readback.evidence(),
        "render": render,
        "wgsl": wgsl,
        "edges": edges,
        "acceptance": acceptance
    });

    let written = run.write_report(&report)?;
    for (field, value) in written.json["acceptance"].as_object().expect("acceptance") {
        if value != true {
            return Err(format!("issue815 acceptance {field} was not true").into());
        }
    }
    println!(
        "ISSUE815_WEATHER_FSV report={} sha256={} state={} horizon_full={}",
        written.path.display(),
        written.sha256,
        readback.row.measurement.measurement_state,
        horizon_full,
    );
    Ok(())
}

fn reject_clear_intensity() -> Result<(), WorldEnvironmentError> {
    let mut a = weather_fixture("edge-clear");
    a.precipitation = PrecipitationKind::Clear;
    a.validate()
}

fn reject_start_past_visibility() -> Result<(), WorldEnvironmentError> {
    let mut a = weather_fixture("edge-start");
    a.fog.start_distance_mm = 90_000; // > visibility (80_000)
    a.validate()
}

fn reject_zero_visibility() -> Result<(), WorldEnvironmentError> {
    let mut a = weather_fixture("edge-vis");
    a.fog.visibility_distance_mm = 0;
    a.validate()
}
