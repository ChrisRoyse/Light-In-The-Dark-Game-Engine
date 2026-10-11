use std::path::Path;

use litd_engine::{
    EngineError,
    render::editor_world::{
        EditorWorldSceneBuffers, EditorWorldSceneBuffersFullArgs, EditorWorldSceneLayerKind,
    },
};
use tracing::warn;

use crate::{
    asset_scene::{EditorAssetSceneReport, LoadedEditorAssetScene},
    asset_scene_foliage_brush::{apply_foliage_brush_state, load_foliage_brush_state},
    asset_scene_foliage_config::load_default_foliage_config,
    asset_vault::AssetVaultCatalog,
};

#[path = "asset_scene_composition_architecture.rs"]
mod architecture;
#[path = "asset_scene_composition_build_helpers.rs"]
mod build_helpers;
#[path = "asset_scene_composition_citadel.rs"]
mod citadel;
#[path = "asset_scene_composition_citadel_detail.rs"]
mod citadel_detail;
#[path = "asset_scene_composition_depth_clusters.rs"]
mod depth_clusters;
#[path = "asset_scene_composition_doodads.rs"]
mod doodads;
#[path = "asset_scene_composition_dressing.rs"]
mod dressing;
#[path = "asset_scene_composition_foliage.rs"]
mod foliage;
#[path = "asset_scene_composition_geometry.rs"]
mod geometry;
#[path = "asset_scene_composition_geometry_support.rs"]
mod geometry_support;
#[path = "asset_scene_composition_gothic.rs"]
mod gothic;
#[path = "asset_scene_composition_kenney.rs"]
mod kenney;
#[path = "asset_scene_composition_light_layers.rs"]
mod light_layers;
#[path = "asset_scene_composition_map.rs"]
mod map;
#[path = "asset_scene_composition_obj.rs"]
mod obj;
#[path = "asset_scene_composition_placements.rs"]
mod placements;
#[path = "asset_scene_composition_polyhaven.rs"]
mod polyhaven;
#[path = "asset_scene_composition_progressive.rs"]
mod progressive;
#[path = "asset_scene_composition_selection.rs"]
mod selection;
#[path = "asset_scene_composition_silhouette.rs"]
mod silhouette;
#[path = "asset_scene_composition_sources.rs"]
mod sources;
#[path = "asset_scene_composition_surface_layers.rs"]
mod surface_layers;
#[path = "asset_scene_composition_terrain.rs"]
mod terrain;
#[path = "asset_scene_composition_atlas.rs"]
mod texture_atlas;
#[path = "asset_scene_composition_vfx.rs"]
mod vfx;
#[path = "asset_scene_composition_view_overlays.rs"]
mod view_overlays;
#[path = "asset_scene_composition_village.rs"]
mod village;

use build_helpers::mark_build_layer;
use citadel_detail::add_citadel_gothic_detail;
use doodads::append_composition_doodad_packs;
use foliage::{add_foliage_layer, foliage_buffers};
use geometry::{CompositionBuild, CompositionStats, bounds};
use light_layers::{add_fog_effect_layers, add_light_pool_layers};
use map::composition_map_report;
use progressive::resolve_composition_packs;
use selection::add_selection_halo_layer;
use silhouette::add_floating_cliff_silhouette;
// Parent imports: child modules access via `use super::invalid_composition` etc.
#[allow(unused_imports)]
use sources::{extracted_record, invalid_composition, manifest_file_record, source_archive_record};
use surface_layers::add_surface_decal_layers;
use terrain::{add_island_terrain, add_path, add_selection_ring};
use texture_atlas::TextureAtlasBuilder;
use vfx::{add_rift_beacon_layers, add_rift_fissure_layers};
use view_overlays::{
    add_camera_bounds_layer, add_camera_objects_layer, add_ground_pathing_overlay,
    add_info_display_layer, add_naval_pathing_overlay, add_regions_layer, add_units_layer,
    add_water_layer,
};

const SCENE_NAME: &str = "vault_composition.stylized_nature_ed11_v1";
const SCENE_ID: &str = "litd.asset.composition.stylized_nature_ed11_v1";
const STYLIZED_ROOT: &str =
    "extracted/quaternius/manual-game-kit/stylizednaturemegakit/Stylized_Nature_MegaKit_Standard";
const PROCEDURAL_MATERIAL_COUNT: usize = 16;
const PROCEDURAL_TEXTURE_COUNT: usize = 3;

pub(super) fn load_composed_editor_world_asset_scene() -> Result<LoadedEditorAssetScene, EngineError>
{
    load_composed_editor_world_asset_scene_with_foliage_brush_path(None)
}

pub(super) fn load_composed_editor_world_asset_scene_with_foliage_brush_path(
    foliage_brush_path: Option<&Path>,
) -> Result<LoadedEditorAssetScene, EngineError> {
    let catalog = AssetVaultCatalog::load_default_cached()?;
    let mut foliage_config = load_default_foliage_config()?;
    if let Some(path) = foliage_brush_path {
        let brush_state = load_foliage_brush_state(path)?;
        foliage_config = apply_foliage_brush_state(foliage_config, &brush_state)?;
    }
    // #1878: kenney core required; manual Quaternius megakits optional with residual.
    let packs = resolve_composition_packs(&catalog)?;
    let mut build = CompositionBuild::default();
    let mut stats = CompositionStats::default();
    let mut texture_atlas = TextureAtlasBuilder::new();

    if let Some(archive) = packs.stylized {
        stats.source_paths.insert(archive.relative_path.clone());
    }
    stats
        .source_paths
        .insert(packs.castle.relative_path.clone());
    stats
        .source_paths
        .insert(packs.modular_dungeon.relative_path.clone());
    if let Some(village) = packs.village {
        stats.source_paths.insert(village.relative_path.clone());
    }
    stats
        .source_paths
        .insert(packs.tower_defense.relative_path.clone());

    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Terrain,
        add_island_terrain,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Doodads,
        add_floating_cliff_silhouette,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Doodads,
        add_citadel_gothic_detail,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Lighting,
        add_light_pool_layers,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::FogEffects,
        add_fog_effect_layers,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Lighting,
        add_rift_beacon_layers,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Blight,
        add_rift_fissure_layers,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::PathingGround,
        add_path,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Water,
        add_water_layer,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Units,
        add_units_layer,
    )?;
    let foliage_layer = add_foliage_layer(&mut build, &foliage_config)?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Regions,
        add_selection_ring,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::InfoDisplay,
        add_selection_halo_layer,
    )?;
    let surface_layer_start = build.index_cursor();
    let surface_stats =
        add_surface_decal_layers(&catalog, &mut build, &mut texture_atlas, packs.progressive)?;
    build.mark_layer(EditorWorldSceneLayerKind::Terrain, surface_layer_start)?;
    stats.source_paths.extend(surface_stats.source_paths);
    stats.material_count += surface_stats.material_count;
    stats.texture_count += surface_stats.texture_count;
    stats.image_count += surface_stats.image_count;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::PathingGround,
        add_ground_pathing_overlay,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::PathingNaval,
        add_naval_pathing_overlay,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::CameraBounds,
        add_camera_bounds_layer,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::Regions,
        add_regions_layer,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::CameraObjects,
        add_camera_objects_layer,
    )?;
    mark_build_layer(
        &mut build,
        EditorWorldSceneLayerKind::InfoDisplay,
        add_info_display_layer,
    )?;

    let doodads_start = build.index_cursor();
    append_composition_doodad_packs(&catalog, &packs, &mut build, &mut stats, &mut texture_atlas)?;
    build.mark_layer(EditorWorldSceneLayerKind::Doodads, doodads_start)?;

    if stats.degenerate_triangle_count > 0 {
        warn!(
            scene_id = SCENE_ID,
            culled_degenerate_triangle_count = stats.degenerate_triangle_count,
            "culled degenerate triangles from editor composition scene"
        );
    }

    let (bounds_min, bounds_max) = bounds(&build.vertices)?;
    let vertex_count = build.vertices.len();
    let index_count = build.indices.len();
    let mesh_primitives = build.mesh_primitives;
    let atlas_textures = texture_atlas.into_texture_data()?;
    let base_color = atlas_textures.base_color;
    let normal = atlas_textures.normal;
    let metallic_roughness = atlas_textures.metallic_roughness;
    let base_color_texture_source_path = base_color.source_path.clone();
    let base_color_texture_width = base_color.width;
    let base_color_texture_height = base_color.height;
    let base_color_texture_bytes = base_color.rgba8.len();
    let normal_texture_source_path = normal.source_path.clone();
    let normal_texture_width = normal.width;
    let normal_texture_height = normal.height;
    let normal_texture_bytes = normal.rgba8.len();
    let metallic_roughness_texture_source_path = metallic_roughness.source_path.clone();
    let metallic_roughness_texture_width = metallic_roughness.width;
    let metallic_roughness_texture_height = metallic_roughness.height;
    let metallic_roughness_texture_bytes = metallic_roughness.rgba8.len();
    let world_map = composition_map_report(
        bounds_min.to_array(),
        bounds_max.to_array(),
        &foliage_config,
    )?;
    let foliage_report = foliage_config.to_report(
        foliage_layer.placed_clump_count,
        foliage_layer.effective_index_count,
    );
    let foliage_buffers = foliage_buffers(std::mem::take(&mut build.foliage_instances))?;

    let (source_path, source_sha256, scene_name, scene_id) = if packs.progressive {
        (
            packs.castle.relative_path.as_str(),
            packs.castle.sha256.clone(),
            "vault_composition.progressive_kenney_core_v1",
            "litd.asset.composition.progressive_kenney_core_v1",
        )
    } else {
        (
            STYLIZED_ROOT,
            packs
                .stylized
                .expect("full composition requires stylized archive")
                .sha256
                .clone(),
            SCENE_NAME,
            SCENE_ID,
        )
    };

    let scene =
        EditorWorldSceneBuffers::new_with_layers_and_foliage(EditorWorldSceneBuffersFullArgs {
            scene_name,
            source_id: scene_id,
            source_path,
            dependency_count: stats.source_paths.len() as u32,
            material_count: (stats.material_count + PROCEDURAL_MATERIAL_COUNT) as u32,
            texture_count: (stats.texture_count + PROCEDURAL_TEXTURE_COUNT) as u32,
            image_count: stats.image_count as u32,
            vertices: build.vertices,
            indices: build.indices,
            layers: build.layers,
            foliage: foliage_buffers,
            mesh_primitives,
            base_color_texture: base_color,
            normal_texture: normal,
            metallic_roughness_texture: metallic_roughness,
        })?;

    Ok(LoadedEditorAssetScene {
        report: EditorAssetSceneReport {
            product_id: scene_id.to_string(),
            source_relative_path: source_path.to_string(),
            source_sha256,
            dependency_count: stats.source_paths.len(),
            material_count: stats.material_count + PROCEDURAL_MATERIAL_COUNT,
            texture_count: stats.texture_count + PROCEDURAL_TEXTURE_COUNT,
            image_count: stats.image_count,
            base_color_texture_source_path,
            base_color_texture_width,
            base_color_texture_height,
            base_color_texture_bytes,
            normal_texture_source_path,
            normal_texture_width,
            normal_texture_height,
            normal_texture_bytes,
            metallic_roughness_texture_source_path,
            metallic_roughness_texture_width,
            metallic_roughness_texture_height,
            metallic_roughness_texture_bytes,
            vertex_count,
            index_count,
            triangle_count: index_count / 3,
            degenerate_triangle_count: stats.degenerate_triangle_count,
            mesh_primitives,
            fitted_bounds_min: bounds_min.to_array(),
            fitted_bounds_max: bounds_max.to_array(),
            foliage: Some(foliage_report),
            world_map,
        },
        scene,
    })
}
