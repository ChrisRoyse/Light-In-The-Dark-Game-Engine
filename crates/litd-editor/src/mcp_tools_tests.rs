use std::path::Path;

use serde_json::{Value, json};

use super::*;

#[test]
fn wrapper_manifest_matches_litd_editor_mcp_source_of_truth() {
    let manifest = default_mcp_domain_toolset_manifest().unwrap();
    let source_manifest = litd_editor_mcp::mcp_tools::default_mcp_domain_toolset_manifest()
        .expect("litd-editor-mcp manifest is the source of truth");
    assert_eq!(manifest, source_manifest);

    let report = build_mcp_domain_toolset_report(&manifest).unwrap();
    let source_report =
        litd_editor_mcp::mcp_tools::build_mcp_domain_toolset_report(&source_manifest)
            .expect("source manifest report should validate");
    assert_eq!(report, source_report);
}

#[test]
fn editor_source_tree_has_no_duplicate_mcp_manifest_implementation() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in [
        "mcp_tools_builders.rs",
        "mcp_tools_calyx_surface.rs",
        "mcp_tools_calyx_surface_tests.rs",
        "mcp_tools_catalog.rs",
        "mcp_tools_catalog_harness.rs",
        "mcp_tools_catalog_project.rs",
    ] {
        assert!(
            !src.join(file).exists(),
            "{file} must not exist in litd-editor; litd-editor-mcp is the manifest source of truth"
        );
    }
}

#[test]
fn wrapper_edges_fail_closed_without_second_catalog() {
    let capability_layer = litd_editor_mcp::capabilities::default_capability_layer_manifest()
        .expect("capability layer should validate");
    let source_manifest = default_mcp_domain_toolset_manifest().unwrap();
    let before = manifest_summary(&source_manifest);

    let mut duplicate = source_manifest.clone();
    duplicate.domains[1].tools[0].tool_name = duplicate.domains[0].tools[0].tool_name.clone();
    let duplicate_error = validate_mcp_domain_toolset_manifest(&duplicate, &capability_layer)
        .unwrap_err()
        .to_string();
    assert_eq!(before, manifest_summary(&source_manifest));
    println!(
        "issue1313 edge duplicate_tool before={} after={} error={}",
        before,
        manifest_summary(&source_manifest),
        duplicate_error
    );
    match validate_mcp_domain_toolset_manifest(&duplicate, &capability_layer) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.mcp_tools.tool.name");
            assert!(reason.contains("duplicate"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }

    let mut missing_readback = source_manifest.clone();
    missing_readback.domains[0].tools[0].fsv_readback_tool_name =
        "litd.editor.world.missing".to_string();
    let missing_error = validate_mcp_domain_toolset_manifest(&missing_readback, &capability_layer)
        .unwrap_err()
        .to_string();
    assert_eq!(before, manifest_summary(&source_manifest));
    println!(
        "issue1313 edge missing_readback before={} after={} error={}",
        before,
        manifest_summary(&source_manifest),
        missing_error
    );
    match validate_mcp_domain_toolset_manifest(&missing_readback, &capability_layer) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.mcp_tools.tool.readback");
            assert!(reason.contains("readback"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }

    let mut non_local = source_manifest.clone();
    non_local.domains[0].tools[0].local_only = false;
    let non_local_error = validate_mcp_domain_toolset_manifest(&non_local, &capability_layer)
        .unwrap_err()
        .to_string();
    assert_eq!(before, manifest_summary(&source_manifest));
    println!(
        "issue1313 edge non_local before={} after={} error={}",
        before,
        manifest_summary(&source_manifest),
        non_local_error
    );
    match validate_mcp_domain_toolset_manifest(&non_local, &capability_layer) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.mcp_tools.tool.safety");
            assert!(reason.contains("local-only"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn manifest_summary(manifest: &McpDomainToolsetManifest) -> Value {
    let report = build_mcp_domain_toolset_report(manifest).unwrap();
    json!({
        "domain_count": report.domain_count,
        "tool_count": report.tool_count,
        "mutating_tool_count": report.mutating_tool_count,
        "readback_tool_count": report.readback_tool_count,
        "toolset_fingerprint": report.toolset_fingerprint,
    })
}
