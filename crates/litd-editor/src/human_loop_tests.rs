use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
};
use litd_engine::math::{Transform, Vec3};

use super::*;

#[test]
fn issue1188_human_loop_source_of_truth_fsv() {
    let document = fixture_document();
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let aster_dir = root.join("aster");

    let tool_ledger = launch_human_loop_playtest(
        &document,
        PlayCameraRig::TopDown,
        launch_request(
            "issue1188-tool-ledger",
            "issue1188-tool-session",
            HumanLoopEntryPoint::McpTool,
            "agent.playtest.orchestrator",
        ),
    )
    .unwrap();
    let tool_ledger = capture_human_feedback(
        &tool_ledger,
        feedback("issue1188-tool-session", "feedback-pathing-readable"),
        HumanLoopEntryPoint::Gui,
    )
    .unwrap();
    let tool_ledger = record_agent_verdict(
        &tool_ledger,
        verdict("issue1188-tool-session", "verdict-agent-pathing"),
        HumanLoopEntryPoint::McpTool,
    )
    .unwrap();
    println!(
        "issue1188 happy before_write source=session_id={} events={} feedback={} verdicts={}",
        tool_ledger.readback.session_id,
        tool_ledger.event_count,
        tool_ledger.readback.human_feedback_count,
        tool_ledger.readback.agent_verdict_count
    );

    let tool_state = root.join("tool-human-loop-ledger.json");
    let tool_writeback = write_human_loop_ledger(&tool_state, &aster_dir, &tool_ledger).unwrap();
    let tool_readback = read_human_loop_ledger(&aster_dir, &tool_ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed_tool = parse_human_loop_ledger(&tool_readback.bytes).unwrap();
    assert!(!tool_state.exists());
    assert_eq!(tool_readback.bytes, tool_writeback.bytes);
    assert_eq!(parsed_tool.readback.human_feedback_count, 1);
    assert_eq!(parsed_tool.readback.agent_verdict_count, 1);
    println!(
        "issue1188 happy after_aster session_id={} seq={} graph_rows={} bytes={} sha={} cell={} index={}",
        tool_readback.session_id,
        tool_readback.seq,
        tool_readback.graph_rows,
        tool_readback.bytes.len(),
        tool_readback.sha256,
        tool_writeback.cell_key,
        tool_writeback.index_key
    );

    let gui_ledger = launch_human_loop_playtest(
        &document,
        PlayCameraRig::TopDown,
        launch_request(
            "issue1188-gui-ledger",
            "issue1188-gui-session",
            HumanLoopEntryPoint::Gui,
            "human.qa.ember",
        ),
    )
    .unwrap();
    let gui_ledger = capture_human_feedback(
        &gui_ledger,
        feedback("issue1188-gui-session", "feedback-pathing-readable"),
        HumanLoopEntryPoint::Gui,
    )
    .unwrap();
    let gui_ledger = record_agent_verdict(
        &gui_ledger,
        verdict("issue1188-gui-session", "verdict-agent-pathing"),
        HumanLoopEntryPoint::McpTool,
    )
    .unwrap();
    let gui_state = root.join("gui-human-loop-ledger.json");
    let gui_writeback = write_human_loop_ledger(&gui_state, &aster_dir, &gui_ledger).unwrap();
    let gui_readback = read_human_loop_ledger(&aster_dir, &gui_ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed_gui = parse_human_loop_ledger(&gui_readback.bytes).unwrap();
    let parity =
        compare_human_loop_gui_tool_parity(&parsed_gui.readback, &parsed_tool.readback).unwrap();

    assert!(!gui_state.exists());
    assert_eq!(gui_readback.bytes, gui_writeback.bytes);
    assert!(parity.parity);
    assert!(parity.mismatched_fields.is_empty());
    assert_eq!(parsed_gui.readback.spawned_bodies, 1);
    assert_eq!(parsed_gui.readback.visible_instances, 1);
    println!(
        "issue1188 parity gui_session={} tool_session={} fields={} gui_graph_rows={} tool_graph_rows={}",
        parity.gui_session_id,
        parity.tool_session_id,
        parity.matching_fields.join("|"),
        gui_readback.graph_rows,
        tool_readback.graph_rows
    );
}

#[test]
fn issue1188_edge_cases_fail_closed_without_state_mutation() {
    let document = fixture_document();
    let ledger = launch_human_loop_playtest(
        &document,
        PlayCameraRig::TopDown,
        launch_request(
            "issue1188-edge-ledger",
            "issue1188-edge-session",
            HumanLoopEntryPoint::McpTool,
            "agent.playtest.orchestrator",
        ),
    )
    .unwrap();

    let before_empty = readback_human_loop_session(&ledger).unwrap();
    let mut empty_feedback = feedback("issue1188-edge-session", "feedback-empty");
    empty_feedback.summary.clear();
    let empty_error = capture_human_feedback(&ledger, empty_feedback, HumanLoopEntryPoint::Gui)
        .unwrap_err()
        .to_string();
    let after_empty = readback_human_loop_session(&ledger).unwrap();
    assert_eq!(before_empty, after_empty);
    assert!(empty_error.contains("editor.human_loop.feedback.summary"));
    println!(
        "issue1188 edge_empty before_feedback={} after_feedback={} error={}",
        before_empty.human_feedback_count, after_empty.human_feedback_count, empty_error
    );

    let before_denied = readback_human_loop_session(&ledger).unwrap();
    let denied_feedback = HumanLoopFeedback {
        actor_id: "agent.playtest.orchestrator".to_string(),
        ..feedback("issue1188-edge-session", "feedback-agent-denied")
    };
    let denied_error =
        capture_human_feedback(&ledger, denied_feedback, HumanLoopEntryPoint::McpTool)
            .unwrap_err()
            .to_string();
    let after_denied = readback_human_loop_session(&ledger).unwrap();
    assert_eq!(before_denied, after_denied);
    assert!(denied_error.contains("editor.human_loop.permission.denied"));
    println!(
        "issue1188 edge_denied before_feedback={} after_feedback={} error={}",
        before_denied.human_feedback_count, after_denied.human_feedback_count, denied_error
    );

    let mut limited_request = launch_request(
        "issue1188-limit-ledger",
        "issue1188-limit-session",
        HumanLoopEntryPoint::McpTool,
        "agent.playtest.orchestrator",
    );
    limited_request.config.max_human_feedback = 1;
    let limited =
        launch_human_loop_playtest(&document, PlayCameraRig::TopDown, limited_request).unwrap();
    let limited = capture_human_feedback(
        &limited,
        feedback("issue1188-limit-session", "feedback-first"),
        HumanLoopEntryPoint::Gui,
    )
    .unwrap();
    let before_limit = readback_human_loop_session(&limited).unwrap();
    let limit_error = capture_human_feedback(
        &limited,
        feedback("issue1188-limit-session", "feedback-second"),
        HumanLoopEntryPoint::Gui,
    )
    .unwrap_err()
    .to_string();
    let after_limit = readback_human_loop_session(&limited).unwrap();
    assert_eq!(before_limit, after_limit);
    assert_eq!(after_limit.human_feedback_count, 1);
    assert!(limit_error.contains("editor.human_loop.feedback.limit"));
    println!(
        "issue1188 edge_limit before_feedback={} after_feedback={} error={}",
        before_limit.human_feedback_count, after_limit.human_feedback_count, limit_error
    );
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
        "litd.project.issue1188.fsv",
        "human.qa.ember",
        "issue1188-human-loop-correlation",
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

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1188-human-loop", "issue1188-rev-1").unwrap();
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
                Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
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

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1188", label)
}
