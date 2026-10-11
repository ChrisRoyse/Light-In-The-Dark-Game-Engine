use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;

use serde_json::{Value, json};

use crate::mcp_project_tools::{
    RANKINGS_REVIEWS_TOOL, call_mcp_project_tool, read_publish_catalog, read_rankings_reviews_state,
};

use crate::mcp_project_social_test_support::{
    accept_friendship, create_friend, create_host, prepare_multiplayer_fixture_for_issue,
    send_friendship,
};

#[test]
fn issue1161_leaderboards_ratings_reviews_fsv() {
    let paths = prepare_competition_fixture("unit");
    let content_address = content_address(&paths.catalog_path);
    let before = rankings_summary(&paths);
    println!(
        "issue1161 rankings before {}",
        serde_json::to_string(&before).unwrap()
    );

    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        leaderboard_args(&paths, &content_address),
    )
    .unwrap();
    let host_proof = write_score_proof(&paths, "host", b"host-score:11250:ticks:3600");
    let friend_proof = write_score_proof(&paths, "friend", b"friend-score:12400:ticks:3520");
    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        score_args(&paths, "account.host", 11_250, 3600, &host_proof),
    )
    .unwrap();
    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        score_args(&paths, "account.friend", 12_400, 3520, &friend_proof),
    )
    .unwrap();
    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        review_args(
            &paths,
            &content_address,
            "review.host",
            "account.host",
            5,
            true,
        ),
    )
    .unwrap();
    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        review_args(
            &paths,
            &content_address,
            "review.friend",
            "account.friend",
            4,
            true,
        ),
    )
    .unwrap();
    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        vote_args(&paths, "account.host", "review.friend"),
    )
    .unwrap();

    let readback = read_rankings_reviews_state(&paths.rankings_state_path).unwrap();
    let state = readback.state;
    let top = state.entries.iter().find(|entry| entry.rank == 1).unwrap();
    let second = state.entries.iter().find(|entry| entry.rank == 2).unwrap();
    let summary = &state.summaries[0];

    assert_eq!(state.leaderboard_count, 1);
    assert_eq!(state.entry_count, 2);
    assert_eq!(state.review_count, 2);
    assert_eq!(state.vote_count, 1);
    assert_eq!(state.summary_count, 1);
    assert_eq!(top.account_id, "account.friend");
    assert_eq!(second.account_id, "account.host");
    assert_eq!(top.score, 12_400);
    assert_eq!(summary.review_count, 2);
    assert_eq!(summary.recommended_count, 2);
    assert_eq!(summary.average_rating_milli, 4500);
    assert_eq!(summary.helpful_vote_count, 1);

    let after = rankings_summary(&paths);
    println!(
        "issue1161 rankings after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1161_rankings_edges_fail_closed_without_state_drift() {
    let paths = prepare_competition_fixture("edges");
    let content_address = content_address(&paths.catalog_path);

    let unknown_before = rankings_summary(&paths);
    let unknown_error = call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        leaderboard_args(
            &paths,
            "litdgame.sha256.0000000000000000000000000000000000000000000000000000000000000000",
        ),
    )
    .unwrap_err()
    .to_string();
    let unknown_after = rankings_summary(&paths);
    assert_eq!(unknown_before, unknown_after);
    println!(
        "issue1161 edge unknown_game before={} after={} error={}",
        unknown_before, unknown_after, unknown_error
    );

    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        leaderboard_args(&paths, &content_address),
    )
    .unwrap();

    let proof_before = rankings_summary(&paths);
    let proof_error = call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        score_args(
            &paths,
            "account.host",
            42,
            999,
            &paths.root.join("missing-score-proof.bin"),
        ),
    )
    .unwrap_err()
    .to_string();
    let proof_after = rankings_summary(&paths);
    assert_eq!(proof_before, proof_after);
    println!(
        "issue1161 edge missing_proof before={} after={} error={}",
        proof_before, proof_after, proof_error
    );

    call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        review_args(
            &paths,
            &content_address,
            "review.host",
            "account.host",
            5,
            true,
        ),
    )
    .unwrap();
    let duplicate_before = rankings_summary(&paths);
    let duplicate_error = call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        review_args(
            &paths,
            &content_address,
            "review.host.duplicate",
            "account.host",
            4,
            true,
        ),
    )
    .unwrap_err()
    .to_string();
    let duplicate_after = rankings_summary(&paths);
    assert_eq!(duplicate_before, duplicate_after);
    println!(
        "issue1161 edge duplicate_review before={} after={} error={}",
        duplicate_before, duplicate_after, duplicate_error
    );

    let vote_before = rankings_summary(&paths);
    let vote_error = call_mcp_project_tool(
        RANKINGS_REVIEWS_TOOL,
        vote_args(&paths, "account.host", "review.host"),
    )
    .unwrap_err()
    .to_string();
    let vote_after = rankings_summary(&paths);
    assert_eq!(vote_before, vote_after);
    println!(
        "issue1161 edge self_vote before={} after={} error={}",
        vote_before, vote_after, vote_error
    );
}

struct CompetitionPaths {
    root: PathBuf,
    state_path: PathBuf,
    ledger_path: PathBuf,
    rankings_state_path: PathBuf,
    catalog_path: PathBuf,
    install_manifest_path: PathBuf,
    social_state_path: PathBuf,
}

fn prepare_competition_fixture(label: &str) -> CompetitionPaths {
    let social_paths = prepare_multiplayer_fixture_for_issue("issue1161_rankings_reviews", label);
    create_host(&social_paths);
    create_friend(&social_paths);
    send_friendship(&social_paths);
    accept_friendship(&social_paths);
    CompetitionPaths {
        root: social_paths.root.clone(),
        state_path: social_paths.state_path,
        ledger_path: social_paths.root.join("rankings-ledger.json"),
        rankings_state_path: social_paths.root.join("rankings-reviews-state.json"),
        catalog_path: social_paths.catalog_path,
        install_manifest_path: social_paths.install_manifest_path,
        social_state_path: social_paths.social_state_path,
    }
}

fn leaderboard_args(paths: &CompetitionPaths, content_address: &str) -> Value {
    rankings_args(
        paths,
        json!({
            "action": "create_leaderboard",
            "leaderboard_id": "leaderboard.social.season1",
            "content_address": content_address,
            "display_name": "Ashen Reach Season 1 Score",
            "season_id": "season.1",
            "sort_method": "descending",
            "display_type": "numeric"
        }),
    )
}

fn score_args(
    paths: &CompetitionPaths,
    account_id: &str,
    score: i64,
    tie_breaker: i64,
    proof_path: &Path,
) -> Value {
    rankings_args(
        paths,
        json!({
            "action": "submit_score",
            "leaderboard_id": "leaderboard.social.season1",
            "account_id": account_id,
            "score": score,
            "tie_breakers": [tie_breaker],
            "score_proof_path": proof_path.display().to_string(),
            "score_update_method": "keep_best"
        }),
    )
}

fn review_args(
    paths: &CompetitionPaths,
    content_address: &str,
    review_id: &str,
    account_id: &str,
    rating: u8,
    recommended: bool,
) -> Value {
    rankings_args(
        paths,
        json!({
            "action": "submit_review",
            "review_id": review_id,
            "content_address": content_address,
            "account_id": account_id,
            "rating": rating,
            "recommended": recommended,
            "review_text": format!("{review_id} deterministic review"),
            "playtime_seconds": 7200
        }),
    )
}

fn vote_args(paths: &CompetitionPaths, voter: &str, target_review_id: &str) -> Value {
    rankings_args(
        paths,
        json!({
            "action": "vote_review",
            "account_id": voter,
            "target_review_id": target_review_id,
            "vote_helpful": true,
            "vote_funny": false
        }),
    )
}

fn rankings_args(paths: &CompetitionPaths, fields: Value) -> Value {
    let mut value = json!({
        "request_id": "issue1161-rankings",
        "requested_by": "litd-editor-issue1161-test",
        "state_path": paths.state_path,
        "state_id": "issue1160-project-state",
        "ledger_path": paths.ledger_path,
        "ledger_id": "issue1161-rankings-ledger",
        "rankings_state_path": paths.rankings_state_path,
        "rankings_state_id": "issue1161-rankings-state",
        "catalog_path": paths.catalog_path,
        "install_manifest_path": paths.install_manifest_path,
        "social_state_path": paths.social_state_path
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

fn rankings_summary(paths: &CompetitionPaths) -> Value {
    let state = read_rankings_reviews_state(&paths.rankings_state_path).ok();
    let top_account = state
        .as_ref()
        .and_then(|state| state.state.entries.iter().find(|entry| entry.rank == 1))
        .map(|entry| entry.account_id.clone())
        .unwrap_or_default();
    json!({
        "rankings_state_exists": paths.rankings_state_path.exists(),
        "rankings_state_bytes": paths.rankings_state_path.metadata().map(|m| m.len()).unwrap_or(0),
        "rankings_state_sha256": fs::read(&paths.rankings_state_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "leaderboard_count": state.as_ref().map(|s| s.state.leaderboard_count).unwrap_or(0),
        "entry_count": state.as_ref().map(|s| s.state.entry_count).unwrap_or(0),
        "review_count": state.as_ref().map(|s| s.state.review_count).unwrap_or(0),
        "vote_count": state.as_ref().map(|s| s.state.vote_count).unwrap_or(0),
        "summary_count": state.as_ref().map(|s| s.state.summary_count).unwrap_or(0),
        "top_account": top_account,
        "average_rating_milli": state.as_ref().and_then(|s| s.state.summaries.first()).map(|s| s.average_rating_milli).unwrap_or(0),
        "helpful_vote_count": state.as_ref().and_then(|s| s.state.summaries.first()).map(|s| s.helpful_vote_count).unwrap_or(0)
    })
}

fn write_score_proof(paths: &CompetitionPaths, label: &str, bytes: &[u8]) -> PathBuf {
    let path = paths.root.join(format!("score-proof-{label}.bin"));
    fs::write(&path, bytes).unwrap();
    path
}

fn content_address(catalog_path: &PathBuf) -> String {
    read_publish_catalog(catalog_path).unwrap().catalog.entries[0]
        .content_address
        .clone()
}
