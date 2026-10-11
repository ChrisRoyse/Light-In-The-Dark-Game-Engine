use std::{fs, path::PathBuf};

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::mcp_project_tools::{
    MODERATION_SAFETY_TOOL, call_mcp_project_tool, read_moderation_safety_state,
    read_publish_catalog,
};

use crate::mcp_project_social_test_support::{
    accept_friendship, create_friend, create_host, prepare_multiplayer_fixture_for_issue,
    send_friendship,
};

#[test]
fn issue1163_moderation_safety_admin_fsv() {
    let paths = prepare_moderation_fixture("unit");
    let content_address = published_content_address(&paths);
    let evidence_sha256 = sha256_hex(b"chat-log: host reported friend for harassment");
    let before = moderation_summary(&paths);
    println!(
        "issue1163 moderation before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(MODERATION_SAFETY_TOOL, owner_role_args(&paths)).unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        report_args(
            &paths,
            &content_address,
            &evidence_sha256,
            "report.harassment.001",
            "account.host",
            "account.friend",
        ),
    )
    .unwrap();
    call_mcp_project_tool(MODERATION_SAFETY_TOOL, mute_action_args(&paths)).unwrap();
    call_mcp_project_tool(MODERATION_SAFETY_TOOL, resolve_report_args(&paths)).unwrap();
    call_mcp_project_tool(MODERATION_SAFETY_TOOL, submit_appeal_args(&paths)).unwrap();
    call_mcp_project_tool(MODERATION_SAFETY_TOOL, decide_appeal_args(&paths, "upheld")).unwrap();

    let readback = read_moderation_safety_state(&paths.moderation_state_path).unwrap();
    let state = &readback.state;
    assert_eq!(state.schema, "litd.ecosystem.moderation_safety.state.v1");
    assert_eq!(state.report_count, 1);
    assert_eq!(state.role_assignment_count, 1);
    assert_eq!(state.admin_action_count, 1);
    assert_eq!(state.appeal_count, 1);
    assert_eq!(state.open_report_count, 0);
    assert_eq!(state.active_restriction_count, 1);
    assert_eq!(state.reports[0].report_id, "report.harassment.001");
    assert_eq!(
        state.reports[0].content_address.as_deref(),
        Some(content_address.as_str())
    );
    assert_eq!(format!("{:?}", state.reports[0].status), "Appealed");
    assert_eq!(
        state.admin_actions[0].target_account_id.as_deref(),
        Some("account.friend")
    );
    assert_eq!(format!("{:?}", state.admin_actions[0].action_kind), "Mute");
    assert!(state.admin_actions[0].active);
    assert_eq!(format!("{:?}", state.role_assignments[0].role), "Owner");
    assert_eq!(format!("{:?}", state.appeals[0].status), "Upheld");

    let after = moderation_summary(&paths);
    println!(
        "issue1163 moderation after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1163_moderation_edges_fail_closed_without_state_drift() {
    let paths = prepare_moderation_fixture("edges");
    let content_address = published_content_address(&paths);
    let evidence_sha256 = sha256_hex(b"chat-log: edge report evidence");

    let unknown_before = moderation_summary(&paths);
    let unknown_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        report_args(
            &paths,
            &content_address,
            &evidence_sha256,
            "report.unknown.001",
            "account.missing",
            "account.friend",
        ),
    )
    .unwrap_err()
    .to_string();
    let unknown_after = moderation_summary(&paths);
    assert_eq!(unknown_before, unknown_after);
    println!(
        "issue1163 edge unknown_reporter before={} after={} error={}",
        unknown_before, unknown_after, unknown_error
    );

    let no_role_before = moderation_summary(&paths);
    let no_role_error = call_mcp_project_tool(MODERATION_SAFETY_TOOL, mute_action_args(&paths))
        .unwrap_err()
        .to_string();
    let no_role_after = moderation_summary(&paths);
    assert_eq!(no_role_before, no_role_after);
    println!(
        "issue1163 edge action_without_role before={} after={} error={}",
        no_role_before, no_role_after, no_role_error
    );

    call_mcp_project_tool(MODERATION_SAFETY_TOOL, owner_role_args(&paths)).unwrap();

    let bad_content_before = moderation_summary(&paths);
    let bad_content_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        report_args(
            &paths,
            "litdgame.sha256.0000000000000000000000000000000000000000000000000000000000000000",
            &evidence_sha256,
            "report.badcontent.001",
            "account.host",
            "account.friend",
        ),
    )
    .unwrap_err()
    .to_string();
    let bad_content_after = moderation_summary(&paths);
    assert_eq!(bad_content_before, bad_content_after);
    println!(
        "issue1163 edge unpublished_content before={} after={} error={}",
        bad_content_before, bad_content_after, bad_content_error
    );

    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        report_args(
            &paths,
            &content_address,
            &evidence_sha256,
            "report.harassment.001",
            "account.host",
            "account.friend",
        ),
    )
    .unwrap();
    let duplicate_before = moderation_summary(&paths);
    let duplicate_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        report_args(
            &paths,
            &content_address,
            &evidence_sha256,
            "report.harassment.001",
            "account.host",
            "account.friend",
        ),
    )
    .unwrap_err()
    .to_string();
    let duplicate_after = moderation_summary(&paths);
    assert_eq!(duplicate_before, duplicate_after);
    println!(
        "issue1163 edge duplicate_report before={} after={} error={}",
        duplicate_before, duplicate_after, duplicate_error
    );
}

struct ModerationPaths {
    state_path: PathBuf,
    ledger_path: PathBuf,
    moderation_state_path: PathBuf,
    catalog_path: PathBuf,
    multiplayer_state_path: PathBuf,
    social_state_path: PathBuf,
}

fn prepare_moderation_fixture(label: &str) -> ModerationPaths {
    let social_paths = prepare_multiplayer_fixture_for_issue("issue1163_moderation_safety", label);
    create_host(&social_paths);
    create_friend(&social_paths);
    send_friendship(&social_paths);
    accept_friendship(&social_paths);
    ModerationPaths {
        state_path: social_paths.state_path,
        ledger_path: social_paths.root.join("moderation-ledger.json"),
        moderation_state_path: social_paths.root.join("moderation-safety-state.json"),
        catalog_path: social_paths.catalog_path,
        multiplayer_state_path: social_paths.multiplayer_state_path,
        social_state_path: social_paths.social_state_path,
    }
}

fn owner_role_args(paths: &ModerationPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1163-role-owner",
            "action": "assign_server_role",
            "assignment_id": "role.lobby.social.owner.host",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": "account.host",
            "role": "owner"
        }),
    )
}

fn report_args(
    paths: &ModerationPaths,
    content_address: &str,
    evidence_sha256: &str,
    report_id: &str,
    reporter: &str,
    reported: &str,
) -> Value {
    base_args(
        paths,
        json!({
            "request_id": format!("issue1163-{report_id}"),
            "action": "submit_report",
            "report_id": report_id,
            "lobby_id": "lobby.social",
            "reporter_account_id": reporter,
            "reported_account_id": reported,
            "content_address": content_address,
            "reason": "harassment",
            "severity": 4,
            "description": "Known deterministic report evidence for moderation FSV",
            "evidence_sha256": evidence_sha256
        }),
    )
}

fn mute_action_args(paths: &ModerationPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1163-action-mute",
            "action": "apply_admin_action",
            "action_id": "action.mute.account.friend.001",
            "report_id": "report.harassment.001",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "target_account_id": "account.friend",
            "admin_action_kind": "mute",
            "duration_minutes": 60,
            "resolution_note": "Muted account.friend for the reported harassment evidence"
        }),
    )
}

fn resolve_report_args(paths: &ModerationPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1163-report-resolve",
            "action": "resolve_report",
            "report_id": "report.harassment.001",
            "action_id": "action.mute.account.friend.001",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "resolution_status": "actioned",
            "resolution_note": "Report actioned with a timed mute"
        }),
    )
}

fn submit_appeal_args(paths: &ModerationPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1163-appeal-submit",
            "action": "submit_appeal",
            "appeal_id": "appeal.action.mute.account.friend.001",
            "action_id": "action.mute.account.friend.001",
            "account_id": "account.friend",
            "appeal_text": "I request a human review of the timed mute"
        }),
    )
}

fn decide_appeal_args(paths: &ModerationPaths, status: &str) -> Value {
    base_args(
        paths,
        json!({
            "request_id": format!("issue1163-appeal-{status}"),
            "action": "decide_appeal",
            "appeal_id": "appeal.action.mute.account.friend.001",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "appeal_status": status,
            "decision_note": "Human review upheld the original timed mute"
        }),
    )
}

fn base_args(paths: &ModerationPaths, mut extra: Value) -> Value {
    let map = extra.as_object_mut().unwrap();
    map.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1163-test"),
    );
    map.insert("state_path".to_string(), json!(paths.state_path));
    map.insert("state_id".to_string(), json!("issue1160-project-state"));
    map.insert("ledger_path".to_string(), json!(paths.ledger_path));
    map.insert(
        "ledger_id".to_string(),
        json!("issue1163-moderation-ledger"),
    );
    map.insert(
        "moderation_state_path".to_string(),
        json!(paths.moderation_state_path),
    );
    map.insert(
        "moderation_state_id".to_string(),
        json!("issue1163-moderation-state"),
    );
    map.insert(
        "social_state_path".to_string(),
        json!(paths.social_state_path),
    );
    map.insert("catalog_path".to_string(), json!(paths.catalog_path));
    map.insert(
        "multiplayer_state_path".to_string(),
        json!(paths.multiplayer_state_path),
    );
    extra
}

fn published_content_address(paths: &ModerationPaths) -> String {
    read_publish_catalog(&paths.catalog_path)
        .unwrap()
        .catalog
        .entries[0]
        .content_address
        .clone()
}

fn moderation_summary(paths: &ModerationPaths) -> Value {
    let state = read_moderation_safety_state(&paths.moderation_state_path).ok();
    json!({
        "moderation_state_exists": paths.moderation_state_path.exists(),
        "moderation_state_bytes": paths.moderation_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "moderation_state_sha256": fs::read(&paths.moderation_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "report_count": state.as_ref().map(|s| s.state.report_count).unwrap_or(0),
        "role_assignment_count": state.as_ref().map(|s| s.state.role_assignment_count).unwrap_or(0),
        "admin_action_count": state.as_ref().map(|s| s.state.admin_action_count).unwrap_or(0),
        "appeal_count": state.as_ref().map(|s| s.state.appeal_count).unwrap_or(0),
        "access_rule_count": state.as_ref().map(|s| s.state.access_rule_count).unwrap_or(0),
        "connection_decision_count": state.as_ref().map(|s| s.state.connection_decision_count).unwrap_or(0),
        "host_control_count": state.as_ref().map(|s| s.state.host_control_count).unwrap_or(0),
        "open_report_count": state.as_ref().map(|s| s.state.open_report_count).unwrap_or(0),
        "active_restriction_count": state.as_ref().map(|s| s.state.active_restriction_count).unwrap_or(0),
        "allow_rule_count": state.as_ref().map(|s| s.state.allow_rule_count).unwrap_or(0),
        "ban_rule_count": state.as_ref().map(|s| s.state.ban_rule_count).unwrap_or(0),
        "report_status": state.as_ref().and_then(|s| s.state.reports.first()).map(|r| format!("{:?}", r.status)).unwrap_or_default(),
        "appeal_status": state.as_ref().and_then(|s| s.state.appeals.first()).map(|a| format!("{:?}", a.status)).unwrap_or_default()
    })
}
