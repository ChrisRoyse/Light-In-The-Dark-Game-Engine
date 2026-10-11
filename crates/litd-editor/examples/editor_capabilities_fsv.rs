use litd_engine::fsv_artifacts::env_path;
use std::{fs, path::PathBuf};

use litd_editor::{
    capabilities::{
        CapabilityLayerManifest, CapabilityLayerReport, build_capability_layer_report,
        default_capability_layer_manifest, validate_capability_layer_manifest,
    },
    history::to_stable_ron,
    modules::default_module_rebase_manifest,
};
use litd_engine::EngineError;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct CapabilitiesFsvSnapshot {
    manifest: CapabilityLayerManifest,
    report: CapabilityLayerReport,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_CAPABILITIES_FSV_STATS",
        "tmp/issue1144-editor-capabilities-fsv.txt",
    );
    let snapshot_path = env_path(
        "LITD_EDITOR_CAPABILITIES_FSV_SNAPSHOT",
        "tmp/issue1144-editor-capabilities.ron",
    );

    let manifest = default_capability_layer_manifest()?;
    let report = build_capability_layer_report(&manifest)?;
    let invalid_bind_code = invalid_bind_host_code()?;
    let invalid_route_code = invalid_route_code()?;

    let snapshot = CapabilitiesFsvSnapshot {
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
             operation_count={}\n\
             gui_bound_count={}\n\
             mcp_tool_count={}\n\
             readback_tool_count={}\n\
             calyx_backed_count={}\n\
             lowering_boundary_count={}\n\
             fsv_readback_required_count={}\n\
             module_coverage_count={}\n\
             source_issues={}\n\
             loopback_bind_host={}\n\
             loopback_only={}\n\
             registry_fingerprint={}\n\
             snapshot_file={}\n\
             snapshot_file_bytes={}\n\
             invalid_bind_host_code={}\n\
             invalid_route_code={}\n\
             window_opened=false\n\
             renderer_touched=false\n",
            report.schema_version,
            report.parent_issue,
            report.root_mcp_epic,
            report.operation_count,
            report.gui_bound_count,
            report.mcp_tool_count,
            report.readback_tool_count,
            report.calyx_backed_count,
            report.lowering_boundary_count,
            report.fsv_readback_required_count,
            report.module_coverage_count,
            report
                .source_issues
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            report.loopback_bind_host,
            report.loopback_only,
            report.registry_fingerprint,
            snapshot_path.display(),
            fs::metadata(&snapshot_path)?.len(),
            invalid_bind_code,
            invalid_route_code,
        ),
    )?;
    println!(
        "wrote editor capabilities FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

fn invalid_bind_host_code() -> Result<&'static str, EngineError> {
    let modules = default_module_rebase_manifest()?;
    let mut manifest = default_capability_layer_manifest()?;
    manifest.loopback_server.bind_host = "0.0.0.0".to_string();
    match validate_capability_layer_manifest(&manifest, &modules) {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => Ok(err.code()),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn invalid_route_code() -> Result<&'static str, EngineError> {
    let modules = default_module_rebase_manifest()?;
    let mut manifest = default_capability_layer_manifest()?;
    manifest.registry.operations[0].gui_route = "other://editor.world/apply".to_string();
    match validate_capability_layer_manifest(&manifest, &modules) {
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
