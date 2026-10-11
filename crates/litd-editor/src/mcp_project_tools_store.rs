use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

use super::{
    EcosystemPublishCatalog, HeadlessPlaytestRecord, LitdGamePackageRecord, LitdWorldProject,
    ProjectLifecycleRecord, PublishedGameRecord,
    mcp_project_aster_store::{
        AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
    },
    mcp_project_tools_litdworld_archive::{inspect_litdworld_archive, write_litdworld_archive},
    mcp_project_tools_validation::{
        reverify_confined_before_write, validate_path, validate_project_file,
        validate_project_state,
    },
};

pub const MCP_PROJECT_STATE_SCHEMA: &str = "litd.mcp.project_tools.state.v1";
pub const MCP_PROJECT_LEDGER_SCHEMA: &str = "litd.mcp.project_tools.ledger.v1";
pub const ECOSYSTEM_PUBLISH_CATALOG_SCHEMA: &str = "litd.ecosystem.publish_catalog.v1";
const MCP_PROJECT_STATE_SOURCE_OF_TRUTH: &str = "Calyx Aster MCP project/playtest/package state";
const MCP_PROJECT_LEDGER_SOURCE_OF_TRUTH: &str = "Calyx Aster MCP project tool ledger";
const ECOSYSTEM_PUBLISH_CATALOG_SOURCE_OF_TRUTH: &str = "Calyx Aster ecosystem publish catalog";
const MCP_PROJECT_STATE_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "project_state",
    collection: "litd_mcp_project_state",
    row: b"mcp_project/state/current",
    column: b"mcp_project_state_json",
    format: "mcp-project-state-json",
};
const MCP_PROJECT_LEDGER_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "project_tool_ledger",
    collection: "litd_mcp_project_ledger",
    row: b"mcp_project/tool_ledger/current",
    column: b"mcp_project_tool_ledger_json",
    format: "mcp-project-ledger-json",
};
const ECOSYSTEM_PUBLISH_CATALOG_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "publish_catalog",
    collection: "litd_ecosystem_publish_catalog",
    row: b"mcp_project/publish_catalog/current",
    column: b"ecosystem_publish_catalog_json",
    format: "litd-publish-catalog-json",
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpProjectState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub update_count: u64,
    pub projects: Vec<ProjectLifecycleRecord>,
    pub playtests: Vec<HeadlessPlaytestRecord>,
    pub packages: Vec<LitdGamePackageRecord>,
    #[serde(default)]
    pub published_games: Vec<PublishedGameRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpProjectStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state_fingerprint: String,
    pub state: McpProjectState,
    pub project_count: usize,
    pub playtest_count: usize,
    pub package_count: usize,
    #[serde(default)]
    pub published_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpProjectToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpProjectToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpProjectToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub state_path: String,
    pub state_sha256: String,
    pub state_bytes: u64,
    pub action_summary: String,
    pub affected_ids: Vec<String>,
    pub project_count: usize,
    pub playtest_count: usize,
    pub package_count: usize,
    #[serde(default)]
    pub published_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpProjectToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpProjectToolLedger,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LitdWorldProjectReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub project: LitdWorldProject,
    pub seq: u64,
    pub graph_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcosystemPublishCatalogReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub catalog: EcosystemPublishCatalog,
}

impl McpProjectState {
    pub fn new(state_id: impl Into<String>) -> Result<Self, EngineError> {
        let state = Self {
            schema: MCP_PROJECT_STATE_SCHEMA.to_string(),
            issue: 926,
            state_id: state_id.into(),
            source_of_truth: MCP_PROJECT_STATE_SOURCE_OF_TRUTH.to_string(),
            update_count: 0,
            projects: Vec::new(),
            playtests: Vec::new(),
            packages: Vec::new(),
            published_games: Vec::new(),
        };
        validate_mcp_project_state(&state)?;
        Ok(state)
    }
}

pub fn load_or_create_mcp_project_state(
    path: impl AsRef<Path>,
    state_id: impl Into<String>,
) -> Result<McpProjectState, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let state_id = state_id.into();
    if let Some(readback) =
        read_json_record_optional(path, &MCP_PROJECT_STATE_SPEC, validate_mcp_project_state)?
    {
        let state = readback.value;
        if state.state_id != state_id {
            return Err(store_error("state_id", "state_id mismatch"));
        }
        Ok(state)
    } else {
        McpProjectState::new(state_id)
    }
}

pub fn write_mcp_project_state(
    path: impl AsRef<Path>,
    state: &McpProjectState,
) -> Result<McpProjectStateReadback, EngineError> {
    validate_mcp_project_state(state)?;
    let path = path.as_ref();
    validate_path("state_path", path)?;
    // #1518: last confinement check before the Aster vault open closes the symlink/junction
    // TOCTOU window left open between the handler's confine_tool_paths and this write.
    reverify_confined_before_write("state_path", path)?;
    let readback = write_json_record(
        path,
        &MCP_PROJECT_STATE_SPEC,
        state,
        validate_mcp_project_state,
    )?;
    readback_from_state(
        Path::new(&readback.path),
        readback.bytes,
        readback.sha256,
        readback.value,
    )
}

pub fn read_mcp_project_state(
    path: impl AsRef<Path>,
) -> Result<McpProjectStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let readback = read_json_record(path, &MCP_PROJECT_STATE_SPEC, validate_mcp_project_state)?;
    readback_from_state(
        Path::new(&readback.path),
        readback.bytes,
        readback.sha256,
        readback.value,
    )
}

pub fn write_litdworld_project(
    path: impl AsRef<Path>,
    project: &LitdWorldProject,
) -> Result<LitdWorldProject, EngineError> {
    validate_project_file(project)?;
    let path = path.as_ref();
    validate_path("project_path", path)?;
    // #1518: last confinement check before the archive write closes the symlink/junction
    // TOCTOU window left open between the handler's confine_tool_paths and this write.
    reverify_confined_before_write("project_path", path)?;
    let readback = write_litdworld_archive(path, project)?;
    Ok(readback.project)
}

pub fn read_litdworld_project(path: impl AsRef<Path>) -> Result<LitdWorldProject, EngineError> {
    let path = path.as_ref();
    validate_path("project_path", path)?;
    let readback = inspect_litdworld_archive(path)?;
    Ok(readback.project)
}

pub fn read_litdworld_project_readback(
    path: impl AsRef<Path>,
) -> Result<LitdWorldProjectReadback, EngineError> {
    let path = path.as_ref();
    validate_path("project_path", path)?;
    let readback = inspect_litdworld_archive(path)?;
    Ok(LitdWorldProjectReadback {
        path: readback.path,
        bytes: u64::try_from(readback.bytes).map_err(|error| {
            store_error(
                "litdworld_project.bytes",
                format!("bytes overflow: {error}"),
            )
        })?,
        sha256: readback.sha256,
        project: readback.project,
        seq: 0,
        graph_rows: readback.zip_entry_count,
    })
}

pub fn write_mcp_project_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpProjectToolRecord,
) -> Result<McpProjectToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    // #1518: last confinement check before the ledger read-modify-write closes the
    // symlink/junction TOCTOU window left open between confine_tool_paths and this write.
    reverify_confined_before_write("ledger_path", path)?;
    let ledger_id = ledger_id.into();
    // Append to the existing ledger instead of overwriting it: the record_count/records
    // Vec schema is an accumulating audit trail, and overwriting left it at a depth of
    // one — every project mutation replaced the prior audit record (#1490). Read the
    // current ledger (if any), push the new record, and persist the full history.
    let mut ledger = match read_json_record_optional(
        path,
        &MCP_PROJECT_LEDGER_SPEC,
        validate_mcp_project_tool_ledger,
    )? {
        Some(readback) => {
            let existing = readback.value;
            if existing.ledger_id != ledger_id {
                return Err(store_error(
                    "project_tool_ledger.ledger_id",
                    format!(
                        "ledger_id mismatch: existing {}, requested {ledger_id}",
                        existing.ledger_id
                    ),
                ));
            }
            existing
        }
        None => McpProjectToolLedger {
            schema: MCP_PROJECT_LEDGER_SCHEMA.to_string(),
            issue: 926,
            ledger_id,
            source_of_truth: MCP_PROJECT_LEDGER_SOURCE_OF_TRUTH.to_string(),
            record_count: 0,
            records: Vec::new(),
        },
    };
    ledger.records.push(record);
    ledger.record_count = ledger.records.len();
    validate_mcp_project_tool_ledger(&ledger)?;
    let readback = write_json_record(
        path,
        &MCP_PROJECT_LEDGER_SPEC,
        &ledger,
        validate_mcp_project_tool_ledger,
    )?;
    Ok(McpProjectToolLedgerReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        ledger: readback.value,
    })
}

pub fn read_mcp_project_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpProjectToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_json_record(
        path,
        &MCP_PROJECT_LEDGER_SPEC,
        validate_mcp_project_tool_ledger,
    )?;
    Ok(McpProjectToolLedgerReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        ledger: readback.value,
    })
}

pub fn load_or_create_publish_catalog(
    path: impl AsRef<Path>,
    catalog_id: impl Into<String>,
) -> Result<EcosystemPublishCatalog, EngineError> {
    let path = path.as_ref();
    validate_path("catalog_path", path)?;
    let catalog_id = catalog_id.into();
    if let Some(readback) = read_json_record_optional(
        path,
        &ECOSYSTEM_PUBLISH_CATALOG_SPEC,
        validate_publish_catalog,
    )? {
        let catalog = readback.value;
        if catalog.catalog_id != catalog_id {
            return Err(store_error(
                "publish_catalog.catalog_id",
                "catalog_id mismatch",
            ));
        }
        Ok(catalog)
    } else {
        let catalog = EcosystemPublishCatalog {
            schema: ECOSYSTEM_PUBLISH_CATALOG_SCHEMA.to_string(),
            issue: 1156,
            catalog_id,
            source_of_truth: ECOSYSTEM_PUBLISH_CATALOG_SOURCE_OF_TRUTH.to_string(),
            entry_count: 0,
            entries: Vec::new(),
        };
        validate_publish_catalog(&catalog)?;
        Ok(catalog)
    }
}

pub fn write_publish_catalog(
    path: impl AsRef<Path>,
    catalog: &EcosystemPublishCatalog,
) -> Result<EcosystemPublishCatalogReadback, EngineError> {
    validate_publish_catalog(catalog)?;
    let path = path.as_ref();
    validate_path("catalog_path", path)?;
    let readback = write_json_record(
        path,
        &ECOSYSTEM_PUBLISH_CATALOG_SPEC,
        catalog,
        validate_publish_catalog,
    )?;
    Ok(EcosystemPublishCatalogReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        catalog: readback.value,
    })
}

pub fn read_publish_catalog(
    path: impl AsRef<Path>,
) -> Result<EcosystemPublishCatalogReadback, EngineError> {
    let path = path.as_ref();
    validate_path("catalog_path", path)?;
    let readback = read_json_record(
        path,
        &ECOSYSTEM_PUBLISH_CATALOG_SPEC,
        validate_publish_catalog,
    )?;
    Ok(EcosystemPublishCatalogReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        catalog: readback.value,
    })
}

pub fn validate_mcp_project_state(state: &McpProjectState) -> Result<(), EngineError> {
    if state.schema != MCP_PROJECT_STATE_SCHEMA
        || state.issue != 926
        || state.state_id.trim().is_empty()
        || state.source_of_truth != MCP_PROJECT_STATE_SOURCE_OF_TRUTH
    {
        return Err(store_error(
            "state",
            "schema, issue, and identity are invalid",
        ));
    }
    validate_project_state(state)
}

pub fn validate_mcp_project_tool_ledger(ledger: &McpProjectToolLedger) -> Result<(), EngineError> {
    if ledger.schema != MCP_PROJECT_LEDGER_SCHEMA
        || ledger.issue != 926
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.source_of_truth != MCP_PROJECT_LEDGER_SOURCE_OF_TRUTH
        || ledger.record_count != ledger.records.len()
        || ledger.records.is_empty()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and count are invalid",
        ));
    }
    for record in &ledger.records {
        if record.request_id.trim().is_empty()
            || record.requested_by.trim().is_empty()
            || record.state_sha256.len() != 64
            || record.state_bytes == 0
            || record.affected_ids.is_empty()
        {
            return Err(store_error("record", "required record fields are invalid"));
        }
    }
    Ok(())
}

fn readback_from_state(
    path: &Path,
    bytes: u64,
    sha256: String,
    state: McpProjectState,
) -> Result<McpProjectStateReadback, EngineError> {
    validate_mcp_project_state(&state)?;
    let state_fingerprint = stable_fingerprint(&state)?;
    Ok(McpProjectStateReadback {
        path: path.display().to_string(),
        bytes,
        sha256,
        state_fingerprint,
        project_count: state.projects.len(),
        playtest_count: state.playtests.len(),
        package_count: state.packages.len(),
        published_count: state.published_games.len(),
        state,
    })
}

fn validate_publish_catalog(catalog: &EcosystemPublishCatalog) -> Result<(), EngineError> {
    if catalog.schema != ECOSYSTEM_PUBLISH_CATALOG_SCHEMA
        || catalog.issue != 1156
        || catalog.catalog_id.trim().is_empty()
        || catalog.source_of_truth != ECOSYSTEM_PUBLISH_CATALOG_SOURCE_OF_TRUTH
        || catalog.entry_count != catalog.entries.len()
    {
        return Err(store_error(
            "publish_catalog",
            "schema, issue, identity, and count are invalid",
        ));
    }
    let mut publish_ids = std::collections::BTreeSet::new();
    let mut content_addresses = std::collections::BTreeSet::new();
    for entry in &catalog.entries {
        super::mcp_project_tools_validation::validate_published_record(entry)?;
        if !publish_ids.insert(entry.publish_id.as_str()) {
            return Err(store_error(
                "publish_catalog.entries",
                "duplicate publish_id",
            ));
        }
        if !content_addresses.insert(entry.content_address.as_str()) {
            return Err(store_error(
                "publish_catalog.entries",
                "duplicate content_address",
            ));
        }
    }
    Ok(())
}

fn store_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.mcp_project_tools.store.{}", field.into()),
        reason: reason.to_string(),
    }
}
