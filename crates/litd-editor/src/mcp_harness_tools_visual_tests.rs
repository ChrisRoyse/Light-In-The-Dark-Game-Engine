use std::{
    fs,
    path::{Path, PathBuf},
};

use image::{ColorType, ImageFormat};
use litd_engine::render::reference::{
    ReferenceSceneSettings, build_reference_scene_plan, render_reference_scene_image,
};
use serde_json::{Value, json};

use crate::mcp_harness_tools::{
    MCP_HARNESS_STATE_SCHEMA, READ_HARNESS_TOOL, SCREENSHOT_DIFF_TOOL, call_mcp_harness_tool,
    read_mcp_harness_aster_ledger, read_mcp_harness_state,
};

#[test]
fn issue1175_headless_reference_scene_diff_reads_real_pixels_and_aster_rows() {
    let root = test_root("headless_reference");
    clean_dir(&root);
    fs::create_dir_all(&root).unwrap();
    let baseline_path = root.join("golden.png");
    let candidate_path = root.join("candidate.png");
    write_reference_png(&baseline_path, 320, 180);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    print_state_summary("issue1175 headless before", &state_path, &aster_dir);
    let diff = call_mcp_harness_tool(
        SCREENSHOT_DIFF_TOOL,
        json_with_context(
            &state_path,
            &aster_dir,
            "headless-ledger",
            json!({
                "diff_id": "issue1175-headless-reference",
                "baseline_path": baseline_path,
                "candidate_path": candidate_path,
                "max_changed_bytes": 0,
                "max_mean_abs_error": 0.0,
                "render_reference_scene": true,
                "render_width": 320,
                "render_height": 180
            }),
        ),
    )
    .unwrap();
    let record = &diff.result["screenshot_diff"];
    assert_eq!(record["verdict"], "passed");
    assert_eq!(record["changed_bytes"], 0);
    assert_eq!(record["changed_pixels"], 0);
    assert_eq!(record["mean_abs_error"], 0.0);
    assert_eq!(record["width"], 320);
    assert_eq!(record["height"], 180);
    assert!(diff.ledger_readback.as_ref().unwrap().aster.seq > 0);

    let state = read_mcp_harness_state(&state_path).unwrap();
    assert_eq!(state.state.schema, MCP_HARNESS_STATE_SCHEMA);
    assert_eq!(state.state.screenshot_diffs.len(), 1);
    assert_eq!(state.state.screenshot_diffs[0].verdict, "passed");
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1175-headless-ledger")
        .unwrap()
        .expect("screenshot diff ledger should be written to Aster");
    assert_eq!(aster.sha256, diff.ledger_readback.as_ref().unwrap().sha256);
    assert!(aster.graph_rows >= 2);

    let read =
        call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.state_readback.unwrap().state.screenshot_diffs.len(), 1);
    print_state_summary("issue1175 headless after", &state_path, &aster_dir);
}

#[test]
fn issue1175_visual_diff_edges_flag_failures_and_preserve_state() {
    let root = test_root("edges");
    clean_dir(&root);
    fs::create_dir_all(&root).unwrap();
    let baseline_path = root.join("baseline.png");
    let identical_path = root.join("identical.png");
    let changed_path = root.join("changed.png");
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    write_solid_png(&baseline_path, 16, 16, [16, 32, 48, 255]);
    write_solid_png(&identical_path, 16, 16, [16, 32, 48, 255]);
    write_solid_png(&changed_path, 16, 16, [250, 32, 48, 255]);

    print_state_summary("issue1175 identical before", &state_path, &aster_dir);
    let identical = call_mcp_harness_tool(
        SCREENSHOT_DIFF_TOOL,
        json_with_context(
            &state_path,
            &aster_dir,
            "identical-ledger",
            json!({
                "diff_id": "issue1175-identical",
                "baseline_path": baseline_path,
                "candidate_path": identical_path,
                "max_changed_bytes": 0,
                "max_mean_abs_error": 0.0
            }),
        ),
    )
    .unwrap();
    assert_eq!(identical.result["screenshot_diff"]["verdict"], "passed");
    assert_eq!(identical.result["screenshot_diff"]["changed_bytes"], 0);
    print_state_summary("issue1175 identical after", &state_path, &aster_dir);

    print_state_summary("issue1175 changed before", &state_path, &aster_dir);
    let changed = call_mcp_harness_tool(
        SCREENSHOT_DIFF_TOOL,
        json_with_context(
            &state_path,
            &aster_dir,
            "changed-ledger",
            json!({
                "diff_id": "issue1175-changed",
                "baseline_path": baseline_path,
                "candidate_path": changed_path,
                "max_changed_bytes": 0,
                "max_mean_abs_error": 0.0
            }),
        ),
    )
    .unwrap();
    assert_eq!(changed.result["screenshot_diff"]["verdict"], "failed");
    assert_eq!(changed.result["screenshot_diff"]["changed_pixels"], 256);
    assert!(
        changed.result["screenshot_diff"]["mean_luma_error"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    print_state_summary("issue1175 changed after", &state_path, &aster_dir);

    let before_missing = state_summary(&state_path, &aster_dir);
    println!(
        "issue1175 missing before {}",
        serde_json::to_string(&before_missing).unwrap()
    );
    let missing = call_mcp_harness_tool(
        SCREENSHOT_DIFF_TOOL,
        json_with_context(
            &state_path,
            &aster_dir,
            "missing-ledger",
            json!({
                "diff_id": "issue1175-missing",
                "baseline_path": root.join("missing-reference.png"),
                "candidate_path": changed_path,
                "max_changed_bytes": 0,
                "max_mean_abs_error": 0.0
            }),
        ),
    )
    .unwrap_err()
    .to_string();
    assert!(missing.contains("screenshot.baseline.read"));
    let after_missing = state_summary(&state_path, &aster_dir);
    println!(
        "issue1175 missing after {}",
        serde_json::to_string(&after_missing).unwrap()
    );
    assert_eq!(before_missing, after_missing);

    let state = read_mcp_harness_state(&state_path).unwrap();
    assert_eq!(state.state.screenshot_diffs.len(), 2);
    assert_eq!(state.event_count, 2);
    assert!(
        read_mcp_harness_aster_ledger(&aster_dir, "issue1175-missing-ledger")
            .unwrap()
            .is_none()
    );
    println!(
        "issue1175 edges state {}",
        serde_json::to_string(&state_summary(&state_path, &aster_dir)).unwrap()
    );
}

fn json_with_context(state_path: &Path, aster_dir: &Path, ledger_id: &str, extra: Value) -> Value {
    let mut value = extra;
    let object = value.as_object_mut().unwrap();
    object.insert(
        "request_id".to_string(),
        json!(format!("issue1175-{ledger_id}")),
    );
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1175-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1175-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(state_path.with_file_name(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1175-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn write_reference_png(path: &Path, width: u32, height: u32) {
    let plan = build_reference_scene_plan(ReferenceSceneSettings { width, height }).unwrap();
    let image = render_reference_scene_image(&plan).unwrap();
    let mut rgba = Vec::with_capacity(image.pixels().len() * 4);
    for pixel in image.pixels() {
        rgba.extend_from_slice(pixel);
    }
    image::save_buffer_with_format(
        path,
        &rgba,
        width,
        height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )
    .unwrap();
}

fn write_solid_png(path: &Path, width: u32, height: u32, pixel: [u8; 4]) {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        rgba.extend_from_slice(&pixel);
    }
    image::save_buffer_with_format(
        path,
        &rgba,
        width,
        height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )
    .unwrap();
}

fn state_summary(state_path: &Path, aster_dir: &Path) -> Value {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "diff_count": state.as_ref().map(|read| read.state.screenshot_diffs.len()).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_dir.exists()
    })
}

fn print_state_summary(label: &str, state_path: &Path, aster_dir: &Path) {
    println!(
        "{label} {}",
        serde_json::to_string(&state_summary(state_path, aster_dir)).unwrap()
    );
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1175_visual_screenshot_diff", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
}
