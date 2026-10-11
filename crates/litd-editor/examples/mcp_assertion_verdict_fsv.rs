use std::{error::Error, fs, path::Path};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        ASSERT_STATE_TOOL, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL, call_mcp_harness_tool,
        read_mcp_harness_aster_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    let root = Path::new("target")
        .join("fsv")
        .join("issue1171_assertion_api")
        .join("manual");
    clean_dir(&root)?;
    let project_path = root.join("assertion-world.litdworld.json");
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    create_project(&root, &project_path)?;

    let initial = state_summary(&state_path);
    println!("manual issue1171 initial {}", initial);
    let run = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "run-ledger",
            json!({
                "run_id": "issue1171-manual-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 8
            }),
        ),
    )?;

    let good = assert_state(
        &root,
        &state_path,
        &aster_dir,
        "good-ledger",
        json!({
            "verdict_id": "issue1171-manual-good",
            "run_id": "issue1171-manual-run",
            "assertions": [
                {"assertion_id": "body-zero-exists", "kind": "body_exists", "body_id": 0},
                {
                    "assertion_id": "body-zero-x",
                    "kind": "body_scalar_in_range",
                    "body_id": 0,
                    "metric": "position",
                    "axis": "x",
                    "min_milli": 1000,
                    "max_milli": 1000
                }
            ]
        }),
    )?;
    let bad = assert_state(
        &root,
        &state_path,
        &aster_dir,
        "bad-ledger",
        json!({
            "verdict_id": "issue1171-manual-bad-range",
            "run_id": "issue1171-manual-run",
            "assertions": [{
                "assertion_id": "body-zero-x-impossible",
                "kind": "body_scalar_in_range",
                "body_id": 0,
                "metric": "position",
                "axis": "x",
                "min_milli": 2000,
                "max_milli": 3000
            }]
        }),
    )?;
    let missing = assert_state(
        &root,
        &state_path,
        &aster_dir,
        "missing-ledger",
        json!({
            "verdict_id": "issue1171-manual-missing-body",
            "run_id": "issue1171-manual-run",
            "assertions": [{
                "assertion_id": "missing-body-position",
                "kind": "body_scalar_in_range",
                "body_id": 999,
                "metric": "position",
                "axis": "x",
                "min_milli": 0,
                "max_milli": 1
            }]
        }),
    )?;
    let empty = fail_closed_edge(
        &root,
        &state_path,
        &aster_dir,
        "empty-ledger",
        json!({
            "verdict_id": "issue1171-manual-empty",
            "run_id": "issue1171-manual-run",
            "assertions": []
        }),
    )?;
    let malformed = fail_closed_edge(
        &root,
        &state_path,
        &aster_dir,
        "malformed-ledger",
        json!({
            "verdict_id": "issue1171-manual-malformed",
            "run_id": "issue1171-manual-run",
            "assertions": [{
                "assertion_id": "bad-range-declaration",
                "kind": "body_scalar_in_range",
                "body_id": 0,
                "metric": "position",
                "axis": "x",
                "min_milli": 3,
                "max_milli": 2
            }]
        }),
    )?;

    let state = read_mcp_harness_state(&state_path)?;
    let read_harness =
        call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path }))?;
    let good_aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1171-good-ledger")?
        .ok_or("good assertion Aster row missing")?;
    let bad_aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1171-bad-ledger")?
        .ok_or("bad assertion Aster row missing")?;
    let missing_aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1171-missing-ledger")?
        .ok_or("missing-body assertion Aster row missing")?;
    let report = json!({
        "issue": 1171,
        "source_of_truth": {
            "state_path": state_path,
            "run_ledger_path": root.join("run-ledger.json"),
            "good_ledger_path": root.join("good-ledger.json"),
            "bad_ledger_path": root.join("bad-ledger.json"),
            "missing_ledger_path": root.join("missing-ledger.json"),
            "aster_dir": aster_dir
        },
        "initial": initial,
        "after": state_summary(&state_path),
        "run": run.result["run"],
        "good": good.result["assertion_verdict"],
        "bad": bad.result["assertion_verdict"],
        "missing": missing.result["assertion_verdict"],
        "empty_assertions": empty,
        "malformed_range": malformed,
        "read_harness": read_harness.result,
        "state_readback": {
            "schema": state.state.schema,
            "sha256": state.sha256,
            "run_count": state.run_count,
            "assertion_count": state.assertion_count,
            "event_count": state.event_count,
            "body0_position_x_raw": state.state.runs[0].final_snapshot.bodies[0].position.x.raw()
        },
        "aster": {
            "good": good_aster,
            "bad": bad_aster,
            "missing": missing_aster
        }
    });
    let report_path = root.join("manual-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "manual issue1171 report={} state_sha={} report_sha={}",
        report_path.display(),
        state.sha256,
        sha256_file(&report_path)?
    );
    Ok(())
}

fn assert_state(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> Result<litd_editor::mcp_harness_tools::McpHarnessToolCallReadback, Box<dyn Error>> {
    Ok(call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(root, state_path, aster_dir, ledger_id, payload),
    )?)
}

fn fail_closed_edge(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    payload: Value,
) -> Result<Value, Box<dyn Error>> {
    let before = state_summary(state_path);
    let error = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(root, state_path, aster_dir, ledger_id, payload),
    )
    .expect_err("edge must fail closed")
    .to_string();
    let after = state_summary(state_path);
    if before != after {
        return Err(format!("edge {ledger_id} mutated state before failure").into());
    }
    Ok(json!({"ledger_id": ledger_id, "before": before, "after": after, "error": error}))
}

fn with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    mut value: Value,
) -> Value {
    let object = value
        .as_object_mut()
        .expect("context target must be object");
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1171-manual"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1171-manual-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1171-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) -> Result<(), Box<dyn Error>> {
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1171-manual-project-save",
            "requested_by": "litd-editor-issue1171-manual",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1171-manual-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1171-manual-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1171_manual",
            "title": "Harness Issue 1171 Manual",
            "version": "1.0.0",
            "tags": ["agent_harness", "assertions", "manual_fsv"],
            "project_path": project_path,
            "snapshot_label": "manual-assertion-source",
            "document": fixture_document()?
        }),
    )?;
    Ok(())
}

fn fixture_document() -> Result<EditorDocument, Box<dyn Error>> {
    let mut document =
        EditorDocument::new("calyx://world/issue1171-manual", "issue1171-manual-rev")?;
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
            Transform::from_translation(Vec3::new(1.0, 0.0, -2.0)),
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
        .ok_or("missing real vault record Rock1.fbx")?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}

fn state_summary(state_path: &Path) -> Value {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "run_count": state.as_ref().map(|read| read.run_count).unwrap_or_default(),
        "assertion_count": state.as_ref().map(|read| read.assertion_count).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default()
    })
}

fn clean_dir(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
