//! FSV for #808: water (deep/shallow) + map boundary + environment skybox.
//!
//! With NO ORT/addon present, author a deep/shallow water body + map boundary +
//! skybox, persist it through the REAL content path (put_content -> pending_measurement,
//! #1791), lower it to a frozen fingerprinted sim package, then reopen the Aster
//! vault with a BRAND-NEW handle and read the row back (source of truth, #800).
//! The renderer consumption is proven by building the GPU water uniform FROM the
//! lowered fields, naga-validating `world_environment.wgsl`, and EXECUTING its real
//! water-tint helper on the GPU (wgpu compute readback) to confirm the shader output
//! equals the CPU oracle (no substring checks, no Rust-vs-Rust crosscheck).
//! Three edge cases prove the authoring boundaries fail loud.

use std::error::Error;

use litd_engine::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING, EngineCalyxContentStore,
        EngineContentMeasurementConfig,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    world_environment::{
        BoundaryKind, MapBoundary, Rgba8, SkyboxPreset, WaterEnvironmentAuthoring,
        record::persist_and_lower_environment,
        uniform::{EnvironmentUniform, apply_water, cpu_water_tint},
        water::WaterBody,
    },
};
use serde_json::{Value, json};

#[path = "world_environment_fsv_common/fixture.rs"]
mod fixture;
#[path = "world_environment_fsv_common/wgsl_verify.rs"]
mod wgsl_verify;

fn water_fixture(id: &str) -> WaterEnvironmentAuthoring {
    WaterEnvironmentAuthoring {
        id: id.to_string(),
        display_name: "Tidewater Basin".to_string(),
        author: "issue808-fsv".to_string(),
        water: WaterBody {
            shallow_depth_mm: 800,
            deep_depth_mm: 4_000,
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

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue808_water_boundary_environment")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue808",
            fixture::unavailable_panel_state(),
        ),
    );

    let authoring = water_fixture("map-tidewater-issue808");
    let payload = authoring.to_payload()?;
    let readback = persist_and_lower_environment(
        &store,
        &authoring.author,
        "editor://worlds/tidewater/water",
        &authoring.display_name,
        &payload,
    )?;

    // Renderer consumption: build the water uniform from the LOWERED fields and
    // cross-check the WGSL tint math against the CPU mirror at shallow/mid/deep.
    let mut uniform = EnvironmentUniform::default();
    apply_water(&mut uniform, &readback.lowered)?;
    let render = water_render_evidence(&uniform)?;
    let wgsl = wgsl_verify::verify_world_environment_wgsl()?;

    // Source-of-truth readback: a fresh handle over the persisted vault.
    let fresh = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let fresh_store = EngineCalyxContentStore::with_measurement_config(
        fresh,
        EngineContentMeasurementConfig::from_preloaded_state(
            "separate-readback:issue808",
            fixture::unavailable_panel_state(),
        ),
    );
    let on_disk = fresh_store.get_content_for_editor(
        litd_engine::calyx_content::EngineContentKind::Terrain,
        &authoring.id,
    )?;

    let edges = json!({
        "deep_shallower_than_shallow_rejected": edge_rejected(reject_deep_order()),
        "inset_consumes_map_rejected": edge_rejected(reject_inset()),
        "deep_more_transparent_rejected": edge_rejected(reject_transparency()),
    });

    let acceptance = json!({
        "persisted_pending_without_ort":
            readback.row.measurement.measurement_state == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        "declared_gate_binding": readback.row.measurement.declared_learned_lenses >= 10,
        "lowered_field_count_positive": !readback.lowered.fields.is_empty(),
        "lowered_deep_depth_mm": render["lowered_deep_depth_mm"] == 4_000,
        "on_disk_sha_matches_writer":
            on_disk.database.sha256 == readback.row.database.sha256,
        "on_disk_cx_matches_writer": on_disk.cx_id == readback.row.cx_id,
        "wgsl_naga_valid": wgsl["naga_validate_ok"] == true,
        "wgsl_gpu_execution_verified": wgsl["gpu_execution"]["verified"] == true,
        "wgsl_water_tint_matches_gpu": wgsl["gpu_execution"]["water"]["within_tolerance"] == true,
        "deep_tint_more_opaque_than_shallow": render["deep_more_opaque"] == true,
        "edge_deep_order_rejected": edges["deep_shallower_than_shallow_rejected"],
        "edge_inset_rejected": edges["inset_consumes_map_rejected"],
        "edge_transparency_rejected": edges["deep_more_transparent_rejected"],
    });

    let report = json!({
        "schema": "litd.engine.issue808.water_boundary_environment_fsv.v1",
        "issue": 808,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "environment": { "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some() },
            "manual_gate": "Read this report + the persisted Aster row under engine_vault_dir; command success alone is not proof."
        },
        "sufficiency_argument": "One authored deep/shallow water map is the smallest input that exercises the whole slice: explicit persist under an addon-absent 12-lens panel (pending_measurement), a durable fresh-handle readback, a frozen lowered package whose integer water fields build the GPU uniform, and a naga-validated WGSL whose tint math mirrors the CPU exactly. Three edges prove the deep>shallow, inset, and opacity invariants fail loud.",
        "persist": readback.evidence(),
        "render": render,
        "wgsl": wgsl,
        "edges": edges,
        "acceptance": acceptance
    });

    let written = run.write_report(&report)?;
    for (field, value) in written.json["acceptance"].as_object().expect("acceptance") {
        if value != true {
            return Err(format!("issue808 acceptance {field} was not true").into());
        }
    }
    println!(
        "ISSUE808_WATER_FSV report={} sha256={} state={} lowered_fields={}",
        written.path.display(),
        written.sha256,
        readback.row.measurement.measurement_state,
        readback.lowered.fields.len(),
    );
    Ok(())
}

fn water_render_evidence(uniform: &EnvironmentUniform) -> Result<Value, Box<dyn Error>> {
    // Shallow / mid / deep tint from the CPU mirror; the WGSL cross-check lives in
    // wgsl_verify (same formula, validated module). Here we assert the physical
    // invariant: deeper water is strictly more opaque.
    let shallow = cpu_water_tint(uniform, uniform.water_depths[0]);
    let mid = cpu_water_tint(
        uniform,
        0.5 * (uniform.water_depths[0] + uniform.water_depths[1]),
    );
    let deep = cpu_water_tint(uniform, uniform.water_depths[1]);
    let deep_more_opaque = deep[3] > mid[3] && mid[3] > shallow[3];
    Ok(json!({
        "lowered_deep_depth_mm": (uniform.water_depths[1] * 1000.0).round() as u64,
        "shallow_alpha": shallow[3],
        "mid_alpha": mid[3],
        "deep_alpha": deep[3],
        "deep_more_opaque": deep_more_opaque,
    }))
}

fn edge_rejected(
    result: Result<(), litd_engine::world_environment::WorldEnvironmentError>,
) -> bool {
    result.is_err()
}

fn reject_deep_order() -> Result<(), litd_engine::world_environment::WorldEnvironmentError> {
    let mut a = water_fixture("edge-deep-order");
    a.water.deep_depth_mm = 700; // < shallow (800)
    a.validate()
}

fn reject_inset() -> Result<(), litd_engine::world_environment::WorldEnvironmentError> {
    let mut a = water_fixture("edge-inset");
    a.boundary.playable_inset_tiles = 48; // 2*48 == 96 == height
    a.validate()
}

fn reject_transparency() -> Result<(), litd_engine::world_environment::WorldEnvironmentError> {
    let mut a = water_fixture("edge-transparency");
    a.water.deep_color = Rgba8::new(10, 30, 70, 50); // more transparent than shallow (90)
    a.validate()
}
