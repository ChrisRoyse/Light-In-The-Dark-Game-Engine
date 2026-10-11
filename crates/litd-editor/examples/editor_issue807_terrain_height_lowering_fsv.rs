//! FSV for #807: cliffs / ramps / height layers lowered into the #1739 CDLOD runtime.
//!
//! Vertical slice: author cliff/ramp height layers with the real terrain bulk-op
//! capability -> persist the world through the REAL Calyx world-record path (Aster)
//! -> read the stored bytes back and parse them -> lower THAT readback document into
//! a `HeightField`+`TerrainConfig` -> feed it to the CDLOD `TerrainRuntime` and read
//! the selected-frame stats. The verdict is an immutable JSON report re-read from
//! disk and re-asserted, so a false green cannot survive the round-trip.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    terrain_bulk_ops::{TerrainBulkScope, adjust_cliff_levels},
    terrain_lowering::{HeightFieldLowerParams, LoweredHeightField, lower_height_field},
};
use litd_engine::{fsv_artifacts::begin_issue_run, math::Vec3, render::terrain::TerrainRuntime};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[path = "support/terrain_lowering_support.rs"]
mod support;

const CLIFF_STEP: f32 = 4.0;

fn params() -> HeightFieldLowerParams {
    HeightFieldLowerParams {
        cliff_step: CLIFF_STEP,
        ..Default::default()
    }
}

fn selection(cells: &[&str]) -> TerrainBulkScope {
    TerrainBulkScope::Selection {
        terrain_cell_ids: cells.iter().map(|c| c.to_string()).collect::<BTreeSet<_>>(),
        object_ids: BTreeSet::new(),
    }
}

/// Author a plateau (+2 levels) and a one-cell ramp (+1) into `document`.
fn author_terrain(document: &mut EditorDocument) -> Result<(), Box<dyn std::error::Error>> {
    let plateau = [
        "terrain_cell_5_5",
        "terrain_cell_6_5",
        "terrain_cell_5_6",
        "terrain_cell_6_6",
    ];
    adjust_cliff_levels(document, 2, &selection(&plateau))?;
    adjust_cliff_levels(document, 1, &selection(&["terrain_cell_4_5"]))?;
    Ok(())
}

fn runtime_stats(lowered: &LoweredHeightField) -> Result<Value, Box<dyn std::error::Error>> {
    let mut runtime = TerrainRuntime::new(&lowered.field, lowered.config.clone())?;
    let stops = [
        Vec3::new(0.0, 4.0, 0.0),
        Vec3::new(6.0, 4.0, 6.0),
        Vec3::new(-8.0, 4.0, -8.0),
    ];
    let mut rows = Vec::new();
    for pos in stops {
        let stats = runtime.select_frame(pos, None).stats;
        rows.push(json!({
            "camera": [pos.x, pos.y, pos.z],
            "tile_count": stats.tile_count,
            "total_vertices": stats.total_vertices,
            "distinct_lods": stats.distinct_lods,
            "vertex_accounting_ok":
                stats.total_vertices == stats.tile_count as u64 * lowered.config.tile_vertex_count() as u64,
        }));
    }
    Ok(Value::Array(rows))
}

fn lowered_tile_count(document: &EditorDocument) -> Result<u32, Box<dyn std::error::Error>> {
    let lowered = lower_height_field(document, &params())?;
    let mut runtime = TerrainRuntime::new(&lowered.field, lowered.config.clone())?;
    Ok(runtime
        .select_frame(Vec3::new(0.0, 4.0, 0.0), None)
        .stats
        .tile_count)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue807_terrain_height_lowering")?;
    let aster_dir = run.artifact_dir("aster")?;

    // Author -> persist -> read back -> parse the STORED bytes.
    let mut record = support::world_record("issue807-cliffs", 8);
    let flat_max = record
        .document
        .terrain_cells
        .values()
        .map(|c| c.height)
        .fold(f32::NEG_INFINITY, f32::max);
    author_terrain(&mut record.document)?;
    support::sync(&mut record, "cliffs_ramp")?;
    let persisted = support::persist_and_readback(&aster_dir, &record)?;

    // Lower the readback document, then consume it in the CDLOD runtime.
    let lowered = lower_height_field(&persisted.document, &params())?;
    let relowered = lower_height_field(&persisted.document, &params())?;
    let plateau_cell = &persisted.document.terrain_cells["terrain_cell_5_5"];
    let ramp_cell = &persisted.document.terrain_cells["terrain_cell_4_5"];

    let report = json!({
        "schema": "litd.issue807.terrain_height_lowering.v1",
        "issue": 807,
        "source_of_truth": "Aster world-record readback -> lower_height_field -> CDLOD TerrainRuntime frame stats",
        "persist": {
            "aster_sha256": persisted.readback.sha256,
            "document_fingerprint": persisted.readback.document_fingerprint,
            "storage_schema": persisted.readback.storage_schema,
            "chunk_count": persisted.readback.chunk_count,
        },
        "authored": {
            "plateau_cliff_level": plateau_cell.cliff_level,
            "ramp_cliff_level": ramp_cell.cliff_level,
            "cliff_step": CLIFF_STEP,
        },
        "happy_path": {
            "lowered_schema": lowered.schema,
            "lowered_fingerprint": lowered.fingerprint,
            "source_fingerprint": lowered.source_fingerprint,
            "field_size": lowered.field.size,
            "sample_count": lowered.sample_count,
            "min_height": lowered.min_height,
            "max_height": lowered.max_height,
            "expected_plateau_height": plateau_cell.cliff_level as f32 * CLIFF_STEP,
            "runtime": runtime_stats(&lowered)?,
        },
        "edges": {
            "flat_vs_cliffed_refinement": {
                "before_flat_max_height": flat_max,
                "flat_tile_count": lowered_tile_count(&support::world_record("issue807-flat", 8).document)?,
                "cliffed_tile_count": lowered_tile_count(&persisted.document)?,
            },
            "ramp_intermediate_height": {
                "ramp_height": ramp_cell.cliff_level as f32 * CLIFF_STEP,
                "min_height": lowered.min_height,
                "max_height": lowered.max_height,
            },
            "deterministic_relower": {
                "fingerprint_a": lowered.fingerprint,
                "fingerprint_b": relowered.fingerprint,
                "equal": lowered.fingerprint == relowered.fingerprint,
            },
            "non_dense_fails_loud": non_dense_edge(&persisted.document),
        },
    });

    verify(&report)?;
    let readback = run.write_report(&report)?;
    let persisted_bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&persisted_bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&persisted_bytes) != readback.sha256 {
        return Err("persisted #807 report hash mismatch".into());
    }
    println!(
        "wrote #807 terrain height lowering FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

/// Edge: a hole in the tile grid must fail loud (never silently guessed).
fn non_dense_edge(document: &EditorDocument) -> Value {
    let mut holed = document.clone();
    holed.terrain_cells.remove("terrain_cell_0_0");
    match lower_height_field(&holed, &params()) {
        Ok(_) => json!({"failed_loud": false}),
        Err(err) => json!({"failed_loud": true, "error": err.to_string()}),
    }
}

fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };
    let happy = &report["happy_path"];
    let expected = happy["expected_plateau_height"].as_f64().unwrap_or(0.0);
    need(
        (happy["max_height"].as_f64().unwrap_or(0.0) - expected).abs() < 1e-3 && expected > 0.0,
        "plateau cliff did not raise the lowered max height",
    )?;
    for row in happy["runtime"].as_array().ok_or("runtime missing")? {
        need(
            row["tile_count"].as_u64().unwrap_or(0) >= 1,
            "no tiles selected",
        )?;
        need(
            row["vertex_accounting_ok"].as_bool().unwrap_or(false),
            "vertex budget inconsistent",
        )?;
    }
    let edges = &report["edges"];
    need(
        edges["flat_vs_cliffed_refinement"]["cliffed_tile_count"]
            .as_u64()
            .unwrap_or(0)
            > edges["flat_vs_cliffed_refinement"]["flat_tile_count"]
                .as_u64()
                .unwrap_or(0),
        "cliff did not force more subdivision than flat ground",
    )?;
    let ramp = &edges["ramp_intermediate_height"];
    let rh = ramp["ramp_height"].as_f64().unwrap_or(-1.0);
    need(
        rh > ramp["min_height"].as_f64().unwrap_or(0.0)
            && rh < ramp["max_height"].as_f64().unwrap_or(0.0),
        "ramp height is not strictly between ground and plateau",
    )?;
    need(
        edges["deterministic_relower"]["equal"]
            .as_bool()
            .unwrap_or(false),
        "re-lower not deterministic",
    )?;
    need(
        edges["non_dense_fails_loud"]["failed_loud"]
            .as_bool()
            .unwrap_or(false),
        "non-dense grid did not fail loud",
    )?;
    Ok(())
}
