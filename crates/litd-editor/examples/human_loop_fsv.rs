use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    human_loop::{
        HumanLoopActorKind, HumanLoopAgentVerdict, HumanLoopEntryPoint, HumanLoopFeedback,
        HumanLoopLaunchRequest, HumanLoopPermission, HumanLoopSessionConfig,
        capture_human_feedback, compare_human_loop_gui_tool_parity, launch_human_loop_playtest,
        parse_human_loop_ledger, read_human_loop_ledger, readback_human_loop_session,
        record_agent_verdict, write_human_loop_ledger,
    },
    play::PlayCameraRig,
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1188_FSV_ROOT",
        "issue1188_human_loop",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let artifact_path = root.join("issue1188-human-loop-readback.json");
    let document = fixture_document()?;

    let mut tool_ledger = launch_human_loop_playtest(
        &document,
        PlayCameraRig::TopDown,
        launch_request(
            "issue1188-manual-tool-ledger",
            "issue1188-manual-tool-session",
            HumanLoopEntryPoint::McpTool,
            "agent.playtest.orchestrator",
        ),
    )?;
    tool_ledger = capture_human_feedback(
        &tool_ledger,
        feedback("issue1188-manual-tool-session", "manual-feedback"),
        HumanLoopEntryPoint::Gui,
    )?;
    tool_ledger = record_agent_verdict(
        &tool_ledger,
        verdict("issue1188-manual-tool-session", "manual-verdict"),
        HumanLoopEntryPoint::McpTool,
    )?;
    println!(
        "happy_path before_write session={} events={} feedback={} verdicts={}",
        tool_ledger.readback.session_id,
        tool_ledger.event_count,
        tool_ledger.readback.human_feedback_count,
        tool_ledger.readback.agent_verdict_count
    );
    let tool_state_path = root.join("tool-human-loop-ledger.json");
    let tool_writeback = write_human_loop_ledger(&tool_state_path, &aster_dir, &tool_ledger)?;
    let tool_aster = read_human_loop_ledger(&aster_dir, &tool_ledger.ledger_id)?
        .ok_or("missing tool human-loop ledger in Aster")?;
    let parsed_tool = parse_human_loop_ledger(&tool_aster.bytes)?;

    let mut gui_ledger = launch_human_loop_playtest(
        &document,
        PlayCameraRig::TopDown,
        launch_request(
            "issue1188-manual-gui-ledger",
            "issue1188-manual-gui-session",
            HumanLoopEntryPoint::Gui,
            "human.qa.ember",
        ),
    )?;
    gui_ledger = capture_human_feedback(
        &gui_ledger,
        feedback("issue1188-manual-gui-session", "manual-feedback"),
        HumanLoopEntryPoint::Gui,
    )?;
    gui_ledger = record_agent_verdict(
        &gui_ledger,
        verdict("issue1188-manual-gui-session", "manual-verdict"),
        HumanLoopEntryPoint::McpTool,
    )?;
    let gui_state_path = root.join("gui-human-loop-ledger.json");
    let gui_writeback = write_human_loop_ledger(&gui_state_path, &aster_dir, &gui_ledger)?;
    let gui_aster = read_human_loop_ledger(&aster_dir, &gui_ledger.ledger_id)?
        .ok_or("missing GUI human-loop ledger in Aster")?;
    let parsed_gui = parse_human_loop_ledger(&gui_aster.bytes)?;
    let parity = compare_human_loop_gui_tool_parity(&parsed_gui.readback, &parsed_tool.readback)?;
    println!(
        "happy_path after_aster tool_seq={} gui_seq={} tool_rows={} gui_rows={} parity={}",
        tool_aster.seq, gui_aster.seq, tool_aster.graph_rows, gui_aster.graph_rows, parity.parity
    );

    let edge_empty = edge_empty_feedback(&document)?;
    let edge_denied = edge_permission_denied(&document)?;
    let edge_limit = edge_feedback_limit(&document)?;

    let artifact = json!({
        "issue": 1188,
        "source_of_truth": "Aster Graph CF human-loop ledger rows plus JSON state-file readback",
        "artifact_path": artifact_path,
        "happy_path": {
            "tool": {
                "ledger_id": tool_ledger.ledger_id,
                "session_id": parsed_tool.readback.session_id,
                "state_path": tool_state_path,
                "state_file_matches_aster": fs::read(&tool_state_path)? == tool_aster.bytes,
                "aster_seq": tool_aster.seq,
                "aster_graph_rows": tool_aster.graph_rows,
                "aster_sha256": tool_aster.sha256,
                "cell_key": tool_writeback.cell_key,
                "index_key": tool_writeback.index_key,
                "events": parsed_tool.event_count,
                "feedback": parsed_tool.readback.human_feedback_count,
                "agent_verdicts": parsed_tool.readback.agent_verdict_count,
                "spawned_bodies": parsed_tool.readback.spawned_bodies,
                "visible_instances": parsed_tool.readback.visible_instances
            },
            "gui": {
                "ledger_id": gui_ledger.ledger_id,
                "session_id": parsed_gui.readback.session_id,
                "state_path": gui_state_path,
                "state_file_matches_aster": fs::read(&gui_state_path)? == gui_aster.bytes,
                "aster_seq": gui_aster.seq,
                "aster_graph_rows": gui_aster.graph_rows,
                "aster_sha256": gui_aster.sha256,
                "cell_key": gui_writeback.cell_key,
                "index_key": gui_writeback.index_key,
                "events": parsed_gui.event_count,
                "feedback": parsed_gui.readback.human_feedback_count,
                "agent_verdicts": parsed_gui.readback.agent_verdict_count,
                "spawned_bodies": parsed_gui.readback.spawned_bodies,
                "visible_instances": parsed_gui.readback.visible_instances
            },
            "parity": parity
        },
        "edge_cases": [edge_empty, edge_denied, edge_limit]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1188 human-loop FSV artifact={} bytes={} artifact_sha={} source_of_truth_graph_rows={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        tool_aster.graph_rows + gui_aster.graph_rows
    );
    Ok(())
}

fn edge_empty_feedback(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let ledger = launch_human_loop_playtest(
        document,
        PlayCameraRig::TopDown,
        launch_request(
            "issue1188-edge-empty-ledger",
            "issue1188-edge-empty-session",
            HumanLoopEntryPoint::McpTool,
            "agent.playtest.orchestrator",
        ),
    )?;
    let before = readback_human_loop_session(&ledger)?;
    let mut empty = feedback("issue1188-edge-empty-session", "empty-feedback");
    empty.summary.clear();
    let error = capture_human_feedback(&ledger, empty, HumanLoopEntryPoint::Gui)
        .unwrap_err()
        .to_string();
    let after = readback_human_loop_session(&ledger)?;
    println!(
        "edge_empty before_feedback={} after_feedback={} error={}",
        before.human_feedback_count, after.human_feedback_count, error
    );
    Ok(json!({
        "name": "empty_feedback_summary",
        "before_feedback": before.human_feedback_count,
        "after_feedback": after.human_feedback_count,
        "state_unchanged": before == after,
        "error": error
    }))
}

fn edge_permission_denied(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let ledger = launch_human_loop_playtest(
        document,
        PlayCameraRig::TopDown,
        launch_request(
            "issue1188-edge-denied-ledger",
            "issue1188-edge-denied-session",
            HumanLoopEntryPoint::McpTool,
            "agent.playtest.orchestrator",
        ),
    )?;
    let before = readback_human_loop_session(&ledger)?;
    let denied = HumanLoopFeedback {
        actor_id: "agent.playtest.orchestrator".to_string(),
        ..feedback("issue1188-edge-denied-session", "denied-feedback")
    };
    let error = capture_human_feedback(&ledger, denied, HumanLoopEntryPoint::McpTool)
        .unwrap_err()
        .to_string();
    let after = readback_human_loop_session(&ledger)?;
    println!(
        "edge_denied before_feedback={} after_feedback={} error={}",
        before.human_feedback_count, after.human_feedback_count, error
    );
    Ok(json!({
        "name": "agent_cannot_submit_human_feedback",
        "before_feedback": before.human_feedback_count,
        "after_feedback": after.human_feedback_count,
        "state_unchanged": before == after,
        "error": error
    }))
}

fn edge_feedback_limit(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let mut request = launch_request(
        "issue1188-edge-limit-ledger",
        "issue1188-edge-limit-session",
        HumanLoopEntryPoint::McpTool,
        "agent.playtest.orchestrator",
    );
    request.config.max_human_feedback = 1;
    let ledger = launch_human_loop_playtest(document, PlayCameraRig::TopDown, request)?;
    let ledger = capture_human_feedback(
        &ledger,
        feedback("issue1188-edge-limit-session", "first-feedback"),
        HumanLoopEntryPoint::Gui,
    )?;
    let before = readback_human_loop_session(&ledger)?;
    let error = capture_human_feedback(
        &ledger,
        feedback("issue1188-edge-limit-session", "second-feedback"),
        HumanLoopEntryPoint::Gui,
    )
    .unwrap_err()
    .to_string();
    let after = readback_human_loop_session(&ledger)?;
    println!(
        "edge_limit before_feedback={} after_feedback={} error={}",
        before.human_feedback_count, after.human_feedback_count, error
    );
    Ok(json!({
        "name": "feedback_limit",
        "before_feedback": before.human_feedback_count,
        "after_feedback": after.human_feedback_count,
        "state_unchanged": before == after,
        "error": error
    }))
}

fn launch_request(
    ledger_id: &str,
    session_id: &str,
    entry_point: HumanLoopEntryPoint,
    actor_id: &str,
) -> HumanLoopLaunchRequest {
    HumanLoopLaunchRequest {
        ledger_id: ledger_id.to_string(),
        entry_point,
        actor_id: actor_id.to_string(),
        config: config(session_id),
    }
}

fn config(session_id: &str) -> HumanLoopSessionConfig {
    HumanLoopSessionConfig::new(
        session_id,
        "litd.project.issue1188.manual",
        "human.qa.ember",
        "issue1188-human-loop-manual-correlation",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        vec![
            HumanLoopPermission {
                actor_id: "human.qa.ember".to_string(),
                actor_kind: HumanLoopActorKind::Human,
                can_launch: true,
                can_submit_feedback: true,
                can_record_agent_verdict: false,
                can_inspect: true,
            },
            HumanLoopPermission {
                actor_id: "agent.playtest.orchestrator".to_string(),
                actor_kind: HumanLoopActorKind::Agent,
                can_launch: true,
                can_submit_feedback: false,
                can_record_agent_verdict: true,
                can_inspect: true,
            },
        ],
    )
}

fn feedback(session_id: &str, feedback_id: &str) -> HumanLoopFeedback {
    HumanLoopFeedback {
        feedback_id: feedback_id.to_string(),
        session_id: session_id.to_string(),
        actor_id: "human.qa.ember".to_string(),
        summary: "Pathing to the ember gate is readable and combat timing feels stable".to_string(),
        severity: "note".to_string(),
        referenced_object_ids: vec!["unit_playtest_hero".to_string()],
    }
}

fn verdict(session_id: &str, verdict_id: &str) -> HumanLoopAgentVerdict {
    HumanLoopAgentVerdict {
        verdict_id: verdict_id.to_string(),
        session_id: session_id.to_string(),
        actor_id: "agent.playtest.orchestrator".to_string(),
        verdict: "pass".to_string(),
        confidence_basis: "deterministic play snapshot and human feedback agree".to_string(),
        artifact_refs: vec!["Aster://issue1188/human-loop/readback".to_string()],
    }
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/issue1188-human-loop", "issue1188-rev-1")?;
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
            Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
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
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.human_loop.fsv.asset".to_string(),
            reason: "missing real Quaternius Rock1 asset record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
