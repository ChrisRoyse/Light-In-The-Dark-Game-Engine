use super::*;

#[test]
fn issue1178_agent_harness_tools_full_cycle_and_read_real_state() {
    let root = test_root("unit");
    clean_dir(&root);
    let project_path = root.join("harness-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");

    let before = state_summary(&state_path, &aster_dir);
    println!(
        "issue1178 before {}",
        serde_json::to_string(&before).unwrap()
    );

    let run = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "run-ledger",
            json!({
                "run_id": "issue1178-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 64
            }),
        ),
    )
    .unwrap();
    assert_eq!(run.state_readback.as_ref().unwrap().run_count, 1);
    assert_eq!(
        run.state_readback.as_ref().unwrap().state.runs[0].final_tick,
        64
    );
    assert!(run.ledger_readback.as_ref().unwrap().aster.seq > 0);

    call_mcp_harness_tool(
        SNAPSHOT_BRANCH_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "branch-ledger",
            json!({
                "run_id": "issue1178-run",
                "snapshot_id": "issue1178-snapshot",
                "branch_id": "issue1178-branch",
                "branch_ticks": 8
            }),
        ),
    )
    .unwrap();

    let assertion = call_mcp_harness_tool(
        ASSERT_STATE_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "assert-ledger",
            json!({
                "verdict_id": "issue1178-assert",
                "run_id": "issue1178-run",
                "assertions": [
                    {"assertion_id": "final-tick", "kind": "final_tick_equals", "expected_u64": 64},
                    {"assertion_id": "spawned", "kind": "spawned_bodies_equals", "expected_u64": 1},
                    {"assertion_id": "verdict", "kind": "verdict_equals", "expected_string": "passed"}
                ]
            }),
        ),
    )
    .unwrap();
    assert_eq!(
        assertion.result["assertion_verdict"]["verdict"].as_str(),
        Some("passed")
    );

    call_mcp_harness_tool(
        RUN_ORACLES_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "oracle-ledger",
            json!({
                "oracle_run_id": "issue1178-oracles",
                "run_id": "issue1178-run",
                "oracles": ["deterministic_replay", "presentation_non_empty", "no_error_verdict"]
            }),
        ),
    )
    .unwrap();

    call_mcp_harness_tool(
        DRIVE_BOT_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "bot-ledger",
            json!({
                "bot_run_id": "issue1178-bot",
                "run_id": "issue1178-run",
                "bot_id": "lowered-agent-1",
                "driver_kind": "heuristic",
                "objective": {"kind": "reach_position_x_at_least", "min_x_milli": 1050},
                "expected_outcome": "completed",
                "ticks": 4,
                "sample_interval_ticks": 1,
                "velocity_x_milli": 1250,
                "velocity_z_milli": 0
            }),
        ),
    )
    .unwrap();

    call_mcp_harness_tool(
        SCREENSHOT_DIFF_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "screenshot-ledger",
            json!({
                "diff_id": "issue1178-same-png",
                "baseline_path": real_png_in(&root),
                "candidate_path": real_png_in(&root),
                "max_changed_bytes": 0
            }),
        ),
    )
    .unwrap();

    call_mcp_harness_tool(
        CALYX_SIGNALS_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "signals-ledger",
            json!({
                "signal_batch_id": "issue1178-signals",
                "run_id": "issue1178-run"
            }),
        ),
    )
    .unwrap();

    let pev = call_mcp_harness_tool(
        RUN_PEV_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "pev-ledger",
            json!({
                "pev_id": "issue1178-pev",
                "run_id": "issue1178-pev-run",
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": 16,
                "assertions": [
                    {"assertion_id": "pev-final-tick", "kind": "final_tick_equals", "expected_u64": 16},
                    {"assertion_id": "pev-spawned", "kind": "spawned_bodies_equals", "expected_u64": 1}
                ],
                "oracles": ["deterministic_replay", "presentation_non_empty"]
            }),
        ),
    )
    .unwrap();
    assert_eq!(pev.result["pev"]["verdict"].as_str(), Some("passed"));

    let read_events = call_mcp_harness_tool(
        READ_EVENTS_TOOL,
        json!({
            "state_path": state_path,
            "after_event_index": 0,
            "limit": 20
        }),
    )
    .unwrap();
    assert_eq!(read_events.result["events"].as_array().unwrap().len(), 8);

    let read_state =
        call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path })).unwrap();
    let state = &read_state.state_readback.as_ref().unwrap().state;
    assert_eq!(state.runs.len(), 2);
    assert_eq!(state.snapshots.len(), 1);
    assert_eq!(state.assertions.len(), 2);
    assert_eq!(state.oracle_runs.len(), 2);
    assert_eq!(state.bot_runs.len(), 1);
    assert_eq!(state.screenshot_diffs.len(), 1);
    assert_eq!(state.calyx_signals.len(), 2);
    assert_eq!(state.pev_runs.len(), 1);
    assert_eq!(state.events.len(), 8);

    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1178-pev-ledger")
        .unwrap()
        .expect("PEV ledger should exist in Aster");
    assert_eq!(aster.sha256, pev.ledger_readback.as_ref().unwrap().sha256);
    assert!(aster.graph_rows >= 2);

    let after = state_summary(&state_path, &aster_dir);
    println!("issue1178 after {}", serde_json::to_string(&after).unwrap());
}
