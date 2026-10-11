use std::{fs, path::Path};

use litd_engine::render::sprite_billboard::SpriteBillboardLibrarySelection;
use serde_json::json;

use super::*;

#[test]
fn issue868_sprite_billboard_mcp_tool_writes_and_reads_real_state() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue868_sprite_billboards", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let ledger_path = root.join("sprite-billboard-ledger.json");
    let library = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();

    let set = call_mcp_render_tool(
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue868-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue868-sprite-billboard-ledger",
            "request_id": "issue868-set-sprite-billboard-library",
            "requested_by": "litd-editor-issue868-test",
            "library": library
        }),
    )
    .unwrap();
    assert_eq!(set.tool_name, SET_SPRITE_BILLBOARD_LIBRARY_TOOL);
    assert_eq!(set.result["sprite_billboard_texture_count"], 3);
    assert_eq!(set.result["sprite_billboard_material_count"], 3);
    assert_eq!(set.result["sprite_billboard_billboard_count"], 4);
    assert_eq!(set.result["sprite_billboard_search_row_count"], 11);

    let read = call_mcp_render_tool(READ_RENDER_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(
        read.result["sprite_billboard_library_id"],
        "library.issue868.sprite_billboards"
    );
    assert_eq!(read.result["sprite_billboard_texture_count"], 3);
    assert_eq!(read.result["sprite_billboard_billboard_count"], 4);
    assert_eq!(
        read.state_readback
            .as_ref()
            .unwrap()
            .sprite_billboard_library
            .as_ref()
            .unwrap()
            .total_index_count,
        24
    );

    let ledger = read_mcp_render_tool_ledger(&ledger_path).unwrap();
    let record = &ledger.ledger.records[0];
    assert_eq!(record.tool_name, SET_SPRITE_BILLBOARD_LIBRARY_TOOL);
    assert_eq!(record.state_update_count, 1);
}

#[test]
fn issue868_sprite_billboard_mcp_edges_fail_closed_without_writing() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue868_sprite_billboards", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("render-state.json");
    let ledger_path = root.join("sprite-billboard-ledger.json");
    let before = state(&state_path, &ledger_path);

    let mut duplicate = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    let mut cloned = duplicate.textures[0].clone();
    cloned.texture_id = "texture.issue868.duplicate_pixels".to_string();
    duplicate.textures.push(cloned);
    let duplicate_error = set_library(&state_path, &ledger_path, duplicate).unwrap_err();
    assert!(
        duplicate_error
            .to_string()
            .contains("duplicate content address")
    );
    assert_eq!(state(&state_path, &ledger_path), before);

    let mut bad_frame = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    bad_frame.billboards[0].frame_index = 4;
    let frame_error = set_library(&state_path, &ledger_path, bad_frame).unwrap_err();
    assert!(frame_error.to_string().contains("frame_index"));
    assert_eq!(state(&state_path, &ledger_path), before);

    let mut zero_size = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    zero_size.billboards[0].size[1] = 0.0;
    let size_error = set_library(&state_path, &ledger_path, zero_size).unwrap_err();
    assert!(size_error.to_string().contains("billboard.size"));
    assert_eq!(state(&state_path, &ledger_path), before);
}

fn set_library(
    state_path: &Path,
    ledger_path: &Path,
    library: SpriteBillboardLibrarySelection,
) -> Result<McpRenderToolCallOutput, litd_engine::EngineError> {
    call_mcp_render_tool(
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        json!({
            "state_path": state_path,
            "state_id": "issue868-render-state",
            "ledger_path": ledger_path,
            "ledger_id": "issue868-sprite-billboard-ledger",
            "request_id": "issue868-set-sprite-billboard-library",
            "requested_by": "litd-editor-issue868-test",
            "library": library
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
        "sprite_billboard_billboard_count": state_readback.as_ref().and_then(|readback| readback.sprite_billboard_library.as_ref()).map(|library| library.billboard_count).unwrap_or(0),
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
