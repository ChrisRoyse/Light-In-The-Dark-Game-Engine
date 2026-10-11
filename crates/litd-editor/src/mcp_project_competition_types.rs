use serde::{Deserialize, Serialize};

pub const RANKINGS_REVIEWS_TOOL: &str = "litd.editor.project.manage_leaderboards_reviews";
pub const RANKINGS_REVIEWS_STATE_SCHEMA: &str = "litd.ecosystem.rankings_reviews.state.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingsReviewsAction {
    CreateLeaderboard,
    SubmitScore,
    SubmitReview,
    VoteReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaderboardSort {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreUpdateMethod {
    KeepBest,
    ForceUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankingsReviewsArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub rankings_state_path: String,
    pub rankings_state_id: String,
    pub catalog_path: String,
    pub install_manifest_path: String,
    pub social_state_path: String,
    pub action: RankingsReviewsAction,
    #[serde(default)]
    pub leaderboard_id: Option<String>,
    #[serde(default)]
    pub content_address: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub season_id: Option<String>,
    #[serde(default)]
    pub sort_method: Option<LeaderboardSort>,
    #[serde(default)]
    pub display_type: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub score: Option<i64>,
    #[serde(default)]
    pub tie_breakers: Vec<i64>,
    #[serde(default)]
    pub score_proof_path: Option<String>,
    #[serde(default)]
    pub score_update_method: Option<ScoreUpdateMethod>,
    #[serde(default)]
    pub review_id: Option<String>,
    #[serde(default)]
    pub rating: Option<u8>,
    #[serde(default)]
    pub recommended: Option<bool>,
    #[serde(default)]
    pub review_text: Option<String>,
    #[serde(default)]
    pub playtime_seconds: Option<u64>,
    #[serde(default)]
    pub target_review_id: Option<String>,
    #[serde(default)]
    pub vote_helpful: Option<bool>,
    #[serde(default)]
    pub vote_funny: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardRecord {
    pub leaderboard_id: String,
    pub content_address: String,
    pub display_name: String,
    pub season_id: String,
    pub sort_method: LeaderboardSort,
    pub display_type: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardEntryRecord {
    pub entry_id: String,
    pub leaderboard_id: String,
    pub account_id: String,
    pub player_id: String,
    pub score: i64,
    pub tie_breakers: Vec<i64>,
    pub rank: u32,
    pub score_proof_path: String,
    pub score_proof_sha256: String,
    pub submitted_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub review_id: String,
    pub content_address: String,
    pub account_id: String,
    pub rating: u8,
    pub recommended: bool,
    pub review_text: String,
    pub playtime_seconds: u64,
    pub helpful_votes: u32,
    pub funny_votes: u32,
    pub submitted_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewVoteRecord {
    pub vote_id: String,
    pub review_id: String,
    pub voter_account_id: String,
    pub helpful: bool,
    pub funny: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatingSummaryRecord {
    pub content_address: String,
    pub review_count: usize,
    pub recommended_count: usize,
    pub not_recommended_count: usize,
    pub rating_total: u32,
    pub average_rating_milli: u32,
    pub helpful_vote_count: u32,
    pub funny_vote_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankingsReviewsState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub catalog_path: String,
    pub install_manifest_path: String,
    pub social_state_path: String,
    pub leaderboard_count: usize,
    pub entry_count: usize,
    pub review_count: usize,
    pub vote_count: usize,
    pub summary_count: usize,
    pub leaderboards: Vec<LeaderboardRecord>,
    pub entries: Vec<LeaderboardEntryRecord>,
    pub reviews: Vec<ReviewRecord>,
    pub votes: Vec<ReviewVoteRecord>,
    pub summaries: Vec<RatingSummaryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankingsReviewsStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: RankingsReviewsState,
}
