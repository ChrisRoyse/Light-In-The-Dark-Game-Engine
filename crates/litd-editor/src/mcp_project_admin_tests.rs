use std::{fs, path::PathBuf};

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::mcp_project_tools::{
    MODERATION_SAFETY_TOOL, call_mcp_project_tool, read_moderation_safety_state,
};

use crate::mcp_project_social_test_support::{
    accept_friendship, create_friend, create_host, prepare_multiplayer_fixture_for_issue,
    send_friendship,
};

#[test]
fn issue1040_server_admin_access_controls_fsv() {
    let paths = prepare_fixture("unit");
    let before = moderation_summary(&paths);
    println!(
        "issue1040 admin before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(MODERATION_SAFETY_TOOL, owner_role_args(&paths)).unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        access_rule_args(&paths, "allow", "account.host"),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        connection_args(&paths, "decision.host.allowed", "account.host"),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        connection_args(&paths, "decision.friend.allowlist", "account.friend"),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        access_rule_args(&paths, "ban", "account.friend"),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        connection_args(&paths, "decision.friend.banned", "account.friend"),
    )
    .unwrap();
    call_mcp_project_tool(MODERATION_SAFETY_TOOL, host_control_args(&paths)).unwrap();

    let readback = read_moderation_safety_state(&paths.moderation_state_path).unwrap();
    let state = &readback.state;
    assert_eq!(state.role_assignment_count, 1);
    assert_eq!(state.access_rule_count, 2);
    assert_eq!(state.allow_rule_count, 1);
    assert_eq!(state.ban_rule_count, 1);
    assert_eq!(state.connection_decision_count, 3);
    assert_eq!(state.host_control_count, 1);
    assert!(state.connection_decisions[0].allowed);
    assert!(!state.connection_decisions[1].allowed);
    assert!(
        state.connection_decisions[1]
            .reasons
            .iter()
            .any(|reason| reason.contains("allowlist"))
    );
    assert!(!state.connection_decisions[2].allowed);
    assert!(
        state.connection_decisions[2]
            .reasons
            .iter()
            .any(|reason| reason.contains("ban"))
    );
    assert_eq!(format!("{:?}", state.host_controls[0].control_kind), "Kick");

    let after = moderation_summary(&paths);
    println!(
        "issue1040 admin after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1040_access_control_edges_fail_closed_without_state_drift() {
    let paths = prepare_fixture("edges");

    let no_role_before = moderation_summary(&paths);
    let no_role_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        connection_args(&paths, "decision.no_role", "account.host"),
    )
    .unwrap_err()
    .to_string();
    let no_role_after = moderation_summary(&paths);
    assert_eq!(no_role_before, no_role_after);
    println!(
        "issue1040 edge connection_without_role before={} after={} error={}",
        no_role_before, no_role_after, no_role_error
    );

    call_mcp_project_tool(MODERATION_SAFETY_TOOL, owner_role_args(&paths)).unwrap();

    let unknown_before = moderation_summary(&paths);
    let unknown_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        access_rule_args(&paths, "allow", "account.missing"),
    )
    .unwrap_err()
    .to_string();
    let unknown_after = moderation_summary(&paths);
    assert_eq!(unknown_before, unknown_after);
    println!(
        "issue1040 edge unknown_access_account before={} after={} error={}",
        unknown_before, unknown_after, unknown_error
    );

    let missing_target_before = moderation_summary(&paths);
    let missing_target_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        host_control_without_target_args(&paths),
    )
    .unwrap_err()
    .to_string();
    let missing_target_after = moderation_summary(&paths);
    assert_eq!(missing_target_before, missing_target_after);
    println!(
        "issue1040 edge host_control_missing_target before={} after={} error={}",
        missing_target_before, missing_target_after, missing_target_error
    );

    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        access_rule_args(&paths, "allow", "account.host"),
    )
    .unwrap();
    let duplicate_before = moderation_summary(&paths);
    let duplicate_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        access_rule_args(&paths, "allow", "account.host"),
    )
    .unwrap_err()
    .to_string();
    let duplicate_after = moderation_summary(&paths);
    assert_eq!(duplicate_before, duplicate_after);
    println!(
        "issue1040 edge duplicate_access_rule before={} after={} error={}",
        duplicate_before, duplicate_after, duplicate_error
    );
}

struct AdminPaths {
    state_path: PathBuf,
    ledger_path: PathBuf,
    moderation_state_path: PathBuf,
    catalog_path: PathBuf,
    multiplayer_state_path: PathBuf,
    social_state_path: PathBuf,
}

fn prepare_fixture(label: &str) -> AdminPaths {
    let paths = prepare_multiplayer_fixture_for_issue("issue1040_server_admin", label);
    create_host(&paths);
    create_friend(&paths);
    send_friendship(&paths);
    accept_friendship(&paths);
    AdminPaths {
        state_path: paths.state_path,
        ledger_path: paths.root.join("admin-ledger.json"),
        moderation_state_path: paths.root.join("moderation-safety-state.json"),
        catalog_path: paths.catalog_path,
        multiplayer_state_path: paths.multiplayer_state_path,
        social_state_path: paths.social_state_path,
    }
}

fn owner_role_args(paths: &AdminPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1040-role-owner",
            "action": "assign_server_role",
            "assignment_id": "role.lobby.social.owner.host",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": "account.host",
            "role": "owner"
        }),
    )
}

fn access_rule_args(paths: &AdminPaths, kind: &str, account_id: &str) -> Value {
    base_args(
        paths,
        json!({
            "request_id": format!("issue1040-rule-{kind}-{account_id}"),
            "action": "set_access_rule",
            "rule_id": format!("rule.lobby.social.{kind}.{account_id}"),
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": account_id,
            "access_rule_kind": kind,
            "resolution_note": format!("{kind} rule for {account_id}")
        }),
    )
}

fn connection_args(paths: &AdminPaths, decision_id: &str, account_id: &str) -> Value {
    base_args(
        paths,
        json!({
            "request_id": format!("issue1040-{decision_id}"),
            "action": "evaluate_connection",
            "decision_id": decision_id,
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": account_id
        }),
    )
}

fn host_control_args(paths: &AdminPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1040-host-control-kick",
            "action": "record_host_control",
            "control_id": "control.kick.account.friend.001",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "target_account_id": "account.friend",
            "host_control_kind": "kick",
            "resolution_note": "Live host kick control recorded for account.friend"
        }),
    )
}

fn host_control_without_target_args(paths: &AdminPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1040-host-control-kick-missing-target",
            "action": "record_host_control",
            "control_id": "control.kick.missing-target.001",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "host_control_kind": "kick",
            "resolution_note": "missing target should fail before state mutation"
        }),
    )
}

fn base_args(paths: &AdminPaths, mut extra: Value) -> Value {
    let map = extra.as_object_mut().unwrap();
    map.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1040-test"),
    );
    map.insert("state_path".to_string(), json!(paths.state_path));
    map.insert("state_id".to_string(), json!("issue1160-project-state"));
    map.insert("ledger_path".to_string(), json!(paths.ledger_path));
    map.insert("ledger_id".to_string(), json!("issue1040-admin-ledger"));
    map.insert(
        "moderation_state_path".to_string(),
        json!(paths.moderation_state_path),
    );
    map.insert(
        "moderation_state_id".to_string(),
        json!("issue1040-admin-state"),
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

fn moderation_summary(paths: &AdminPaths) -> Value {
    let state = read_moderation_safety_state(&paths.moderation_state_path).ok();
    json!({
        "moderation_state_exists": paths.moderation_state_path.exists(),
        "moderation_state_bytes": paths.moderation_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "moderation_state_sha256": fs::read(&paths.moderation_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "role_assignment_count": state.as_ref().map(|s| s.state.role_assignment_count).unwrap_or(0),
        "access_rule_count": state.as_ref().map(|s| s.state.access_rule_count).unwrap_or(0),
        "allow_rule_count": state.as_ref().map(|s| s.state.allow_rule_count).unwrap_or(0),
        "ban_rule_count": state.as_ref().map(|s| s.state.ban_rule_count).unwrap_or(0),
        "connection_decision_count": state.as_ref().map(|s| s.state.connection_decision_count).unwrap_or(0),
        "host_control_count": state.as_ref().map(|s| s.state.host_control_count).unwrap_or(0),
        "last_decision_allowed": state.as_ref().and_then(|s| s.state.connection_decisions.last()).map(|d| d.allowed)
    })
}
