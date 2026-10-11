use litd_engine::{
    EngineError,
    render::editor_world::{
        EditorWorldSceneBuffers, EditorWorldSceneBuffersNewArgs, EditorWorldTextureData,
    },
};

use super::{EditorAssetSceneReport, EditorWorldMapReport, LoadedEditorAssetScene};

const BLANK_WORLD_SOURCE_ID: &str = "litd.editor.blank_world.authoring_v1";
const BLANK_WORLD_SOURCE_PATH: &str = "generated://editor/blank-world/authoring-v1";
const BLANK_WORLD_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const BLANK_WORLD_EXTENT: f32 = 96.0;

pub(super) fn load_blank_editor_world_asset_scene_impl()
-> Result<LoadedEditorAssetScene, EngineError> {
    let base_color_texture = solid_texture(
        "blank-world-base-color",
        "generated://editor/blank-world/default-base-color",
        [28, 38, 48, 255],
    )?;
    let normal_texture = solid_texture(
        "blank-world-normal",
        "generated://editor/blank-world/default-normal-flat",
        [128, 128, 255, 255],
    )?;
    let metallic_roughness_texture = solid_texture(
        "blank-world-metallic-roughness",
        "generated://editor/blank-world/default-metallic-roughness",
        [0, 128, 255, 255],
    )?;
    let scene = EditorWorldSceneBuffers::new(EditorWorldSceneBuffersNewArgs {
        scene_name: "blank_world.authoring_v1",
        source_id: BLANK_WORLD_SOURCE_ID,
        source_path: BLANK_WORLD_SOURCE_PATH,
        dependency_count: 0,
        material_count: 0,
        texture_count: 0,
        image_count: 0,
        vertices: Vec::new(),
        indices: Vec::new(),
        mesh_primitives: 0,
        base_color_texture,
        normal_texture,
        metallic_roughness_texture,
    })?;
    let bounds_min = [-BLANK_WORLD_EXTENT * 0.5, 0.0, -BLANK_WORLD_EXTENT * 0.5];
    let bounds_max = [BLANK_WORLD_EXTENT * 0.5, 0.0, BLANK_WORLD_EXTENT * 0.5];
    let world_map =
        EditorWorldMapReport::from_fitted_bounds("blank_authoring_bounds", bounds_min, bounds_max)?;
    Ok(LoadedEditorAssetScene {
        scene,
        report: EditorAssetSceneReport {
            product_id: BLANK_WORLD_SOURCE_ID.to_string(),
            source_relative_path: BLANK_WORLD_SOURCE_PATH.to_string(),
            source_sha256: BLANK_WORLD_SHA.to_string(),
            dependency_count: 0,
            material_count: 0,
            texture_count: 0,
            image_count: 0,
            base_color_texture_source_path: "generated://editor/blank-world/default-base-color"
                .to_string(),
            base_color_texture_width: 1,
            base_color_texture_height: 1,
            base_color_texture_bytes: 4,
            normal_texture_source_path: "generated://editor/blank-world/default-normal-flat"
                .to_string(),
            normal_texture_width: 1,
            normal_texture_height: 1,
            normal_texture_bytes: 4,
            metallic_roughness_texture_source_path:
                "generated://editor/blank-world/default-metallic-roughness".to_string(),
            metallic_roughness_texture_width: 1,
            metallic_roughness_texture_height: 1,
            metallic_roughness_texture_bytes: 4,
            vertex_count: 0,
            index_count: 0,
            triangle_count: 0,
            degenerate_triangle_count: 0,
            mesh_primitives: 0,
            fitted_bounds_min: bounds_min,
            fitted_bounds_max: bounds_max,
            foliage: None,
            world_map,
        },
    })
}

fn solid_texture(
    label: &'static str,
    source_path: &'static str,
    rgba: [u8; 4],
) -> Result<EditorWorldTextureData, EngineError> {
    EditorWorldTextureData::new(label, source_path, 1, 1, rgba.to_vec())
}
