use std::{fs, path::Path};

use litd_engine::render::vfx::{
    VfxRenderCueSourceKind, count_effects_in_content_store, read_effect_from_content_store,
    validate_effect_descriptor,
};
use serde_json::json;

use super::*;

#[test]
fn issue922_vfx_mcp_tools_author_attach_and_read_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue922_mcp_vfx_tools", "unit").unwrap();
    reset_root(&root);
    let store_root = root.join("store");
    let ledger_path = root.join("mcp-vfx-ledger.json");
    let before = state(&store_root, &ledger_path);
    println!("issue922 before {before}");

    let described = call_mcp_vfx_tool(
        DESCRIBE_EFFECT_TOOL,
        json!({
            "ledger_path": ledger_path,
            "ledger_id": "issue922-describe-ledger",
            "store_root": store_root,
            "request_id": "issue922-describe-fireball",
            "prompt": "fireball explosion",
            "seed": 922,
            "requested_by": "litd-editor-issue922-test"
        }),
    )
    .unwrap();
    assert_eq!(described.tool_name, DESCRIBE_EFFECT_TOOL);
    let descriptor: VfxEffectDescriptor =
        serde_json::from_value(described.result["descriptor"].clone()).unwrap();
    assert_eq!(descriptor.effect_id, "generated.vfx.fire.explosion");
    let readback = read_effect_from_content_store(&store_root, &descriptor).unwrap();
    assert_eq!(readback.sha256, descriptor.sha256().unwrap());

    let read = call_mcp_vfx_tool(
        READ_VFX_TOOL,
        json!({
            "store_root": store_root,
            "descriptor": descriptor
        }),
    )
    .unwrap();
    assert_eq!(
        read.result["effect"]["effect_id"],
        "generated.vfx.fire.explosion"
    );

    let mut edited = readback.effect.clone();
    edited.effect_id = "issue922.stack.fireball".to_string();
    edited.display_name = "Issue 922 Stack Fireball".to_string();
    edited.emitters[0].max_particles = 4096;
    let stacked = call_mcp_vfx_tool(
        SET_PARTICLE_STACK_TOOL,
        json!({
            "ledger_path": root.join("mcp-vfx-stack-ledger.json"),
            "ledger_id": "issue922-stack-ledger",
            "store_root": store_root,
            "request_id": "issue922-set-stack",
            "requested_by": "litd-editor-issue922-test",
            "effect": edited
        }),
    )
    .unwrap();
    let stack_descriptor: VfxEffectDescriptor =
        serde_json::from_value(stacked.result["descriptor"].clone()).unwrap();
    assert_eq!(stack_descriptor.effect_id, "issue922.stack.fireball");

    let attached = call_mcp_vfx_tool(
        ATTACH_EFFECT_TOOL,
        json!({
            "ledger_path": root.join("mcp-vfx-attach-ledger.json"),
            "ledger_id": "issue922-attach-ledger",
            "store_root": store_root,
            "request_id": "issue922-attach-ability",
            "requested_by": "litd-editor-issue922-test",
            "cue_id": "cue.issue922.fireball.cast",
            "source_kind": "ability",
            "source_id": "ability.issue922.fireball",
            "descriptor": stack_descriptor
        }),
    )
    .unwrap();
    assert_eq!(
        attached.result["cue"]["source_id"],
        "ability.issue922.fireball"
    );
    assert_eq!(
        attached.ledger_readback.unwrap().ledger.records[0]
            .cue
            .as_ref()
            .unwrap()
            .source_kind,
        VfxRenderCueSourceKind::Ability
    );

    let after = state(&store_root, &ledger_path);
    println!("issue922 after {after}");
    assert_eq!(after["store_objects"], 2);
    assert_eq!(after["store_graph_rows"], 4);
    assert_eq!(after["ledger_exists"], true);
}

#[test]
fn issue922_vfx_mcp_tools_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue922_mcp_vfx_tools", "edges").unwrap();
    reset_root(&root);
    let store_root = root.join("store");
    let ledger_path = root.join("edge-ledger.json");

    let before_empty = state(&store_root, &ledger_path);
    let empty_prompt = call_mcp_vfx_tool(
        DESCRIBE_EFFECT_TOOL,
        json!({
            "ledger_path": ledger_path,
            "ledger_id": "issue922-empty-ledger",
            "store_root": store_root,
            "request_id": "issue922-empty",
            "prompt": "",
            "seed": 1,
            "requested_by": "litd-editor-issue922-test"
        }),
    )
    .unwrap_err();
    let after_empty = state(&store_root, &ledger_path);
    println!("issue922 empty before={before_empty} after={after_empty} error={empty_prompt}");
    assert_eq!(before_empty, after_empty);
    assert!(
        empty_prompt
            .to_string()
            .contains("editor.vfx_text_generation")
    );

    let before_unknown = state(&store_root, &ledger_path);
    let unknown = call_mcp_vfx_tool("litd.engine.vfx.not_registered", json!({})).unwrap_err();
    let after_unknown = state(&store_root, &ledger_path);
    println!("issue922 unknown before={before_unknown} after={after_unknown} error={unknown}");
    assert_eq!(before_unknown, after_unknown);
    assert!(
        unknown
            .to_string()
            .contains("editor.mcp_vfx_tools.tool_name")
    );

    let descriptor = descriptor_from_sha(
        "issue922.missing",
        "6fe4ee04e4c1f938504a6622ca7f2fb62de3e11da045b5b967ca1c7086d824c2",
        3731,
    )
    .unwrap();
    validate_effect_descriptor(&descriptor).unwrap();
    let before_missing = state(&store_root, &ledger_path);
    let missing_store = call_mcp_vfx_tool(
        ATTACH_EFFECT_TOOL,
        json!({
            "ledger_path": root.join("missing-ledger.json"),
            "ledger_id": "issue922-missing-ledger",
            "store_root": root.join("missing-store"),
            "request_id": "issue922-missing",
            "requested_by": "litd-editor-issue922-test",
            "cue_id": "cue.issue922.missing",
            "source_kind": "ability",
            "source_id": "ability.issue922.missing",
            "descriptor": descriptor
        }),
    )
    .unwrap_err();
    let after_missing = state(&store_root, &ledger_path);
    println!(
        "issue922 missing before={before_missing} after={after_missing} error={missing_store}"
    );
    assert_eq!(before_missing, after_missing);
    assert!(
        missing_store
            .to_string()
            .contains("render.vfx.content_store.read")
    );
}

fn state(store_root: &Path, ledger_path: &Path) -> serde_json::Value {
    let store = count_effects_in_content_store(store_root).unwrap();
    json!({
        "store_objects": store.effect_count,
        "store_graph_rows": store.graph_rows,
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
