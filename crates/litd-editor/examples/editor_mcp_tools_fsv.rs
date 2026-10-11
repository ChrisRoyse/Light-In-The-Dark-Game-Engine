use litd_engine::fsv_artifacts::env_path;
use std::{fs, path::PathBuf};

use litd_editor::{
    capabilities::default_capability_layer_manifest,
    history::to_stable_ron,
    mcp_tools::{
        McpDomainToolsetManifest, McpDomainToolsetReport, build_mcp_domain_toolset_report,
        default_mcp_domain_toolset_manifest, validate_mcp_domain_toolset_manifest,
    },
};
use litd_engine::EngineError;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct McpToolsFsvSnapshot {
    manifest: McpDomainToolsetManifest,
    report: McpDomainToolsetReport,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_MCP_TOOLS_FSV_STATS",
        "tmp/issue1145-editor-mcp-tools-fsv.txt",
    );
    let snapshot_path = env_path(
        "LITD_EDITOR_MCP_TOOLS_FSV_SNAPSHOT",
        "tmp/issue1145-editor-mcp-tools.ron",
    );

    let manifest = default_mcp_domain_toolset_manifest()?;
    let report = build_mcp_domain_toolset_report(&manifest)?;
    let duplicate_tool_code = duplicate_tool_code()?;
    let missing_readback_code = missing_readback_code()?;
    let legacy_render_code = legacy_render_code()?;

    let snapshot = McpToolsFsvSnapshot {
        manifest,
        report: report.clone(),
    };
    write_text(&snapshot_path, &to_stable_ron(&snapshot)?)?;
    write_text(
        &stats_path,
        &format!(
            "schema_version={}\n\
             parent_issue={}\n\
             root_mcp_epic={}\n\
             source_mcp_epic={}\n\
             capability_layer_issue={}\n\
             domain_count={}\n\
             tool_count={}\n\
             mutating_tool_count={}\n\
             readback_tool_count={}\n\
             fsv_tool_count={}\n\
             local_only_count={}\n\
             structured_result_count={}\n\
             lowering_guarded_mutation_count={}\n\
             ward_guarded_mutation_count={}\n\
             ledger_audited_count={}\n\
             shared_capability_coverage_count={}\n\
             child_issues={}\n\
             source_issues={}\n\
             readme_refs={}\n\
             shared_capability_fingerprint={}\n\
             toolset_fingerprint={}\n\
             snapshot_file={}\n\
             snapshot_file_bytes={}\n\
             duplicate_tool_code={}\n\
             missing_readback_code={}\n\
             legacy_render_code={}\n\
             window_opened=false\n\
             renderer_touched=false\n",
            report.schema_version,
            report.parent_issue,
            report.root_mcp_epic,
            report.source_mcp_epic,
            report.capability_layer_issue,
            report.domain_count,
            report.tool_count,
            report.mutating_tool_count,
            report.readback_tool_count,
            report.fsv_tool_count,
            report.local_only_count,
            report.structured_result_count,
            report.lowering_guarded_mutation_count,
            report.ward_guarded_mutation_count,
            report.ledger_audited_count,
            report.shared_capability_coverage_count,
            report
                .child_issues
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            report
                .source_issues
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            report.readme_refs.join("|"),
            report.shared_capability_fingerprint,
            report.toolset_fingerprint,
            snapshot_path.display(),
            fs::metadata(&snapshot_path)?.len(),
            duplicate_tool_code,
            missing_readback_code,
            legacy_render_code,
        ),
    )?;
    println!(
        "wrote editor MCP tools FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

fn duplicate_tool_code() -> Result<&'static str, EngineError> {
    let capability_layer = default_capability_layer_manifest()?;
    let mut manifest = default_mcp_domain_toolset_manifest()?;
    manifest.domains[1].tools[0].tool_name = manifest.domains[0].tools[0].tool_name.clone();
    match validate_mcp_domain_toolset_manifest(&manifest, &capability_layer) {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => Ok(err.code()),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn missing_readback_code() -> Result<&'static str, EngineError> {
    let capability_layer = default_capability_layer_manifest()?;
    let mut manifest = default_mcp_domain_toolset_manifest()?;
    manifest.domains[0].tools[0].fsv_readback_tool_name = "litd.editor.world.missing".to_string();
    match validate_mcp_domain_toolset_manifest(&manifest, &capability_layer) {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => Ok(err.code()),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn legacy_render_code() -> Result<&'static str, EngineError> {
    let capability_layer = default_capability_layer_manifest()?;
    let mut manifest = default_mcp_domain_toolset_manifest()?;
    manifest.domains[5].title = "g3n render controls".to_string();
    match validate_mcp_domain_toolset_manifest(&manifest, &capability_layer) {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => Ok(err.code()),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn write_text(path: &PathBuf, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
