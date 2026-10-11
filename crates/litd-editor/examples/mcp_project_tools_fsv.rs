use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        NEW_OPEN_SAVE_TOOL, PACKAGE_LITDGAME_TOOL, READ_PROJECT_TOOL, RUN_HEADLESS_PLAYTEST_TOOL,
        call_mcp_project_tool, read_mcp_project_state,
    },
};
use litd_engine::math::{Transform, Vec3};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE926_FSV_ROOT",
        "issue926_mcp_project_tools",
        "manual",
    )?;

    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("ashen-reach.litdgame.json");
    let document = fixture_document()?;
    let before = summary(&state_path, &project_path, &package_path);

    let saved = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue926-save",
            "requested_by": "litd-editor-issue926-fsv",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue926-project-ledger",
            "action": "save_snapshot",
            "project_id": "project.ashen_reach_issue926",
            "title": "Ashen Reach Issue 926",
            "version": "1.0.0",
            "tags": ["dark_fantasy", "playtest", "local_first"],
            "project_path": project_path,
            "snapshot_label": "manual-fsv-save",
            "document": document
        }),
    )?;
    let playtest = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue926-playtest",
            "requested_by": "litd-editor-issue926-fsv",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("playtest-ledger.json"),
            "ledger_id": "issue926-playtest-ledger",
            "project_path": project_path,
            "camera_rig": "FirstPerson",
            "ticks": 192
        }),
    )?;
    let package = call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue926-package",
            "requested_by": "litd-editor-issue926-fsv",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("package-ledger.json"),
            "ledger_id": "issue926-package-ledger",
            "project_path": project_path,
            "package_path": package_path,
            "share_id": "issue926-share",
            "created_by": "agent.project.builder",
            "host_port": 19260,
            "max_package_bytes": 262144
        }),
    )?;
    let read = call_mcp_project_tool(READ_PROJECT_TOOL, json!({ "state_path": state_path }))?;
    let after = summary(&state_path, &project_path, &package_path);
    let edges = vec![
        edge_invalid_title(&root, &state_path, &project_path, &package_path)?,
        edge_missing_project(&root, &state_path, &project_path, &package_path)?,
        edge_zero_ticks(&root, &state_path, &project_path, &package_path)?,
        edge_tiny_package(&root, &state_path, &project_path, &package_path)?,
    ];
    let report = json!({
        "issue": 926,
        "source_of_truth": {
            "state": state_path,
            "project": project_path,
            "package": package_path
        },
        "states": { "before": before, "after": after },
        "happy_path": {
            "new_open_save": saved,
            "run_headless_playtest": playtest,
            "package_litdgame": package,
            "read_project": read
        },
        "edges": edges
    });
    let report_path = root.join("mcp-project-tools-fsv-readback.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let report_readback = fsv_run.record_json_file(&report_path)?;
    let state_sha = sha256(&fs::read(&state_path)?);
    let project_sha = sha256(&fs::read(&project_path)?);
    let package_sha = sha256(&fs::read(&package_path)?);
    println!(
        "issue926 mcp_project_tools_fsv report={} report_sha256={} state_sha256={} project_sha256={} package_sha256={}",
        report_path.display(),
        report_readback.sha256,
        state_sha,
        project_sha,
        package_sha
    );
    Ok(())
}

fn edge_invalid_title(
    root: &std::path::Path,
    state_path: &std::path::Path,
    project_path: &std::path::Path,
    package_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = summary(state_path, project_path, package_path);
    let error = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue926-bad-title",
            "requested_by": "litd-editor-issue926-fsv",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("bad-title-ledger.json"),
            "ledger_id": "issue926-bad-title-ledger",
            "action": "save_snapshot",
            "project_id": "project.ashen_reach_issue926",
            "title": "",
            "version": "1.0.0",
            "tags": ["dark_fantasy"],
            "project_path": project_path,
            "snapshot_label": "invalid",
            "document": fixture_document()?
        }),
    )
    .unwrap_err()
    .to_string();
    let after = summary(state_path, project_path, package_path);
    Ok(edge("invalid_title", before, after, error))
}

fn edge_missing_project(
    root: &std::path::Path,
    state_path: &std::path::Path,
    project_path: &std::path::Path,
    package_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = summary(state_path, project_path, package_path);
    let error = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue926-missing-project",
            "requested_by": "litd-editor-issue926-fsv",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("missing-ledger.json"),
            "ledger_id": "issue926-missing-ledger",
            "project_path": root.join("missing.litdworld.json"),
            "camera_rig": "FirstPerson",
            "ticks": 64
        }),
    )
    .unwrap_err()
    .to_string();
    let after = summary(state_path, project_path, package_path);
    Ok(edge("missing_project", before, after, error))
}

fn edge_zero_ticks(
    root: &std::path::Path,
    state_path: &std::path::Path,
    project_path: &std::path::Path,
    package_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = summary(state_path, project_path, package_path);
    let error = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue926-zero-ticks",
            "requested_by": "litd-editor-issue926-fsv",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("zero-ledger.json"),
            "ledger_id": "issue926-zero-ledger",
            "project_path": project_path,
            "camera_rig": "TopDown",
            "ticks": 0
        }),
    )
    .unwrap_err()
    .to_string();
    let after = summary(state_path, project_path, package_path);
    Ok(edge("zero_ticks", before, after, error))
}

fn edge_tiny_package(
    root: &std::path::Path,
    state_path: &std::path::Path,
    project_path: &std::path::Path,
    package_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = summary(state_path, project_path, package_path);
    let error = call_mcp_project_tool(
        PACKAGE_LITDGAME_TOOL,
        json!({
            "request_id": "issue926-tiny-package",
            "requested_by": "litd-editor-issue926-fsv",
            "state_path": state_path,
            "state_id": "issue926-project-state",
            "ledger_path": root.join("tiny-ledger.json"),
            "ledger_id": "issue926-tiny-ledger",
            "project_path": project_path,
            "package_path": root.join("tiny.litdgame.json"),
            "share_id": "issue926-too-large",
            "created_by": "agent.project.builder",
            "host_port": 19261,
            "max_package_bytes": 256
        }),
    )
    .unwrap_err()
    .to_string();
    let after = summary(state_path, project_path, package_path);
    Ok(edge("tiny_package", before, after, error))
}

fn edge(name: &str, before: Value, after: Value, error: String) -> Value {
    json!({
        "name": name,
        "before": before,
        "after": after,
        "state_unchanged": before == after,
        "error": error
    })
}

fn fixture_document() -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new("calyx://world/issue926-project", "issue926-rev-1")?;
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
            Transform::from_translation(Vec3::new(2.0, 0.0, -4.0)),
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

fn summary(
    state_path: &std::path::Path,
    project_path: &std::path::Path,
    package_path: &std::path::Path,
) -> Value {
    let state = read_mcp_project_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": state_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "project_file_exists": project_path.exists(),
        "project_file_bytes": project_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "package_file_exists": package_path.exists(),
        "package_file_bytes": package_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "project_count": state.as_ref().map(|readback| readback.project_count).unwrap_or(0),
        "playtest_count": state.as_ref().map(|readback| readback.playtest_count).unwrap_or(0),
        "package_count": state.as_ref().map(|readback| readback.package_count).unwrap_or(0)
    })
}

fn sha256(bytes: &[u8]) -> String {
    litd_hash::sha256_hex(bytes)
}
