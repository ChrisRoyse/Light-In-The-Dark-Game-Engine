use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use crate::{asset_scene::EditorWorldMapArea, asset_vault::AssetVaultCatalog};

use super::{
    geometry::{CompositionBuild, CompositionStats},
    invalid_composition, manifest_file_record,
    progressive::try_manifest_file_record,
    terrain::terrain_height_at,
    texture_atlas::{AtlasRegion, MetallicRoughnessSource, TextureAtlasBuilder},
};

#[derive(Debug, Clone, Copy)]
struct SurfaceDecal {
    base_color_path: &'static str,
    normal_path: &'static str,
    metallic_roughness_path: &'static str,
    metallic_roughness_source: MetallicRoughnessSource,
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    uv_scale: [f32; 2],
    y_offset: f32,
    color_tint: [f32; 4],
    map_kind: &'static str,
}

pub(super) fn add_surface_decal_layers(
    catalog: &AssetVaultCatalog,
    build: &mut CompositionBuild,
    texture_atlas: &mut TextureAtlasBuilder,
    progressive: bool,
) -> Result<CompositionStats, EngineError> {
    let mut stats = CompositionStats::default();
    let mut skipped = 0usize;
    for decal in SURFACE_DECALS {
        let (base_color, normal, metallic_roughness) = if progressive {
            let Some(base_color) = try_manifest_file_record(catalog, decal.base_color_path) else {
                skipped += 1;
                continue;
            };
            let Some(normal) = try_manifest_file_record(catalog, decal.normal_path) else {
                skipped += 1;
                continue;
            };
            let Some(metallic_roughness) =
                try_manifest_file_record(catalog, decal.metallic_roughness_path)
            else {
                skipped += 1;
                continue;
            };
            (base_color, normal, metallic_roughness)
        } else {
            (
                manifest_file_record(catalog, decal.base_color_path)?,
                manifest_file_record(catalog, decal.normal_path)?,
                manifest_file_record(catalog, decal.metallic_roughness_path)?,
            )
        };
        let is_new_material = !stats.source_paths.contains(&base_color.relative_path);
        let atlas = texture_atlas.region_for_manifest_pbr(
            base_color,
            normal,
            metallic_roughness,
            decal.metallic_roughness_source,
        )?;
        push_decal(build, decal, atlas)?;
        if is_new_material {
            stats.material_count += 1;
        }
        for record in [base_color, normal, metallic_roughness] {
            if stats.source_paths.insert(record.relative_path.clone()) {
                stats.texture_count += 1;
                stats.image_count += 1;
            }
        }
    }
    if progressive && skipped > 0 {
        eprintln!(
            "ASSET_SCENE_COMPOSITION_RESIDUAL skipped_surface_decals={skipped} progressive=true"
        );
    }
    Ok(stats)
}

pub(super) fn surface_decal_map_areas() -> Vec<EditorWorldMapArea> {
    SURFACE_DECALS
        .into_iter()
        .map(|decal| EditorWorldMapArea {
            kind: decal.map_kind.to_string(),
            points: decal.points_xz(),
        })
        .collect()
}

const SURFACE_DECALS: [SurfaceDecal; 8] = [
    ruins_floor([-2.0, -31.0], 4.0, 24.0, 8.5, [3.4, 1.3]),
    ruins_floor([-16.0, -10.0], 32.0, 18.0, 7.0, [2.7, 1.1]),
    ruins_floor([18.0, -8.0], -24.0, 17.0, 6.8, [2.4, 1.0]),
    rocky_ground([5.0, 33.0], 8.0, 23.0, 10.0, [3.0, 1.4]),
    rocky_ground([-26.0, 27.0], -28.0, 16.0, 7.5, [2.2, 1.0]),
    aerial_rock([31.0, 22.0], 38.0, 18.0, 7.2, [2.4, 1.0]),
    mossy_stone([-38.0, -25.0], -38.0, 16.0, 6.8, [2.1, 0.9]),
    mossy_stone([39.0, -24.0], 34.0, 16.0, 6.8, [2.1, 0.9]),
];

const fn ruins_floor(
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    uv_scale: [f32; 2],
) -> SurfaceDecal {
    decal(DecalArgs {
        base_color_path: "sources/polyhaven/surfaces/rabdentse_ruins_wall/4k/textures/rabdentse_ruins_wall_diff_4k.jpg",
        normal_path: "sources/polyhaven/surfaces/rabdentse_ruins_wall/4k/textures/rabdentse_ruins_wall_nor_gl_4k.jpg",
        metallic_roughness_path: "sources/polyhaven/surfaces/rabdentse_ruins_wall/4k/textures/rabdentse_ruins_wall_arm_4k.jpg",
        metallic_roughness_source: MetallicRoughnessSource::Arm,
        center_xz,
        yaw_degrees,
        half_length,
        half_width,
        uv_scale,
        color_tint: [0.46, 0.52, 0.60, 0.0],
        map_kind: "polyhaven_ruins_surface",
    })
}

const fn rocky_ground(
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    uv_scale: [f32; 2],
) -> SurfaceDecal {
    decal(DecalArgs {
        base_color_path: "sources/polyhaven/surfaces/rocks_ground_04/4k/textures/rocks_ground_04_diff_4k.jpg",
        normal_path: "sources/polyhaven/surfaces/rocks_ground_04/4k/textures/rocks_ground_04_nor_gl_4k.jpg",
        metallic_roughness_path: "sources/polyhaven/surfaces/rocks_ground_04/4k/textures/rocks_ground_04_rough_4k.jpg",
        metallic_roughness_source: MetallicRoughnessSource::RoughnessOnly,
        center_xz,
        yaw_degrees,
        half_length,
        half_width,
        uv_scale,
        color_tint: [0.43, 0.49, 0.55, 0.0],
        map_kind: "polyhaven_rocky_surface",
    })
}

const fn aerial_rock(
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    uv_scale: [f32; 2],
) -> SurfaceDecal {
    decal(DecalArgs {
        base_color_path: "sources/polyhaven/surfaces/aerial_ground_rock/4k/textures/aerial_ground_rock_diff_4k.jpg",
        normal_path: "sources/polyhaven/surfaces/aerial_ground_rock/4k/textures/aerial_ground_rock_nor_gl_4k.jpg",
        metallic_roughness_path: "sources/polyhaven/surfaces/aerial_ground_rock/4k/textures/aerial_ground_rock_rough_4k.jpg",
        metallic_roughness_source: MetallicRoughnessSource::RoughnessOnly,
        center_xz,
        yaw_degrees,
        half_length,
        half_width,
        uv_scale,
        color_tint: [0.47, 0.53, 0.61, 0.0],
        map_kind: "polyhaven_rocky_surface",
    })
}

const fn mossy_stone(
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    uv_scale: [f32; 2],
) -> SurfaceDecal {
    decal(DecalArgs {
        base_color_path: "sources/polyhaven/surfaces/mossy_stone_wall/4k/textures/mossy_stone_wall_diff_4k.jpg",
        normal_path: "sources/polyhaven/surfaces/mossy_stone_wall/4k/textures/mossy_stone_wall_nor_gl_4k.jpg",
        metallic_roughness_path: "sources/polyhaven/surfaces/mossy_stone_wall/4k/textures/mossy_stone_wall_arm_4k.jpg",
        metallic_roughness_source: MetallicRoughnessSource::Arm,
        center_xz,
        yaw_degrees,
        half_length,
        half_width,
        uv_scale,
        color_tint: [0.34, 0.42, 0.40, 0.0],
        map_kind: "polyhaven_mossy_surface",
    })
}

struct DecalArgs {
    base_color_path: &'static str,
    normal_path: &'static str,
    metallic_roughness_path: &'static str,
    metallic_roughness_source: MetallicRoughnessSource,
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    uv_scale: [f32; 2],
    color_tint: [f32; 4],
    map_kind: &'static str,
}

const fn decal(args: DecalArgs) -> SurfaceDecal {
    let DecalArgs {
        base_color_path,
        normal_path,
        metallic_roughness_path,
        metallic_roughness_source,
        center_xz,
        yaw_degrees,
        half_length,
        half_width,
        uv_scale,
        color_tint,
        map_kind,
    } = args;
    SurfaceDecal {
        base_color_path,
        normal_path,
        metallic_roughness_path,
        metallic_roughness_source,
        center_xz,
        yaw_degrees,
        half_length,
        half_width,
        uv_scale,
        y_offset: 0.36,
        color_tint,
        map_kind,
    }
}

impl SurfaceDecal {
    fn points_xz(self) -> [[f32; 2]; 4] {
        let yaw = self.yaw_degrees.to_radians();
        let forward = Vec2::new(yaw.sin(), yaw.cos());
        let side = Vec2::new(forward.y, -forward.x);
        let center = Vec2::from_array(self.center_xz);
        let length = [
            self.half_length * 0.92,
            self.half_length * 1.06,
            self.half_length * 0.88,
            self.half_length * 1.11,
        ];
        let width = [
            self.half_width * 1.08,
            self.half_width * 0.84,
            self.half_width * 1.15,
            self.half_width * 0.91,
        ];
        [
            (center - forward * length[0] - side * width[0]).to_array(),
            (center + forward * length[1] - side * width[1]).to_array(),
            (center + forward * length[2] + side * width[2]).to_array(),
            (center - forward * length[3] + side * width[3]).to_array(),
        ]
    }

    fn points(self) -> [Vec3; 4] {
        self.points_xz().map(|point| {
            Vec3::new(
                point[0],
                terrain_height_at(point[0], point[1]) + self.y_offset,
                point[1],
            )
        })
    }
}

fn push_decal(
    build: &mut CompositionBuild,
    decal: SurfaceDecal,
    atlas: AtlasRegion,
) -> Result<(), EngineError> {
    let uv = [
        atlas.map_uv([0.0, 0.0]),
        atlas.map_uv([decal.uv_scale[0], 0.0]),
        atlas.map_uv(decal.uv_scale),
        atlas.map_uv([0.0, decal.uv_scale[1]]),
    ];
    let points = decal.points();
    push_triangle(
        build,
        [points[0], points[1], points[2]],
        [uv[0], uv[1], uv[2]],
        decal.color_tint,
    )?;
    push_triangle(
        build,
        [points[0], points[2], points[3]],
        [uv[0], uv[2], uv[3]],
        decal.color_tint,
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn push_triangle(
    build: &mut CompositionBuild,
    mut points: [Vec3; 3],
    mut uvs: [[f32; 2]; 3],
    color: [f32; 4],
) -> Result<(), EngineError> {
    let mut normal = (points[1] - points[0])
        .cross(points[2] - points[0])
        .normalize_or_zero();
    if normal.y < 0.0 {
        points.swap(1, 2);
        uvs.swap(1, 2);
        normal = -normal;
    }
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition(
            "surface_decal.normals",
            "surface decal triangle is degenerate",
        );
    }
    let base = checked_vertex_base(build.vertices.len())?;
    for index in 0..3 {
        build
            .vertices
            .push(vertex(points[index], normal, uvs[index], color)?);
    }
    build.indices.extend([base, base + 1, base + 2]);
    Ok(())
}

fn vertex(
    position: Vec3,
    normal: Vec3,
    uv: [f32; 2],
    color: [f32; 4],
) -> Result<EditorWorldVertex, EngineError> {
    if !position.is_finite()
        || !normal.is_finite()
        || !Vec2::from_array(uv).is_finite()
        || color.iter().any(|value| !value.is_finite())
    {
        return invalid_composition("surface_decal.vertex", "surface decal vertex is invalid");
    }
    Ok(EditorWorldVertex::new(
        position.to_array(),
        normal.to_array(),
        orthogonal_tangent(normal)?,
        uv,
        color,
        [0.0, 0.86, 0.72, 0.0],
    ))
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
            "surface_decal.tangent",
            "failed to derive tangent from surface decal normal",
        );
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.surface_decal.vertex_count".to_string(),
        reason: "surface decals exceed u32 index space".to_string(),
    })
}
