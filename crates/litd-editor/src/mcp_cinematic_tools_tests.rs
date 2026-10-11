use std::fs;
use std::path::Path;

use litd_engine::cinematic_timeline::{CinematicFade, FadeKind};
use litd_engine::math::Vec3;
use serde_json::json;

use crate::cinematic_authoring::EditorCinematicTimeline;
use crate::history::{EditorDocument, EditorNamedCamera};
use crate::mcp_world_tools::sample_issue918_document;

use super::*;

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}

fn initial_document() -> EditorDocument {
    let mut document = sample_issue918_document();
    document
        .upsert_named_camera(EditorNamedCamera::top_down(
            "camera.a",
            "A",
            Vec3::new(1.0, 0.0, 1.0),
        ))
        .unwrap();
    document
        .upsert_named_camera(EditorNamedCamera::top_down(
            "camera.b",
            "B",
            Vec3::new(3.0, 0.0, 3.0),
        ))
        .unwrap();
    document
}

fn authored_timeline() -> EditorCinematicTimeline {
    EditorCinematicTimeline::new("cine.intro", "Intro", 4.0)
        .with_camera_key(0.0, "camera.a")
        .with_camera_key(4.0, "camera.b")
        .with_fade(CinematicFade {
            start_seconds: 0.0,
            duration_seconds: 2.0,
            kind: FadeKind::In,
            color: [0.0, 0.0, 0.0, 1.0],
        })
}

#[test]
fn author_then_evaluate_timeline_roundtrips_through_calyx() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_run, root) =
        crate::fsv_support::artifact_dir("issue892_cinematic_tools_unit", "unit").unwrap();
    reset_root(&root);
    let document_path = root.join("cine-document.json");
    let ledger_path = root.join("cine-ledger.json");

    let authored = call_mcp_cinematic_tool(
        AUTHOR_TIMELINE_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "cine-ledger",
            "request_id": "author-1",
            "requested_by": "issue892-test",
            "initial_document": initial_document(),
            "timeline": authored_timeline(),
        }),
    )
    .unwrap();
    assert_eq!(authored.tool_name, AUTHOR_TIMELINE_TOOL);

    // Independent readback: the persisted document really holds the timeline.
    let stored = read_mcp_world_document(&document_path).unwrap();
    assert!(
        stored
            .document
            .cinematic_timelines
            .contains_key("cine.intro")
    );

    let evaluated = call_mcp_cinematic_tool(
        EVALUATE_TIMELINE_TOOL,
        json!({
            "document_path": document_path,
            "timeline_id": "cine.intro",
            "sample_times": [0.0, 2.0, 4.0],
        }),
    )
    .unwrap();
    let samples = evaluated.result["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 3);
    // t=0: fade fully covered; t=2: fade fully revealed.
    assert_eq!(samples[0]["fade"]["alpha"].as_f64().unwrap(), 1.0);
    assert_eq!(samples[1]["fade"]["alpha"].as_f64().unwrap(), 0.0);
}

#[test]
fn evaluate_missing_timeline_fails_loud() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_run, root) =
        crate::fsv_support::artifact_dir("issue892_cinematic_tools_missing", "unit").unwrap();
    reset_root(&root);
    let document_path = root.join("cine-document.json");
    let ledger_path = root.join("cine-ledger.json");
    call_mcp_cinematic_tool(
        AUTHOR_TIMELINE_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "cine-ledger",
            "request_id": "author-1",
            "requested_by": "issue892-test",
            "initial_document": initial_document(),
            "timeline": authored_timeline(),
        }),
    )
    .unwrap();
    let error = call_mcp_cinematic_tool(
        EVALUATE_TIMELINE_TOOL,
        json!({
            "document_path": document_path,
            "timeline_id": "cine.nope",
            "sample_times": [0.0],
        }),
    )
    .unwrap_err();
    assert!(matches!(error, EngineError::UiRenderInvalidConfig { .. }));
}
