//! FSV for #806: tile/tileset palette + custom/mixed tilesets lowered into the
//! render splat-index buffer the CDLOD terrain draw consumes.
//!
//! Vertical slice: author a mixed/custom tileset onto the tile grid -> persist the
//! world through the REAL Calyx world-record path (Aster) -> read the stored bytes
//! back and parse them -> lower THAT readback document into a palette + per-tile
//! `u16` splat-index buffer -> prove the render consumption by reconstructing every
//! authored tile id from `palette[indices[..]]`. Verdict is re-read from disk.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    terrain_lowering::{LoweredTileSplat, lower_tile_splat_map},
};
use litd_engine::fsv_artifacts::begin_issue_run;
use serde_json::{Value, json};

#[path = "support/terrain_lowering_support.rs"]
mod support;

const CUSTOM_TILE: &str = "tile.custom.obsidian_road";
const ROCK_TILE: &str = "tile.rock.dark";

/// Repaint a mixed/custom tileset: a rock block plus a custom road diagonal.
fn author_tiles(document: &mut EditorDocument) {
    for (x, z) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        set_tile(document, x, z, ROCK_TILE);
    }
    for i in 0..6 {
        set_tile(document, i, i, CUSTOM_TILE);
    }
}

fn set_tile(document: &mut EditorDocument, x: i32, z: i32, tile: &str) {
    if let Some(cell) = document
        .terrain_cells
        .get_mut(&format!("terrain_cell_{x}_{z}"))
    {
        cell.tile_id = tile.to_string();
    }
}

/// Reconstruct every authored tile id from the lowered palette+index buffer.
fn reconstruct(document: &EditorDocument, splat: &LoweredTileSplat) -> (usize, usize) {
    let mut matched = 0usize;
    let mut total = 0usize;
    for cell in document.terrain_cells.values() {
        total += 1;
        if splat.tile_id_at(cell.tile_x, cell.tile_z) == Some(cell.tile_id.as_str()) {
            matched += 1;
        }
    }
    (matched, total)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue806_tile_palette_lowering")?;
    let aster_dir = run.artifact_dir("aster")?;

    // Author -> persist -> read back -> parse the STORED bytes.
    let mut record = support::world_record("issue806-tiles", 6);
    author_tiles(&mut record.document);
    support::sync(&mut record, "mixed_tileset")?;
    let persisted = support::persist_and_readback(&aster_dir, &record)?;

    let splat = lower_tile_splat_map(&persisted.document)?;
    let relowered = lower_tile_splat_map(&persisted.document)?;
    let (matched, total) = reconstruct(&persisted.document, &splat);

    // Edge 1: a uniform single-tileset world lowers to a 1-entry palette.
    let uniform = support::world_record("issue806-uniform", 6);
    let uniform_splat = lower_tile_splat_map(&uniform.document)?;

    // Edge 2: the custom tile id survives the lowering and round-trips.
    let custom_index = splat.palette.iter().position(|t| t == CUSTOM_TILE);
    let custom_roundtrip = splat.tile_id_at(3, 3) == Some(CUSTOM_TILE);

    // Edge 3: palette is sorted+unique (a stable render binding order).
    let sorted = splat.palette.windows(2).all(|w| w[0] < w[1]);

    let report = json!({
        "schema": "litd.issue806.tile_palette_lowering.v1",
        "issue": 806,
        "source_of_truth": "Aster world-record readback -> lower_tile_splat_map -> palette[indices] reconstruction",
        "persist": {
            "aster_sha256": persisted.readback.sha256,
            "document_fingerprint": persisted.readback.document_fingerprint,
            "storage_schema": persisted.readback.storage_schema,
        },
        "happy_path": {
            "lowered_schema": splat.schema,
            "lowered_fingerprint": splat.fingerprint,
            "palette": splat.palette,
            "palette_len": splat.palette.len(),
            "cell_count": splat.cell_count,
            "grid": [splat.tiles_x, splat.tiles_z],
            "index_buffer_len": splat.indices.len(),
            "reconstructed_cells": matched,
            "total_cells": total,
            "all_reconstructed": matched == total,
        },
        "edges": {
            "uniform_single_palette": {
                "palette_len": uniform_splat.palette.len(),
                "all_zero_indices": uniform_splat.indices.iter().all(|&i| i == 0),
            },
            "custom_tile_roundtrip": {
                "custom_tile": CUSTOM_TILE,
                "present_in_palette": custom_index.is_some(),
                "roundtrip_at_3_3": custom_roundtrip,
            },
            "palette_sorted_unique": {
                "sorted": sorted,
                "deterministic_fingerprint": splat.fingerprint == relowered.fingerprint,
            },
        },
    });

    verify(&report)?;
    let readback = run.write_report(&report)?;
    let bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&bytes) != readback.sha256 {
        return Err("persisted #806 report hash mismatch".into());
    }
    println!(
        "wrote #806 tile palette lowering FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };
    let happy = &report["happy_path"];
    need(
        happy["palette_len"].as_u64().unwrap_or(0) >= 3,
        "mixed/custom tileset should yield palette >= 3",
    )?;
    need(
        happy["all_reconstructed"].as_bool().unwrap_or(false),
        "not every authored tile reconstructed from the splat buffer",
    )?;
    need(
        happy["index_buffer_len"].as_u64() == happy["cell_count"].as_u64()
            && happy["cell_count"].as_u64() == happy["total_cells"].as_u64(),
        "splat index buffer does not cover exactly the authored ground grid",
    )?;
    let edges = &report["edges"];
    need(
        edges["uniform_single_palette"]["palette_len"].as_u64() == Some(1),
        "uniform world palette must be 1",
    )?;
    need(
        edges["uniform_single_palette"]["all_zero_indices"]
            .as_bool()
            .unwrap_or(false),
        "uniform world indices must all be 0",
    )?;
    need(
        edges["custom_tile_roundtrip"]["present_in_palette"]
            .as_bool()
            .unwrap_or(false),
        "custom tile missing from palette",
    )?;
    need(
        edges["custom_tile_roundtrip"]["roundtrip_at_3_3"]
            .as_bool()
            .unwrap_or(false),
        "custom tile did not round-trip",
    )?;
    need(
        edges["palette_sorted_unique"]["sorted"]
            .as_bool()
            .unwrap_or(false),
        "palette not sorted-unique",
    )?;
    need(
        edges["palette_sorted_unique"]["deterministic_fingerprint"]
            .as_bool()
            .unwrap_or(false),
        "tile splat lowering not deterministic",
    )?;
    Ok(())
}
