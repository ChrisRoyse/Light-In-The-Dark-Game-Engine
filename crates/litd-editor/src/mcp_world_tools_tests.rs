use std::{fs, path::Path};

use litd_engine::math::{Transform, Vec3};
use serde_json::json;

use crate::{
    history::{EditorObject, EditorRegion},
    play::{PlayCameraConfig, PlayCameraRig},
    terrain_advanced_tools::{
        TerrainBrushFalloff, TerrainBrushOperation, TerrainBrushShape, TerrainBrushSpec,
    },
};

use super::*;

#[test]
fn issue918_world_mcp_tools_build_edit_and_read_real_document() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue918_mcp_world_tools", "unit").unwrap();
    reset_root(&root);
    let document_path = root.join("world-document.json");
    let paint_ledger = root.join("paint-ledger.json");
    let bulk_ledger = root.join("bulk-ledger.json");
    let place_ledger = root.join("place-ledger.json");
    let starts_ledger = root.join("starts-ledger.json");
    let before = state(&document_path, &paint_ledger);
    println!("issue918 before {before}");

    let paint = call_mcp_world_tool(
        PAINT_TILES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": paint_ledger,
            "ledger_id": "issue918-paint-ledger",
            "request_id": "issue918-paint",
            "requested_by": "litd-editor-issue918-test",
            "initial_document": sample_issue918_document(),
            "brush": paint_brush(),
            "pathing": {
                "cell_ids": ["terrain_cell_1_1", "terrain_cell_1_2"],
                "mode": "Blocked"
            }
        }),
    )
    .unwrap();
    assert_eq!(paint.tool_name, PAINT_TILES_TOOL);
    let painted = read_mcp_world_document(&document_path).unwrap();
    assert_eq!(
        painted.document.terrain_cells["terrain_cell_1_1"].tile_id,
        "tile.ashen_path"
    );
    assert_eq!(painted.pathing_override_count, 2);

    let bulk = call_mcp_world_tool(
        BULK_EDIT_TERRAIN_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": bulk_ledger,
            "ledger_id": "issue918-bulk-ledger",
            "request_id": "issue918-bulk",
            "requested_by": "litd-editor-issue918-test",
            "initial_document": null,
            "edit": { "AdjustCliffLevels": { "delta": 1, "scope": "WholeMap" } }
        }),
    )
    .unwrap();
    assert_eq!(bulk.tool_name, BULK_EDIT_TERRAIN_TOOL);

    let place = call_mcp_world_tool(
        PLACE_ACTOR_REGION_CAMERA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": place_ledger,
            "ledger_id": "issue918-place-ledger",
            "request_id": "issue918-place",
            "requested_by": "litd-editor-issue918-test",
            "initial_document": null,
            "objects": [sample_doodad()],
            "regions": [sample_region()],
            "camera": PlayCameraConfig::preset(PlayCameraRig::TopDown)
        }),
    )
    .unwrap();
    assert_eq!(place.tool_name, PLACE_ACTOR_REGION_CAMERA_TOOL);

    let starts = call_mcp_world_tool(
        SET_START_LOCATIONS_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": starts_ledger,
            "ledger_id": "issue918-starts-ledger",
            "request_id": "issue918-starts",
            "requested_by": "litd-editor-issue918-test",
            "initial_document": null,
            "starts": sample_starts()
        }),
    )
    .unwrap();
    assert_eq!(starts.tool_name, SET_START_LOCATIONS_TOOL);

    let read =
        call_mcp_world_tool(READ_WORLD_TOOL, json!({ "document_path": document_path })).unwrap();
    assert_eq!(read.result["document_readback"]["terrain_cell_count"], 16);
    assert_eq!(
        read.result["document_readback"]["pathing_override_count"],
        2
    );
    assert_eq!(read.result["document_readback"]["start_location_count"], 2);
    let after = state(&document_path, &paint_ledger);
    println!("issue918 after {after}");
    assert_eq!(after["document_exists"], true);
    assert_eq!(after["terrain_cell_count"], 16);
    assert_eq!(after["start_location_count"], 2);
    assert_eq!(after["ledger_exists"], true);
}

#[test]
fn issue918_world_mcp_tools_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue918_mcp_world_tools", "edges").unwrap();
    reset_root(&root);
    let document_path = root.join("world-document.json");
    seed_document(&document_path, &root);
    let edge_ledger = root.join("edge-ledger.json");

    let before_empty = state(&document_path, &edge_ledger);
    let empty = call_mcp_world_tool(
        PAINT_TILES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue918-empty-brush",
            "request_id": "issue918-empty-brush",
            "requested_by": "litd-editor-issue918-test",
            "initial_document": null,
            "brush": empty_brush(),
            "pathing": null
        }),
    )
    .unwrap_err();
    let after_empty = state(&document_path, &edge_ledger);
    println!("issue918 empty before={before_empty} after={after_empty} error={empty}");
    assert_eq!(before_empty, after_empty);
    assert!(empty.to_string().contains("brush.matches"));

    let before_unknown = state(&document_path, &edge_ledger);
    let unknown = call_mcp_world_tool("litd.editor.world.not_registered", json!({})).unwrap_err();
    let after_unknown = state(&document_path, &edge_ledger);
    println!("issue918 unknown before={before_unknown} after={after_unknown} error={unknown}");
    assert_eq!(before_unknown, after_unknown);
    assert!(
        unknown
            .to_string()
            .contains("editor.mcp_world_tools.tool_name")
    );

    let missing_path = root.join("missing-document.json");
    let missing_ledger = root.join("missing-ledger.json");
    let before_missing = state(&missing_path, &missing_ledger);
    let missing = call_mcp_world_tool(
        SET_START_LOCATIONS_TOOL,
        json!({
            "document_path": missing_path,
            "ledger_path": missing_ledger,
            "ledger_id": "issue918-missing",
            "request_id": "issue918-missing",
            "requested_by": "litd-editor-issue918-test",
            "initial_document": null,
            "starts": sample_starts()
        }),
    )
    .unwrap_err();
    let after_missing = state(&missing_path, &missing_ledger);
    println!("issue918 missing before={before_missing} after={after_missing} error={missing}");
    assert_eq!(before_missing, after_missing);
    assert!(missing.to_string().contains("document.missing"));
}

fn seed_document(document_path: &Path, root: &Path) {
    call_mcp_world_tool(
        PAINT_TILES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": root.join("seed-ledger.json"),
            "ledger_id": "issue918-seed",
            "request_id": "issue918-seed",
            "requested_by": "litd-editor-issue918-test",
            "initial_document": sample_issue918_document(),
            "brush": paint_brush(),
            "pathing": null
        }),
    )
    .unwrap();
}

fn paint_brush() -> TerrainBrushSpec {
    TerrainBrushSpec {
        center_tile_x: 1,
        center_tile_z: 1,
        size_tiles: 2,
        shape: TerrainBrushShape::Square,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::PaintTile {
            tile_id: "tile.ashen_path".to_string(),
        },
    }
}

fn empty_brush() -> TerrainBrushSpec {
    TerrainBrushSpec {
        center_tile_x: 99,
        center_tile_z: 99,
        size_tiles: 1,
        shape: TerrainBrushShape::Square,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::PaintTile {
            tile_id: "tile.void".to_string(),
        },
    }
}

fn sample_doodad() -> EditorObject {
    EditorObject::new(
        "doodad.issue918.obelisk",
        "Doodad",
        None,
        Transform::from_translation(Vec3::new(2.0, 0.0, 2.0)),
    )
    .with_property("type_id", "doodad.obelisk")
}

fn sample_region() -> EditorRegion {
    EditorRegion::new(
        "region.issue918.center",
        "Issue 918 Center",
        Vec3::new(2.0, 0.0, 2.0),
        Vec3::new(1.0, 4.0, 1.0),
    )
}

fn sample_starts() -> Vec<WorldStartLocation> {
    vec![
        WorldStartLocation {
            slot: 0,
            player_id: "player_1".to_string(),
            object_id: "start.issue918.player_1".to_string(),
            position: Vec3::new(0.5, 0.0, 0.5),
            yaw_radians: 0.0,
        },
        WorldStartLocation {
            slot: 1,
            player_id: "player_2".to_string(),
            object_id: "start.issue918.player_2".to_string(),
            position: Vec3::new(3.5, 0.0, 3.5),
            yaw_radians: std::f32::consts::PI,
        },
    ]
}

fn state(document_path: &Path, ledger_path: &Path) -> serde_json::Value {
    let readback = if document_path.exists() {
        Some(read_mcp_world_document(document_path).unwrap())
    } else {
        None
    };
    json!({
        "document_exists": document_path.exists(),
        "document_bytes": fs::metadata(document_path).map(|metadata| metadata.len()).unwrap_or(0),
        "terrain_cell_count": readback.as_ref().map(|readback| readback.terrain_cell_count).unwrap_or(0),
        "object_count": readback.as_ref().map(|readback| readback.object_count).unwrap_or(0),
        "region_count": readback.as_ref().map(|readback| readback.region_count).unwrap_or(0),
        "pathing_override_count": readback.as_ref().map(|readback| readback.pathing_override_count).unwrap_or(0),
        "start_location_count": readback.as_ref().map(|readback| readback.start_location_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
