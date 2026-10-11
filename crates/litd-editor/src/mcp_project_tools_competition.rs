use std::{cmp::Ordering, fs, path::Path};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

#[path = "mcp_project_competition_store.rs"]
mod mcp_project_competition_store;
#[path = "mcp_project_competition_types.rs"]
mod mcp_project_competition_types;
#[path = "mcp_project_competition_validation.rs"]
mod mcp_project_competition_validation;

pub use self::{
    mcp_project_competition_store::read_rankings_reviews_state,
    mcp_project_competition_types::{
        LeaderboardEntryRecord, LeaderboardRecord, LeaderboardSort, RANKINGS_REVIEWS_STATE_SCHEMA,
        RANKINGS_REVIEWS_TOOL, RankingsReviewsAction, RankingsReviewsState,
        RankingsReviewsStateReadback, RatingSummaryRecord, ReviewRecord, ReviewVoteRecord,
        ScoreUpdateMethod,
    },
};

use self::{
    mcp_project_competition_store::{
        load_or_create_rankings_reviews_state, write_rankings_reviews_state,
    },
    mcp_project_competition_types::RankingsReviewsArgs,
    mcp_project_competition_validation::{rankings_error, validate_id, validate_rankings_args},
};
use super::{
    McpProjectToolCallReadback, READ_PROJECT_TOOL,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, write_mcp_project_state,
        write_mcp_project_tool_ledger,
    },
    parse_args, read_litdgame_install_manifest, read_publish_catalog, read_social_graph_state,
    validate_mutating_route,
};

pub(super) fn manage_leaderboards_reviews(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: RankingsReviewsArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every filesystem-sink path to the project root before any fs effect.
    // score_proof_path IS confined (when present): submit_score does fs::read(score_proof_path),
    // so a poisoned value would be an arbitrary out-of-root file-read primitive.
    let mut confined: Vec<(&str, &std::path::Path)> = vec![
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        (
            "rankings_state_path",
            std::path::Path::new(&args.rankings_state_path),
        ),
        ("catalog_path", std::path::Path::new(&args.catalog_path)),
        (
            "install_manifest_path",
            std::path::Path::new(&args.install_manifest_path),
        ),
        (
            "social_state_path",
            std::path::Path::new(&args.social_state_path),
        ),
    ];
    if let Some(p) = args.score_proof_path.as_deref() {
        confined.push(("score_proof_path", std::path::Path::new(p)));
    }
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&confined)?;
    validate_rankings_args(&args)?;
    let mut rankings_state = load_or_create_rankings_reviews_state(
        &args.rankings_state_path,
        &args.rankings_state_id,
        &args.catalog_path,
        &args.install_manifest_path,
        &args.social_state_path,
    )?;
    let result = match args.action {
        RankingsReviewsAction::CreateLeaderboard => create_leaderboard(&args, &mut rankings_state)?,
        RankingsReviewsAction::SubmitScore => submit_score(&args, &mut rankings_state)?,
        RankingsReviewsAction::SubmitReview => submit_review(&args, &mut rankings_state)?,
        RankingsReviewsAction::VoteReview => vote_review(&args, &mut rankings_state)?,
    };
    refresh_counts(&mut rankings_state);
    let rankings_readback =
        write_rankings_reviews_state(&args.rankings_state_path, &rankings_state)?;

    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    state.update_count = state.update_count.saturating_add(1);
    let state_readback = write_mcp_project_state(&args.state_path, &state)?;
    let ledger_readback = write_mcp_project_tool_ledger(
        &args.ledger_path,
        &args.ledger_id,
        McpProjectToolRecord {
            request_id: args.request_id.clone(),
            requested_by: args.requested_by.clone(),
            tool_name: route.tool_name.clone(),
            operation_id: route.operation_id.clone(),
            state_path: state_readback.path.clone(),
            state_sha256: state_readback.sha256.clone(),
            state_bytes: state_readback.bytes,
            action_summary:
                "managed leaderboards, verified score proofs, ratings, reviews, or review votes"
                    .to_string(),
            affected_ids: affected_ids(&rankings_readback.state),
            project_count: state_readback.project_count,
            playtest_count: state_readback.playtest_count,
            package_count: state_readback.package_count,
            published_count: state_readback.published_count,
        },
    )?;

    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth:
            "Rankings/reviews JSON + publish catalog + install manifest + social graph JSON"
                .to_string(),
        result: json!({
            "rankings_state_readback": rankings_readback,
            "operation": result
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

fn create_leaderboard(
    args: &RankingsReviewsArgs,
    state: &mut RankingsReviewsState,
) -> Result<Value, EngineError> {
    let leaderboard_id = required(args.leaderboard_id.as_deref(), "leaderboard_id")?;
    let content_address = required(args.content_address.as_deref(), "content_address")?;
    ensure_game_installed(args, content_address)?;
    if state
        .leaderboards
        .iter()
        .any(|leaderboard| leaderboard.leaderboard_id == leaderboard_id)
    {
        return Err(rankings_error(
            "leaderboard_id",
            "leaderboard already exists",
        ));
    }
    let leaderboard = LeaderboardRecord {
        leaderboard_id: leaderboard_id.to_string(),
        content_address: content_address.to_string(),
        display_name: required(args.display_name.as_deref(), "display_name")?.to_string(),
        season_id: required(args.season_id.as_deref(), "season_id")?.to_string(),
        sort_method: args
            .sort_method
            .ok_or_else(|| rankings_error("sort_method", "sort_method is required"))?,
        display_type: required(args.display_type.as_deref(), "display_type")?.to_string(),
        created_by: args.requested_by.clone(),
    };
    state.leaderboards.push(leaderboard.clone());
    Ok(json!({ "leaderboard": leaderboard }))
}

fn submit_score(
    args: &RankingsReviewsArgs,
    state: &mut RankingsReviewsState,
) -> Result<Value, EngineError> {
    let leaderboard_id = required(args.leaderboard_id.as_deref(), "leaderboard_id")?;
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    let leaderboard = state
        .leaderboards
        .iter()
        .find(|leaderboard| leaderboard.leaderboard_id == leaderboard_id)
        .cloned()
        .ok_or_else(|| rankings_error("leaderboard_id", "leaderboard does not exist"))?;
    ensure_game_installed(args, &leaderboard.content_address)?;
    let account = ensure_social_account(args, account_id)?;
    let proof_path = required(args.score_proof_path.as_deref(), "score_proof_path")?;
    let proof_bytes = fs::read(proof_path).map_err(|error| {
        rankings_error(
            "score_proof.read",
            format!("read {}: {error}", Path::new(proof_path).display()),
        )
    })?;
    if proof_bytes.is_empty() {
        return Err(rankings_error(
            "score_proof",
            "score proof bytes must be non-empty",
        ));
    }
    let score = args
        .score
        .ok_or_else(|| rankings_error("score", "score is required"))?;
    let score_proof_sha256 = sha256_hex(&proof_bytes);
    let entry_id = format!("entry.{leaderboard_id}.{account_id}");
    let candidate = LeaderboardEntryRecord {
        entry_id: entry_id.clone(),
        leaderboard_id: leaderboard_id.to_string(),
        account_id: account_id.to_string(),
        player_id: account.player_id,
        score,
        tie_breakers: args.tie_breakers.clone(),
        rank: 1,
        score_proof_path: proof_path.to_string(),
        score_proof_sha256,
        submitted_by: args.requested_by.clone(),
    };
    let method = args
        .score_update_method
        .unwrap_or(ScoreUpdateMethod::KeepBest);
    if let Some(existing) = state
        .entries
        .iter_mut()
        .find(|entry| entry.leaderboard_id == leaderboard_id && entry.account_id == account_id)
    {
        if method == ScoreUpdateMethod::KeepBest
            && !is_better_score(&leaderboard, &candidate, existing)
        {
            return Err(rankings_error(
                "score.keep_best",
                "submitted score does not improve the existing best",
            ));
        }
        *existing = candidate.clone();
    } else {
        state.entries.push(candidate.clone());
    }
    recalculate_ranks(state, leaderboard_id)?;
    let entry = state
        .entries
        .iter()
        .find(|entry| entry.entry_id == entry_id)
        .cloned()
        .ok_or_else(|| rankings_error("entry", "score entry readback missing"))?;
    Ok(json!({ "entry": entry }))
}

fn submit_review(
    args: &RankingsReviewsArgs,
    state: &mut RankingsReviewsState,
) -> Result<Value, EngineError> {
    let review_id = required(args.review_id.as_deref(), "review_id")?;
    let content_address = required(args.content_address.as_deref(), "content_address")?;
    let account_id = required(args.account_id.as_deref(), "account_id")?;
    ensure_game_installed(args, content_address)?;
    ensure_social_account(args, account_id)?;
    if state.reviews.iter().any(|review| {
        review.review_id == review_id
            || (review.content_address == content_address && review.account_id == account_id)
    }) {
        return Err(rankings_error(
            "review",
            "one review per account per game is allowed",
        ));
    }
    let review = ReviewRecord {
        review_id: review_id.to_string(),
        content_address: content_address.to_string(),
        account_id: account_id.to_string(),
        rating: args
            .rating
            .ok_or_else(|| rankings_error("rating", "rating is required"))?,
        recommended: args
            .recommended
            .ok_or_else(|| rankings_error("recommended", "recommended is required"))?,
        review_text: required(args.review_text.as_deref(), "review_text")?.to_string(),
        playtime_seconds: args
            .playtime_seconds
            .ok_or_else(|| rankings_error("playtime_seconds", "playtime_seconds is required"))?,
        helpful_votes: 0,
        funny_votes: 0,
        submitted_by: args.requested_by.clone(),
    };
    state.reviews.push(review.clone());
    refresh_summaries(state);
    Ok(json!({ "review": review }))
}

fn vote_review(
    args: &RankingsReviewsArgs,
    state: &mut RankingsReviewsState,
) -> Result<Value, EngineError> {
    let voter = required(args.account_id.as_deref(), "account_id")?;
    let review_id = required(args.target_review_id.as_deref(), "target_review_id")?;
    ensure_social_account(args, voter)?;
    let review = state
        .reviews
        .iter_mut()
        .find(|review| review.review_id == review_id)
        .ok_or_else(|| rankings_error("target_review_id", "review does not exist"))?;
    if review.account_id == voter {
        return Err(rankings_error(
            "review_vote.self",
            "review author cannot vote on their own review",
        ));
    }
    let vote_id = format!("vote.{review_id}.{voter}");
    if state.votes.iter().any(|vote| vote.vote_id == vote_id) {
        return Err(rankings_error(
            "review_vote.duplicate",
            "account already voted on this review",
        ));
    }
    let vote = ReviewVoteRecord {
        vote_id,
        review_id: review_id.to_string(),
        voter_account_id: voter.to_string(),
        helpful: args.vote_helpful.unwrap_or(false),
        funny: args.vote_funny.unwrap_or(false),
    };
    if vote.helpful {
        review.helpful_votes = review.helpful_votes.saturating_add(1);
    }
    if vote.funny {
        review.funny_votes = review.funny_votes.saturating_add(1);
    }
    state.votes.push(vote.clone());
    refresh_summaries(state);
    Ok(json!({ "vote": vote }))
}

fn ensure_game_installed(
    args: &RankingsReviewsArgs,
    content_address: &str,
) -> Result<(), EngineError> {
    let catalog = read_publish_catalog(&args.catalog_path)?.catalog;
    let published = catalog
        .entries
        .iter()
        .find(|entry| entry.content_address == content_address)
        .ok_or_else(|| rankings_error("catalog.content_address", "game is not published"))?;
    let install = read_litdgame_install_manifest(&args.install_manifest_path)?.manifest;
    let installed = install
        .records
        .iter()
        .find(|record| record.content_address == content_address)
        .ok_or_else(|| rankings_error("install.content_address", "game is not installed"))?;
    if installed.installed_package_sha256 != published.package_sha256
        || !installed.runtime_lock_verified
        || !installed.world_hash_matches
    {
        return Err(rankings_error(
            "install.readback",
            "installed package readback does not match published game",
        ));
    }
    Ok(())
}

fn ensure_social_account(
    args: &RankingsReviewsArgs,
    account_id: &str,
) -> Result<super::AccountRecord, EngineError> {
    read_social_graph_state(&args.social_state_path)?
        .state
        .accounts
        .into_iter()
        .find(|account| account.account_id == account_id)
        .ok_or_else(|| rankings_error("account_id", "account is not in social graph state"))
}

fn recalculate_ranks(
    state: &mut RankingsReviewsState,
    leaderboard_id: &str,
) -> Result<(), EngineError> {
    let leaderboard = state
        .leaderboards
        .iter()
        .find(|leaderboard| leaderboard.leaderboard_id == leaderboard_id)
        .cloned()
        .ok_or_else(|| rankings_error("leaderboard_id", "leaderboard does not exist"))?;
    let mut ordered: Vec<usize> = state
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (entry.leaderboard_id == leaderboard_id).then_some(index))
        .collect();
    ordered.sort_by(|left, right| {
        compare_entries(&leaderboard, &state.entries[*left], &state.entries[*right])
    });
    for (rank, index) in ordered.into_iter().enumerate() {
        state.entries[index].rank = (rank + 1) as u32;
    }
    Ok(())
}

fn is_better_score(
    leaderboard: &LeaderboardRecord,
    candidate: &LeaderboardEntryRecord,
    existing: &LeaderboardEntryRecord,
) -> bool {
    compare_entries(leaderboard, candidate, existing) == Ordering::Less
}

fn compare_entries(
    leaderboard: &LeaderboardRecord,
    left: &LeaderboardEntryRecord,
    right: &LeaderboardEntryRecord,
) -> Ordering {
    let score_order = match leaderboard.sort_method {
        LeaderboardSort::Descending => right.score.cmp(&left.score),
        LeaderboardSort::Ascending => left.score.cmp(&right.score),
    };
    score_order
        .then_with(|| left.tie_breakers.cmp(&right.tie_breakers))
        .then_with(|| left.account_id.cmp(&right.account_id))
}

fn refresh_summaries(state: &mut RankingsReviewsState) {
    let mut content_addresses = state
        .reviews
        .iter()
        .map(|review| review.content_address.clone())
        .collect::<Vec<_>>();
    content_addresses.sort();
    content_addresses.dedup();
    state.summaries = content_addresses
        .into_iter()
        .map(|content_address| {
            let reviews = state
                .reviews
                .iter()
                .filter(|review| review.content_address == content_address)
                .collect::<Vec<_>>();
            let review_count = reviews.len();
            let rating_total = reviews
                .iter()
                .map(|review| review.rating as u32)
                .sum::<u32>();
            let recommended_count = reviews.iter().filter(|review| review.recommended).count();
            let helpful_vote_count = reviews
                .iter()
                .map(|review| review.helpful_votes)
                .sum::<u32>();
            let funny_vote_count = reviews.iter().map(|review| review.funny_votes).sum::<u32>();
            RatingSummaryRecord {
                content_address,
                review_count,
                recommended_count,
                not_recommended_count: review_count.saturating_sub(recommended_count),
                rating_total,
                average_rating_milli: if review_count == 0 {
                    0
                } else {
                    (rating_total * 1000) / review_count as u32
                },
                helpful_vote_count,
                funny_vote_count,
            }
        })
        .collect();
}

fn refresh_counts(state: &mut RankingsReviewsState) {
    refresh_summaries(state);
    state.leaderboard_count = state.leaderboards.len();
    state.entry_count = state.entries.len();
    state.review_count = state.reviews.len();
    state.vote_count = state.votes.len();
    state.summary_count = state.summaries.len();
}

fn affected_ids(state: &RankingsReviewsState) -> Vec<String> {
    let mut ids = state
        .leaderboards
        .iter()
        .map(|leaderboard| leaderboard.leaderboard_id.clone())
        .chain(state.entries.iter().map(|entry| entry.entry_id.clone()))
        .chain(state.reviews.iter().map(|review| review.review_id.clone()))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.push(state.state_id.clone());
    }
    ids
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, EngineError> {
    let value = value.ok_or_else(|| rankings_error(field, "required field missing"))?;
    if field.ends_with("_id") {
        validate_id(field, value)?;
    }
    Ok(value)
}
