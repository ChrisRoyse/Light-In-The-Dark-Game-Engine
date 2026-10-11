use std::path::Path;

use litd_engine::EngineError;

use super::super::mcp_project_aster_store::{
    AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_multiplayer_types::{
    MULTIPLAYER_SESSION_STATE_SCHEMA, MultiplayerSessionState, MultiplayerSessionStateReadback,
};
use super::mcp_project_multiplayer_validation::{multiplayer_error, validate_multiplayer_state};

const MULTIPLAYER_SOURCE_OF_TRUTH: &str = "Calyx Aster multiplayer session state";
const MULTIPLAYER_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "multiplayer_session_state",
    collection: "litd_project_multiplayer_session",
    row: b"mcp_project/multiplayer_session/current",
    column: b"multiplayer_session_state_json",
    format: "litd-multiplayer-session-state-json",
};

pub fn load_or_create_multiplayer_session_state(
    path: impl AsRef<Path>,
    state_id: &str,
    hub_state_path: &str,
    install_manifest_path: &str,
) -> Result<MultiplayerSessionState, EngineError> {
    let path = path.as_ref();
    validate_path("multiplayer_state_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &MULTIPLAYER_SPEC, validate_multiplayer_state)?
    {
        let state = readback.value;
        if state.state_id != state_id
            || state.hub_state_path != hub_state_path
            || state.install_manifest_path != install_manifest_path
        {
            return Err(multiplayer_error(
                "state.identity",
                "multiplayer state identity or source paths mismatch",
            ));
        }
        Ok(state)
    } else {
        let state = MultiplayerSessionState {
            schema: MULTIPLAYER_SESSION_STATE_SCHEMA.to_string(),
            issue: 1038,
            state_id: state_id.to_string(),
            source_of_truth: MULTIPLAYER_SOURCE_OF_TRUTH.to_string(),
            hub_state_path: hub_state_path.to_string(),
            install_manifest_path: install_manifest_path.to_string(),
            lobby_count: 0,
            party_count: 0,
            join_count: 0,
            ticket_count: 0,
            server_count: 0,
            running_server_count: 0,
            lobbies: Vec::new(),
            servers: Vec::new(),
            parties: Vec::new(),
            joins: Vec::new(),
            tickets: Vec::new(),
        };
        validate_multiplayer_state(&state)?;
        Ok(state)
    }
}

pub fn read_multiplayer_session_state(
    path: impl AsRef<Path>,
) -> Result<MultiplayerSessionStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("multiplayer_state_path", path)?;
    let readback = read_json_record(path, &MULTIPLAYER_SPEC, validate_multiplayer_state)?;
    Ok(MultiplayerSessionStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn write_multiplayer_session_state(
    path: impl AsRef<Path>,
    state: &MultiplayerSessionState,
) -> Result<MultiplayerSessionStateReadback, EngineError> {
    validate_multiplayer_state(state)?;
    let path = path.as_ref();
    validate_path("multiplayer_state_path", path)?;
    let readback = write_json_record(path, &MULTIPLAYER_SPEC, state, validate_multiplayer_state)?;
    Ok(MultiplayerSessionStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}
