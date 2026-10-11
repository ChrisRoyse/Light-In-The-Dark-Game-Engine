//! FSV for #1739: CDLOD/quadtree GPU-driven terrain runtime.
//!
//! Two things are proven here, and the report says exactly what each does and
//! does NOT establish:
//!
//! A. CPU-side LOD selection (the runtime this PR ships). Source of truth = an
//!    immutable JSON report under `target/fsv/issue1739_cdlod_terrain/`, written
//!    then re-read from disk and re-asserted so a false green cannot slip through.
//!      1. Build a heightfield world and walk the camera across >=3 LOD rings.
//!      2. Read back tile/draw stats proving the vertex budget stays ~constant as
//!         the world AREA grows 16x (constant vertex budget).
//!      3. Verify the geomorph seam math: the odd edge vertex snaps EXACTLY onto
//!         its coarse neighbour at the hand-off distance (seam delta == 0).
//!      4. Frame-time (CPU selection) within a min-spec budget.
//!    Plus 3 edge cases (flat early-out, streaming coarse fallback, frustum cull)
//!    with explicit before/after state.
//!
//! B. The geomorph shader `cdlod_morph.wgsl` is REAL: it is parsed + validated
//!    through `naga` (wgpu's WGSL frontend) so a broken shader fails this FSV, its
//!    entry points / helper functions are checked against the validated IR, and
//!    its morph math is cross-checked bit-for-bit against the CPU mirror. See
//!    `wgsl_verify`. This does NOT execute the shader on a GPU (headless, no
//!    display), so the rasterized image is NOT verified here -- the report and PR
//!    state that limit explicitly rather than implying a GPU verdict.

use std::time::Instant;

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    math::{Mat4, Vec3},
    render::{
        camera::CameraFrustum,
        terrain::{
            HeightField, TerrainConfig, TerrainRuntime, TileResidency, morph_factor, morph_grid_xz,
            morph_height,
        },
    },
};
use serde_json::{Value, json};

#[path = "issue1739_cdlod_terrain_fsv/wgsl_verify.rs"]
mod wgsl_verify;

const GRID_DIM: u32 = 9; // 81 verts / tile
const LOD0_RANGE: f32 = 6.0;
const MORPH_RATIO: f32 = 0.66;
const FLAT_CUTOFF: f32 = 0.05;
const CAMERA: Vec3 = Vec3::new(0.0, 2.0, 0.0);

fn hilly(leaf: u32, span: u32) -> HeightField {
    let size = leaf * span + 1;
    let mut heights = Vec::with_capacity((size * size) as usize);
    for z in 0..size {
        for x in 0..size {
            let fx = x as f32 * 0.35;
            let fz = z as f32 * 0.35;
            heights.push(5.0 * (fx.sin() + fz.cos()));
        }
    }
    HeightField::new(size, 1.0, heights).expect("valid hilly field")
}

fn cfg(leaf: u32) -> TerrainConfig {
    TerrainConfig::new(leaf, GRID_DIM, LOD0_RANGE, MORPH_RATIO, FLAT_CUTOFF).expect("config")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();

    let camera_walk = camera_walk_report()?;
    let budget = budget_scaling_report()?;
    let seam = geomorph_seam_report();
    let frame_time = frame_time_report()?;
    let edges = edge_cases_report()?;
    // Genuine shader validation (naga parse+validate, IR inspection, CPU cross-
    // check, negative controls). Returns Err -> the example aborts before writing
    // if the shader is broken or its math diverges from the CPU mirror.
    let wgsl = wgsl_verify::report(&cfg(16))?;

    let report = json!({
        "schema_version": 2,
        "issue": 1739,
        "config": {
            "grid_dim": GRID_DIM,
            "tile_vertex_count": GRID_DIM * GRID_DIM,
            "lod0_range": LOD0_RANGE,
            "morph_start_ratio": MORPH_RATIO,
            "flat_error_cutoff": FLAT_CUTOFF,
        },
        "wgsl": wgsl,
        "camera_walk": camera_walk,
        "budget_scaling": budget,
        "geomorph_seam": seam,
        "frame_time": frame_time,
        "edge_cases": edges,
    });

    // Assert on the live values BEFORE persisting (fail loud).
    verify(&report)?;

    let run = begin_issue_run("issue1739_cdlod_terrain")?;
    let readback = run.write_report(&report)?;

    // Independent re-read from disk: the verdict is the persisted bytes, not the
    // in-memory value.
    let persisted = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&persisted)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&persisted) != readback.sha256 {
        return Err("persisted report hash does not match manifest".into());
    }

    println!(
        "wrote issue1739 CDLOD terrain FSV report to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

/// Walk the camera; every stop must span >=3 distinct LOD rings.
fn camera_walk_report() -> Result<Value, Box<dyn std::error::Error>> {
    let field = hilly(16, 4);
    let mut runtime = TerrainRuntime::new(&field, cfg(16))?;
    let stops = [
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(12.0, 2.0, 8.0),
        Vec3::new(-20.0, 2.0, -18.0),
    ];
    let mut walk = Vec::new();
    for pos in stops {
        let frame = runtime.select_frame(pos, None);
        walk.push(json!({
            "camera": [pos.x, pos.y, pos.z],
            "tile_count": frame.stats.tile_count,
            "total_vertices": frame.stats.total_vertices,
            "total_triangles": frame.stats.total_triangles,
            "distinct_lods": frame.stats.distinct_lods,
            "tiles_per_lod": frame.stats.tiles_per_lod,
            "min_center_morph": frame.stats.min_center_morph,
            "max_center_morph": frame.stats.max_center_morph,
        }));
    }
    Ok(Value::Array(walk))
}

/// Grow world area 16x (8x8 -> 32x32 leaves) at a fixed camera; budget must stay
/// bounded while LOD rings increase.
fn budget_scaling_report() -> Result<Value, Box<dyn std::error::Error>> {
    let mut rows = Vec::new();
    let mut first_verts = 0u64;
    for (i, leaf) in [8u32, 16, 32].into_iter().enumerate() {
        let field = hilly(leaf, 4);
        let mut runtime = TerrainRuntime::new(&field, cfg(leaf))?;
        let stats = runtime.select_frame(CAMERA, None).stats;
        if i == 0 {
            first_verts = stats.total_vertices;
        }
        rows.push(json!({
            "leaf_nodes_per_side": leaf,
            "world_area_ratio": (leaf * leaf) / 64,
            "tile_count": stats.tile_count,
            "total_vertices": stats.total_vertices,
            "distinct_lods": stats.distinct_lods,
            "vertex_growth_vs_smallest": stats.total_vertices as f64 / first_verts as f64,
        }));
    }
    Ok(Value::Array(rows))
}

/// The no-popping proof: at the hand-off distance the finer tile's morphed edge
/// vertex must coincide EXACTLY with the coarser grid vertex (positional +
/// height seam delta of 0). Evaluate one representative odd edge vertex.
fn geomorph_seam_report() -> Value {
    let config = cfg(16);
    // LOD 0 morphs toward LOD 1 over its morph region.
    let params = config.morph_params(1);
    let odd_grid = 3u32; // an odd vertex absent from the coarser grid
    let even_neighbor = (odd_grid - 1) as f32;

    // Before: at morph start, full detail (odd vertex at its own position).
    let m_start = morph_factor(params, params.start);
    let before_x = morph_grid_xz(odd_grid, m_start);
    // After: at morph end (hand-off distance), fully collapsed onto the coarse grid.
    let m_end = morph_factor(params, params.end);
    let after_x = morph_grid_xz(odd_grid, m_end);

    let fine_h = 3.0f32;
    let coarse_h = 7.5f32;
    let seam_height = morph_height(fine_h, coarse_h, m_end);

    json!({
        "morph_start_dist": params.start,
        "morph_end_dist": params.end,
        "morph_factor_at_start": m_start,
        "morph_factor_at_end": m_end,
        "odd_vertex_grid": odd_grid,
        "even_neighbor_grid": even_neighbor,
        "before_pos_grid": before_x,
        "after_pos_grid": after_x,
        "seam_pos_delta": (after_x - even_neighbor).abs(),
        "seam_height_at_handoff": seam_height,
        "seam_height_delta_vs_coarse": (seam_height - coarse_h).abs(),
    })
}

/// CPU selection cost across many frames; must fit a min-spec budget.
fn frame_time_report() -> Result<Value, Box<dyn std::error::Error>> {
    const BUDGET_US: f64 = 2000.0; // 2ms of a 16.6ms/60fps frame, min-spec headroom
    let field = hilly(32, 4);
    let mut runtime = TerrainRuntime::new(&field, cfg(32))?;
    // Warm.
    runtime.select_frame(CAMERA, None);
    let iters = 200u32;
    let mut max_us = 0.0f64;
    let start = Instant::now();
    let mut sink = 0u64;
    for i in 0..iters {
        let pos = Vec3::new(
            (i as f32 * 0.13).sin() * 20.0,
            2.0,
            (i as f32 * 0.11).cos() * 20.0,
        );
        let t = Instant::now();
        let frame = runtime.select_frame(pos, None);
        let us = t.elapsed().as_secs_f64() * 1e6;
        max_us = max_us.max(us);
        sink += frame.stats.total_vertices;
    }
    let avg_us = start.elapsed().as_secs_f64() * 1e6 / iters as f64;
    std::hint::black_box(sink);
    Ok(json!({
        "iterations": iters,
        "avg_select_us": avg_us,
        "max_select_us": max_us,
        "budget_us": BUDGET_US,
        "within_budget": avg_us < BUDGET_US,
    }))
}

/// Three edge cases with before/after state.
fn edge_cases_report() -> Result<Value, Box<dyn std::error::Error>> {
    // Edge 1: flat ground collapses to a single coarse tile (adaptive density).
    let flat = HeightField::new(65, 1.0, vec![0.0; 65 * 65])?;
    let flat_tiles = TerrainRuntime::new(&flat, cfg(16))?
        .select_frame(CAMERA, None)
        .stats
        .tile_count;
    let hilly_tiles = TerrainRuntime::new(&hilly(16, 4), cfg(16))?
        .select_frame(CAMERA, None)
        .stats
        .tile_count;

    // Edge 2: bounded atlas -> coarse fallback, then stream a tile in and it
    // resolves. Capacity 2: root pinned (everything finer falls back) plus one
    // slot for the streamed target.
    let field = hilly(16, 4);
    let mut runtime = TerrainRuntime::with_atlas_capacity(&field, cfg(16), 2)?;
    let f1 = runtime.select_frame(CAMERA, None);
    let fallback_before = f1.stats.fallback_count;
    let requests = f1.stats.request_count;
    let target = f1
        .tiles
        .iter()
        .find(|t| matches!(t.residency, TileResidency::Fallback { .. }))
        .map(|t| t.node.id)
        .ok_or("expected a fallback tile with a full atlas")?;
    runtime.stream_in(target);
    let f2 = runtime.select_frame(CAMERA, None);
    let target_resident = f2
        .tiles
        .iter()
        .find(|t| t.node.id == target)
        .map(|t| t.residency == TileResidency::Resident)
        .unwrap_or(false);

    // Edge 3: frustum culling drops off-screen tiles vs the unculled set.
    let mut runtime = TerrainRuntime::new(&field, cfg(16))?;
    let unculled = runtime.select_frame(CAMERA, None).stats.tile_count;
    let eye = Vec3::new(0.0, 30.0, -40.0);
    let view = Mat4::look_at_rh(eye, Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
    let proj = Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.5, 500.0);
    let frustum = CameraFrustum::from_view_projection(proj * view)?;
    let culled = runtime.select_frame(eye, Some(&frustum)).stats.tile_count;

    Ok(json!({
        "flat_early_out": {
            "flat_tile_count": flat_tiles,
            "hilly_tile_count": hilly_tiles,
        },
        "streaming_fallback": {
            "atlas_capacity": 2,
            "fallback_count_before": fallback_before,
            "request_count": requests,
            "streamed_tile_resident_after": target_resident,
        },
        "frustum_cull": {
            "unculled_tile_count": unculled,
            "culled_tile_count": culled,
        },
    }))
}

/// Re-usable assertion set run against BOTH the live and the re-read report.
fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };

    for stop in report["camera_walk"]
        .as_array()
        .ok_or("camera_walk missing")?
    {
        need(
            stop["distinct_lods"].as_u64().unwrap_or(0) >= 3,
            "a camera stop saw <3 LOD rings",
        )?;
        need(
            stop["total_vertices"].as_u64().unwrap_or(0)
                == stop["tile_count"].as_u64().unwrap_or(0) * 81,
            "vertex budget accounting is inconsistent",
        )?;
    }

    let budget = report["budget_scaling"]
        .as_array()
        .ok_or("budget missing")?;
    let biggest = budget.last().ok_or("no budget rows")?;
    need(
        biggest["world_area_ratio"].as_u64().unwrap_or(0) == 16,
        "biggest world should be 16x area",
    )?;
    need(
        biggest["vertex_growth_vs_smallest"]
            .as_f64()
            .unwrap_or(99.0)
            < 3.0,
        "vertex budget grew too fast for a 16x-area world",
    )?;
    need(
        biggest["distinct_lods"].as_u64().unwrap_or(0)
            > budget[0]["distinct_lods"].as_u64().unwrap_or(0),
        "bigger world should add LOD rings",
    )?;

    let seam = &report["geomorph_seam"];
    need(
        seam["morph_factor_at_start"].as_f64().unwrap_or(1.0) == 0.0,
        "morph not 0 at start",
    )?;
    need(
        seam["morph_factor_at_end"].as_f64().unwrap_or(0.0) == 1.0,
        "morph not 1 at hand-off",
    )?;
    need(
        seam["seam_pos_delta"].as_f64().unwrap_or(9.0) == 0.0,
        "geomorph seam has a position gap (popping)",
    )?;
    need(
        seam["seam_height_delta_vs_coarse"].as_f64().unwrap_or(9.0) < 1e-4,
        "geomorph seam height does not match the coarse grid (popping)",
    )?;

    need(
        report["frame_time"]["within_budget"]
            .as_bool()
            .unwrap_or(false),
        "CPU selection exceeded the min-spec frame budget",
    )?;

    let edges = &report["edge_cases"];
    need(
        edges["flat_early_out"]["flat_tile_count"].as_u64() == Some(1),
        "flat ground not one tile",
    )?;
    need(
        edges["flat_early_out"]["hilly_tile_count"]
            .as_u64()
            .unwrap_or(0)
            > 1,
        "hilly ground should refine to many tiles",
    )?;
    need(
        edges["streaming_fallback"]["fallback_count_before"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "no fallback under a full atlas",
    )?;
    need(
        edges["streaming_fallback"]["request_count"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "no stream requests emitted",
    )?;
    need(
        edges["streaming_fallback"]["streamed_tile_resident_after"]
            .as_bool()
            .unwrap_or(false),
        "streamed tile did not resolve",
    )?;
    let unculled = edges["frustum_cull"]["unculled_tile_count"]
        .as_u64()
        .unwrap_or(0);
    let culled = edges["frustum_cull"]["culled_tile_count"]
        .as_u64()
        .unwrap_or(0);
    need(
        culled > 0 && culled < unculled,
        "frustum culling did not reduce the tile set",
    )?;

    // WGSL: the live run already did the real naga parse/validate/cross-check and
    // would have aborted on failure; these re-assert the persisted evidence so a
    // false green cannot survive the disk round-trip.
    let wgsl = &report["wgsl"];
    for key in [
        "naga_parse_ok",
        "naga_validate_ok",
        "grid_dim_from_uniform",
        "arraylength_placeholder_removed",
    ] {
        need(
            wgsl[key].as_bool().unwrap_or(false),
            "CDLOD WGSL structural check failed",
        )?;
    }
    need(
        wgsl["vertex_entry"].as_str() == Some("vs_terrain"),
        "CDLOD WGSL vertex entry point is not vs_terrain",
    )?;
    need(
        wgsl["fragment_entry"].as_str() == Some("fs_terrain"),
        "CDLOD WGSL fragment entry point is not fs_terrain",
    )?;
    need(
        wgsl["morph_factor_fn_args"].as_u64() == Some(3)
            && wgsl["snap_fn_args"].as_u64() == Some(2),
        "CDLOD WGSL morph helpers have unexpected signatures",
    )?;
    let cross = &wgsl["cpu_wgsl_crosscheck"];
    need(
        cross["samples"].as_u64().unwrap_or(0) > 0,
        "CDLOD WGSL cross-check ran no samples",
    )?;
    need(
        cross["max_morph_factor_delta"].as_f64() == Some(0.0)
            && cross["max_snap_delta"].as_f64() == Some(0.0),
        "CDLOD WGSL morph math diverged from the CPU mirror",
    )?;
    need(
        wgsl["negative_controls"]["syntax_error_rejected"]
            .as_bool()
            .unwrap_or(false)
            && wgsl["negative_controls"]["type_error_rejected"]
                .as_bool()
                .unwrap_or(false),
        "CDLOD WGSL naga gate failed to reject a broken shader",
    )?;
    Ok(())
}
