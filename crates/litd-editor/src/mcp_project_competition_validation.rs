use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::super::mcp_project_tools_validation::{
    validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_competition_types::{
    LeaderboardEntryRecord, LeaderboardRecord, RankingsReviewsAction, RankingsReviewsArgs,
    RankingsReviewsState, ReviewRecord, ReviewVoteRecord,
};

pub fn validate_rankings_args(args: &RankingsReviewsArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path("rankings_state_path", Path::new(&args.rankings_state_path))?;
    validate_path("catalog_path", Path::new(&args.catalog_path))?;
    validate_path(
        "install_manifest_path",
        Path::new(&args.install_manifest_path),
    )?;
    validate_path("social_state_path", Path::new(&args.social_state_path))?;
    validate_id("rankings_state_id", &args.rankings_state_id)?;
    match args.action {
        RankingsReviewsAction::CreateLeaderboard => {
            require_id("leaderboard_id", args.leaderboard_id.as_deref())?;
            require_line("content_address", args.content_address.as_deref())?;
            require_line("display_name", args.display_name.as_deref())?;
            require_id("season_id", args.season_id.as_deref())?;
            require_line("display_type", args.display_type.as_deref())?;
            if args.sort_method.is_none() {
                return Err(rankings_error("sort_method", "sort_method is required"));
            }
        }
        RankingsReviewsAction::SubmitScore => {
            require_id("leaderboard_id", args.leaderboard_id.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
            if args.score.is_none() {
                return Err(rankings_error("score", "score is required"));
            }
            require_line("score_proof_path", args.score_proof_path.as_deref())?;
            if args.tie_breakers.len() > 4 {
                return Err(rankings_error(
                    "tie_breakers",
                    "at most four tie breakers are supported",
                ));
            }
        }
        RankingsReviewsAction::SubmitReview => {
            require_id("review_id", args.review_id.as_deref())?;
            require_line("content_address", args.content_address.as_deref())?;
            require_id("account_id", args.account_id.as_deref())?;
            validate_rating(args.rating)?;
            if args.recommended.is_none() {
                return Err(rankings_error("recommended", "recommended is required"));
            }
            require_line("review_text", args.review_text.as_deref())?;
            if args.playtime_seconds.unwrap_or(0) == 0 {
                return Err(rankings_error(
                    "playtime_seconds",
                    "playtime_seconds must be non-zero",
                ));
            }
        }
        RankingsReviewsAction::VoteReview => {
            require_id("account_id", args.account_id.as_deref())?;
            require_id("target_review_id", args.target_review_id.as_deref())?;
            if !args.vote_helpful.unwrap_or(false) && !args.vote_funny.unwrap_or(false) {
                return Err(rankings_error(
                    "review_vote",
                    "a review vote must mark helpful, funny, or both",
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_rankings_state(state: &RankingsReviewsState) -> Result<(), EngineError> {
    if state.schema != super::mcp_project_competition_types::RANKINGS_REVIEWS_STATE_SCHEMA
        || state.issue != 1161
        || state.source_of_truth != "Calyx Aster rankings and reviews state"
        || state.state_id.trim().is_empty()
        || state.catalog_path.trim().is_empty()
        || state.install_manifest_path.trim().is_empty()
        || state.social_state_path.trim().is_empty()
        || state.leaderboard_count != state.leaderboards.len()
        || state.entry_count != state.entries.len()
        || state.review_count != state.reviews.len()
        || state.vote_count != state.votes.len()
        || state.summary_count != state.summaries.len()
    {
        return Err(rankings_error(
            "state",
            "schema, identity, source, paths, and counts are invalid",
        ));
    }
    let mut leaderboards = BTreeSet::new();
    for leaderboard in &state.leaderboards {
        validate_leaderboard(leaderboard)?;
        if !leaderboards.insert(leaderboard.leaderboard_id.as_str()) {
            return Err(rankings_error("leaderboards", "duplicate leaderboard_id"));
        }
    }
    let mut entries = BTreeSet::new();
    for entry in &state.entries {
        validate_entry(entry)?;
        if !leaderboards.contains(entry.leaderboard_id.as_str()) {
            return Err(rankings_error(
                "entry.leaderboard_id",
                "entry must reference an existing leaderboard",
            ));
        }
        let key = format!("{}\n{}", entry.leaderboard_id, entry.account_id);
        if !entries.insert(key) {
            return Err(rankings_error(
                "entries",
                "one score entry per account per leaderboard is allowed",
            ));
        }
    }
    let mut review_ids = BTreeSet::new();
    let mut review_pairs = BTreeSet::new();
    for review in &state.reviews {
        validate_review(review)?;
        if !review_ids.insert(review.review_id.as_str()) {
            return Err(rankings_error("reviews", "duplicate review_id"));
        }
        let key = format!("{}\n{}", review.content_address, review.account_id);
        if !review_pairs.insert(key) {
            return Err(rankings_error(
                "reviews",
                "one review per account per game is allowed",
            ));
        }
    }
    let mut votes = BTreeSet::new();
    for vote in &state.votes {
        validate_vote(vote)?;
        if !review_ids.contains(vote.review_id.as_str()) {
            return Err(rankings_error(
                "vote.review_id",
                "vote must reference an existing review",
            ));
        }
        let key = format!("{}\n{}", vote.review_id, vote.voter_account_id);
        if !votes.insert(key) {
            return Err(rankings_error(
                "votes",
                "one vote per account per review is allowed",
            ));
        }
    }
    Ok(())
}

pub fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(rankings_error(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

pub fn rankings_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    let field = format!("editor.mcp_project_tools.rankings_reviews.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::rankings_reviews",
        field = %field,
        reason = %reason,
        code = "LITD-RANKINGS-REVIEWS-VALIDATION",
        "rankings and reviews operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn validate_leaderboard(record: &LeaderboardRecord) -> Result<(), EngineError> {
    validate_id("leaderboard.leaderboard_id", &record.leaderboard_id)?;
    validate_single_line("leaderboard.content_address", &record.content_address)?;
    validate_single_line("leaderboard.display_name", &record.display_name)?;
    validate_id("leaderboard.season_id", &record.season_id)?;
    validate_single_line("leaderboard.display_type", &record.display_type)?;
    validate_id("leaderboard.created_by", &record.created_by)
}

fn validate_entry(record: &LeaderboardEntryRecord) -> Result<(), EngineError> {
    validate_id("entry.entry_id", &record.entry_id)?;
    validate_id("entry.leaderboard_id", &record.leaderboard_id)?;
    validate_id("entry.account_id", &record.account_id)?;
    validate_id("entry.player_id", &record.player_id)?;
    validate_single_line("entry.score_proof_path", &record.score_proof_path)?;
    validate_sha256("entry.score_proof_sha256", &record.score_proof_sha256)?;
    validate_id("entry.submitted_by", &record.submitted_by)?;
    if record.rank == 0 {
        return Err(rankings_error("entry.rank", "rank must be non-zero"));
    }
    Ok(())
}

fn validate_review(record: &ReviewRecord) -> Result<(), EngineError> {
    validate_id("review.review_id", &record.review_id)?;
    validate_single_line("review.content_address", &record.content_address)?;
    validate_id("review.account_id", &record.account_id)?;
    validate_rating(Some(record.rating))?;
    validate_single_line("review.review_text", &record.review_text)?;
    if record.playtime_seconds == 0 {
        return Err(rankings_error(
            "review.playtime_seconds",
            "playtime_seconds must be non-zero",
        ));
    }
    validate_id("review.submitted_by", &record.submitted_by)
}

fn validate_vote(record: &ReviewVoteRecord) -> Result<(), EngineError> {
    validate_id("vote.vote_id", &record.vote_id)?;
    validate_id("vote.review_id", &record.review_id)?;
    validate_id("vote.voter_account_id", &record.voter_account_id)?;
    if !record.helpful && !record.funny {
        return Err(rankings_error(
            "vote",
            "vote must mark helpful, funny, or both",
        ));
    }
    Ok(())
}

fn validate_rating(value: Option<u8>) -> Result<(), EngineError> {
    let rating = value.ok_or_else(|| rankings_error("rating", "rating is required"))?;
    if !(1..=5).contains(&rating) {
        return Err(rankings_error("rating", "rating must be in 1..=5"));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !litd_hash::is_sha256_hex(value) {
        return Err(rankings_error(
            field,
            "value must be a 64-character lowercase SHA-256 hex",
        ));
    }
    Ok(())
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_id(
        field,
        value.ok_or_else(|| rankings_error(field, "required field missing"))?,
    )
}

fn require_line(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    validate_single_line(
        field,
        value.ok_or_else(|| rankings_error(field, "required field missing"))?,
    )
    .map_err(|error| rankings_error(field, format!("{} failed validation: {error}", field)))
}
