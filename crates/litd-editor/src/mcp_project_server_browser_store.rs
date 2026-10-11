use std::path::Path;

use litd_engine::EngineError;

use super::super::mcp_project_aster_store::{
    AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_server_browser_types::{
    SERVER_BROWSER_STATE_SCHEMA, ServerBrowserState, ServerBrowserStateReadback,
};
use super::mcp_project_server_browser_validation::{
    server_browser_error, validate_server_browser_state,
};

const SERVER_BROWSER_SOURCE_OF_TRUTH: &str = "Calyx Aster server browser state";
const SERVER_BROWSER_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "server_browser_state",
    collection: "litd_project_server_browser",
    row: b"mcp_project/server_browser/current",
    column: b"server_browser_state_json",
    format: "litd-server-browser-state-json",
};

pub fn load_or_create_server_browser_state(
    path: impl AsRef<Path>,
    state_id: &str,
    multiplayer_state_path: &str,
    moderation_state_path: &str,
    social_state_path: &str,
) -> Result<ServerBrowserState, EngineError> {
    let path = path.as_ref();
    validate_path("server_browser_state_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &SERVER_BROWSER_SPEC, validate_server_browser_state)?
    {
        let state = readback.value;
        if state.state_id != state_id
            || state.multiplayer_state_path != multiplayer_state_path
            || state.moderation_state_path != moderation_state_path
            || state.social_state_path != social_state_path
        {
            return Err(server_browser_error(
                "state.identity",
                "server browser state identity or source paths mismatch",
            ));
        }
        Ok(state)
    } else {
        let state = ServerBrowserState {
            schema: SERVER_BROWSER_STATE_SCHEMA.to_string(),
            issue: 1042,
            state_id: state_id.to_string(),
            source_of_truth: SERVER_BROWSER_SOURCE_OF_TRUTH.to_string(),
            multiplayer_state_path: multiplayer_state_path.to_string(),
            moderation_state_path: moderation_state_path.to_string(),
            social_state_path: social_state_path.to_string(),
            listing_count: 0,
            public_listing_count: 0,
            private_listing_count: 0,
            query_count: 0,
            listings: Vec::new(),
            queries: Vec::new(),
        };
        validate_server_browser_state(&state)?;
        Ok(state)
    }
}

pub fn read_server_browser_state(
    path: impl AsRef<Path>,
) -> Result<ServerBrowserStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("server_browser_state_path", path)?;
    let readback = read_json_record(path, &SERVER_BROWSER_SPEC, validate_server_browser_state)?;
    Ok(ServerBrowserStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn write_server_browser_state(
    path: impl AsRef<Path>,
    state: &ServerBrowserState,
) -> Result<ServerBrowserStateReadback, EngineError> {
    validate_server_browser_state(state)?;
    let path = path.as_ref();
    validate_path("server_browser_state_path", path)?;
    let readback = write_json_record(
        path,
        &SERVER_BROWSER_SPEC,
        state,
        validate_server_browser_state,
    )?;
    Ok(ServerBrowserStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}
