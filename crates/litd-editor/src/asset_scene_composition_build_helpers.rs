use litd_engine::{EngineError, render::editor_world::EditorWorldSceneLayerKind};

use super::geometry::CompositionBuild;

pub(super) fn stylized_asset_tint(file_name: &str) -> [f32; 4] {
    let lower = file_name.to_ascii_lowercase();
    if lower.contains("tree") || lower.contains("pine") {
        [0.09, 0.13, 0.11, 0.0]
    } else if lower.contains("rock") || lower.contains("pebble") {
        [0.30, 0.36, 0.44, 0.0]
    } else if lower.contains("mushroom") {
        [0.32, 0.38, 0.58, 0.90]
    } else if lower.contains("flower") || lower.contains("plant") || lower.contains("grass") {
        [0.16, 0.21, 0.17, 0.0]
    } else {
        [0.30, 0.34, 0.36, 0.0]
    }
}

pub(super) fn mark_build_layer(
    build: &mut CompositionBuild,
    kind: EditorWorldSceneLayerKind,
    add_layer: fn(&mut CompositionBuild) -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    let start = build.index_cursor();
    add_layer(build)?;
    build.mark_layer(kind, start)
}
