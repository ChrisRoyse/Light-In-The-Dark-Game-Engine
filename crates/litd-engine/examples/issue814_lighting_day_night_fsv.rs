//! FSV for #814: lighting + day/night cycle, integrating the merged #1742
//! probe-GI baked day/night states.
//!
//! Bakes two probe-GI volumes (bright "day" sky vs dark "night" sky) with the real
//! `bake_probe_volume`/`lower_probe_volume` pipeline, proving the day and night
//! baked states are DISTINCT (different fingerprints). Those fingerprints anchor
//! the authored day/night keyframes. The lighting surface then persists through
//! the real content path (pending_measurement, no ORT) and lowers to a frozen
//! package; a fresh vault handle reads the row back. `world_environment.wgsl` is
//! naga-validated AND its fog/water math is executed on a real GPU and cross-checked
//! against the CPU oracle (the shared wgsl_verify). Three edges prove the keyframe
//! and baked-state invariants fail loud.

use std::error::Error;

use litd_engine::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING, EngineCalyxContentStore, EngineContentKind,
        EngineContentMeasurementConfig,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    math::Vec3,
    render::probe_gi::{
        BakeConfig, BakeScene, EmissivePanel, PanelAxis, ProbeGridSpec, SkyLighting,
        bake_probe_volume, lower_probe_volume,
    },
    world_environment::{
        BakedGiStateRef, DayNightCyclePolicy, LightingEnvironmentAuthoring, Rgb8,
        TimeOfDayKeyframe, WorldEnvironmentError, record::persist_and_lower_environment,
    },
};
use serde_json::json;

#[path = "world_environment_fsv_common/fixture.rs"]
mod fixture;
#[path = "world_environment_fsv_common/wgsl_verify.rs"]
mod wgsl_verify;

fn bake_state_fingerprint(state: &str, sky: Vec3) -> Result<String, Box<dyn Error>> {
    let spec = ProbeGridSpec {
        origin: Vec3::new(0.0, 0.0, 0.0),
        spacing: Vec3::splat(1.0),
        counts: [3, 3, 3],
        irradiance_dim: 6,
        visibility_dim: 6,
    };
    let scene = BakeScene::new(
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
    );
    let volume = bake_probe_volume(spec, &scene, BakeConfig::default())?;
    let lowered = lower_probe_volume(&volume)?;
    Ok(lowered.fingerprint)
}

fn lighting_fixture(id: &str, day_fp: &str, night_fp: &str) -> LightingEnvironmentAuthoring {
    LightingEnvironmentAuthoring {
        id: id.to_string(),
        display_name: "Ashen Dawn Cycle".to_string(),
        author: "issue814-fsv".to_string(),
        cycle: DayNightCyclePolicy::Enabled,
        cycle_seconds: 600,
        keyframes: vec![
            TimeOfDayKeyframe {
                time_permille: 250,
                sun_color: Rgb8::new(255, 240, 210),
                sun_intensity_milli: 900,
                ambient_color: Rgb8::new(120, 130, 150),
                sun_azimuth_deg: 90,
                sun_elevation_deg: 40,
            },
            TimeOfDayKeyframe {
                time_permille: 750,
                sun_color: Rgb8::new(40, 45, 95),
                sun_intensity_milli: 120,
                ambient_color: Rgb8::new(22, 24, 44),
                sun_azimuth_deg: 270,
                sun_elevation_deg: -18,
            },
        ],
        baked_gi_states: vec![
            BakedGiStateRef {
                state_label: "day".to_string(),
                time_permille: 250,
                probe_volume_fingerprint: day_fp.to_string(),
            },
            BakedGiStateRef {
                state_label: "night".to_string(),
                time_permille: 750,
                probe_volume_fingerprint: night_fp.to_string(),
            },
        ],
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue814_lighting_day_night")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");

    // #1742 integration: bake distinct day/night probe-GI states.
    let day_fp = bake_state_fingerprint("day", Vec3::new(0.10, 0.12, 0.16))?;
    let night_fp = bake_state_fingerprint("night", Vec3::new(0.01, 0.012, 0.02))?;
    let states_distinct = day_fp != night_fp;
    if !states_distinct {
        return Err("day and night baked GI states must differ (probe fingerprints equal)".into());
    }

    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue814",
            fixture::unavailable_panel_state(),
        ),
    );

    let authoring = lighting_fixture("light-ashen-dawn-issue814", &day_fp, &night_fp);
    let payload = authoring.to_payload()?;
    let readback = persist_and_lower_environment(
        &store,
        &authoring.author,
        "editor://worlds/ashen/lighting",
        &authoring.display_name,
        &payload,
    )?;

    let wgsl = wgsl_verify::verify_world_environment_wgsl()?;

    // Source-of-truth readback with a fresh vault handle.
    let fresh = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let fresh_store = EngineCalyxContentStore::with_measurement_config(
        fresh,
        EngineContentMeasurementConfig::from_preloaded_state(
            "separate-readback:issue814",
            fixture::unavailable_panel_state(),
        ),
    );
    let on_disk = fresh_store.get_content_for_editor(EngineContentKind::Terrain, &authoring.id)?;

    // The persisted payload references BOTH baked fingerprints.
    let payload_refs_day = on_disk.payload["baked_gi_states"]
        .as_array()
        .map(|a| a.iter().any(|s| s["probe_volume_fingerprint"] == day_fp))
        .unwrap_or(false);
    let payload_refs_night = on_disk.payload["baked_gi_states"]
        .as_array()
        .map(|a| a.iter().any(|s| s["probe_volume_fingerprint"] == night_fp))
        .unwrap_or(false);

    let edges = json!({
        "non_increasing_keyframes_rejected": reject_keyframe_order().is_err(),
        "enabled_cycle_zero_seconds_rejected": reject_zero_cycle().is_err(),
        "bad_probe_fingerprint_rejected": reject_bad_fingerprint(&day_fp).is_err(),
    });

    let acceptance = json!({
        "day_night_baked_states_distinct": states_distinct,
        "persisted_pending_without_ort":
            readback.row.measurement.measurement_state == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        "declared_gate_binding": readback.row.measurement.declared_learned_lenses >= 10,
        "lowered_field_count_positive": !readback.lowered.fields.is_empty(),
        "on_disk_references_day_state": payload_refs_day,
        "on_disk_references_night_state": payload_refs_night,
        "on_disk_sha_matches_writer": on_disk.database.sha256 == readback.row.database.sha256,
        "wgsl_naga_valid": wgsl["naga_validate_ok"] == true,
        "wgsl_gpu_execution_verified": wgsl["gpu_execution"]["verified"] == true,
        "edge_keyframe_order_rejected": edges["non_increasing_keyframes_rejected"],
        "edge_zero_cycle_rejected": edges["enabled_cycle_zero_seconds_rejected"],
        "edge_bad_fingerprint_rejected": edges["bad_probe_fingerprint_rejected"],
    });

    let report = json!({
        "schema": "litd.engine.issue814.lighting_day_night_fsv.v1",
        "issue": 814,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "day_probe_fingerprint": day_fp,
            "night_probe_fingerprint": night_fp,
            "environment": { "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some() },
            "manual_gate": "Read this report + the persisted Aster row; command success alone is not proof."
        },
        "sufficiency_argument": "Two real probe-GI bakes (bright day vs dark night sky) produce distinct lowered fingerprints, proving the merged #1742 day/night baked states are integrated as authored references. One lighting surface with two keyframes + two baked-state refs is the smallest input exercising persist (pending_measurement), a durable readback that still references both fingerprints, lowering, and the naga-validated ambient shader. Three edges prove keyframe-order, cycle-length, and fingerprint-shape invariants fail loud.",
        "probe_gi_integration": { "day": day_fp, "night": night_fp, "distinct": states_distinct },
        "persist": readback.evidence(),
        "wgsl": wgsl,
        "edges": edges,
        "acceptance": acceptance
    });

    let written = run.write_report(&report)?;
    for (field, value) in written.json["acceptance"].as_object().expect("acceptance") {
        if value != true {
            return Err(format!("issue814 acceptance {field} was not true").into());
        }
    }
    println!(
        "ISSUE814_LIGHTING_FSV report={} sha256={} day={} night={} distinct={}",
        written.path.display(),
        written.sha256,
        &day_fp[..12],
        &night_fp[..12],
        states_distinct,
    );
    Ok(())
}

fn reject_keyframe_order() -> Result<(), WorldEnvironmentError> {
    let mut a = lighting_fixture("edge-order", &"a".repeat(64), &"b".repeat(64));
    a.keyframes[1].time_permille = a.keyframes[0].time_permille;
    a.validate()
}

fn reject_zero_cycle() -> Result<(), WorldEnvironmentError> {
    let mut a = lighting_fixture("edge-cycle", &"a".repeat(64), &"b".repeat(64));
    a.cycle_seconds = 0;
    a.validate()
}

fn reject_bad_fingerprint(day_fp: &str) -> Result<(), WorldEnvironmentError> {
    let mut a = lighting_fixture("edge-fp", day_fp, "too-short");
    a.baked_gi_states[1].probe_volume_fingerprint = "too-short".to_string();
    a.validate()
}
