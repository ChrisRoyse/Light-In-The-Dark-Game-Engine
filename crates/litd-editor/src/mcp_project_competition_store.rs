use std::path::Path;

use litd_engine::EngineError;

use super::super::mcp_project_aster_store::{
    AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_competition_types::{
    RANKINGS_REVIEWS_STATE_SCHEMA, RankingsReviewsState, RankingsReviewsStateReadback,
};
use super::mcp_project_competition_validation::{rankings_error, validate_rankings_state};

const RANKINGS_SOURCE_OF_TRUTH: &str = "Calyx Aster rankings and reviews state";
const RANKINGS_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "rankings_reviews_state",
    collection: "litd_project_rankings_reviews",
    row: b"mcp_project/rankings_reviews/current",
    column: b"rankings_reviews_state_json",
    format: "litd-rankings-reviews-state-json",
};

pub fn load_or_create_rankings_reviews_state(
    path: impl AsRef<Path>,
    state_id: &str,
    catalog_path: &str,
    install_manifest_path: &str,
    social_state_path: &str,
) -> Result<RankingsReviewsState, EngineError> {
    let path = path.as_ref();
    validate_path("rankings_state_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &RANKINGS_SPEC, validate_rankings_state)?
    {
        let state = readback.value;
        if state.state_id != state_id
            || state.catalog_path != catalog_path
            || state.install_manifest_path != install_manifest_path
            || state.social_state_path != social_state_path
        {
            return Err(rankings_error(
                "state.identity",
                "rankings state identity or source path mismatch",
            ));
        }
        Ok(state)
    } else {
        let state = RankingsReviewsState {
            schema: RANKINGS_REVIEWS_STATE_SCHEMA.to_string(),
            issue: 1161,
            state_id: state_id.to_string(),
            source_of_truth: RANKINGS_SOURCE_OF_TRUTH.to_string(),
            catalog_path: catalog_path.to_string(),
            install_manifest_path: install_manifest_path.to_string(),
            social_state_path: social_state_path.to_string(),
            leaderboard_count: 0,
            entry_count: 0,
            review_count: 0,
            vote_count: 0,
            summary_count: 0,
            leaderboards: Vec::new(),
            entries: Vec::new(),
            reviews: Vec::new(),
            votes: Vec::new(),
            summaries: Vec::new(),
        };
        validate_rankings_state(&state)?;
        Ok(state)
    }
}

pub fn read_rankings_reviews_state(
    path: impl AsRef<Path>,
) -> Result<RankingsReviewsStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("rankings_state_path", path)?;
    let readback = read_json_record(path, &RANKINGS_SPEC, validate_rankings_state)?;
    Ok(RankingsReviewsStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn write_rankings_reviews_state(
    path: impl AsRef<Path>,
    state: &RankingsReviewsState,
) -> Result<RankingsReviewsStateReadback, EngineError> {
    validate_rankings_state(state)?;
    let path = path.as_ref();
    validate_path("rankings_state_path", path)?;
    let readback = write_json_record(path, &RANKINGS_SPEC, state, validate_rankings_state)?;
    Ok(RankingsReviewsStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}
