use std::path::Path;

use litd_engine::EngineError;

use super::super::mcp_project_aster_store::{
    AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_updates_types::{
    GAME_UPDATES_STATE_SCHEMA, GameUpdatesState, GameUpdatesStateReadback,
};
use super::mcp_project_updates_validation::{update_error, validate_update_state};

const GAME_UPDATES_SOURCE_OF_TRUTH: &str = "Calyx Aster game updates state";
const GAME_UPDATES_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "game_updates_state",
    collection: "litd_project_game_updates",
    row: b"mcp_project/game_updates/current",
    column: b"game_updates_state_json",
    format: "litd-game-updates-state-json",
};

pub fn load_or_create_game_updates_state(
    path: impl AsRef<Path>,
    state_id: &str,
    catalog_path: &str,
    install_manifest_path: &str,
    social_state_path: &str,
) -> Result<GameUpdatesState, EngineError> {
    let path = path.as_ref();
    validate_path("updates_state_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &GAME_UPDATES_SPEC, validate_update_state)?
    {
        let state = readback.value;
        if state.state_id != state_id
            || state.catalog_path != catalog_path
            || state.install_manifest_path != install_manifest_path
            || state.social_state_path != social_state_path
        {
            return Err(update_error(
                "state.identity",
                "game updates state identity or source path mismatch",
            ));
        }
        Ok(state)
    } else {
        let state = GameUpdatesState {
            schema: GAME_UPDATES_STATE_SCHEMA.to_string(),
            issue: 1162,
            state_id: state_id.to_string(),
            source_of_truth: GAME_UPDATES_SOURCE_OF_TRUTH.to_string(),
            catalog_path: catalog_path.to_string(),
            install_manifest_path: install_manifest_path.to_string(),
            social_state_path: social_state_path.to_string(),
            channel_count: 0,
            release_count: 0,
            compatibility_count: 0,
            notification_count: 0,
            channels: Vec::new(),
            releases: Vec::new(),
            compatibilities: Vec::new(),
            notifications: Vec::new(),
        };
        validate_update_state(&state)?;
        Ok(state)
    }
}

pub fn read_game_updates_state(
    path: impl AsRef<Path>,
) -> Result<GameUpdatesStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("updates_state_path", path)?;
    let readback = read_json_record(path, &GAME_UPDATES_SPEC, validate_update_state)?;
    Ok(GameUpdatesStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn write_game_updates_state(
    path: impl AsRef<Path>,
    state: &GameUpdatesState,
) -> Result<GameUpdatesStateReadback, EngineError> {
    validate_update_state(state)?;
    let path = path.as_ref();
    validate_path("updates_state_path", path)?;
    let readback = write_json_record(path, &GAME_UPDATES_SPEC, state, validate_update_state)?;
    Ok(GameUpdatesStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}
