use std::path::Path;

use litd_engine::EngineError;

use super::super::mcp_project_aster_store::{AsterJsonSpec, read_json_record, write_json_record};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_hub_types::{
    HUB_LAUNCHER_STATE_SCHEMA, HubLauncherState, HubLauncherStateReadback,
};
use super::mcp_project_hub_validation::validate_hub_state;

const HUB_SOURCE_OF_TRUTH: &str =
    "Calyx Aster hub launcher state + publish catalog + install manifest";
const HUB_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "hub_launcher_state",
    collection: "litd_project_hub_launcher",
    row: b"mcp_project/hub_launcher/current",
    column: b"hub_launcher_state_json",
    format: "litd-hub-launcher-state-json",
};

pub fn read_hub_launcher_state(
    path: impl AsRef<Path>,
) -> Result<HubLauncherStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("hub_state_path", path)?;
    let readback = read_json_record(path, &HUB_SPEC, validate_hub_state)?;
    Ok(HubLauncherStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn write_hub_launcher_state(
    path: impl AsRef<Path>,
    state: &HubLauncherState,
) -> Result<HubLauncherStateReadback, EngineError> {
    validate_hub_state(state)?;
    let path = path.as_ref();
    validate_path("hub_state_path", path)?;
    let readback = write_json_record(path, &HUB_SPEC, state, validate_hub_state)?;
    Ok(HubLauncherStateReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        state: readback.value,
    })
}

pub fn new_hub_launcher_state(
    hub_id: impl Into<String>,
    catalog_path: impl Into<String>,
    install_manifest_path: impl Into<String>,
) -> HubLauncherState {
    HubLauncherState {
        schema: HUB_LAUNCHER_STATE_SCHEMA.to_string(),
        issue: 1157,
        hub_id: hub_id.into(),
        source_of_truth: HUB_SOURCE_OF_TRUTH.to_string(),
        catalog_path: catalog_path.into(),
        install_manifest_path: install_manifest_path.into(),
        query: None,
        required_tags: Vec::new(),
        installed_only: false,
        sort: super::mcp_project_hub_types::HubSortMode::Title,
        result_count: 0,
        cards: Vec::new(),
        launch_count: 0,
        launches: Vec::new(),
    }
}
