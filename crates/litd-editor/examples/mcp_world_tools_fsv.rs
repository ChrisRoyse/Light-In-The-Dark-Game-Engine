use std::fs;

use litd_editor::mcp_world_tools::{
    BULK_EDIT_TERRAIN_TOOL, PAINT_TILES_TOOL, PLACE_ACTOR_REGION_CAMERA_TOOL, READ_WORLD_TOOL,
    SET_START_LOCATIONS_TOOL, WorldStartLocation, call_mcp_world_tool, read_mcp_world_document,
    sample_issue918_document,
};
use litd_engine::math::{Transform, Vec3};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use litd_editor::{
    history::{EditorObject, EditorRegion},
    play::{PlayCameraConfig, PlayCameraRig},
    terrain_advanced_tools::{
        TerrainBrushFalloff, TerrainBrushOperation, TerrainBrushShape, TerrainBrushSpec,
    },
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue918Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_WORLD_TOOLS_FSV_ROOT",
        "issue918_mcp_world_tools",
        "manual",
    )?;

    let document_path = root.join("world-document.json");
    let paint_ledger = root.join("paint-ledger.json");
    let bulk_ledger = root.join("bulk-ledger.json");
    let place_ledger = root.join("place-ledger.json");
    let starts_ledger = root.join("starts-ledger.json");
    let report_path = root.join("mcp-world-tools-fsv-readback.json");
    let before = state(&document_path, &paint_ledger);

    let paint = call_mcp_world_tool(
        PAINT_TILES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": paint_ledger,
            "ledger_id": "issue918-paint-ledger",
            "request_id": "issue918-paint",
            "requested_by": "litd-editor-issue918-fsv",
            "initial_document": sample_issue918_document(),
            "brush": paint_brush(),
            "pathing": {
                "cell_ids": ["terrain_cell_1_1", "terrain_cell_1_2"],
                "mode": "Blocked"
            }
        }),
    )?;
    let bulk = call_mcp_world_tool(
        BULK_EDIT_TERRAIN_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": bulk_ledger,
            "ledger_id": "issue918-bulk-ledger",
            "request_id": "issue918-bulk",
            "requested_by": "litd-editor-issue918-fsv",
            "initial_document": null,
            "edit": { "AdjustCliffLevels": { "delta": 1, "scope": "WholeMap" } }
        }),
    )?;
    let place = call_mcp_world_tool(
        PLACE_ACTOR_REGION_CAMERA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": place_ledger,
            "ledger_id": "issue918-place-ledger",
            "request_id": "issue918-place",
            "requested_by": "litd-editor-issue918-fsv",
            "initial_document": null,
            "objects": [sample_doodad()],
            "regions": [sample_region()],
            "camera": PlayCameraConfig::preset(PlayCameraRig::TopDown)
        }),
    )?;
    let starts = call_mcp_world_tool(
        SET_START_LOCATIONS_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": starts_ledger,
            "ledger_id": "issue918-starts-ledger",
            "request_id": "issue918-starts",
            "requested_by": "litd-editor-issue918-fsv",
            "initial_document": null,
            "starts": sample_starts()
        }),
    )?;
    let read = call_mcp_world_tool(READ_WORLD_TOOL, json!({ "document_path": document_path }))?;
    let after = state(&document_path, &paint_ledger);
    let edges = vec![
        edge_empty_brush(&root, &document_path)?,
        edge_unknown_tool(&document_path, &paint_ledger),
        edge_missing_document(&root)?,
    ];

    let fsv = Issue918Fsv {
        issue: 918,
        source_of_truth: json!({
            "document": document_path,
            "paint_ledger": paint_ledger,
            "bulk_ledger": bulk_ledger,
            "place_ledger": place_ledger,
            "starts_ledger": starts_ledger
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "paint_tiles": paint,
            "bulk_edit_terrain": bulk,
            "place_actor_region_camera": place,
            "set_start_locations": starts,
            "read_world": read
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    let readback = read_json::<Issue918Fsv>(&report_path)?;
    if readback != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    // document_path and every *_ledger path are Calyx Aster VAULT DIRECTORIES, not files
    // (the world tools persist via write_calyx_json_bytes(vault_dir: path)). Hashing them
    // with fs::read would open a directory, which on Windows fails with `Os error 5,
    // PermissionDenied` (#1529). Use the SHA-256 the store computed over the canonical
    // stored bytes — the real source of truth — read back through the store instead.
    let document_sha256 = read_mcp_world_document(&document_path)?.sha256;
    println!(
        "issue918 mcp_world_tools_fsv report={} report_sha256={} document_sha256={} paint_ledger_sha256={} bulk_ledger_sha256={} place_ledger_sha256={} starts_ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        document_sha256,
        ledger_sha256(&paint, "paint")?,
        ledger_sha256(&bulk, "bulk")?,
        ledger_sha256(&place, "place")?,
        ledger_sha256(&starts, "starts")?,
    );
    Ok(())
}

/// Pull the store-computed ledger SHA-256 out of a tool call output. The ledger, like the
/// world document, lives in a Calyx Aster vault directory, so the canonical hash is the
/// one the store returned over the stored bytes — never `fs::read` of the vault path.
/// Fails loud (naming which tool) if a mutating call returned no ledger readback.
fn ledger_sha256(
    output: &litd_editor::mcp_world_tools::McpWorldToolCallOutput,
    label: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(output
        .ledger_readback
        .as_ref()
        .ok_or_else(|| format!("{label} tool call returned no ledger readback"))?
        .sha256
        .clone())
}

fn edge_empty_brush(
    root: &std::path::Path,
    document_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-empty-brush-ledger.json");
    let before = state(document_path, &ledger);
    let error = call_mcp_world_tool(
        PAINT_TILES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger,
            "ledger_id": "issue918-empty-brush",
            "request_id": "issue918-empty-brush",
            "requested_by": "litd-editor-issue918-fsv",
            "initial_document": null,
            "brush": empty_brush(),
            "pathing": null
        }),
    )
    .unwrap_err();
    let after = state(document_path, &ledger);
    Ok(json!({
        "name": "empty_brush",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn edge_unknown_tool(document_path: &std::path::Path, ledger: &std::path::Path) -> Value {
    let before = state(document_path, ledger);
    let error = call_mcp_world_tool("litd.editor.world.not_registered", json!({})).unwrap_err();
    let after = state(document_path, ledger);
    json!({
        "name": "unknown_tool",
        "before": before,
        "after": after,
        "error": error.to_string()
    })
}

fn edge_missing_document(root: &std::path::Path) -> Result<Value, Box<dyn std::error::Error>> {
    let document = root.join("missing-document.json");
    let ledger = root.join("missing-ledger.json");
    let before = state(&document, &ledger);
    let error = call_mcp_world_tool(
        SET_START_LOCATIONS_TOOL,
        json!({
            "document_path": document,
            "ledger_path": ledger,
            "ledger_id": "issue918-missing",
            "request_id": "issue918-missing",
            "requested_by": "litd-editor-issue918-fsv",
            "initial_document": null,
            "starts": sample_starts()
        }),
    )
    .unwrap_err();
    let after = state(&document, &ledger);
    Ok(json!({
        "name": "missing_document",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
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

// 3.14 below is a deliberate fixture yaw value, not an approximation of PI; replacing it with
// std::f32::consts::PI would change the persisted/hashed FSV fingerprint. Lint suppressed.
#[allow(clippy::approx_constant)]
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
            yaw_radians: 3.14,
        },
    ]
}

fn state(document_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
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

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    Ok(serde_json::from_slice(&serde_json::to_vec_pretty(value)?)?)
}
