//! #1455 editor-side content-authoring MCP tool.
//!
//! `author_race` is the keystone that lets an editor author write a race straight into the
//! CANONICAL engine content-panel vault, so the row loads at runtime (via
//! `litd_engine::calyx_content_runtime`) instead of living only in a `#[cfg(test)]` fixture.
//!
//! The record is built through the SAME canonical builder the fixture uses
//! (`race_spec_content_records`), which forces the `editor://` source path, forbids self-attesting
//! `TestPass` anchors, and stamps the diegetic-map guardrail flags to false. That means an
//! authored race is byte-identical to the known-good spec oracle and its lowered fingerprint
//! matches by construction - the authoring tool cannot smuggle in a compiled `rpg://` row or a
//! free-map flag, because every write goes back through `persist_and_lower_rpg_authored_content`,
//! which validates and rejects those closed.

use litd_engine::{
    EngineError,
    calyx_content::{EngineContentKind, resolve_engine_content_panel_vault},
    calyx_content_runtime::{FrozenContentTable, open_canonical_engine_content_store},
    rpg::{RaceSpec, persist_and_lower_rpg_authored_content, race_spec_content_records},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

pub const AUTHOR_RACE_TOOL: &str = "litd.editor.rpg.author_race";

/// The one race id the #1455 vertical slice loads at editor boot. The Elf row is authored by the
/// `author_race` tool (or the FSV harness) into the canonical vault; boot freezes it back so it is
/// live and editable, not stranded in a `#[cfg(test)]` fixture.
pub const ELF_RACE_CONTENT_ID: &str = "race.elf";

/// Outcome of the boot-time authored-content load. Held by the editor app so the content browser
/// can show the loaded rows. A load failure is captured here (and logged at error level by the
/// caller), never swallowed into a fake success - but it does not abort editor boot, since a fresh
/// checkout has not authored the row yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthoredContentBootReport {
    pub schema: String,
    pub attempted_ids: Vec<String>,
    pub loaded_ids: Vec<String>,
    pub elf_lowered_fingerprint: Option<String>,
    pub error: Option<String>,
}

pub const AUTHORED_CONTENT_BOOT_SCHEMA: &str = "litd.editor.authored_content_boot.v1";

/// Load the #1455 authored race rows from the CANONICAL vault at editor startup and freeze them
/// into an in-memory table (outside any hot path). Returns a structured report instead of erroring
/// so the editor still boots when the row has not been authored yet; the error, if any, is loud in
/// the report and logged by the caller.
pub fn boot_load_authored_race_content() -> AuthoredContentBootReport {
    let ids = [(EngineContentKind::Unit, ELF_RACE_CONTENT_ID.to_string())];
    let attempted_ids = ids.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>();
    match open_canonical_engine_content_store()
        .and_then(|store| FrozenContentTable::freeze(&store, &ids))
    {
        Ok(table) => AuthoredContentBootReport {
            schema: AUTHORED_CONTENT_BOOT_SCHEMA.to_string(),
            loaded_ids: table.rows.iter().map(|row| row.id.clone()).collect(),
            elf_lowered_fingerprint: table
                .fingerprint(EngineContentKind::Unit, ELF_RACE_CONTENT_ID)
                .map(str::to_string),
            attempted_ids,
            error: None,
        },
        Err(error) => AuthoredContentBootReport {
            schema: AUTHORED_CONTENT_BOOT_SCHEMA.to_string(),
            attempted_ids,
            loaded_ids: Vec::new(),
            elf_lowered_fingerprint: None,
            error: Some(error.to_string()),
        },
    }
}

/// Editor-boot entry point (#1455): load authored race rows from the canonical vault and log the
/// outcome loudly. Kept in the lib (not `main.rs`) so the binary boot hook stays a single call and
/// the load logic is unit-testable. Returns the report for callers that want to retain it.
pub fn load_and_log_authored_race_content_at_boot() -> AuthoredContentBootReport {
    let report = boot_load_authored_race_content();
    match &report.error {
        None => tracing::info!(
            target: "litd_editor::mcp_content_tools",
            loaded_ids = ?report.loaded_ids,
            elf_lowered_fingerprint = ?report.elf_lowered_fingerprint,
            "authored race content loaded from canonical vault"
        ),
        Some(error) => tracing::error!(
            target: "litd_editor::mcp_content_tools",
            attempted_ids = ?report.attempted_ids,
            error = %error,
            "authored race content not loaded at boot (author it via author_race)"
        ),
    }
    report
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorRaceArgs {
    pub request_id: String,
    pub requested_by: String,
    pub spec: RaceSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorRaceReadback {
    pub tool_name: String,
    pub source_of_truth: String,
    pub request_id: String,
    pub requested_by: String,
    pub vault_path: String,
    pub kind: EngineContentKind,
    pub id: String,
    pub display_name: String,
    pub source_path: String,
    pub cx_id: String,
    pub input_sha256: String,
    pub lowered_package_fingerprint: String,
}

/// Dispatch entry for the content MCP tool family. Kept tiny - the registry routes
/// `AUTHOR_RACE_TOOL` here before the generic domain fan-out.
pub fn call_mcp_content_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<AuthorRaceReadback, EngineError> {
    match tool_name {
        AUTHOR_RACE_TOOL => author_race(arguments),
        other => Err(invalid_mcp_content(
            "tool_name",
            format!("{other} is not a registered content MCP tool"),
        )),
    }
}

/// Author a race into the canonical engine content-panel vault and return the persisted readback.
pub fn author_race(arguments: Value) -> Result<AuthorRaceReadback, EngineError> {
    let args: AuthorRaceArgs = parse_args(AUTHOR_RACE_TOOL, arguments)?;
    validate_request_identity(&args.request_id, &args.requested_by)?;

    // Canonical builder -> editor:// source, guardrail flags false, no TestPass anchor.
    let records = race_spec_content_records(&args.spec)?;
    let vault_dir = resolve_engine_content_panel_vault()
        .map_err(|error| invalid_mcp_content("canonical_vault", error.to_string()))?;
    let store = open_canonical_engine_content_store()
        .map_err(|error| invalid_mcp_content("open_canonical_store", error.to_string()))?;
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)
        .map_err(|error| invalid_mcp_content("persist_and_lower", error.to_string()))?;

    let entity = lowered.entities.first().ok_or_else(|| {
        invalid_mcp_content(
            "lowered.entities",
            "authoring produced no persisted entity readback",
        )
    })?;

    Ok(AuthorRaceReadback {
        tool_name: AUTHOR_RACE_TOOL.to_string(),
        source_of_truth: "canonical engine content-panel Calyx vault".to_string(),
        request_id: args.request_id,
        requested_by: args.requested_by,
        vault_path: vault_dir.display().to_string(),
        kind: entity.kind,
        id: entity.id.clone(),
        display_name: entity.display_name.clone(),
        source_path: entity.source_path.clone(),
        cx_id: entity.cx_id.clone(),
        input_sha256: entity.input_sha256.clone(),
        lowered_package_fingerprint: entity.lowered_package_fingerprint.clone(),
    })
}

fn validate_request_identity(request_id: &str, requested_by: &str) -> Result<(), EngineError> {
    if request_id.trim().is_empty() {
        return Err(invalid_mcp_content(
            "request_id",
            "request_id must be non-empty",
        ));
    }
    if requested_by.trim().is_empty() {
        return Err(invalid_mcp_content(
            "requested_by",
            "requested_by must be non-empty",
        ));
    }
    Ok(())
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_content(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

fn invalid_mcp_content(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.mcp_content_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_content_tools",
        field = %field,
        reason = %reason,
        "invalid content MCP tool call"
    );
    EngineError::RpgStatsInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "mcp_content_tools_tests.rs"]
mod tests;
