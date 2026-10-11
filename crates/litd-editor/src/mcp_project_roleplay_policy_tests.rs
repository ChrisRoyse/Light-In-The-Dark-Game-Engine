use std::{fs, path::PathBuf};

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::mcp_project_tools::{
    MODERATION_SAFETY_TOOL, RoleplayPolicyVerdict, call_mcp_project_tool,
    read_moderation_safety_state,
};

use crate::mcp_project_social_test_support::{
    accept_friendship, create_friend, create_host, prepare_multiplayer_fixture_for_issue,
    send_friendship,
};

#[test]
fn issue1041_roleplay_policy_fsv() {
    let paths = prepare_fixture("unit");
    let before = policy_summary(&paths);
    println!(
        "issue1041 policy before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(MODERATION_SAFETY_TOOL, owner_role_args(&paths)).unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        set_policy_args(
            &paths,
            "policy.lobby.social.hardcore",
            None,
            800,
            1500,
            2000,
            2,
        ),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        evaluate_policy_args(&paths, "eval.host.reward", "account.host", 900, 2),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        evaluate_policy_args(&paths, "eval.friend.punish", "account.friend", 350, 2),
    )
    .unwrap();
    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        evaluate_policy_args(&paths, "eval.friend.neutral", "account.friend", 700, 2),
    )
    .unwrap();

    let readback = read_moderation_safety_state(&paths.moderation_state_path).unwrap();
    let state = &readback.state;
    assert_eq!(state.role_assignment_count, 1);
    assert_eq!(state.roleplay_policy_count, 1);
    assert_eq!(state.active_roleplay_policy_count, 1);
    assert_eq!(state.roleplay_evaluation_count, 3);
    assert_eq!(state.reward_verdict_count, 1);
    assert_eq!(state.punishment_verdict_count, 1);
    assert_eq!(state.roleplay_policies[0].reward_threshold_milli, 840);
    assert_eq!(state.roleplay_policies[0].punishment_threshold_milli, 520);
    assert_eq!(
        state.roleplay_evaluations[0].verdict,
        RoleplayPolicyVerdict::Reward
    );
    assert_eq!(state.roleplay_evaluations[0].reward_delta_milli, 90);
    assert_eq!(
        state.roleplay_evaluations[1].verdict,
        RoleplayPolicyVerdict::Punish
    );
    assert_eq!(state.roleplay_evaluations[1].punishment_delta_milli, 340);
    assert_eq!(
        state.roleplay_evaluations[2].verdict,
        RoleplayPolicyVerdict::Neutral
    );

    let after = policy_summary(&paths);
    println!(
        "issue1041 policy after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1041_roleplay_policy_edges_fail_closed_without_state_drift() {
    let paths = prepare_fixture("edges");

    let no_role_before = policy_summary(&paths);
    let no_role_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        evaluate_policy_args(&paths, "eval.no-role", "account.host", 900, 2),
    )
    .unwrap_err()
    .to_string();
    let no_role_after = policy_summary(&paths);
    assert_eq!(no_role_before, no_role_after);
    println!(
        "issue1041 edge evaluate_without_role before={} after={} error={}",
        no_role_before, no_role_after, no_role_error
    );

    call_mcp_project_tool(MODERATION_SAFETY_TOOL, owner_role_args(&paths)).unwrap();

    let invalid_strictness_before = policy_summary(&paths);
    let invalid_strictness_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        set_policy_args(
            &paths,
            "policy.lobby.social.invalid-strictness",
            None,
            1200,
            1500,
            2000,
            2,
        ),
    )
    .unwrap_err()
    .to_string();
    let invalid_strictness_after = policy_summary(&paths);
    assert_eq!(invalid_strictness_before, invalid_strictness_after);
    println!(
        "issue1041 edge invalid_strictness before={} after={} error={}",
        invalid_strictness_before, invalid_strictness_after, invalid_strictness_error
    );

    call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        set_policy_args(
            &paths,
            "policy.lobby.social.hardcore",
            None,
            800,
            1500,
            2000,
            2,
        ),
    )
    .unwrap();

    let insufficient_before = policy_summary(&paths);
    let insufficient_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        evaluate_policy_args(&paths, "eval.insufficient-evidence", "account.host", 900, 1),
    )
    .unwrap_err()
    .to_string();
    let insufficient_after = policy_summary(&paths);
    assert_eq!(insufficient_before, insufficient_after);
    println!(
        "issue1041 edge insufficient_evidence before={} after={} error={}",
        insufficient_before, insufficient_after, insufficient_error
    );

    let duplicate_active_before = policy_summary(&paths);
    let duplicate_active_error = call_mcp_project_tool(
        MODERATION_SAFETY_TOOL,
        set_policy_args(
            &paths,
            "policy.lobby.social.casual",
            None,
            100,
            1000,
            1000,
            1,
        ),
    )
    .unwrap_err()
    .to_string();
    let duplicate_active_after = policy_summary(&paths);
    assert_eq!(duplicate_active_before, duplicate_active_after);
    println!(
        "issue1041 edge duplicate_active_policy before={} after={} error={}",
        duplicate_active_before, duplicate_active_after, duplicate_active_error
    );
}

struct PolicyPaths {
    state_path: PathBuf,
    ledger_path: PathBuf,
    moderation_state_path: PathBuf,
    catalog_path: PathBuf,
    multiplayer_state_path: PathBuf,
    social_state_path: PathBuf,
}

fn prepare_fixture(label: &str) -> PolicyPaths {
    let paths = prepare_multiplayer_fixture_for_issue("issue1041_roleplay_policy", label);
    create_host(&paths);
    create_friend(&paths);
    send_friendship(&paths);
    accept_friendship(&paths);
    PolicyPaths {
        state_path: paths.state_path,
        ledger_path: paths.root.join("roleplay-policy-ledger.json"),
        moderation_state_path: paths.root.join("moderation-safety-state.json"),
        catalog_path: paths.catalog_path,
        multiplayer_state_path: paths.multiplayer_state_path,
        social_state_path: paths.social_state_path,
    }
}

fn owner_role_args(paths: &PolicyPaths) -> Value {
    base_args(
        paths,
        json!({
            "request_id": "issue1041-role-owner",
            "action": "assign_server_role",
            "assignment_id": "role.lobby.social.owner.host",
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": "account.host",
            "role": "owner"
        }),
    )
}

fn set_policy_args(
    paths: &PolicyPaths,
    policy_id: &str,
    supersedes_policy_id: Option<&str>,
    strictness_milli: u16,
    reward_multiplier_milli: u16,
    punishment_multiplier_milli: u16,
    min_evidence_refs: u8,
) -> Value {
    let mut args = base_args(
        paths,
        json!({
            "request_id": format!("issue1041-set-{policy_id}"),
            "action": "set_roleplay_policy",
            "policy_id": policy_id,
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "roleplay_policy_mode": "privileged_hardcore",
            "strictness_milli": strictness_milli,
            "reward_multiplier_milli": reward_multiplier_milli,
            "punishment_multiplier_milli": punishment_multiplier_milli,
            "min_evidence_refs": min_evidence_refs
        }),
    );
    if let Some(value) = supersedes_policy_id {
        args.as_object_mut()
            .unwrap()
            .insert("supersedes_policy_id".to_string(), json!(value));
    }
    args
}

fn evaluate_policy_args(
    paths: &PolicyPaths,
    evaluation_id: &str,
    account_id: &str,
    rp_fit_score_milli: u16,
    evidence_count: usize,
) -> Value {
    let refs = (0..evidence_count)
        .map(|index| format!("calyx.anchor.rp.{index:03}"))
        .collect::<Vec<_>>();
    base_args(
        paths,
        json!({
            "request_id": format!("issue1041-{evaluation_id}"),
            "action": "evaluate_roleplay_policy",
            "evaluation_id": evaluation_id,
            "lobby_id": "lobby.social",
            "actor_account_id": "account.host",
            "account_id": account_id,
            "rp_fit_score_milli": rp_fit_score_milli,
            "evidence_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "evidence_refs": refs
        }),
    )
}

fn base_args(paths: &PolicyPaths, mut extra: Value) -> Value {
    let map = extra.as_object_mut().unwrap();
    map.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1041-test"),
    );
    map.insert("state_path".to_string(), json!(paths.state_path));
    map.insert("state_id".to_string(), json!("issue1160-project-state"));
    map.insert("ledger_path".to_string(), json!(paths.ledger_path));
    map.insert("ledger_id".to_string(), json!("issue1041-policy-ledger"));
    map.insert(
        "moderation_state_path".to_string(),
        json!(paths.moderation_state_path),
    );
    map.insert(
        "moderation_state_id".to_string(),
        json!("issue1041-policy-state"),
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

fn policy_summary(paths: &PolicyPaths) -> Value {
    let state = read_moderation_safety_state(&paths.moderation_state_path).ok();
    json!({
        "moderation_state_exists": paths.moderation_state_path.exists(),
        "moderation_state_bytes": paths.moderation_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "moderation_state_sha256": fs::read(&paths.moderation_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "role_assignment_count": state.as_ref().map(|s| s.state.role_assignment_count).unwrap_or(0),
        "roleplay_policy_count": state.as_ref().map(|s| s.state.roleplay_policy_count).unwrap_or(0),
        "active_roleplay_policy_count": state.as_ref().map(|s| s.state.active_roleplay_policy_count).unwrap_or(0),
        "roleplay_evaluation_count": state.as_ref().map(|s| s.state.roleplay_evaluation_count).unwrap_or(0),
        "reward_verdict_count": state.as_ref().map(|s| s.state.reward_verdict_count).unwrap_or(0),
        "punishment_verdict_count": state.as_ref().map(|s| s.state.punishment_verdict_count).unwrap_or(0),
        "last_verdict": state.as_ref().and_then(|s| s.state.roleplay_evaluations.last()).map(|evaluation| format!("{:?}", evaluation.verdict))
    })
}
