use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        ASSERT_STATE_TOOL, READ_EVENTS_TOOL, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL, RUN_PEV_TOOL,
        SCREENSHOT_DIFF_TOOL, call_mcp_harness_tool, read_mcp_harness_aster_ledger,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let root = Path::new("target")
        .join("fsv")
        .join("issue1178_mcp_harness_tools")
        .join("manual");
    clean_dir(&root)?;
    let project_path = root.join("harness-world.litdworld.json");
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    create_project(&root, &project_path)?;

    let before = state_summary(&state_path);
    println!("manual issue1178 before {}", before);

    let run = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "manual-run-ledger",
            json!({
                "run_id": "manual-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 32
            }),
        ),
    )?;

    let assertion = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "manual-assert-ledger",
            json!({
                "verdict_id": "manual-assert",
                "run_id": "manual-run",
                "assertions": [
                    {"assertion_id": "final-tick", "kind": "final_tick_equals", "expected_u64": 32},
                    {"assertion_id": "spawned", "kind": "spawned_bodies_equals", "expected_u64": 1}
                ]
            }),
        ),
    )?;

    let screenshot = call_mcp_harness_tool(
        SCREENSHOT_DIFF_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "manual-screenshot-ledger",
            json!({
                "diff_id": "manual-real-png",
                "baseline_path": real_png_in(&root),
                "candidate_path": real_png_in(&root),
                "max_changed_bytes": 0
            }),
        ),
    )?;

    let pev = call_mcp_harness_tool(
        RUN_PEV_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "manual-pev-ledger",
            json!({
                "pev_id": "manual-pev",
                "run_id": "manual-pev-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 12,
                "assertions": [
                    {"assertion_id": "pev-final-tick", "kind": "final_tick_equals", "expected_u64": 12},
                    {"assertion_id": "pev-spawned", "kind": "spawned_bodies_equals", "expected_u64": 1}
                ],
                "oracles": ["deterministic_replay", "presentation_non_empty"]
            }),
        ),
    )?;

    let failed_before = state_summary(&state_path);
    let failed_pev = call_mcp_harness_tool(
        RUN_PEV_TOOL,
        with_context(
            &root,
            &state_path,
            &aster_dir,
            "manual-failed-pev-ledger",
            json!({
                "pev_id": "manual-failed-pev",
                "run_id": "manual-failed-pev-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 6,
                "assertions": [
                    {"assertion_id": "wrong-final-tick", "kind": "final_tick_equals", "expected_u64": 999}
                ],
                "oracles": ["deterministic_replay"]
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    let failed_after = state_summary(&state_path);

    let read_events = call_mcp_harness_tool(
        READ_EVENTS_TOOL,
        json!({"state_path": state_path, "after_event_index": 0, "limit": 20}),
    )?;
    let read_harness = call_mcp_harness_tool(READ_HARNESS_TOOL, json!({"state_path": state_path}))?;
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1178-manual-pev-ledger")?
        .ok_or("missing manual PEV Aster ledger")?;
    let after = state_summary(&state_path);

    let report = json!({
        "source_of_truth": "harness-state.json + manual-*-ledger.json + Aster agent_harness ledger rows",
        "before": before,
        "after": after,
        "run": run.result,
        "assertion": assertion.result,
        "screenshot": screenshot.result,
        "pev": pev.result,
        "failed_pev": {
            "before": failed_before,
            "after": failed_after,
            "error": failed_pev
        },
        "read_events": read_events.result,
        "read_harness": read_harness.result,
        "aster_pev": aster
    });
    let report_path = root.join("manual-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    println!("manual issue1178 report={}", report_path.display());
    println!("manual issue1178 after {}", after);
    Ok(())
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
        json!("litd-editor-issue1178-manual-fsv"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert(
        "state_id".to_string(),
        json!("issue1178-manual-harness-state"),
    );
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1178-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let document = fixture_document()?;
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1178-manual-project-save",
            "requested_by": "litd-editor-issue1178-manual-fsv",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1178-manual-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1178-manual-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1178_manual",
            "title": "Harness Issue 1178 Manual",
            "version": "1.0.0",
            "tags": ["agent_harness", "manual_fsv"],
            "project_path": project_path,
            "snapshot_label": "manual-harness-source",
            "document": document
        }),
    )?;
    Ok(())
}

fn fixture_document() -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new(
        "calyx://world/issue1178-manual-harness",
        "issue1178-manual-rev",
    )?;
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
    let bytes = fs::read(state_path).ok();
    let json = bytes
        .as_ref()
        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": bytes.as_ref().map(|bytes| bytes.len()).unwrap_or_default(),
        "state_sha256": bytes.as_ref().map(|bytes| sha256_hex(bytes)),
        "run_count": json_array_len(&json, "runs"),
        "assertion_count": json_array_len(&json, "assertions"),
        "event_count": json_array_len(&json, "events"),
        "pev_count": json_array_len(&json, "pev_runs")
    })
}

fn json_array_len(json: &Option<Value>, key: &str) -> usize {
    json.as_ref()
        .and_then(|value| value.get(key))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

// #1765: stage a real PNG INSIDE the confinement root instead of the out-of-git-tree
// `local-asset-vault` (which lives beside the PRIMARY checkout, so the #1518-confined
// screenshot-diff tool rejects it from any linked worktree and a vault-less box cannot read it).
// A real generated PNG under `root` keeps the path in-root on any checkout and still exercises
// real PNG decode + pixel diff.
fn real_png_in(root: &Path) -> PathBuf {
    let path = root.join("harness-baseline.png");
    let (width, height) = (8_u32, 8_u32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[(x * 24) as u8, (y * 24) as u8, 96, 255]);
        }
    }
    image::save_buffer_with_format(
        &path,
        &rgba,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .expect("stage a real PNG inside the harness FSV confinement root");
    path
}

fn clean_dir(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}
