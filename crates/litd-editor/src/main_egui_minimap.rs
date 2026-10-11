use std::env;

use litd_editor::asset_scene::{EditorAssetSceneReport, EditorWorldMapReport};
use litd_engine::EngineError;

const MINIMAP_PROFILE_ENV: &str = "LITD_EDITOR_MINIMAP_PROFILE";
const MINIMAP_PROFILE_DEFAULT: &str = "default";
const MINIMAP_PROFILE_EMPTY_AUTHORING: &str = "empty_authoring";

pub(crate) fn editor_shell_scene_report(
    report: &EditorAssetSceneReport,
) -> Result<EditorAssetSceneReport, EngineError> {
    match env::var(MINIMAP_PROFILE_ENV) {
        Ok(value) => match normalize_minimap_profile(&value).as_str() {
            MINIMAP_PROFILE_DEFAULT => Ok(report.clone()),
            MINIMAP_PROFILE_EMPTY_AUTHORING => empty_authoring_minimap_report(report),
            _ => Err(EngineError::UiRenderInvalidConfig {
                field: "editor.minimap.profile".to_string(),
                reason: format!(
                    "unknown {MINIMAP_PROFILE_ENV} '{value}'; expected '{MINIMAP_PROFILE_DEFAULT}' or '{MINIMAP_PROFILE_EMPTY_AUTHORING}'"
                ),
            }),
        },
        Err(env::VarError::NotPresent) => Ok(report.clone()),
        Err(err) => Err(EngineError::UiRenderInvalidConfig {
            field: "editor.minimap.profile".to_string(),
            reason: format!("{MINIMAP_PROFILE_ENV}: {err}"),
        }),
    }
}

fn empty_authoring_minimap_report(
    report: &EditorAssetSceneReport,
) -> Result<EditorAssetSceneReport, EngineError> {
    let mut report = report.clone();
    report.world_map = EditorWorldMapReport::new(
        report.world_map.source_kind.clone(),
        report.world_map.bounds_min,
        report.world_map.bounds_max,
        report.world_map.outline.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )?;
    Ok(report)
}

fn normalize_minimap_profile(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimap_profile_normalization_accepts_cli_friendly_spellings() {
        assert_eq!(
            normalize_minimap_profile(" empty-authoring "),
            MINIMAP_PROFILE_EMPTY_AUTHORING
        );
        assert_eq!(
            normalize_minimap_profile("DEFAULT"),
            MINIMAP_PROFILE_DEFAULT
        );
    }
}
