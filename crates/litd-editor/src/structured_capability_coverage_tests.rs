use std::fs;

use litd_engine::EngineError;

use super::*;

#[test]
fn issue1421_covers_all_current_calyx_tools_and_subsystems() {
    let audit = build_structured_capability_coverage_audit(
        "issue1421-coverage",
        "runtime tools/list and structured-data issue evidence",
        current_tools(),
    )
    .unwrap();

    assert_eq!(audit.issue, 1421);
    assert_eq!(audit.advertised_tool_count, 31);
    assert_eq!(audit.mapped_tool_count, audit.advertised_tool_count);
    assert_eq!(audit.subsystem_count, 13);
    assert_eq!(audit.covered_subsystem_count, audit.subsystem_count);
    assert!(audit.missing_tools.is_empty());
    assert!(audit.unknown_tools.is_empty());
    assert!(audit.idle_subsystems.is_empty());
    assert_eq!(
        audit.surface_parity.declaration_count,
        crate::mcp_tool_registry::implemented_mcp_tool_names().len()
    );
    assert_eq!(audit.surface_parity.documented_exemption_count, 3);
    assert!(
        audit
            .surface_parity
            .missing_mcp_surface_declarations
            .is_empty()
    );
    assert!(
        audit
            .surface_parity
            .undeclared_single_surface_capabilities
            .is_empty()
    );
    assert!(audit.surface_parity.declarations.iter().any(|declaration| {
        declaration.mcp_tool_name.as_deref()
            == Some(crate::mcp_project_tools::MODERATION_SAFETY_TOOL)
            && declaration.gui_route.as_deref()
                == Some("egui://editor.commands/editor.project.moderation.submit_report")
            && declaration.exemption.is_none()
    }));
    assert!(
        audit
            .tool_uses
            .iter()
            .any(|tool| tool.tool_name == "calyx.ingest")
    );
    validate_structured_capability_coverage_audit(&audit).unwrap();
}

#[test]
fn issue1467_single_surface_capability_without_exemption_fails_validation() {
    let mut audit = build_structured_capability_coverage_audit(
        "issue1421-coverage",
        "runtime tools/list",
        current_tools(),
    )
    .unwrap();
    let declaration = audit
        .surface_parity
        .declarations
        .iter_mut()
        .find(|declaration| {
            declaration.mcp_tool_name.as_deref()
                == Some(crate::mcp_project_tools::MULTIPLAYER_SESSION_TOOL)
        })
        .unwrap();
    declaration.exemption = None;
    audit.surface_parity.documented_exemption_count -= 1;
    let error = validate_structured_capability_coverage_audit(&audit).unwrap_err();

    assert_ui_error(
        error,
        "editor.structured_capability_coverage.surface_parity.exemption",
        "needs an explicit exemption",
    );
}

#[test]
fn issue1467_missing_surface_declaration_fails_validation() {
    let mut audit = build_structured_capability_coverage_audit(
        "issue1421-coverage",
        "runtime tools/list",
        current_tools(),
    )
    .unwrap();
    audit.surface_parity.declarations.retain(|declaration| {
        declaration.mcp_tool_name.as_deref() != Some(crate::mcp_editor_session_tools::UNDO_TOOL)
    });
    audit.surface_parity.declaration_count = audit.surface_parity.declarations.len();
    audit.surface_parity.missing_mcp_surface_declarations =
        vec![crate::mcp_editor_session_tools::UNDO_TOOL.to_string()];
    let error = validate_structured_capability_coverage_audit(&audit).unwrap_err();

    assert_ui_error(
        error,
        "editor.structured_capability_coverage.coverage",
        "surface declarations",
    );
}

#[test]
fn issue1421_runtime_tool_drift_fails_loud() {
    let tools = current_tools()
        .into_iter()
        .filter(|tool| tool != "calyx.bits")
        .collect::<Vec<_>>();
    let error = build_structured_capability_coverage_audit(
        "issue1421-coverage",
        "runtime tools/list",
        tools,
    )
    .unwrap_err();

    assert_ui_error(
        error,
        "editor.structured_capability_coverage.live_tools",
        "runtime tools/list",
    );
}

#[test]
fn issue1421_missing_mapping_fails_validation() {
    let mut audit = build_structured_capability_coverage_audit(
        "issue1421-coverage",
        "runtime tools/list",
        current_tools(),
    )
    .unwrap();
    audit
        .tool_uses
        .retain(|tool| tool.tool_name != "calyx.bits");
    audit.mapped_tool_count = audit.tool_uses.len();
    audit.missing_tools = vec!["calyx.bits".to_string()];
    let error = validate_structured_capability_coverage_audit(&audit).unwrap_err();

    assert_ui_error(
        error,
        "editor.structured_capability_coverage.coverage",
        "must be covered",
    );
}

#[test]
fn issue1421_idle_subsystem_fails_validation() {
    let mut audit = build_structured_capability_coverage_audit(
        "issue1421-coverage",
        "runtime tools/list",
        current_tools(),
    )
    .unwrap();
    audit
        .subsystem_uses
        .retain(|use_case| use_case.subsystem != CalyxStructuredSubsystem::Forge);
    audit.covered_subsystem_count = audit.subsystem_uses.len();
    audit.idle_subsystems = vec![CalyxStructuredSubsystem::Forge];
    let error = validate_structured_capability_coverage_audit(&audit).unwrap_err();

    assert_ui_error(
        error,
        "editor.structured_capability_coverage.coverage",
        "must be covered",
    );
}

#[test]
fn issue1421_store_round_trips_audit_bytes() {
    let audit = build_structured_capability_coverage_audit(
        "issue1421-coverage",
        "runtime tools/list and structured-data issue evidence",
        current_tools(),
    )
    .unwrap();
    let root = temp_root("issue1421-store");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let writeback = write_structured_capability_coverage_audit(&root, &root, &audit).unwrap();
    let loaded = read_structured_capability_coverage_audit(&root, &audit.audit_id)
        .unwrap()
        .unwrap();
    let parsed = parse_structured_capability_coverage_audit(&loaded.bytes).unwrap();

    assert_eq!(writeback.bytes, loaded.bytes);
    assert_eq!(writeback.sha256, loaded.sha256);
    assert_eq!(parsed, audit);
    let _ = fs::remove_dir_all(&root);
}

fn current_tools() -> Vec<String> {
    crate::mcp_tools::expected_calyx_mcp_tool_names().unwrap()
}

fn assert_ui_error(error: EngineError, field: &str, reason_contains: &str) {
    match error {
        EngineError::UiRenderInvalidConfig {
            field: actual,
            reason,
        } => {
            assert_eq!(actual, field);
            assert!(reason.contains(reason_contains), "{reason}");
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn temp_root(label: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(label)
}
