use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::{
        EditorWorldFoliageBuffers, EditorWorldFoliageInstance, EditorWorldVertex,
    },
};

use crate::{
    asset_scene::EditorWorldMapPoint,
    asset_scene_foliage_config::{
        EditorFoliageConfig, FoliagePatchDefinition, FoliagePrototypeDefinition,
    },
};

use super::{
    architecture::architecture_placements,
    citadel::citadel_placements,
    depth_clusters::depth_cluster_placements,
    dressing::dressing_placements,
    geometry::{AssetPlacement, CompositionBuild},
    gothic::gothic_placements,
    invalid_composition,
    kenney::kenney_skyline_placements,
    placements::asset_placements,
    polyhaven::polyhaven_placements,
    selection::selection_halo_map_points,
    terrain::{ISLAND_X_RADIUS, ISLAND_Z_RADIUS, PATH_SURFACES, RIFT_SURFACES, terrain_height_at},
    village::village_placements,
};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];
const GOLDEN_ANGLE: f32 = 2.399_963_1;
const MAX_PATCH_CLUMPS: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FoliageLayerBuildReport {
    pub placed_clump_count: u32,
    pub effective_index_count: u32,
}

pub(super) fn add_foliage_layer(
    build: &mut CompositionBuild,
    config: &EditorFoliageConfig,
) -> Result<FoliageLayerBuildReport, EngineError> {
    let start_primitives = build.mesh_primitives;
    let mut placed_clump_count = 0;
    for patch in &config.patches {
        placed_clump_count += add_patch(build, patch, prototype_for(config, patch)?)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition("foliage", "ED11 foliage layer added no primitives");
    }
    if placed_clump_count != config.target_clump_count() {
        return invalid_composition(
            "foliage.config",
            format!(
                "foliage config target {} clumps but placed {placed_clump_count}",
                config.target_clump_count()
            ),
        );
    }
    Ok(FoliageLayerBuildReport {
        placed_clump_count,
        effective_index_count: placed_clump_count * foliage_template_index_count(),
    })
}

pub(super) fn foliage_buffers(
    instances: Vec<EditorWorldFoliageInstance>,
) -> Result<EditorWorldFoliageBuffers, EngineError> {
    EditorWorldFoliageBuffers::new(
        foliage_template_vertices()?,
        foliage_template_indices(),
        instances,
    )
}

pub(super) fn foliage_map_points(config: &EditorFoliageConfig) -> Vec<EditorWorldMapPoint> {
    config
        .patches
        .iter()
        .map(|patch| EditorWorldMapPoint {
            kind: "ed11_foliage_patch".to_string(),
            world_xz: patch.center_xz,
            radius: patch.radius,
        })
        .collect()
}

fn add_patch(
    build: &mut CompositionBuild,
    patch: &FoliagePatchDefinition,
    prototype: &FoliagePrototypeDefinition,
) -> Result<u32, EngineError> {
    validate_patch(patch)?;
    let mut placed = 0;
    let candidates = MAX_PATCH_CLUMPS * 8;
    for index in 0..candidates {
        if placed == patch.target_clumps {
            break;
        }
        let xz = candidate_xz(patch, index);
        if foliage_suppressed_at(xz) {
            continue;
        }
        add_clump(build, patch, prototype, xz, placed)?;
        placed += 1;
    }
    if placed != patch.target_clumps {
        return invalid_composition(
            "foliage.placement",
            format!(
                "foliage patch at {:.1},{:.1} placed {placed}/{} clumps after suppression",
                patch.center_xz[0], patch.center_xz[1], patch.target_clumps
            ),
        );
    }
    build.mesh_primitives += 1;
    Ok(placed)
}

fn add_clump(
    build: &mut CompositionBuild,
    patch: &FoliagePatchDefinition,
    prototype: &FoliagePrototypeDefinition,
    xz: [f32; 2],
    ordinal: u32,
) -> Result<(), EngineError> {
    let base_y = terrain_height_at(xz[0], xz[1]) + 0.08;
    let scale = 0.74 + ((ordinal * 17 + 5) % 19) as f32 * 0.026;
    let height = patch.height * scale;
    let width = height * 0.30;
    let yaw = (patch.yaw_degrees.to_radians() + ordinal as f32 * 0.82) % std::f32::consts::TAU;
    build
        .foliage_instances
        .push(EditorWorldFoliageInstance::new(
            [xz[0], base_y, xz[1]],
            height,
            yaw,
            width,
            patch.color,
            [prototype.cull_start, prototype.cull_end],
            prototype.wind_strength,
        )?);
    Ok(())
}

fn prototype_for<'a>(
    config: &'a EditorFoliageConfig,
    patch: &FoliagePatchDefinition,
) -> Result<&'a FoliagePrototypeDefinition, EngineError> {
    config
        .prototypes
        .iter()
        .find(|prototype| prototype.prototype_id == patch.prototype_id)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.foliage.prototype_id".to_string(),
            reason: format!(
                "patch references unknown prototype '{}'",
                patch.prototype_id
            ),
        })
}

fn foliage_template_index_count() -> u32 {
    foliage_template_indices().len() as u32
}

fn foliage_template_indices() -> Vec<u32> {
    vec![0, 1, 2, 1, 3, 2, 4, 5, 6, 5, 7, 6]
}

fn foliage_template_vertices() -> Result<Vec<EditorWorldVertex>, EngineError> {
    let leaf = [1.0, 1.0, 1.0, 0.0];
    let stem = twig_color(leaf);
    Ok(vec![
        template_vertex([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], stem)?,
        template_vertex([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], stem)?,
        template_vertex([0.0, 0.48, -0.04], [0.0, 0.0, 1.0], stem)?,
        template_vertex([0.0, 1.0, 0.16], [0.0, 0.0, 1.0], leaf)?,
        template_vertex([0.0, 0.0, -0.82], [1.0, 0.0, 0.0], stem)?,
        template_vertex([0.0, 0.0, 0.82], [1.0, 0.0, 0.0], stem)?,
        template_vertex([-0.04, 0.44, 0.0], [1.0, 0.0, 0.0], stem)?,
        template_vertex([0.14, 0.92, 0.0], [1.0, 0.0, 0.0], leaf)?,
    ])
}

fn template_vertex(
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
) -> Result<EditorWorldVertex, EngineError> {
    if position.iter().any(|value| !value.is_finite())
        || normal.iter().any(|value| !value.is_finite())
        || color.iter().any(|value| !value.is_finite())
    {
        return invalid_composition(
            "foliage.template_vertex",
            "foliage template contains invalid data",
        );
    }
    let normal_vec = Vec3::from_array(normal);
    Ok(EditorWorldVertex::new(
        position,
        normal,
        orthogonal_tangent(normal_vec)?,
        WHITE_ATLAS_UV,
        color,
        [0.0, 0.92, 0.35, -1.0],
    ))
}

fn validate_patch(patch: &FoliagePatchDefinition) -> Result<(), EngineError> {
    if patch.prototype_id.trim().is_empty()
        || patch.center_xz.iter().any(|value| !value.is_finite())
        || !patch.radius.is_finite()
        || !patch.height.is_finite()
        || patch.color.iter().any(|value| !value.is_finite())
        || patch.radius <= 0.0
        || patch.height <= 0.0
        || patch.target_clumps == 0
        || patch.target_clumps > MAX_PATCH_CLUMPS
    {
        return invalid_composition(
            "foliage.patch",
            "foliage patches require finite center/radius/height/color and bounded clumps",
        );
    }
    if !compatible_ground_at(patch.center_xz) {
        return invalid_composition(
            "foliage.patch",
            format!(
                "foliage patch center {:.1},{:.1} is outside compatible terrain",
                patch.center_xz[0], patch.center_xz[1]
            ),
        );
    }
    Ok(())
}

fn candidate_xz(patch: &FoliagePatchDefinition, index: u32) -> [f32; 2] {
    let radial_seed = ((index * 37 + 11) % 101) as f32 / 100.0;
    let radius = patch.radius * (0.18 + 0.82 * radial_seed.sqrt());
    let angle = patch.yaw_degrees.to_radians() + index as f32 * GOLDEN_ANGLE;
    [
        patch.center_xz[0] + angle.cos() * radius,
        patch.center_xz[1] + angle.sin() * radius,
    ]
}

fn foliage_suppressed_at(xz: [f32; 2]) -> bool {
    !compatible_ground_at(xz)
        || PATH_SURFACES
            .iter()
            .any(|surface| point_in_surface(*surface, xz, 2.4))
        || RIFT_SURFACES
            .iter()
            .any(|rift| point_in_surface(rift.surface, xz, 2.8))
        || selection_halo_map_points()
            .iter()
            .any(|point| point_in_circle(xz, point.world_xz, point.radius + 1.8))
        || blocked_by_authored_placements(xz)
}

fn compatible_ground_at(xz: [f32; 2]) -> bool {
    if xz.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let radius = ((xz[0] / ISLAND_X_RADIUS).powi(2) + (xz[1] / ISLAND_Z_RADIUS).powi(2)).sqrt();
    radius <= 0.88
}

fn blocked_by_authored_placements(xz: [f32; 2]) -> bool {
    asset_placements()
        .iter()
        .any(|placement| asset_blocks_foliage(*placement, xz))
        || architecture_placements()
            .iter()
            .any(|placement| point_in_circle(xz, placement.translation_xz(), placement.map_radius))
        || citadel_placements()
            .iter()
            .any(|placement| point_in_circle(xz, placement.translation_xz(), placement.map_radius))
        || village_placements()
            .iter()
            .any(|placement| point_in_circle(xz, placement.translation_xz(), placement.map_radius))
        || kenney_skyline_placements()
            .iter()
            .any(|placement| point_in_circle(xz, placement.translation_xz(), placement.map_radius))
        || gothic_placements()
            .iter()
            .any(|placement| point_in_circle(xz, placement.translation_xz(), placement.map_radius))
        || polyhaven_placements()
            .iter()
            .any(|placement| polyhaven_blocks_foliage(*placement, xz))
        || depth_cluster_placements()
            .iter()
            .any(|placement| point_in_circle(xz, placement.translation_xz(), placement.map_radius))
        || dressing_placements()
            .iter()
            .any(|placement| point_in_circle(xz, placement.translation_xz(), placement.map_radius))
}

fn asset_blocks_foliage(placement: AssetPlacement, xz: [f32; 2]) -> bool {
    let radius = if placement.file_name.contains("Grass")
        || placement.file_name.contains("Fern")
        || placement.file_name.contains("Plant")
        || placement.file_name.contains("Flower")
        || placement.file_name.contains("Bush")
        || placement.file_name.contains("Mushroom")
    {
        placement.scale * 1.2
    } else {
        placement.scale * 2.8
    };
    point_in_circle(
        xz,
        [placement.translation[0], placement.translation[2]],
        radius,
    )
}

fn polyhaven_blocks_foliage(placement: super::polyhaven::PolyhavenPlacement, xz: [f32; 2]) -> bool {
    let radius = placement.map_radius.min(placement.scale * 4.0 + 4.0);
    point_in_circle(xz, placement.translation_xz(), radius)
}

fn point_in_surface(
    surface: super::terrain::TerrainSurfaceQuad,
    xz: [f32; 2],
    margin: f32,
) -> bool {
    let yaw = surface.yaw_degrees.to_radians();
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    let side = Vec2::new(forward.y, -forward.x);
    let delta = Vec2::new(xz[0] - surface.center_xz[0], xz[1] - surface.center_xz[1]);
    delta.dot(forward).abs() <= surface.half_length + margin
        && delta.dot(side).abs() <= surface.half_width + margin
}

fn point_in_circle(xz: [f32; 2], center: [f32; 2], radius: f32) -> bool {
    let delta = Vec2::new(xz[0] - center[0], xz[1] - center[1]);
    delta.length_squared() <= radius * radius
}

fn twig_color(color: [f32; 4]) -> [f32; 4] {
    [color[0] * 0.72, color[1] * 0.76, color[2] * 0.70, color[3]]
}

fn orthogonal_tangent(normal: Vec3) -> Result<[f32; 4], EngineError> {
    let reference = if normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = reference.cross(normal).normalize_or_zero();
    if !tangent.is_finite() || tangent == Vec3::ZERO {
        return invalid_composition(
            "foliage.tangent",
            "failed to derive tangent from foliage normal",
        );
    }
    Ok(tangent.extend(1.0).to_array())
}

trait PlacementFootprint {
    fn translation_xz(&self) -> [f32; 2];
}

macro_rules! placement_footprint {
    ($type:ty) => {
        impl PlacementFootprint for $type {
            fn translation_xz(&self) -> [f32; 2] {
                [self.translation[0], self.translation[2]]
            }
        }
    };
}

placement_footprint!(super::architecture::ArchitecturePlacement);
placement_footprint!(super::citadel::CitadelPlacement);
placement_footprint!(super::depth_clusters::DepthClusterPlacement);
placement_footprint!(super::dressing::DressingPlacement);
placement_footprint!(super::gothic::GothicPlacement);
placement_footprint!(super::kenney::KenneyPlacement);
placement_footprint!(super::polyhaven::PolyhavenPlacement);
placement_footprint!(super::village::VillagePlacement);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foliage_layer_places_bounded_suppressed_clumps() {
        let mut build = CompositionBuild::default();
        let config = crate::asset_scene_foliage_config::load_default_foliage_config().unwrap();
        let report = add_foliage_layer(&mut build, &config).unwrap();

        let configured_clumps = config.target_clump_count() as usize;
        assert_eq!(report.placed_clump_count, config.target_clump_count());
        assert_eq!(build.mesh_primitives, config.patches.len() as u32);
        assert_eq!(build.foliage_instances.len(), configured_clumps);
        assert_eq!(build.indices.len(), 0);
        assert_eq!(build.vertices.len(), 0);
        assert_eq!(
            report.effective_index_count,
            config.target_clump_count() * 12
        );
        let buffers = foliage_buffers(build.foliage_instances).unwrap();
        assert_eq!(buffers.template_index_count(), 12);
        assert_eq!(buffers.instance_count(), config.target_clump_count());
        assert_eq!(foliage_map_points(&config).len(), config.patches.len());
        assert!(
            foliage_map_points(&config)
                .iter()
                .all(|point| point.kind == "ed11_foliage_patch")
        );
    }
}
