use std::path::Path;

use litd_engine::EngineError;

use super::super::mcp_project_aster_store::{
    AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_social_types::{
    SOCIAL_GRAPH_STATE_SCHEMA, SocialGraphState, SocialGraphStateReadback,
};
use super::mcp_project_social_validation::{social_error, validate_social_state};

const SOCIAL_GRAPH_SOURCE_OF_TRUTH: &str = "Calyx Aster social graph state";
const SOCIAL_GRAPH_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "social_graph_state",
    collection: "litd_project_social_graph",
    row: b"mcp_project/social_graph/current",
    column: b"social_graph_state_json",
    format: "litd-social-graph-state-json",
};

pub fn load_or_create_social_graph_state(
    path: impl AsRef<Path>,
    state_id: &str,
    multiplayer_state_path: &str,
) -> Result<SocialGraphState, EngineError> {
    let path = path.as_ref();
    validate_path("social_state_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &SOCIAL_GRAPH_SPEC, validate_social_state)?
    {
        let state = readback.value;
        if state.state_id != state_id || state.multiplayer_state_path != multiplayer_state_path {
            return Err(social_error(
                "state.identity",
                "social state identity or multiplayer source path mismatch",
            ));
        }
        Ok(state)
    } else {
        let state = SocialGraphState {
            schema: SOCIAL_GRAPH_STATE_SCHEMA.to_string(),
            issue: 1039,
            state_id: state_id.to_string(),
            source_of_truth: SOCIAL_GRAPH_SOURCE_OF_TRUTH.to_string(),
            multiplayer_state_path: multiplayer_state_path.to_string(),
            account_count: 0,
            profile_count: 0,
            friendship_count: 0,
            presence_count: 0,
            invite_count: 0,
            connection_count: 0,
            accounts: Vec::new(),
            profiles: Vec::new(),
            friendships: Vec::new(),
            presences: Vec::new(),
            invites: Vec::new(),
            join_connections: Vec::new(),
        };
        validate_social_state(&state)?;
        Ok(state)
    }
}

pub fn read_social_graph_state(
    path: impl AsRef<Path>,
) -> Result<SocialGraphStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("social_state_path", path)?;
    let readback = read_json_record(path, &SOCIAL_GRAPH_SPEC, validate_social_state)?;
    Ok(SocialGraphStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn write_social_graph_state(
    path: impl AsRef<Path>,
    state: &SocialGraphState,
) -> Result<SocialGraphStateReadback, EngineError> {
    validate_social_state(state)?;
    let path = path.as_ref();
    validate_path("social_state_path", path)?;
    let readback = write_json_record(path, &SOCIAL_GRAPH_SPEC, state, validate_social_state)?;
    Ok(SocialGraphStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}
