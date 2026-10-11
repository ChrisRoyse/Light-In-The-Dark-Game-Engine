use std::{fs, path::Path};

use litd_engine::render::primitive::{PrimitiveGallerySelection, PrimitiveKind};
use serde_json::json;

use super::*;

#[test]
fn issue867_primitive_gallery_mcp_tool_writes_and_reads_real_state() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue867_primitive_gallery", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let ledger_path = root.join("primitive-gallery-ledger.json");
    let gallery = PrimitiveGallerySelection::blockout_defaults().unwrap();

    let set = call_mcp_render_tool(
        SET_PRIMITIVE_GALLERY_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue867-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue867-primitive-ledger",
            "request_id": "issue867-set-primitive-gallery",
            "requested_by": "litd-editor-issue867-test",
            "gallery": gallery
        }),
    )
    .unwrap();
    assert_eq!(set.tool_name, SET_PRIMITIVE_GALLERY_TOOL);
    assert_eq!(set.result["primitive_gallery_selected_id"], "blockout.box");
    assert_eq!(set.result["primitive_gallery_count"], 9);
    assert_eq!(set.result["primitive_gallery_total_vertex_count"], 588);
    assert_eq!(set.result["primitive_gallery_total_index_count"], 2284);

    let read = call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.result["primitive_gallery_count"], 9);
    assert_eq!(read.result["primitive_gallery_selected_id"], "blockout.box");
    assert_eq!(
        read.result["primitive_gallery_topology_ids"][7],
        "point_list"
    );
    assert_eq!(
        read.result["primitive_gallery_topology_ids"][8],
        "line_list"
    );

    let ledger = read_mcp_render_tool_ledger(&ledger_path).unwrap();
    let record = &ledger.ledger.records[0];
    assert_eq!(record.primitive_gallery_count, 9);
    assert_eq!(
        record.primitive_gallery_selected_id.as_deref(),
        Some("blockout.box")
    );
    assert_eq!(record.primitive_gallery_topology_ids.len(), 9);
}

#[test]
fn issue867_primitive_gallery_edges_fail_closed_without_writing() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue867_primitive_gallery", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let ledger_path = root.join("primitive-gallery-ledger.json");
    let before = state(&state_path, &ledger_path);

    let mut duplicate = PrimitiveGallerySelection::blockout_defaults().unwrap();
    duplicate.primitives[1].primitive_id = duplicate.primitives[0].primitive_id.clone();
    let duplicate_error = set_gallery(&state_path, &ledger_path, duplicate).unwrap_err();
    assert!(duplicate_error.to_string().contains("duplicate"));
    assert_eq!(state(&state_path, &ledger_path), before);

    let mut bad_sphere = PrimitiveGallerySelection::blockout_defaults().unwrap();
    bad_sphere.primitives[1].kind = PrimitiveKind::Sphere {
        radius: 0.55,
        segments: 2,
        rings: 8,
    };
    let sphere_error = set_gallery(&state_path, &ledger_path, bad_sphere).unwrap_err();
    assert!(sphere_error.to_string().contains("sphere.segments"));
    assert_eq!(state(&state_path, &ledger_path), before);

    let mut bad_lines = PrimitiveGallerySelection::blockout_defaults().unwrap();
    bad_lines.primitives[8].kind = PrimitiveKind::Lines {
        points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    };
    let lines_error = set_gallery(&state_path, &ledger_path, bad_lines).unwrap_err();
    assert!(lines_error.to_string().contains("line-list"));
    assert_eq!(state(&state_path, &ledger_path), before);
}

fn set_gallery(
    state_path: &Path,
    ledger_path: &Path,
    gallery: PrimitiveGallerySelection,
) -> Result<McpRenderToolCallOutput, litd_engine::EngineError> {
    call_mcp_render_tool(
        SET_PRIMITIVE_GALLERY_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue867-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue867-primitive-ledger",
            "request_id": "issue867-set-primitive-gallery",
            "requested_by": "litd-editor-issue867-test",
            "gallery": gallery
        }),
    )
}

fn state(state_path: &Path, ledger_path: &Path) -> serde_json::Value {
    let state_readback = if state_path.exists() {
        Some(read_mcp_render_state(state_path).unwrap())
    } else {
        None
    };
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "state_update_count": state_readback.as_ref().map(|readback| readback.state.update_count).unwrap_or(0),
        "primitive_gallery_count": state_readback.as_ref().map(|readback| readback.primitive_gallery_count).unwrap_or(0),
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
