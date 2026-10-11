use std::{fs, path::Path, path::PathBuf};

use litd_engine::{
    math::{Transform, Vec3, Vec4},
    render::material::PbrMaterial,
};
use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_agent_tools::{BIND_AGENT_TOOL, call_mcp_agent_tool},
    mcp_asset_tools::{GENERATE_IMPORT_ASSET_TOOL, call_mcp_asset_tool},
    mcp_object_tools::{APPLY_OBJECT_DELTA_TOOL, call_mcp_object_tool, read_mcp_object_document},
    mcp_project_tools::{
        NEW_OPEN_SAVE_TOOL, PACKAGE_LITDGAME_TOOL, RUN_HEADLESS_PLAYTEST_TOOL,
        call_mcp_project_tool,
    },
    mcp_render_tools::{SET_PBR_MATERIAL_TOOL, call_mcp_render_tool},
    mcp_story_tools::{UPSERT_LORE_CONTEXT_TOOL, call_mcp_story_tool},
    mcp_trigger_tools::{UPSERT_VARIABLE_TOOL, call_mcp_trigger_tool},
    mcp_world_tools::{read_mcp_world_document, write_mcp_world_document},
};

use super::*;

#[test]
fn issue927_fsv_reads_real_artifacts_across_domains() {
    let root = test_root("unit");
    reset_root(&root);
    let paths = write_artifacts(&root);

    let world = call_mcp_fsv_tool(
        READ_WORLD_STATE_TOOL,
        json!({ "artifacts": [artifact("world", "world_document", &paths.world)] }),
    )
    .unwrap();
    assert_eq!(world.artifact_count, 1);
    assert_eq!(world.artifacts[0].summary["object_count"], 2);

    let object = call_mcp_fsv_tool(
        READ_OBJECT_STATE_TOOL,
        json!({ "artifacts": [artifact("objects", "object_document", &paths.object)] }),
    )
    .unwrap();
    assert_eq!(object.artifacts[0].summary["unit_count"], 1);

    let trigger = call_mcp_fsv_tool(
        READ_TRIGGER_RUNTIME_STATE_TOOL,
        json!({ "artifacts": [artifact("trigger", "trigger_state", &paths.trigger_state)] }),
    )
    .unwrap();
    assert_eq!(trigger.artifacts[0].summary["variable_count"], 1);

    let story_render = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({ "artifacts": [
            artifact("story", "story_state", &paths.story_state),
            artifact("render", "render_state", &paths.render_state),
            artifact("agent", "agent_state", &paths.agent_state),
            artifact("asset", "asset_state", &paths.asset_state)
        ] }),
    )
    .unwrap();
    assert_eq!(story_render.artifact_count, 4);
    assert_eq!(story_render.artifacts[0].summary["lore_context_count"], 1);
    assert_eq!(story_render.artifacts[1].summary["material_count"], 1);
    assert_eq!(story_render.artifacts[2].summary["binding_count"], 1);
    assert_eq!(story_render.artifacts[3].summary["asset_count"], 1);

    let project = call_mcp_fsv_tool(
        READ_PROJECT_ARTIFACTS_TOOL,
        json!({ "artifacts": [
            artifact("project-state", "project_state", &paths.project_state),
            artifact("litdworld", "litd_world", &paths.litdworld),
            artifact("litdgame", "litd_game", &paths.litdgame)
        ] }),
    )
    .unwrap();
    assert_eq!(project.artifact_count, 3);
    assert_eq!(project.artifacts[0].summary["project_count"], 1);
    assert_eq!(project.artifacts[1].summary["object_count"], 2);
    assert_eq!(project.artifacts[1].summary["unit_count"], 1);
    assert_eq!(
        project.artifacts[2].summary["runtime_id"],
        "litd.engine.runtime.v1"
    );
    assert_eq!(project.artifacts[2].summary["asset_count"], 1);
}

#[test]
fn issue927_fsv_edges_fail_loud_without_mutation() {
    let root = test_root("edges");
    reset_root(&root);
    let paths = write_artifacts(&root);
    let before = file_state(&paths.world);

    let wrong_kind = call_mcp_fsv_tool(
        READ_WORLD_STATE_TOOL,
        json!({ "artifacts": [artifact("object-as-world", "object_document", &paths.object)] }),
    )
    .unwrap_err()
    .to_string();
    let after_wrong = file_state(&paths.world);
    assert_eq!(before, after_wrong);
    println!("issue927 wrong_kind before={before} after={after_wrong} error={wrong_kind}");

    let missing = call_mcp_fsv_tool(
        READ_OBJECT_STATE_TOOL,
        json!({ "artifacts": [artifact("missing", "object_document", root.join("missing-aster"))] }),
    )
    .unwrap_err()
    .to_string();
    let after_missing = file_state(&paths.world);
    assert_eq!(before, after_missing);
    println!("issue927 missing before={before} after={after_missing} error={missing}");

    let empty = call_mcp_fsv_tool(READ_PROJECT_ARTIFACTS_TOOL, json!({ "artifacts": [] }))
        .unwrap_err()
        .to_string();
    let after_empty = file_state(&paths.world);
    assert_eq!(before, after_empty);
    println!("issue927 empty before={before} after={after_empty} error={empty}");
}

#[test]
fn issue1518_fsv_artifact_size_cap_rejects_oversized_without_read() {
    let root = test_root("size-cap");
    reset_root(&root);

    // Happy path: a valid, under-cap FSV report reads back byte-for-byte.
    let report_path = root.join("fsv-report.json");
    let report = json!({
        "schema": "litd.editor.fsv.report.v1",
        "issue": 1518,
        "verdict": "pass"
    });
    fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let ok = call_mcp_fsv_tool(
        READ_PROJECT_ARTIFACTS_TOOL,
        json!({ "artifacts": [artifact("report", "fsv_report", &report_path)] }),
    )
    .unwrap();
    assert_eq!(ok.artifact_count, 1);
    assert_eq!(ok.artifacts[0].summary["verdict"], "pass");
    assert_eq!(ok.artifacts[0].summary["issue"], 1518);

    // Oversize path: a real on-disk file whose reported length exceeds the cap must be
    // rejected on the metadata() check BEFORE a single byte is read into memory. `set_len`
    // gives us a genuine >64 MiB length without materialising 64 MiB of data.
    let big_path = root.join("oversized-report.json");
    let file = fs::File::create(&big_path).unwrap();
    file.set_len(MAX_FSV_ARTIFACT_BYTES + 1).unwrap();
    drop(file);
    let before = file_state(&big_path);
    assert_eq!(before["bytes"], MAX_FSV_ARTIFACT_BYTES + 1);

    let error = call_mcp_fsv_tool(
        READ_PROJECT_ARTIFACTS_TOOL,
        json!({ "artifacts": [artifact("oversized", "fsv_report", &big_path)] }),
    )
    .unwrap_err()
    .to_string();
    let after = file_state(&big_path);
    assert_eq!(before, after, "oversized artifact must not be mutated");
    assert!(
        error.contains("artifact.read.size") && error.contains("exceeding the read limit"),
        "expected size-limit rejection, got: {error}"
    );
    println!("issue1518 size-cap before={before} after={after} error={error}");
}

struct ArtifactPaths {
    world: PathBuf,
    object: PathBuf,
    trigger_state: PathBuf,
    story_state: PathBuf,
    render_state: PathBuf,
    agent_state: PathBuf,
    asset_state: PathBuf,
    project_state: PathBuf,
    litdworld: PathBuf,
    litdgame: PathBuf,
}

fn write_artifacts(root: &Path) -> ArtifactPaths {
    let document = fixture_document();
    let world = root.join("world-document-aster");
    let object = root.join("object-document-aster");
    write_mcp_world_document(&world, &document).unwrap();
    read_mcp_world_document(&world).unwrap();
    call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": object,
            "ledger_path": root.join("object-ledger-aster"),
            "ledger_id": "issue927-object-ledger",
            "request_id": "issue927-object",
            "requested_by": "litd-editor-issue927-test",
            "initial_document": document,
            "object_id": "unit_playtest_hero",
            "delta": {
                "set_kind": null,
                "upsert_properties": { "stats.hit_points": "927" },
                "remove_properties": []
            }
        }),
    )
    .unwrap();
    read_mcp_object_document(&object).unwrap();

    let trigger_state = root.join("trigger-state.json");
    call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": trigger_state,
            "ledger_path": root.join("trigger-ledger.json"),
            "state_id": "issue927-trigger-state",
            "ledger_id": "issue927-trigger-ledger",
            "request_id": "issue927-trigger",
            "requested_by": "litd-editor-issue927-test",
            "variable": {
                "name": "fsv_gate_open",
                "value_type": "Boolean",
                "scope": "Global",
                "is_array": false,
                "initial_value": "true"
            }
        }),
    )
    .unwrap();

    let story_state = root.join("story-state.json");
    call_mcp_story_tool(
        UPSERT_LORE_CONTEXT_TOOL,
        json!({
            "state_path": story_state,
            "ledger_path": root.join("story-ledger.json"),
            "state_id": "issue927-story-state",
            "ledger_id": "issue927-story-ledger",
            "request_id": "issue927-story",
            "requested_by": "litd-editor-issue927-test",
            "lore": {
                "lore_id": "lore_issue927_fsv",
                "title": "FSV Doctrine",
                "body": "Agents read physical state before closing work.",
                "source_refs": ["issue_800", "issue_927"],
                "tags": ["fsv", "mcp"]
            }
        }),
    )
    .unwrap();

    let render_state = root.join("render-state.json");
    call_mcp_render_tool(
        SET_PBR_MATERIAL_TOOL,
        json!({
            "state_path": render_state,
            "state_id": "issue927-render-state",
            "ledger_path": root.join("render-ledger.json"),
            "ledger_id": "issue927-render-ledger",
            "request_id": "issue927-render",
            "requested_by": "litd-editor-issue927-test",
            "binding": material_binding()
        }),
    )
    .unwrap();

    let agent_state = root.join("agent-state.json");
    call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": agent_state,
            "ledger_path": root.join("agent-ledger.json"),
            "state_id": "issue927-agent-state",
            "ledger_id": "issue927-agent-ledger",
            "request_id": "issue927-agent",
            "requested_by": "litd-editor-issue927-test",
            "binding": agent_binding()
        }),
    )
    .unwrap();

    let asset_state = root.join("asset-state.json");
    call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": asset_state,
            "ledger_path": root.join("asset-ledger.json"),
            "state_id": "issue927-asset-state",
            "ledger_id": "issue927-asset-ledger",
            "request_id": "issue927-asset",
            "requested_by": "litd-editor-issue927-test",
            "asset": asset_record()
        }),
    )
    .unwrap();

    let project_state = root.join("project-state.json");
    let litdworld = root.join("issue927.litdworld.json");
    let litdgame = root.join("issue927.litdgame.json");
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue927-project",
            "requested_by": "litd-editor-issue927-test",
            "state_path": project_state,
            "state_id": "issue927-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue927-project-ledger",
            "action": "save_snapshot",
            "project_id": "project.issue927",
            "title": "Issue 927 FSV Project",
            "version": "1.0.0",
            "tags": ["fsv", "mcp"],
            "project_path": litdworld,
            "snapshot_label": "fsv",
            "document": document
        }),
    )
    .unwrap();
    call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue927-playtest",
            "requested_by": "litd-editor-issue927-test",
            "state_path": project_state,
            "state_id": "issue927-project-state",
            "ledger_path": root.join("playtest-ledger.json"),
            "ledger_id": "issue927-playtest-ledger",
            "project_path": litdworld,
            "camera_rig": "TopDown",
            "ticks": 32
        }),
    )
    .unwrap();
    call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue927-package",
            "requested_by": "litd-editor-issue927-test",
            "state_path": project_state,
            "state_id": "issue927-project-state",
            "ledger_path": root.join("package-ledger.json"),
            "ledger_id": "issue927-package-ledger",
            "project_path": litdworld,
            "package_path": litdgame,
            "share_id": "issue927-share",
            "created_by": "agent.fsv.reader",
            "host_port": 19270,
            "max_package_bytes": 262144
        }),
    )
    .unwrap();

    ArtifactPaths {
        world,
        object,
        trigger_state,
        story_state,
        render_state,
        agent_state,
        asset_state,
        project_state,
        litdworld,
        litdgame,
    }
}

fn artifact(label: &str, kind: &str, path: impl AsRef<Path>) -> Value {
    json!({ "label": label, "kind": kind, "path": path.as_ref() })
}

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new("calyx://world/issue927-fsv", "issue927-rev-1").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_playtest_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(1.0, 0.0, 2.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    document
}

fn material_binding() -> Value {
    let mut material = PbrMaterial::new("issue927 fsv steel");
    material.base_color_factor = Vec4::new(0.4, 0.45, 0.5, 1.0);
    material.metallic_factor = 0.7;
    material.roughness_factor = 0.35;
    material.emissive_factor = Vec3::new(0.0, 0.4, 0.7);
    json!({
        "material_id": "mat.issue927.fsv_steel",
        "target_asset_id": "asset.issue927.hero",
        "material": material,
        "channel_notes": ["fsv-readback"]
    })
}

fn agent_binding() -> Value {
    json!({
        "binding_id": "binding.issue927.guardian",
        "agent_id": "agent.issue927.guardian",
        "object_id": "unit_playtest_hero",
        "display_name": "Issue 927 Guardian",
        "deterministic_order_sink": "orders.issue927.guardian",
        "activation": {
            "mode": "signal",
            "signal_refs": ["signal.issue927.tick"],
            "cooldown_ticks": 16
        },
        "enabled": true
    })
}

fn asset_record() -> Value {
    json!({
        "asset_id": "asset.issue927.hero",
        "display_name": "Issue 927 Hero",
        "asset_type": "character",
        "source_kind": "generated",
        "prompt": "FSV hero asset for source-of-truth readback.",
        "source_ref": "prompt.issue927.hero",
        "content_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "license": "project_internal",
        "provenance_refs": ["prompt.issue927.hero"]
    })
}

fn file_state(path: &Path) -> Value {
    json!({
        "exists": path.exists(),
        "bytes": path.metadata().map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue927_mcp_fsv_tools", label)
}

fn reset_root(path: &Path) {
    // write_artifacts drives BIND_AGENT_TOOL, which now confines tool-supplied paths within
    // LITD_EDITOR_PROJECT_ROOT (#1518). Fixtures live under the workspace `target/fsv/` tree,
    // so point the confinement root at the workspace root.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
