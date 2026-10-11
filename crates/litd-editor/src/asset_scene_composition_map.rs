use litd_engine::EngineError;

use crate::asset_scene::{
    EditorWorldMapArea, EditorWorldMapPoint, EditorWorldMapReport, EditorWorldMapRing,
};
use crate::asset_scene_foliage_config::EditorFoliageConfig;

use super::{
    architecture::architecture_placements,
    citadel::citadel_placements,
    depth_clusters::depth_cluster_placements,
    dressing::dressing_placements,
    foliage::foliage_map_points,
    gothic::gothic_placements,
    kenney::kenney_skyline_placements,
    light_layers::light_layer_map_points,
    placements::asset_placements,
    polyhaven::polyhaven_placements,
    selection::selection_halo_map_points,
    surface_layers::surface_decal_map_areas,
    terrain::{PATH_SURFACES, RIFT_SURFACES, SELECTION_RINGS, island_outline_xz},
    vfx::{rift_beacon_map_points, rift_fissure_map_areas},
    village::village_placements,
};

pub(super) fn composition_map_report(
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    foliage_config: &EditorFoliageConfig,
) -> Result<EditorWorldMapReport, EngineError> {
    let mut areas = Vec::with_capacity(PATH_SURFACES.len() + RIFT_SURFACES.len() + 16);
    areas.extend(PATH_SURFACES.into_iter().map(|surface| EditorWorldMapArea {
        kind: "path".to_string(),
        points: surface.points_xz(),
    }));
    areas.extend(
        RIFT_SURFACES
            .into_iter()
            .enumerate()
            .map(|(index, rift)| EditorWorldMapArea {
                kind: if index == 0 {
                    "rift_blue".to_string()
                } else {
                    "rift_lava".to_string()
                },
                points: rift.surface.points_xz(),
            }),
    );
    areas.extend(surface_decal_map_areas());
    areas.extend(rift_fissure_map_areas()?);

    let mut points = asset_placements()
        .iter()
        .map(|placement| EditorWorldMapPoint {
            kind: placement_map_kind(placement.file_name).to_string(),
            world_xz: [placement.translation[0], placement.translation[2]],
            radius: placement.scale,
        })
        .collect::<Vec<_>>();
    points.extend(
        architecture_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(
        citadel_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(
        village_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(
        kenney_skyline_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(
        gothic_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(
        polyhaven_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(
        depth_cluster_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(
        dressing_placements()
            .iter()
            .map(|placement| EditorWorldMapPoint {
                kind: placement.map_kind.to_string(),
                world_xz: [placement.translation[0], placement.translation[2]],
                radius: placement.map_radius,
            }),
    );
    points.extend(light_layer_map_points());
    points.extend(foliage_map_points(foliage_config));
    points.extend(selection_halo_map_points());
    points.extend(rift_beacon_map_points());

    let rings = SELECTION_RINGS
        .into_iter()
        .map(|ring| EditorWorldMapRing {
            kind: "selection".to_string(),
            center_xz: ring.center_xz,
            radius: ring.outer_radius,
        })
        .collect::<Vec<_>>();

    EditorWorldMapReport::new(
        "composition.stylized_nature_ed11_v1",
        [bounds_min[0], bounds_min[2]],
        [bounds_max[0], bounds_max[2]],
        island_outline_xz(64)?,
        areas,
        points,
        rings,
    )
}

fn placement_map_kind(file_name: &str) -> &'static str {
    let lower = file_name.to_ascii_lowercase();
    if lower.contains("tree") || lower.contains("pine") {
        "tree"
    } else if lower.contains("rock") {
        "rock"
    } else if lower.contains("mushroom") {
        "mushroom"
    } else if lower.contains("flower") || lower.contains("plant") || lower.contains("grass") {
        "plant"
    } else {
        "doodad"
    }
}
