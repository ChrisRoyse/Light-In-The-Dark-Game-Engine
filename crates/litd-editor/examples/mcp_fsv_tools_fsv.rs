use std::{fs, path::Path, path::PathBuf};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_agent_tools::{BIND_AGENT_TOOL, call_mcp_agent_tool},
    mcp_asset_tools::{GENERATE_IMPORT_ASSET_TOOL, call_mcp_asset_tool},
    mcp_fsv_tools::{
        READ_OBJECT_STATE_TOOL, READ_PROJECT_ARTIFACTS_TOOL, READ_STORY_RENDER_STATE_TOOL,
        READ_TRIGGER_RUNTIME_STATE_TOOL, READ_WORLD_STATE_TOOL, call_mcp_fsv_tool,
    },
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
use litd_engine::{
    math::{Transform, Vec3, Vec4},
    render::material::PbrMaterial,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE927_FSV_ROOT",
        "issue927_mcp_fsv_tools",
        "manual",
    )?;
    reset_root(&root)?;
    let paths = write_artifacts(&root)?;
    let before = file_state(&paths.world);

    let world = call_mcp_fsv_tool(
        READ_WORLD_STATE_TOOL,
        json!({ "artifacts": [artifact("world", "world_document", &paths.world)] }),
    )?;
    let object = call_mcp_fsv_tool(
        READ_OBJECT_STATE_TOOL,
        json!({ "artifacts": [artifact("objects", "object_document", &paths.object)] }),
    )?;
    let trigger = call_mcp_fsv_tool(
        READ_TRIGGER_RUNTIME_STATE_TOOL,
        json!({ "artifacts": [artifact("trigger", "trigger_state", &paths.trigger_state)] }),
    )?;
    let story_render = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({ "artifacts": [
            artifact("story", "story_state", &paths.story_state),
            artifact("render", "render_state", &paths.render_state),
            artifact("agent", "agent_state", &paths.agent_state),
            artifact("asset", "asset_state", &paths.asset_state)
        ] }),
    )?;
    let project = call_mcp_fsv_tool(
        READ_PROJECT_ARTIFACTS_TOOL,
        json!({ "artifacts": [
            artifact("project-state", "project_state", &paths.project_state),
            artifact("litdworld", "litd_world", &paths.litdworld),
            artifact("litdgame", "litd_game", &paths.litdgame)
        ] }),
    )?;

    // world/object/project_state are Aster vault DIRECTORIES, so hash the real persisted bytes
    // via the FSV tool readbacks (FsvArtifactReadback.sha256) instead of a flat fs::read of the
    // directory, which fails with PermissionDenied on Windows (#1525). litdgame is a flat
    // package file; take it from the same project readback for consistency.
    let world_sha256 = world.artifacts[0].sha256.clone();
    let object_sha256 = object.artifacts[0].sha256.clone();
    let project_state_sha256 = project.artifacts[0].sha256.clone();
    let litdgame_sha256 = project.artifacts[2].sha256.clone();

    let edges = vec![
        edge_wrong_kind(&paths)?,
        edge_missing(&root, &paths)?,
        edge_empty(&paths)?,
    ];
    let after = file_state(&paths.world);
    let report = json!({
        "issue": 927,
        "source_of_truth": paths.to_json(),
        "states": { "before": before, "after": after },
        "happy_path": {
            "world": world,
            "object": object,
            "trigger": trigger,
            "story_render": story_render,
            "project": project
        },
        "edges": edges
    });
    let report_path = root.join("mcp-fsv-tools-fsv-readback.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue927 mcp_fsv_tools_fsv report={} report_sha256={} world_sha256={} object_sha256={} project_sha256={} package_sha256={}",
        report_path.display(),
        report_readback.sha256,
        world_sha256,
        object_sha256,
        project_state_sha256,
        litdgame_sha256
    );
    Ok(())
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

impl ArtifactPaths {
    fn to_json(&self) -> Value {
        json!({
            "world": self.world,
            "object": self.object,
            "trigger_state": self.trigger_state,
            "story_state": self.story_state,
            "render_state": self.render_state,
            "agent_state": self.agent_state,
            "asset_state": self.asset_state,
            "project_state": self.project_state,
            "litdworld": self.litdworld,
            "litdgame": self.litdgame
        })
    }
}

fn write_artifacts(root: &Path) -> Result<ArtifactPaths, Box<dyn std::error::Error>> {
    let document = fixture_document()?;
    // The world/object stores are Calyx Aster vault DIRECTORIES, not flat JSON files. Write
    // through the Aster-aware MCP tools (matching the unit fixture), not fs::write â€” a flat
    // file makes read_mcp_world_document fail loud with "is a file; expected a Calyx Aster
    // vault directory" (#1525).
    let world = root.join("world-document-aster");
    let object = root.join("object-document-aster");
    write_mcp_world_document(&world, &document)?;
    read_mcp_world_document(&world)?;
    call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": object,
            "ledger_path": root.join("object-ledger-aster"),
            "ledger_id": "issue927-object-ledger",
            "request_id": "issue927-object",
            "requested_by": "litd-editor-issue927-fsv",
            "initial_document": document,
            "object_id": "unit_playtest_hero",
            "delta": {
                "set_kind": null,
                "upsert_properties": { "stats.hit_points": "927" },
                "remove_properties": []
            }
        }),
    )?;
    read_mcp_object_document(&object)?;

    let trigger_state = root.join("trigger-state.json");
    call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": trigger_state,
            "ledger_path": root.join("trigger-ledger.json"),
            "state_id": "issue927-trigger-state",
            "ledger_id": "issue927-trigger-ledger",
            "request_id": "issue927-trigger",
            "requested_by": "litd-editor-issue927-fsv",
            "variable": {
                "name": "fsv_gate_open",
                "value_type": "Boolean",
                "scope": "Global",
                "is_array": false,
                "initial_value": "true"
            }
        }),
    )?;

    let story_state = root.join("story-state.json");
    call_mcp_story_tool(
        UPSERT_LORE_CONTEXT_TOOL,
        json!({
            "state_path": story_state,
            "ledger_path": root.join("story-ledger.json"),
            "state_id": "issue927-story-state",
            "ledger_id": "issue927-story-ledger",
            "request_id": "issue927-story",
            "requested_by": "litd-editor-issue927-fsv",
            "lore": {
                "lore_id": "lore_issue927_fsv",
                "title": "FSV Doctrine",
                "body": "Agents read physical state before closing work.",
                "source_refs": ["issue_800", "issue_927"],
                "tags": ["fsv", "mcp"]
            }
        }),
    )?;

    let render_state = root.join("render-state.json");
    call_mcp_render_tool(
        SET_PBR_MATERIAL_TOOL,
        json!({
            "state_path": render_state,
            "state_id": "issue927-render-state",
            "ledger_path": root.join("render-ledger.json"),
            "ledger_id": "issue927-render-ledger",
            "request_id": "issue927-render",
            "requested_by": "litd-editor-issue927-fsv",
            "binding": material_binding()
        }),
    )?;

    let agent_state = root.join("agent-state.json");
    call_mcp_agent_tool(
        BIND_AGENT_TOOL,
        json!({
            "state_path": agent_state,
            "ledger_path": root.join("agent-ledger.json"),
            "state_id": "issue927-agent-state",
            "ledger_id": "issue927-agent-ledger",
            "request_id": "issue927-agent",
            "requested_by": "litd-editor-issue927-fsv",
            "binding": agent_binding()
        }),
    )?;

    let asset_state = root.join("asset-state.json");
    call_mcp_asset_tool(
        GENERATE_IMPORT_ASSET_TOOL,
        json!({
            "state_path": asset_state,
            "ledger_path": root.join("asset-ledger.json"),
            "state_id": "issue927-asset-state",
            "ledger_id": "issue927-asset-ledger",
            "request_id": "issue927-asset",
            "requested_by": "litd-editor-issue927-fsv",
            "asset": asset_record()
        }),
    )?;

    let project_state = root.join("project-state.json");
    let litdworld = root.join("issue927.litdworld.json");
    let litdgame = root.join("issue927.litdgame.json");
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue927-project",
            "requested_by": "litd-editor-issue927-fsv",
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
    )?;
    call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue927-playtest",
            "requested_by": "litd-editor-issue927-fsv",
            "state_path": project_state,
            "state_id": "issue927-project-state",
            "ledger_path": root.join("playtest-ledger.json"),
            "ledger_id": "issue927-playtest-ledger",
            "project_path": litdworld,
            "camera_rig": "TopDown",
            "ticks": 32
        }),
    )?;
    call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue927-package",
            "requested_by": "litd-editor-issue927-fsv",
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
    )?;

    Ok(ArtifactPaths {
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
    })
}

fn edge_wrong_kind(paths: &ArtifactPaths) -> Result<Value, Box<dyn std::error::Error>> {
    let before = file_state(&paths.world);
    let error = call_mcp_fsv_tool(
        READ_WORLD_STATE_TOOL,
        json!({ "artifacts": [artifact("object-as-world", "object_document", &paths.object)] }),
    )
    .unwrap_err()
    .to_string();
    let after = file_state(&paths.world);
    Ok(edge("wrong_kind", before, after, error))
}

fn edge_missing(root: &Path, paths: &ArtifactPaths) -> Result<Value, Box<dyn std::error::Error>> {
    let before = file_state(&paths.world);
    let error = call_mcp_fsv_tool(
        READ_OBJECT_STATE_TOOL,
        json!({ "artifacts": [artifact("missing", "object_document", root.join("missing.json"))] }),
    )
    .unwrap_err()
    .to_string();
    let after = file_state(&paths.world);
    Ok(edge("missing_artifact", before, after, error))
}

fn edge_empty(paths: &ArtifactPaths) -> Result<Value, Box<dyn std::error::Error>> {
    let before = file_state(&paths.world);
    let error = call_mcp_fsv_tool(READ_PROJECT_ARTIFACTS_TOOL, json!({ "artifacts": [] }))
        .unwrap_err()
        .to_string();
    let after = file_state(&paths.world);
    Ok(edge("empty_artifacts", before, after, error))
}

fn edge(name: &str, before: Value, after: Value, error: String) -> Value {
    json!({ "name": name, "before": before, "after": after, "state_unchanged": before == after, "error": error })
}

fn artifact(label: &str, kind: &str, path: impl AsRef<Path>) -> Value {
    json!({ "label": label, "kind": kind, "path": path.as_ref() })
}

fn fixture_document() -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new("calyx://world/issue927-fsv", "issue927-rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_playtest_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(1.0, 0.0, 2.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or("missing real asset vault record")?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
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
    json!({ "exists": path.exists(), "bytes": path.metadata().map(|metadata| metadata.len()).unwrap_or(0) })
}

fn reset_root(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}
