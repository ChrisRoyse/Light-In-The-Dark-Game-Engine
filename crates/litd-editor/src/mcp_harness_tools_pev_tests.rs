use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        RUN_PEV_TOOL, call_mcp_harness_tool, read_mcp_harness_aster_ledger,
        read_mcp_harness_ledger, read_mcp_harness_state, validate_harness_pev_transcript,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};

#[test]
fn issue1177_pev_loop_converges_and_records_hash_chain() {
    let root = test_root("happy");
    clean_dir(&root);
    let project_path = root.join("harness-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    let ledger_path = root.join("happy-ledger.json");

    let before = state_summary(&state_path);
    println!("issue1177 happy before {}", before);

    let result = call_mcp_harness_tool(
        RUN_PEV_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            "happy-ledger",
            loop_payload(
                &project_path,
                "happy-transcript",
                "happy-pev",
                "happy-run",
                2,
                json!([
                    {
                        "turn_id": "buggy-ticks",
                        "author_delta": "seeded bug keeps tick budget at four",
                        "changed_paths": ["scenario.tick_budget"],
                        "ticks": 4,
                        "assertions": assertions_for_tick(8),
                        "oracles": ["deterministic_replay", "presentation_non_empty", "no_error_verdict"]
                    },
                    {
                        "turn_id": "fix-ticks",
                        "author_delta": "change only scenario.tick_budget from four to eight",
                        "changed_paths": ["scenario.tick_budget"],
                        "ticks": 8,
                        "assertions": assertions_for_tick(8),
                        "oracles": ["deterministic_replay", "presentation_non_empty", "no_error_verdict"]
                    }
                ]),
            ),
        ),
    )
    .unwrap();
    assert_eq!(
        result.result["pev_transcript"]["final_verdict"].as_str(),
        Some("passed")
    );

    let state = read_mcp_harness_state(&state_path).unwrap();
    let transcript = state.state.pev_transcripts.first().unwrap();
    validate_harness_pev_transcript(transcript).unwrap();
    assert_eq!(state.run_count, 1);
    assert_eq!(state.assertion_count, 1);
    assert_eq!(state.oracle_run_count, 1);
    assert_eq!(state.pev_transcript_count, 1);
    assert_eq!(transcript.turn_count, 2);
    assert_eq!(transcript.accepted_turn_id.as_deref(), Some("fix-ticks"));
    assert!(!transcript.turns[0].passed);
    assert!(transcript.turns[1].passed);
    assert_eq!(transcript.turns[1].rerun_scope, "changed_only");
    assert_eq!(
        transcript.turns[1].previous_turn_hash,
        transcript.turns[0].turn_hash
    );
    assert_eq!(state.state.runs[0].final_tick, 8);

    let ledger = read_mcp_harness_ledger(&ledger_path).unwrap();
    assert_eq!(ledger.ledger.records[0].transcript_turns.len(), 2);
    assert_eq!(ledger.ledger.records[0].state_sha256, state.sha256);
    let aster = read_mcp_harness_aster_ledger(&aster_dir, "issue1177-happy-ledger")
        .unwrap()
        .expect("happy ledger should exist in Aster");
    assert_eq!(aster.sha256, ledger.sha256);
    assert!(aster.graph_rows >= 2);

    let after = state_summary(&state_path);
    println!("issue1177 happy after {}", after);
}

#[test]
fn issue1177_pev_loop_edges_record_halts_and_reverts() {
    let non_converging = run_edge_case(
        "non_converging",
        1,
        json!([
            {
                "turn_id": "still-red",
                "author_delta": "leave the tick budget below acceptance criteria",
                "changed_paths": ["scenario.tick_budget"],
                "ticks": 2,
                "assertions": assertions_for_tick(9),
                "oracles": ["deterministic_replay", "presentation_non_empty"]
            }
        ]),
    );
    assert_eq!(non_converging.0.final_verdict, "halted_non_converging");
    assert_eq!(non_converging.1.run_count, 0);
    assert!(non_converging.0.halt_reason.is_some());

    let no_op = run_edge_case(
        "no_op",
        2,
        json!([
            {
                "turn_id": "first-red",
                "author_delta": "miss the target tick",
                "changed_paths": ["scenario.tick_budget"],
                "ticks": 3,
                "assertions": assertions_for_tick(12),
                "oracles": ["deterministic_replay", "presentation_non_empty"]
            },
            {
                "turn_id": "second-red",
                "author_delta": "renamed turn but left the functional proposal unchanged",
                "changed_paths": ["scenario.tick_budget"],
                "ticks": 3,
                "assertions": assertions_for_tick(12),
                "oracles": ["deterministic_replay", "presentation_non_empty"]
            }
        ]),
    );
    assert_eq!(no_op.0.final_verdict, "halted_no_op");
    assert!(no_op.0.turns[1].no_op);
    assert_eq!(no_op.1.run_count, 0);

    let regression = run_edge_case(
        "regression",
        2,
        json!([
            {
                "turn_id": "green",
                "author_delta": "set the tick budget to the accepted value",
                "changed_paths": ["scenario.tick_budget"],
                "ticks": 4,
                "assertions": assertions_for_tick(4),
                "oracles": ["deterministic_replay", "presentation_non_empty"]
            },
            {
                "turn_id": "regressed",
                "author_delta": "overshoot the tick budget after a green turn",
                "changed_paths": ["scenario.tick_budget"],
                "ticks": 5,
                "assertions": assertions_for_tick(4),
                "oracles": ["deterministic_replay", "presentation_non_empty"]
            }
        ]),
    );
    assert_eq!(regression.0.final_verdict, "halted_regression_reverted");
    assert_eq!(regression.0.reverted_to_turn_id.as_deref(), Some("green"));
    assert!(regression.0.turns[1].regression);
    assert_eq!(regression.1.run_count, 1);
    assert_eq!(regression.1.state.runs[0].final_tick, 4);
}

fn run_edge_case(
    name: &str,
    max_turns: u32,
    turns: Value,
) -> (
    crate::mcp_harness_tools::HarnessPevTranscriptRecord,
    crate::mcp_harness_tools::mcp_harness_tools_store::McpHarnessStateReadback,
) {
    let root = test_root(name);
    clean_dir(&root);
    let project_path = root.join("harness-world.litdworld.json");
    create_project(&root, &project_path);
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    let ledger_id = format!("{name}-ledger");
    let before = state_summary(&state_path);
    println!("issue1177 {name} before {}", before);

    let result = call_mcp_harness_tool(
        RUN_PEV_TOOL,
        json_with_context(
            &root,
            &state_path,
            &aster_dir,
            &ledger_id,
            loop_payload(
                &project_path,
                &format!("{name}-transcript"),
                &format!("{name}-pev"),
                &format!("{name}-run"),
                max_turns,
                turns,
            ),
        ),
    )
    .unwrap();
    let readback = read_mcp_harness_state(&state_path).unwrap();
    let transcript = readback.state.pev_transcripts.first().unwrap().clone();
    validate_harness_pev_transcript(&transcript).unwrap();
    let ledger_path = root.join(format!("{ledger_id}.json"));
    let ledger = read_mcp_harness_ledger(ledger_path).unwrap();
    assert_eq!(
        ledger.ledger.records[0].transcript_turns.len(),
        transcript.turn_count
    );
    let aster = read_mcp_harness_aster_ledger(&aster_dir, &format!("issue1177-{ledger_id}"))
        .unwrap()
        .expect("edge ledger should exist in Aster");
    assert_eq!(aster.sha256, ledger.sha256);
    assert_eq!(
        result.result["pev_transcript"]["final_verdict"].as_str(),
        Some(transcript.final_verdict.as_str())
    );
    let after = state_summary(&state_path);
    println!("issue1177 {name} after {}", after);
    (transcript, readback)
}

fn loop_payload(
    project_path: &Path,
    transcript_id: &str,
    pev_id: &str,
    run_id: &str,
    max_turns: u32,
    turns: Value,
) -> Value {
    json!({
        "pev_id": pev_id,
        "run_id": run_id,
        "project_path": project_path,
        "camera_rig": "TopDown",
        "ticks": 1,
        "assertions": assertions_for_tick(1),
        "oracles": ["deterministic_replay", "presentation_non_empty"],
        "transcript_id": transcript_id,
        "intent": "drive a seeded buggy PEV loop to a recorded verdict",
        "acceptance_criteria": ["final tick matches the expected value", "spawned body remains present"],
        "max_turns": max_turns,
        "turns": turns
    })
}

fn assertions_for_tick(tick: u64) -> Value {
    json!([
        {"assertion_id": "final-tick", "kind": "final_tick_equals", "expected_u64": tick},
        {"assertion_id": "spawned", "kind": "spawned_bodies_equals", "expected_u64": 1}
    ])
}

fn json_with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    mut value: Value,
) -> Value {
    let object = value.as_object_mut().unwrap();
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1177-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1177-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1177-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn create_project(root: &Path, project_path: &Path) {
    let document = fixture_document();
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1177-project-save",
            "requested_by": "litd-editor-issue1177-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1177-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1177-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1177",
            "title": "Harness Issue 1177",
            "version": "1.0.0",
            "tags": ["agent_harness", "pev_loop", "fsv"],
            "project_path": project_path,
            "snapshot_label": "harness-source",
            "document": document
        }),
    )
    .unwrap();
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1177-harness", "issue1177-rev-1").unwrap();
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
                Transform::from_translation(Vec3::new(1.0, 0.0, -2.0)),
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
        .expect("real vault must contain Quaternius Rock1");
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    document
}

fn state_summary(state_path: &Path) -> String {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "run_count": state.as_ref().map(|read| read.run_count).unwrap_or_default(),
        "assertion_count": state.as_ref().map(|read| read.assertion_count).unwrap_or_default(),
        "oracle_run_count": state.as_ref().map(|read| read.oracle_run_count).unwrap_or_default(),
        "pev_transcript_count": state.as_ref().map(|read| read.pev_transcript_count).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default()
    })
    .to_string()
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1177_pev_loop", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
