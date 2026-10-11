use std::{env, path::Path};

use litd_editor::asset_scene::{
    LoadedEditorAssetScene, load_blank_editor_world_asset_scene,
    load_default_editor_world_asset_scene_with_foliage_brush,
};
use litd_engine::EngineError;

const WORLD_PROFILE_ENV: &str = "LITD_EDITOR_WORLD_PROFILE";
const WORLD_PROFILE_DEFAULT: &str = "default";
const WORLD_PROFILE_BLANK_AUTHORING: &str = "blank_authoring";

pub(crate) fn load_editor_world_profile_with_foliage_brush_path(
    foliage_brush_path: Option<&Path>,
) -> Result<LoadedEditorAssetScene, EngineError> {
    match env::var(WORLD_PROFILE_ENV) {
        Ok(value) => match normalize_world_profile(&value).as_str() {
            WORLD_PROFILE_DEFAULT => {
                load_default_editor_world_asset_scene_with_foliage_brush(foliage_brush_path)
            }
            WORLD_PROFILE_BLANK_AUTHORING => load_blank_editor_world_asset_scene(),
            _ => Err(EngineError::UiRenderInvalidConfig {
                field: "editor.world.profile".to_string(),
                reason: format!(
                    "unknown {WORLD_PROFILE_ENV} '{value}'; expected '{WORLD_PROFILE_DEFAULT}' or '{WORLD_PROFILE_BLANK_AUTHORING}'"
                ),
            }),
        },
        Err(env::VarError::NotPresent) => {
            load_default_editor_world_asset_scene_with_foliage_brush(foliage_brush_path)
        }
        Err(err) => Err(EngineError::UiRenderInvalidConfig {
            field: "editor.world.profile".to_string(),
            reason: format!("{WORLD_PROFILE_ENV}: {err}"),
        }),
    }
}

fn normalize_world_profile(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_profile_normalization_accepts_cli_friendly_spellings() {
        assert_eq!(
            normalize_world_profile(" blank-authoring "),
            WORLD_PROFILE_BLANK_AUTHORING
        );
        assert_eq!(normalize_world_profile("DEFAULT"), WORLD_PROFILE_DEFAULT);
    }
}
