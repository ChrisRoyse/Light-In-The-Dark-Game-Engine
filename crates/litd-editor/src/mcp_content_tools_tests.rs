//! Guard-path coverage for the #1455 content-authoring MCP tool.
//!
//! The full author -> persist -> readback success path (which needs the canonical vault and its
//! measurement panel) is proven by `litd-engine/examples/issue1455_elf_runtime_slice_fsv.rs`.
//! These unit tests pin the cheap, Calyx-free guards: an unknown tool name and malformed arguments
//! must both fail loudly with a structured `editor.mcp_content_tools.*` field.

use serde_json::json;

use super::{
    AUTHOR_RACE_TOOL, AUTHORED_CONTENT_BOOT_SCHEMA, AuthorRaceReadback, author_race,
    boot_load_authored_race_content, call_mcp_content_tool,
};
use litd_engine::EngineError;

fn field_of(error: &EngineError) -> String {
    match error {
        EngineError::RpgStatsInvalidConfig { field, .. } => field.clone(),
        other => panic!("expected RpgStatsInvalidConfig, got {other:?}"),
    }
}

#[test]
fn unknown_content_tool_fails_loud() {
    let error = call_mcp_content_tool("litd.editor.rpg.not_a_tool", json!({})).unwrap_err();
    let field = field_of(&error);
    assert_eq!(field, "editor.mcp_content_tools.tool_name");
}

#[test]
fn malformed_author_race_arguments_fail_loud() {
    // Missing `spec`/`request_id` -> serde decode error surfaced as a structured arguments field,
    // never a silent default.
    let error = author_race(json!({ "request_id": "req-1" })).unwrap_err();
    let field = field_of(&error);
    assert_eq!(field, "editor.mcp_content_tools.arguments");
}

#[test]
fn author_race_tool_name_is_stable() {
    // The registry, catalog, and dispatch all key off this exact string.
    assert_eq!(AUTHOR_RACE_TOOL, "litd.editor.rpg.author_race");
}

#[test]
fn readback_is_serializable() {
    // The dispatch surface serializes the readback into the tool-call JSON result.
    let readback = AuthorRaceReadback {
        tool_name: AUTHOR_RACE_TOOL.to_string(),
        source_of_truth: "canonical engine content-panel Calyx vault".to_string(),
        request_id: "req-1".to_string(),
        requested_by: "editor.tester".to_string(),
        // #1825: derive from the single canonical-root constant instead of duplicating the
        // hard-pinned literal (#779). No foreign hardcoded path in test code.
        vault_path: std::path::Path::new(litd_engine::calyx_paths::GAMEEDITOR_CALYX_ROOT)
            .join("vaults")
            .join("x")
            .display()
            .to_string(),
        kind: litd_engine::calyx_content::EngineContentKind::Unit,
        id: "race.elf".to_string(),
        display_name: "Race Spec: Elf".to_string(),
        source_path: "editor://rpg/ashen_reach/race_specs/race.elf".to_string(),
        cx_id: "cx".to_string(),
        input_sha256: "abc".to_string(),
        lowered_package_fingerprint: "def".to_string(),
    };
    let value = serde_json::to_value(&readback).unwrap();
    assert_eq!(value["tool_name"], AUTHOR_RACE_TOOL);
    assert_eq!(value["id"], "race.elf");
}

#[test]
fn boot_load_is_well_formed_and_never_panics() {
    // Boot must not abort on a not-yet-authored row: the report is always well-formed, always
    // attempts race.elf, and is either loaded OR carries a loud error string (never both empty
    // AND ok).
    let report = boot_load_authored_race_content();
    assert_eq!(report.schema, AUTHORED_CONTENT_BOOT_SCHEMA);
    assert_eq!(report.attempted_ids, vec!["race.elf".to_string()]);
    let loaded_elf = report.loaded_ids.contains(&"race.elf".to_string());
    assert_eq!(
        loaded_elf,
        report.error.is_none(),
        "exactly one of loaded/error must hold: {report:?}"
    );
    if loaded_elf {
        assert!(report.elf_lowered_fingerprint.is_some());
    }
}
