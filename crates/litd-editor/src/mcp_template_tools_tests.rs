use std::fs;
use std::path::Path;

use litd_engine::math::Vec3;
use serde_json::json;

use crate::history::EditorRegion;
use crate::mcp_world_tools::write_mcp_world_document;
use crate::template_gallery::{MELEE_SKIRMISH_TEMPLATE, STORY_CINEMATIC_TEMPLATE};

use super::*;

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}

#[test]
fn list_templates_tool_returns_gallery() {
    let output = call_mcp_template_tool(LIST_TEMPLATES_TOOL, json!({})).unwrap();
    assert_eq!(output.result["template_count"].as_u64().unwrap(), 2);
}

#[test]
fn clone_deep_copies_and_leaves_template_untouched() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_run, root) =
        crate::fsv_support::artifact_dir("issue901_template_tools_unit", "unit").unwrap();
    reset_root(&root);
    let template_path = root.join("template-melee");
    let clone_path = root.join("clone-melee");
    let ledger_path = root.join("clone-ledger.json");

    let output = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": MELEE_SKIRMISH_TEMPLATE,
            "template_path": template_path,
            "clone_path": clone_path,
            "clone_calyx_document_id": "calyx://project/my-melee",
            "clone_revision_id": "my-melee-rev-1",
            "ledger_path": ledger_path,
            "ledger_id": "clone-ledger",
            "request_id": "clone-1",
            "requested_by": "issue901-test",
        }),
    )
    .unwrap();
    let template_fingerprint_before = read_document_fingerprint(&template_path).unwrap();
    assert_eq!(
        output.result["template_fingerprint"].as_str().unwrap(),
        template_fingerprint_before
    );

    // Mutate the CLONE independently and persist it.
    let mut clone = read_document(&clone_path).unwrap();
    clone
        .upsert_region(EditorRegion::new(
            "region.mutation",
            "Mutation",
            Vec3::new(4.0, 0.0, 4.0),
            Vec3::new(1.0, 4.0, 1.0),
        ))
        .unwrap();
    write_mcp_world_document(&clone_path, &clone).unwrap();

    // Independent readback of the TEMPLATE proves it is byte-for-byte untouched.
    let template_fingerprint_after = read_document_fingerprint(&template_path).unwrap();
    assert_eq!(template_fingerprint_before, template_fingerprint_after);
    let template_doc = read_document(&template_path).unwrap();
    assert!(!template_doc.regions.contains_key("region.mutation"));
}

#[test]
fn clone_into_existing_project_is_a_typed_collision() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_run, root) =
        crate::fsv_support::artifact_dir("issue901_template_tools_collision", "unit").unwrap();
    reset_root(&root);
    let template_path = root.join("template-melee");
    let clone_path = root.join("clone-collide");
    let ledger_path = root.join("clone-ledger.json");

    // Pre-create the destination so the clone must refuse it.
    let existing = build_template_document(STORY_CINEMATIC_TEMPLATE).unwrap();
    write_mcp_world_document(&clone_path, &existing).unwrap();

    let error = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": MELEE_SKIRMISH_TEMPLATE,
            "template_path": template_path,
            "clone_path": clone_path,
            "clone_calyx_document_id": "calyx://project/collide",
            "clone_revision_id": "collide-rev-1",
            "ledger_path": ledger_path,
            "ledger_id": "clone-ledger",
            "request_id": "clone-1",
            "requested_by": "issue901-test",
        }),
    )
    .unwrap_err();
    match error {
        EngineError::UiRenderInvalidConfig { field, .. } => {
            assert_eq!(field, "editor.mcp_template_tools.clone_path");
        }
        other => panic!("expected typed collision error, got {other:?}"),
    }
}
