use litd_engine::EngineError;

use crate::asset_vault::{AssetVaultCatalog, AssetVaultRecord};

pub(super) fn source_archive_record<'a>(
    catalog: &'a AssetVaultCatalog,
    asset_id: &str,
) -> Result<&'a AssetVaultRecord, EngineError> {
    catalog
        .records()
        .iter()
        .find(|record| record.record_type == "download" && record.asset_id == asset_id)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.archive_record".to_string(),
            reason: format!("missing source archive manifest row for {asset_id}"),
        })
}

pub(super) fn extracted_record<'a>(
    catalog: &'a AssetVaultCatalog,
    relative_path: &str,
) -> Result<&'a AssetVaultRecord, EngineError> {
    let record = catalog
        .records()
        .iter()
        .find(|record| record.record_type == "extraction" && record.relative_path == relative_path)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.record".to_string(),
            reason: format!("missing extracted vault record {relative_path}"),
        })?;
    if !record.absolute_path.is_file() {
        return invalid_composition(
            "record.exists",
            format!(
                "extracted vault file is missing: {}",
                record.absolute_path.display()
            ),
        );
    }
    Ok(record)
}

pub(super) fn manifest_file_record<'a>(
    catalog: &'a AssetVaultCatalog,
    relative_path: &str,
) -> Result<&'a AssetVaultRecord, EngineError> {
    let record = catalog
        .records()
        .iter()
        .find(|record| {
            matches!(record.record_type.as_str(), "download" | "extraction")
                && record.relative_path == relative_path
        })
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.record".to_string(),
            reason: format!("missing vault manifest record {relative_path}"),
        })?;
    if !record.absolute_path.is_file() {
        return invalid_composition(
            "record.exists",
            format!(
                "vault manifest file is missing: {}",
                record.absolute_path.display()
            ),
        );
    }
    Ok(record)
}

pub(super) fn invalid_composition<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.composition.{}", field.into()),
        reason: reason.into(),
    })
}
