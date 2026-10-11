//! FSV #809: doodad placement brush -> persist -> readback -> lowered artifact.
//!
//! Authors a WC3-style doodad scatter brush (variation/scale/rotation/density),
//! persists the resolved placements through the REAL Calyx content path with NO
//! ORT present (the row lands as `pending_measurement`), lowers the verified
//! readback into a frozen sim package, and proves the persisted Aster bytes are a
//! self-contained, deterministic, integer-only artifact a sim can consume without
//! querying Calyx. A brand-new database handle reopens the vault to read the row
//! back (the source of truth, #800). Three edge cases prove the boundaries.

use std::error::Error;

use litd_engine::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING, EngineCalyxContentStore,
        EngineContentConstellationReadback, EngineContentKind, EngineContentMeasurementConfig,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    world_authoring::persist_and_lower_world_content,
    world_scatter::{
        DOODAD_SCATTER_SCHEMA, DoodadScatterBrush, DoodadScatterPayload, DoodadVariant, ScatterCell,
    },
};
use serde_json::{Value, json};

#[path = "fixtures/world_scenery_panel_fixture.rs"]
mod panel;

const PANEL_LENSES: usize = 12;
const CONTENT_KIND: EngineContentKind = EngineContentKind::Terrain;

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue809_doodad_scatter")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database.clone(),
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue809",
            panel::unavailable_panel_state(PANEL_LENSES),
        ),
    );

    // Author + persist + lower.
    let brush = pine_grove_brush();
    let authored = brush.scatter()?;
    let record = brush.to_content_record("issue809-fsv")?;
    let lowering = persist_and_lower_world_content(&store, &record)?;
    let readback = store.get_content_for_editor(CONTENT_KIND, &brush.brush_id)?;

    // Reopen the vault with a fresh handle: durable readback of the source of truth.
    let on_disk = fresh_readback(&vault_dir, &brush.brush_id)?;

    // Sim consumption: reconstruct placements straight from the persisted payload
    // and prove they equal what the brush deterministically authored.
    let persisted_payload: DoodadScatterPayload = serde_json::from_value(on_disk.payload.clone())?;
    let consumable = persisted_payload.schema == DOODAD_SCATTER_SCHEMA
        && persisted_payload.placements == authored
        && persisted_payload.placement_count as usize == authored.len();

    let author = json!({
        "brush_id": brush.brush_id,
        "footprint_cells": brush.cells.len(),
        "density_percent": brush.density_percent,
        "variant_count": brush.variants.len(),
        "placement_count": authored.len(),
        "measurement_state": readback.measurement.measurement_state,
        "is_pending_without_ort": readback.measurement.measurement_state
            == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        "cx_id": readback.cx_id,
        "input_sha256": readback.input_sha256,
        "row_seq": readback.database.seq,
        "row_sha256": readback.database.sha256,
    });
    let lowered = json!({
        "lowered_field_count": lowering.lowered_readback.field_count,
        "package_fingerprint": lowering.lowered_readback.package_fingerprint,
        "sim_access_mode": lowering.lowered_package.sim_access_mode,
        "calyx_live_query_allowed_in_hot_path":
            lowering.lowered_package.calyx_live_query_allowed_in_hot_path,
        "source_cx_id_matches": lowering.lowered_package.source_cx_id == readback.cx_id,
        "lowered_fields_nonempty": lowering.lowered_readback.field_count > 0,
    });
    let durable = json!({
        "on_disk_cx_matches_writer": on_disk.cx_id == readback.cx_id,
        "on_disk_sha_matches_writer": on_disk.database.sha256 == readback.database.sha256,
        "on_disk_placement_count": persisted_payload.placement_count,
        "sim_consumable_from_disk": consumable,
    });

    let edges = json!({
        "density_governs_count": density_edge()?,
        "seed_changes_layout": seed_edge()?,
        "invalid_brush_fails_closed": invalid_edge(&store)?,
    });

    let acceptance = json!({
        "authored_placements_nonempty": author["placement_count"].as_u64().unwrap_or(0) > 0,
        "persisted_pending_without_ort": author["is_pending_without_ort"],
        "lowered_artifact_produced": lowered["lowered_fields_nonempty"],
        "lowered_no_live_calyx": lowered["calyx_live_query_allowed_in_hot_path"] == false,
        "lowered_cx_matches_row": lowered["source_cx_id_matches"],
        "durable_readback_cx_matches": durable["on_disk_cx_matches_writer"],
        "durable_readback_sha_matches": durable["on_disk_sha_matches_writer"],
        "sim_consumable_from_persisted_bytes": durable["sim_consumable_from_disk"],
        "edge_density_governs_count": edges["density_governs_count"]["passed"],
        "edge_seed_changes_layout": edges["seed_changes_layout"]["passed"],
        "edge_invalid_fails_closed": edges["invalid_brush_fails_closed"]["passed"],
    });

    let report = json!({
        "schema": "litd.engine.issue809.doodad_scatter_fsv.v1",
        "issue": 809,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "environment": {
                "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some(),
            },
            "manual_gate": "Manual FSV must read this report and the persisted Aster rows under engine_vault_dir; command success alone is not proof."
        },
        "sufficiency_argument": "An 8x8 footprint with two weighted variants, a scale range, a full-circle yaw range, and 60% density is the smallest input that exercises every scatter axis (variation, scale, rotation, density) at once. It persists addon-absent (pending_measurement), lowers into a frozen integer-only sim package, and the persisted bytes reconstruct the exact authored placements -- proving the deterministic persist->lower->consume slice end to end.",
        "author": author,
        "lowered": lowered,
        "durable_readback": durable,
        "edges": edges,
        "acceptance": acceptance
    });

    let written = run.write_report(&report)?;
    for (field, value) in written.json["acceptance"]
        .as_object()
        .expect("acceptance object")
    {
        if value != true {
            return Err(format!("issue809 acceptance {field} was not true").into());
        }
    }
    println!(
        "ISSUE809_DOODAD_SCATTER_FSV report={} sha256={} placements={} lowered_fields={} state={}",
        written.path.display(),
        written.sha256,
        author["placement_count"],
        lowered["lowered_field_count"],
        author["measurement_state"],
    );
    Ok(())
}

fn pine_grove_brush() -> DoodadScatterBrush {
    DoodadScatterBrush {
        brush_id: "issue809_pine_grove".to_string(),
        display_name: "Pine Grove".to_string(),
        variants: vec![
            DoodadVariant {
                variant_id: "pine_a".to_string(),
                weight: 3,
            },
            DoodadVariant {
                variant_id: "pine_b".to_string(),
                weight: 1,
            },
        ],
        density_percent: 60,
        min_scale_milli: 800,
        max_scale_milli: 1200,
        min_yaw_deg: 0,
        max_yaw_deg: 359,
        cells: (0..8)
            .flat_map(|x| {
                (0..8).map(move |z| ScatterCell {
                    tile_x: x,
                    tile_z: z,
                })
            })
            .collect(),
        seed: 0x1234_5678,
    }
}

fn density_edge() -> Result<Value, Box<dyn Error>> {
    let mut low = pine_grove_brush();
    low.brush_id = "issue809_density_low".to_string();
    low.density_percent = 20;
    let mut high = pine_grove_brush();
    high.brush_id = "issue809_density_high".to_string();
    high.density_percent = 100;
    let before = low.scatter()?.len();
    let after = high.scatter()?.len();
    Ok(json!({
        "before_count_at_20pct": before,
        "after_count_at_100pct": after,
        "full_grid": high.cells.len(),
        "passed": after == high.cells.len() && before < after,
    }))
}

fn seed_edge() -> Result<Value, Box<dyn Error>> {
    let mut a = pine_grove_brush();
    a.brush_id = "issue809_seed_a".to_string();
    let mut b = pine_grove_brush();
    b.brush_id = "issue809_seed_a".to_string();
    b.seed = 0x9999_0000;
    let before = fingerprint_layout(&a.scatter()?);
    let after = fingerprint_layout(&b.scatter()?);
    Ok(json!({
        "before_layout_fingerprint": before,
        "after_layout_fingerprint": after,
        "passed": before != after,
    }))
}

fn invalid_edge(store: &EngineCalyxContentStore) -> Result<Value, Box<dyn Error>> {
    let mut brush = pine_grove_brush();
    brush.brush_id = "issue809_invalid".to_string();
    brush.density_percent = 0;
    let record_result = brush.to_content_record("issue809-fsv");
    let code = record_result.as_ref().err().map(|error| error.code());
    let row_absent = store
        .get_content_for_editor(CONTENT_KIND, "issue809_invalid")
        .is_err();
    Ok(json!({
        "before": "no row authored",
        "after": "record build rejected, nothing persisted",
        "error_code": code,
        "row_absent": row_absent,
        "passed": code == Some("WORLD_SCATTER_DENSITY") && row_absent,
    }))
}

fn fingerprint_layout(placements: &[litd_engine::world_scatter::DoodadPlacement]) -> String {
    let bytes = serde_json::to_vec(placements).expect("serialize placements");
    litd_hash::sha256_hex(&bytes)
}

fn fresh_readback(
    vault_dir: &std::path::Path,
    id: &str,
) -> Result<EngineContentConstellationReadback, Box<dyn Error>> {
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            "separate-readback:issue809",
            panel::unavailable_panel_state(PANEL_LENSES),
        ),
    );
    Ok(store.get_content_for_editor(CONTENT_KIND, id)?)
}
