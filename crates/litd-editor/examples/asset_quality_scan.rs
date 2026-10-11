use litd_engine::EngineError;

use litd_editor::{asset_vault::AssetVaultCatalog, gltf_validation::validate_gltf_source};

fn main() -> Result<(), EngineError> {
    let requested = std::env::args().skip(1).collect::<Vec<_>>();
    if requested.is_empty() {
        return invalid_scan("args", "pass one or more vault-relative glTF/GLB paths");
    }
    let catalog = AssetVaultCatalog::load_default()?;
    let mut failure_count = 0;
    for relative_path in requested {
        let record = catalog
            .records()
            .iter()
            .find(|record| record.relative_path == relative_path)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.asset_quality_scan.record".to_string(),
                reason: format!("vault record not found: {relative_path}"),
            })?;
        match validate_gltf_source(&record.absolute_path, &record.relative_path) {
            Ok(report) => {
                println!(
                    "asset_quality source={} status=ok meshes={} primitives={} vertices={} indices={} triangles={} degenerates={} dependencies={}",
                    record.relative_path,
                    report.mesh_count,
                    report.primitive_count,
                    report.vertex_count,
                    report.index_count,
                    report.triangle_count,
                    report.degenerate_triangle_count,
                    report.dependencies.len(),
                );
            }
            Err(error) => {
                failure_count += 1;
                println!(
                    "asset_quality source={} status=error error={}",
                    record.relative_path, error
                );
            }
        }
    }
    if failure_count > 0 {
        return invalid_scan(
            "quality",
            format!("{failure_count} requested source asset(s) failed quality validation"),
        );
    }
    Ok(())
}

fn invalid_scan<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_quality_scan.{}", field.into()),
        reason: reason.into(),
    })
}
