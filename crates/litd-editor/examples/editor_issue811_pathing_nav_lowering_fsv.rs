//! FSV for #811: pathing (blockers, walkable bridges, per-doodad footprints)
//! lowered into the litd-engine nav bake + deterministic A*.
//!
//! Vertical slice: author blockers / a walkable bridge / a doodad footprint with the
//! real pathing + placement capabilities -> persist the world through the REAL Calyx
//! world-record path (Aster) -> read the stored bytes back and parse them -> lower
//! THAT readback document into a `NavBakeInput` -> `bake_navmesh` -> query the frozen
//! A* (`find_path`) and read the route. Verdict is re-read from disk and re-asserted.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    terrain_authoring::{
        TerrainObjectPlacementRequest, TerrainPlacementKind, place_terrain_object,
    },
    terrain_lowering::lower_nav_bake_input,
    terrain_pathing::{
        TerrainPathingFlags, TerrainPathingPaintMode, paint_pathing_cells, set_global_tile_pathing,
    },
};
use litd_engine::{
    fsv_artifacts::begin_issue_run,
    nav::{NavCellCoord, bake_navmesh, find_path},
};
use serde_json::{Value, json};

#[path = "support/terrain_lowering_support.rs"]
mod support;

const CLIFF_STEP: f32 = 4.0;
const DIM: u32 = 6;

/// Block the z=3 wall except a walkable bridge at x=0, and place a doodad footprint.
fn author_pathing(document: &mut EditorDocument) -> Result<(), Box<dyn std::error::Error>> {
    let wall: Vec<String> = (1..DIM as i32)
        .map(|x| format!("terrain_cell_{x}_3"))
        .collect();
    paint_pathing_cells(document, wall, TerrainPathingPaintMode::Blocked)?;
    place_terrain_object(
        document,
        &TerrainObjectPlacementRequest {
            object_id: "doodad.boulder.1".to_string(),
            kind: TerrainPlacementKind::Doodad,
            type_id: "doodad.boulder".to_string(),
            target_tile_x: 0,
            target_tile_z: 1,
            yaw_degrees: 0.0,
            clamp_to_playable_bounds: false,
            reject_unwalkable_ground: false,
            owner: None,
        },
    )?;
    Ok(())
}

fn route(
    document: &EditorDocument,
    from: (i32, i32),
    to: (i32, i32),
) -> Result<Value, Box<dyn std::error::Error>> {
    let lowered = lower_nav_bake_input(document, CLIFF_STEP)?;
    let navmesh = bake_navmesh(&lowered.input)?;
    match find_path(
        &navmesh,
        NavCellCoord::new(from.0, from.1),
        NavCellCoord::new(to.0, to.1),
    ) {
        Ok(path) => {
            let cells: Vec<[i32; 2]> = path.nodes.iter().map(|n| [n.x, n.z]).collect();
            Ok(json!({
                "found": true,
                "cells": cells,
                "crosses_bridge_x0_z3": path.nodes.iter().any(|n| n.x == 0 && n.z == 3),
                "avoids_doodad_0_1": !path.nodes.iter().any(|n| n.x == 0 && n.z == 1),
            }))
        }
        Err(err) => Ok(json!({"found": false, "error": err.to_string()})),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue811_pathing_nav_lowering")?;
    let aster_dir = run.artifact_dir("aster")?;

    // Author -> persist -> read back -> parse the STORED bytes.
    let mut record = support::world_record("issue811-pathing", DIM);
    author_pathing(&mut record.document)?;
    support::sync(&mut record, "blocked_wall_bridge_doodad")?;
    let persisted = support::persist_and_readback(&aster_dir, &record)?;

    let lowered = lower_nav_bake_input(&persisted.document, CLIFF_STEP)?;
    let navmesh = bake_navmesh(&lowered.input)?;
    let doodad_cell = navmesh
        .cells
        .iter()
        .find(|c| c.x == 0 && c.z == 1)
        .ok_or("doodad cell missing from navmesh")?;
    let happy_route = route(&persisted.document, (0, 0), (0, DIM as i32 - 1))?;

    // Edge 1: seal the bridge too -> the far side becomes unreachable.
    let mut walled = record.clone();
    paint_pathing_cells(
        &mut walled.document,
        ["terrain_cell_0_3".to_string()],
        TerrainPathingPaintMode::Blocked,
    )?;
    let walled_route = route(&walled.document, (0, 0), (0, DIM as i32 - 1))?;

    // Edge 2: a global tile-pathing rule blocks a whole tile; a per-cell walkable
    // override reopens exactly one crossing (the classic walkable bridge over water).
    let bridge_before;
    let bridge_after;
    {
        let mut water = support::world_record("issue811-bridge", DIM);
        for x in 0..DIM as i32 {
            water
                .document
                .terrain_cells
                .get_mut(&format!("terrain_cell_{x}_3"))
                .unwrap()
                .tile_id = "tile.water.deep".to_string();
        }
        set_global_tile_pathing(
            &mut water.document,
            "tile.water.deep",
            TerrainPathingFlags::blocked(),
        )?;
        bridge_before = route(&water.document, (0, 0), (0, DIM as i32 - 1))?;
        paint_pathing_cells(
            &mut water.document,
            ["terrain_cell_2_3".to_string()],
            TerrainPathingPaintMode::Walkable,
        )?;
        bridge_after = route(&water.document, (0, 0), (0, DIM as i32 - 1))?;
    }

    let report = json!({
        "schema": "litd.issue811.pathing_nav_lowering.v1",
        "issue": 811,
        "source_of_truth": "Aster world-record readback -> lower_nav_bake_input -> bake_navmesh -> A* find_path route",
        "persist": {
            "aster_sha256": persisted.readback.sha256,
            "document_fingerprint": persisted.readback.document_fingerprint,
        },
        "happy_path": {
            "lowered_schema": lowered.schema,
            "lowered_fingerprint": lowered.fingerprint,
            "cell_count": lowered.cell_count,
            "blocked_count": lowered.blocked_count,
            "blocker_count": lowered.blocker_count,
            "navmesh_fnv1a64": navmesh.fnv1a64,
            "doodad_cell_blocker_ids": doodad_cell.static_blocker_ids,
            "route": happy_route,
        },
        "edges": {
            "sealed_bridge_unreachable": {
                "before": happy_route,
                "after": walled_route,
            },
            "walkable_bridge_over_blocked_tile": {
                "before_all_water_blocked": bridge_before,
                "after_bridge_override": bridge_after,
            },
            "doodad_footprint_blocker": {
                "blocker_count": lowered.blocker_count,
                "doodad_cell_carries_blocker": !doodad_cell.static_blocker_ids.is_empty(),
                "route_avoids_doodad": happy_route["avoids_doodad_0_1"].as_bool(),
            },
        },
    });

    verify(&report)?;
    let readback = run.write_report(&report)?;
    let bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&bytes) != readback.sha256 {
        return Err("persisted #811 report hash mismatch".into());
    }
    println!(
        "wrote #811 pathing nav lowering FSV to {}",
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
        happy["blocked_count"].as_u64() == Some(5),
        "expected 5 blocked wall cells",
    )?;
    need(
        happy["blocker_count"].as_u64() == Some(1),
        "expected 1 doodad footprint blocker",
    )?;
    need(
        happy["route"]["found"].as_bool().unwrap_or(false),
        "A* found no route across the bridge",
    )?;
    need(
        happy["route"]["crosses_bridge_x0_z3"]
            .as_bool()
            .unwrap_or(false),
        "route did not use the x=0 bridge",
    )?;
    let edges = &report["edges"];
    need(
        edges["sealed_bridge_unreachable"]["before"]["found"]
            .as_bool()
            .unwrap_or(false),
        "bridge route should exist before sealing",
    )?;
    need(
        !edges["sealed_bridge_unreachable"]["after"]["found"]
            .as_bool()
            .unwrap_or(true),
        "sealed bridge should be unreachable",
    )?;
    need(
        !edges["walkable_bridge_over_blocked_tile"]["before_all_water_blocked"]["found"]
            .as_bool()
            .unwrap_or(true),
        "fully blocked water should have no route",
    )?;
    need(
        edges["walkable_bridge_over_blocked_tile"]["after_bridge_override"]["found"]
            .as_bool()
            .unwrap_or(false),
        "walkable override should reopen a crossing",
    )?;
    let doodad = &edges["doodad_footprint_blocker"];
    need(
        doodad["doodad_cell_carries_blocker"]
            .as_bool()
            .unwrap_or(false),
        "doodad cell must carry its static blocker id",
    )?;
    need(
        doodad["route_avoids_doodad"].as_bool() == Some(true),
        "route must avoid the doodad footprint",
    )?;
    Ok(())
}
