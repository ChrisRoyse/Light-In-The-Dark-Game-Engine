use litd_engine::fsv_artifacts::env_path;
use std::{fs, path::PathBuf};

use litd_editor::{
    history::to_stable_ron,
    modules::{
        build_module_rebase_report, default_module_rebase_manifest, validate_module_rebase_manifest,
    },
};
use litd_engine::EngineError;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ModulesFsvSnapshot {
    manifest: litd_editor::modules::EditorModuleRebaseManifest,
    report: litd_editor::modules::EditorModuleRebaseReport,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_MODULES_FSV_STATS",
        "tmp/issue1142-editor-modules-fsv.txt",
    );
    let snapshot_path = env_path(
        "LITD_EDITOR_MODULES_FSV_SNAPSHOT",
        "tmp/issue1142-editor-modules.ron",
    );

    let manifest = default_module_rebase_manifest()?;
    let report = build_module_rebase_report(&manifest)?;
    let invalid_missing_code = invalid_missing_required_module_code()?;
    let invalid_namespace_code = invalid_namespace_code()?;

    let snapshot = ModulesFsvSnapshot {
        manifest,
        report: report.clone(),
    };
    write_text(&snapshot_path, &to_stable_ron(&snapshot)?)?;
    write_text(
        &stats_path,
        &format!(
            "schema_version={}\n\
             parent_issue={}\n\
             root_editor_epic={}\n\
             module_count={}\n\
             source_issues={}\n\
             families={}\n\
             gui_surface_count={}\n\
             mcp_tool_group_count={}\n\
             rust_engine_bound_count={}\n\
             calyx_bound_count={}\n\
             lowering_valve_count={}\n\
             shared_capability_count={}\n\
             readme_refs={}\n\
             snapshot_file={}\n\
             snapshot_file_bytes={}\n\
             invalid_missing_required_code={}\n\
             invalid_namespace_code={}\n\
             legacy_engine_refs=false\n\
             window_opened=false\n\
             renderer_touched=false\n",
            report.schema_version,
            report.parent_issue,
            report.root_editor_epic,
            report.module_count,
            report
                .source_issues
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            report
                .families
                .iter()
                .map(|family| format!("{family:?}"))
                .collect::<Vec<_>>()
                .join("|"),
            report.gui_surface_count,
            report.mcp_tool_group_count,
            report.rust_engine_bound_count,
            report.calyx_bound_count,
            report.lowering_valve_count,
            report.shared_capability_count,
            report.readme_refs.join("|"),
            snapshot_path.display(),
            fs::metadata(&snapshot_path)?.len(),
            invalid_missing_code,
            invalid_namespace_code,
        ),
    )?;
    println!("wrote editor modules FSV stats to {}", stats_path.display());
    Ok(())
}

fn invalid_missing_required_module_code() -> Result<&'static str, EngineError> {
    let mut manifest = default_module_rebase_manifest()?;
    manifest
        .modules
        .retain(|module| module.id != "lore.worldbuilding");
    match validate_module_rebase_manifest(&manifest) {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => Ok(err.code()),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn invalid_namespace_code() -> Result<&'static str, EngineError> {
    let mut manifest = default_module_rebase_manifest()?;
    manifest.modules[0].capability_namespace = "legacy.world".to_string();
    match validate_module_rebase_manifest(&manifest) {
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
