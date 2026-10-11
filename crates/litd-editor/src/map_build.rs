//! Deterministic map build artifacts: Lua script export and static-shadow bake.

use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_engine::EngineError;

pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const SOURCE_ISSUE: u32 = 1273;
pub(crate) const SCRIPT_BUILDER_ID: &str = "litd.editor.map_build.lua_export";
pub(crate) const SCRIPT_BUILDER_FINGERPRINT: &str = "map-build-lua-v1";
pub(crate) const SHADOW_BUILDER_ID: &str = "litd.editor.map_build.static_shadow_bake";
pub(crate) const SHADOW_BUILDER_FINGERPRINT: &str = "static-shadow-bake-v1";

#[path = "map_build_script.rs"]
mod script;
#[path = "map_build_shadow.rs"]
mod shadow;

pub use script::{
    MapScriptExport, MapScriptSummary, export_script, generate_lua_script,
    parse_generated_map_script,
};
pub use shadow::{
    MapBuildSaveReceipt, SavedMapBuild, StaticShadowBake, StaticShadowBakeReceipt,
    StaticShadowBakeSettings, StaticShadowCaster, bake_static_shadows,
    calculate_shadows_and_save_map, load_saved_map_build, load_static_shadow_bake,
    save_static_shadow_bake, validate_static_shadow_bake_for_document,
};

pub(crate) fn write_text_artifact(
    path: &Path,
    text: &str,
    field: &str,
) -> Result<String, EngineError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(&format!("{field}.parent"), error))?;
    }
    fs::write(path, text).map_err(|error| io_error(&format!("{field}.write"), error))?;
    let readback = fs::read(path).map_err(|error| io_error(&format!("{field}.read"), error))?;
    if readback != text.as_bytes() {
        return invalid_map_build(
            format!("{field}.readback"),
            "artifact bytes changed on readback",
        );
    }
    Ok(sha256_hex(&readback))
}

pub(crate) fn validate_output_extension(
    path: &Path,
    expected: &str,
    field: &str,
) -> Result<(), EngineError> {
    if path.extension().and_then(|extension| extension.to_str()) != Some(expected) {
        return invalid_map_build(field, format!("output path must end in .{expected}"));
    }
    Ok(())
}

pub(crate) fn io_error(field: &str, error: std::io::Error) -> EngineError {
    invalid_map_build_error(field, error.to_string())
}

pub(crate) fn invalid_map_build<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_map_build_error(field, reason))
}

pub(crate) fn invalid_map_build_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.map_build.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "map_build_tests.rs"]
mod tests;
