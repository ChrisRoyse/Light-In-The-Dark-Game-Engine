use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        HarnessPevTranscriptRecord, RUN_PEV_TOOL, call_mcp_harness_tool,
        read_mcp_harness_aster_ledger, read_mcp_harness_ledger, validate_harness_pev_transcript,
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
        .join("issue1177_pev_loop")
        .join("manual");
    clean_dir(&root)?;

    let reports = vec![
        run_case(
            &root,
            "happy",
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
        )?,
        run_case(
            &root,
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
        )?,
        run_case(
            &root,
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
        )?,
        run_case(
            &root,
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
        )?,
    ];

    let report = json!({
        "source_of_truth": "per-case harness-state.json + per-case ledger JSON + Aster agent_harness ledger rows",
        "cases": reports
    });
    let report_path = root.join("manual-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    println!("manual issue1177 report={}", report_path.display());
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_case(
    root: &Path,
    name: &str,
    max_turns: u32,
    turns: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let case_root = root.join(name);
    clean_dir(&case_root)?;
    let project_path = case_root.join("harness-world.litdworld.json");
    let state_path = case_root.join("harness-state.json");
    let aster_dir = case_root.join("aster");
    let ledger_id = format!("{name}-ledger");
    let ledger_path = case_root.join(format!("{ledger_id}.json"));
    create_project(&case_root, &project_path, name)?;
    let before = state_summary(&state_path)?;
    println!("manual issue1177 {name} before {}", before);

    let result = call_mcp_harness_tool(
        RUN_PEV_TOOL,
        with_context(
            &case_root,
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
    )?;

    let state_bytes = fs::read(&state_path)?;
    let state_value: Value = serde_json::from_slice(&state_bytes)?;
    let transcript: HarnessPevTranscriptRecord =
        serde_json::from_value(state_value["pev_transcripts"][0].clone())?;
    validate_harness_pev_transcript(&transcript)?;
    let ledger = read_mcp_harness_ledger(&ledger_path)?;
    let aster = read_mcp_harness_aster_ledger(&aster_dir, &format!("issue1177-{ledger_id}"))?
        .ok_or("missing Aster ledger row")?;
    if ledger.ledger.records[0].state_sha256 != sha256_hex(&state_bytes) {
        return Err("ledger state SHA did not match physical state file".into());
    }
    if aster.sha256 != ledger.sha256 {
        return Err("Aster row SHA did not match physical ledger file".into());
    }
    let after = state_summary(&state_path)?;
    println!("manual issue1177 {name} after {}", after);

    Ok(json!({
        "case": name,
        "before": before,
        "after": after,
        "tool_result_final_verdict": result.result["pev_transcript"]["final_verdict"],
        "transcript": {
            "id": transcript.transcript_id,
            "turn_count": transcript.turn_count,
            "final_verdict": transcript.final_verdict,
            "halt_reason": transcript.halt_reason,
            "accepted_turn_id": transcript.accepted_turn_id,
            "reverted_to_turn_id": transcript.reverted_to_turn_id,
            "final_turn_hash": transcript.final_turn_hash,
            "turns": transcript.turns
        },
        "ledger": {
            "path": ledger.path,
            "sha256": ledger.sha256,
            "state_sha256": ledger.ledger.records[0].state_sha256,
            "transcript_turn_count": ledger.ledger.records[0].transcript_turns.len()
        },
        "aster": aster
    }))
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
        json!("litd-editor-issue1177-manual-fsv"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert(
        "state_id".to_string(),
        json!("issue1177-manual-harness-state"),
    );
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

fn create_project(
    root: &Path,
    project_path: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let document = fixture_document(name)?;
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": format!("issue1177-{name}-project-save"),
            "requested_by": "litd-editor-issue1177-manual-fsv",
            "state_path": root.join("project-state.json"),
            "state_id": format!("issue1177-{name}-project-state"),
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": format!("issue1177-{name}-project-ledger"),
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": format!("project.harness_issue1177_{name}"),
            "title": format!("Harness Issue 1177 {name}"),
            "version": "1.0.0",
            "tags": ["agent_harness", "pev_loop", "manual_fsv"],
            "project_path": project_path,
            "snapshot_label": "manual-harness-source",
            "document": document
        }),
    )?;
    Ok(())
}

fn fixture_document(name: &str) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new(
        format!("calyx://world/issue1177-{name}-harness"),
        format!("issue1177-{name}-rev"),
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

fn state_summary(state_path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(state_path).ok();
    let state = bytes
        .as_ref()
        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
    Ok(json!({
        "state_exists": state_path.exists(),
        "state_bytes": bytes.as_ref().map(Vec::len).unwrap_or_default(),
        "state_sha256": bytes.as_ref().map(|bytes| sha256_hex(bytes)),
        "run_count": array_len(&state, "runs"),
        "assertion_count": array_len(&state, "assertions"),
        "oracle_run_count": array_len(&state, "oracle_runs"),
        "pev_run_count": array_len(&state, "pev_runs"),
        "pev_transcript_count": array_len(&state, "pev_transcripts"),
        "event_count": array_len(&state, "events")
    }))
}

fn array_len(state: &Option<Value>, key: &str) -> usize {
    state
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn clean_dir(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}
