//! FSV for #812 (region enter/leave events), #813 (named cameras), #817 (start locations).
//! Drives the REAL editor world MCP tools against a Calyx Aster vault, reads the stored bytes
//! back, asserts the happy-path readback, and records a per-issue verdict artifact (happy path
//! plus three fail-loud/determinism edge cases each).

use std::fs;

use litd_editor::history::{EditorCameraKind, EditorDocument, EditorNamedCamera, EditorRegion};
use litd_editor::mcp_world_tools::{
    EVALUATE_REGION_TRANSITIONS_TOOL, PAINT_TILES_TOOL, PLACE_ACTOR_REGION_CAMERA_TOOL,
    READ_WORLD_TOOL, SET_START_LOCATIONS_TOOL, WorldStartLocation, call_mcp_world_tool,
    read_mcp_world_document, sample_issue918_document,
};
use litd_editor::play::{PlayCameraConfig, PlayCameraRig};
use litd_editor::region_transitions::RegionTransitionReport;
use litd_editor::terrain_advanced_tools::{
    TerrainBrushFalloff, TerrainBrushOperation, TerrainBrushShape, TerrainBrushSpec,
};
use litd_engine::math::Vec3;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Fsv {
    issues: Vec<u32>,
    document: String,
    issue817_starts: IssueBlock,
    issue813_named_cameras: IssueBlock,
    issue812_region_events: IssueBlock,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct IssueBlock {
    issue: u32,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_REGIONS_CAMERAS_STARTS_FSV_ROOT",
        "issue812_813_817_regions_cameras_starts",
        "manual",
    )?;
    let doc = root.join("world-document.json");
    let report_path = root.join("regions-cameras-starts-fsv-readback.json");

    // Base world: paint tile (1,1) BLOCKED so #817 can prove an unpathable start fails.
    call_mcp_world_tool(
        PAINT_TILES_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": root.join("paint-ledger.json"),
            "ledger_id": "regions-cameras-starts-paint",
            "request_id": "base-paint",
            "requested_by": "issue812-813-817-fsv",
            "initial_document": sample_issue918_document(),
            "brush": path_brush(),
            "pathing": { "cell_ids": ["terrain_cell_1_1"], "mode": "Blocked" }
        }),
    )?;

    let starts = issue817_starts(&root, &doc)?;
    let cameras = issue813_named_cameras(&root, &doc)?;
    let events = issue812_region_events(&root, &doc)?;

    let fsv = Fsv {
        issues: vec![812, 813, 817],
        document: doc.display().to_string(),
        issue817_starts: starts,
        issue813_named_cameras: cameras,
        issue812_region_events: events,
    };
    let fsv = roundtrip(&fsv)?;
    fs::write(&report_path, serde_json::to_vec_pretty(&fsv)?)?;
    let readback: Fsv = serde_json::from_slice(&fs::read(&report_path)?)?;
    if readback != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let recorded = fsv_run.record_json_file(&report_path)?;
    let document_sha256 = read_mcp_world_document(&doc)?.sha256;
    println!(
        "issue812_813_817 regions_cameras_starts_fsv report={} report_sha256={} document_sha256={}",
        report_path.display(),
        recorded.sha256,
        document_sha256,
    );
    Ok(())
}

fn issue817_starts(
    root: &std::path::Path,
    doc: &std::path::Path,
) -> Result<IssueBlock, Box<dyn std::error::Error>> {
    // Happy: two valid starts on walkable ground, contiguous slots 0/1.
    let happy = call_mcp_world_tool(
        SET_START_LOCATIONS_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": root.join("starts-ledger.json"),
            "ledger_id": "regions-cameras-starts-starts",
            "request_id": "starts-happy",
            "requested_by": "issue817-fsv",
            "initial_document": null,
            "starts": [
                start(0, "player_1", "start.p1", Vec3::new(0.5, 0.0, 0.5)),
                start(1, "player_2", "start.p2", Vec3::new(3.5, 0.0, 3.5)),
            ]
        }),
    )?;
    // FSV: read the stored bytes back; prove the source of truth holds our two starts, slots 0/1.
    let document = read_document(doc)?;
    let start_slots = start_location_slots(&document);
    assert_eq!(
        start_slots,
        vec![
            (0, "player_1".to_string(), "start.p1".to_string()),
            (1, "player_2".to_string(), "start.p2".to_string()),
        ],
        "issue817 start readback mismatch"
    );

    let edges = vec![
        start_edge(
            root,
            doc,
            "off_map_bounds",
            vec![start(0, "player_1", "start.p1", Vec3::new(9.5, 0.0, 9.5))],
        ),
        start_edge(
            root,
            doc,
            "unpathable_blocked_cell",
            vec![start(0, "player_1", "start.p1", Vec3::new(1.5, 0.0, 1.5))],
        ),
        start_edge(
            root,
            doc,
            "duplicate_slot",
            vec![
                start(0, "player_1", "start.p1", Vec3::new(0.5, 0.0, 0.5)),
                start(0, "player_2", "start.p2", Vec3::new(3.5, 0.0, 3.5)),
            ],
        ),
    ];

    Ok(IssueBlock {
        issue: 817,
        // `happy` embeds the readback (StartLocations) as evidence; the assert_eq above enforces it.
        happy_path: json!({ "set_start_locations": happy }),
        edges,
    })
}

fn start_edge(
    root: &std::path::Path,
    doc: &std::path::Path,
    name: &str,
    starts: Vec<Value>,
) -> Value {
    let before = document_state(doc);
    let error = call_mcp_world_tool(
        SET_START_LOCATIONS_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": root.join(format!("starts-edge-{name}-ledger.json")),
            "ledger_id": format!("starts-edge-{name}"),
            "request_id": format!("starts-edge-{name}"),
            "requested_by": "issue817-fsv",
            "initial_document": null,
            "starts": starts
        }),
    )
    .unwrap_err();
    let after = document_state(doc);
    json!({ "name": name, "before": before, "after": after, "error": error.to_string() })
}

fn start(slot: u32, player: &str, object: &str, position: Vec3) -> Value {
    json!(WorldStartLocation {
        slot,
        player_id: player.to_string(),
        object_id: object.to_string(),
        position,
        yaw_radians: 0.0,
    })
}

fn issue813_named_cameras(
    root: &std::path::Path,
    doc: &std::path::Path,
) -> Result<IssueBlock, Box<dyn std::error::Error>> {
    // Happy: a top-down default camera plus a cinematic framed on the LAST playable
    // edge tile (3,3) of the 4x4 world -- "camera at map edge" is accepted (inclusive).
    let happy = call_mcp_world_tool(
        PLACE_ACTOR_REGION_CAMERA_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": root.join("cameras-ledger.json"),
            "ledger_id": "regions-cameras-starts-cameras",
            "request_id": "cameras-happy",
            "requested_by": "issue813-fsv",
            "initial_document": null,
            "objects": [],
            "regions": [],
            "camera": null,
            "named_cameras": [
                json!(EditorNamedCamera::top_down(
                    "camera.overview",
                    "Overview",
                    Vec3::new(2.0, 0.0, 2.0)
                )),
                json!(EditorNamedCamera::cinematic(
                    "camera.finale",
                    "Finale",
                    PlayCameraConfig::preset(PlayCameraRig::Isometric),
                    Vec3::new(3.5, 0.0, 3.5)
                )),
            ]
        }),
    )?;
    // FSV: read the stored bytes back; prove the two named cameras exist with expected name+kind.
    let document = read_document(doc)?;
    let cameras = &document.named_cameras;
    let overview = &cameras["camera.overview"];
    assert_eq!(
        (overview.name.as_str(), overview.kind),
        ("Overview", EditorCameraKind::TopDown)
    );
    let finale = &cameras["camera.finale"];
    assert_eq!(
        (finale.name.as_str(), finale.kind),
        ("Finale", EditorCameraKind::Cinematic)
    );
    assert_eq!(cameras.len(), 2, "issue813 named camera count");

    let edges = vec![
        camera_edge(
            root,
            doc,
            "look_at_past_map_edge",
            EditorNamedCamera::cinematic(
                "camera.offmap",
                "Offmap",
                PlayCameraConfig::preset(PlayCameraRig::TopDown),
                Vec3::new(4.5, 0.0, 2.0),
            ),
        ),
        camera_edge(
            root,
            doc,
            "invalid_config_zero_distance",
            zero_distance_camera(),
        ),
        camera_edge(
            root,
            doc,
            "empty_name",
            EditorNamedCamera::top_down("camera.blank", "", Vec3::new(2.0, 0.0, 2.0)),
        ),
    ];

    Ok(IssueBlock {
        issue: 813,
        // `happy` embeds the readback (both cameras) as evidence; the asserts above enforce it.
        happy_path: json!({ "place_named_cameras": happy }),
        edges,
    })
}

fn camera_edge(
    root: &std::path::Path,
    doc: &std::path::Path,
    name: &str,
    camera: EditorNamedCamera,
) -> Value {
    let before = document_state(doc);
    let error = call_mcp_world_tool(
        PLACE_ACTOR_REGION_CAMERA_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": root.join(format!("cameras-edge-{name}-ledger.json")),
            "ledger_id": format!("cameras-edge-{name}"),
            "request_id": format!("cameras-edge-{name}"),
            "requested_by": "issue813-fsv",
            "initial_document": null,
            "objects": [],
            "regions": [],
            "camera": null,
            "named_cameras": [json!(camera)]
        }),
    )
    .unwrap_err();
    let after = document_state(doc);
    json!({ "name": name, "before": before, "after": after, "error": error.to_string() })
}

fn zero_distance_camera() -> EditorNamedCamera {
    let mut config = PlayCameraConfig::preset(PlayCameraRig::TopDown);
    config.distance = 0.0;
    EditorNamedCamera::cinematic("camera.bad", "Bad", config, Vec3::new(2.0, 0.0, 2.0))
}

fn issue812_region_events(
    root: &std::path::Path,
    doc: &std::path::Path,
) -> Result<IssueBlock, Box<dyn std::error::Error>> {
    // Place two OVERLAPPING regions (region.a, region.b). A move into their overlap emits THREE
    // ENTER events sorted by id: those two PLUS the world_creation.rs auto-injected playable_bounds
    // region (it contains the overlap too). The assertions below encode all three explicitly.
    call_mcp_world_tool(
        PLACE_ACTOR_REGION_CAMERA_TOOL,
        json!({
            "document_path": doc,
            "ledger_path": root.join("regions-ledger.json"),
            "ledger_id": "regions-cameras-starts-regions",
            "request_id": "regions-place",
            "requested_by": "issue812-fsv",
            "initial_document": null,
            "objects": [],
            "regions": [
                json!(EditorRegion::new(
                    "region.a",
                    "Region A",
                    Vec3::new(2.0, 0.0, 2.0),
                    Vec3::new(1.5, 4.0, 1.5)
                )),
                json!(EditorRegion::new(
                    "region.b",
                    "Region B",
                    Vec3::new(2.0, 0.0, 2.0),
                    Vec3::new(1.0, 4.0, 1.0)
                )),
            ],
            "camera": null,
            "named_cameras": []
        }),
    )?;

    // Happy: outside every region -> the overlap: exactly three ordered ENTER events.
    let happy = transitions(doc, Vec3::new(9.0, 0.0, 9.0), Vec3::new(2.0, 0.0, 2.0))?;
    let happy_events = region_events(&happy)?;
    assert_events(
        &happy_events,
        &["Enter:playable_bounds", "Enter:region.a", "Enter:region.b"],
        "issue812 move_into_overlap",
    )?;

    // Stay inside the overlap: membership unchanged, so NO events fire.
    let stay = transitions(doc, Vec3::new(1.8, 0.0, 1.8), Vec3::new(2.2, 0.0, 2.2))?;
    let stay_events = region_events(&stay)?;
    assert_events(&stay_events, &[], "issue812 stay_inside_overlap")?;

    // Leave every region: mirror of the happy path -> three ordered LEAVE events.
    let leave = transitions(doc, Vec3::new(2.0, 0.0, 2.0), Vec3::new(9.0, 0.0, 9.0))?;
    let leave_events = region_events(&leave)?;
    assert_events(
        &leave_events,
        &["Leave:playable_bounds", "Leave:region.a", "Leave:region.b"],
        "issue812 leave_all_regions",
    )?;

    let nan_error = call_mcp_world_tool(
        EVALUATE_REGION_TRANSITIONS_TOOL,
        json!({
            "document_path": doc,
            "previous": Vec3::new(f32::NAN, 0.0, 0.0),
            "next": Vec3::new(2.0, 0.0, 2.0)
        }),
    )
    .unwrap_err();

    let edges = vec![
        json!({ "name": "stay_inside_overlap_no_events", "events": stay_events }),
        json!({ "name": "leave_all_regions", "events": leave_events }),
        json!({ "name": "non_finite_endpoint_fails_loud", "error": nan_error.to_string() }),
    ];

    Ok(IssueBlock {
        issue: 812,
        happy_path: json!({
            "move_into_overlap": happy,
            "enter_events": happy_events
        }),
        edges,
    })
}

fn transitions(
    doc: &std::path::Path,
    previous: Vec3,
    next: Vec3,
) -> Result<Value, Box<dyn std::error::Error>> {
    let output = call_mcp_world_tool(
        EVALUATE_REGION_TRANSITIONS_TOOL,
        json!({ "document_path": doc, "previous": previous, "next": next }),
    )?;
    Ok(output.result)
}

/// Fail-loud ordered `kind:region_id` events from a transition result. Deserializes the
/// typed report, so a missing/malformed structure errors (never a silently-empty list).
fn region_events(result: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let value = result
        .get("region_transitions")
        .ok_or("region-transition result missing `region_transitions`")?;
    let report: RegionTransitionReport = serde_json::from_value(value.clone())?;
    Ok(report
        .events
        .iter()
        .map(|event| format!("{:?}:{}", event.kind, event.region_id))
        .collect())
}

/// Assert an event list equals the expected ordered literals exactly; fail loud otherwise.
fn assert_events(
    actual: &[String],
    expected: &[&str],
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if actual
        .iter()
        .map(String::as_str)
        .ne(expected.iter().copied())
    {
        return Err(format!("{label}: expected {expected:?}, got {actual:?}").into());
    }
    Ok(())
}

/// Fresh disk read of the stored source-of-truth document; fails loud if none is returned.
fn read_document(doc: &std::path::Path) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let output = call_mcp_world_tool(READ_WORLD_TOOL, json!({ "document_path": doc }))?;
    output
        .document_readback
        .map(|readback| readback.document)
        .ok_or_else(|| "read_world returned no document_readback".into())
}

/// Ordered (slot, player_id, object_id) for every StartLocation stored; panics loud if a
/// start object is missing its slot/player_id property.
fn start_location_slots(document: &EditorDocument) -> Vec<(u32, String, String)> {
    let mut slots: Vec<_> = document
        .objects
        .values()
        .filter(|object| object.kind == "StartLocation")
        .map(|object| {
            let slot: u32 = object.properties["slot"]
                .parse()
                .expect("start slot parses");
            (
                slot,
                object.properties["player_id"].clone(),
                object.id.clone(),
            )
        })
        .collect();
    slots.sort();
    slots
}

fn path_brush() -> TerrainBrushSpec {
    TerrainBrushSpec {
        center_tile_x: 1,
        center_tile_z: 1,
        size_tiles: 1,
        shape: TerrainBrushShape::Square,
        falloff: TerrainBrushFalloff::Constant,
        operation: TerrainBrushOperation::PaintTile {
            tile_id: "tile.ashen_path".to_string(),
        },
    }
}

fn document_state(doc: &std::path::Path) -> Value {
    let readback = if doc.exists() {
        Some(read_mcp_world_document(doc).unwrap())
    } else {
        None
    };
    json!({
        "document_sha256": readback.as_ref().map(|readback| readback.sha256.clone()),
        "region_count": readback.as_ref().map(|readback| readback.region_count).unwrap_or(0),
        "named_camera_count": readback.as_ref().map(|readback| readback.named_camera_count).unwrap_or(0),
        "start_location_count": readback.as_ref().map(|readback| readback.start_location_count).unwrap_or(0)
    })
}

fn roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    Ok(serde_json::from_slice(&serde_json::to_vec_pretty(value)?)?)
}
