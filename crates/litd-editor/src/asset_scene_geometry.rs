use litd_engine::{
    EngineError,
    math::{Mat4, Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use crate::asset_scene_texture::{PbrTextureSetRef, resolve_pbr_texture_set_ref};

#[path = "asset_scene_geometry_args.rs"]
mod asset_scene_geometry_args;
use asset_scene_geometry_args::*;

#[derive(Default)]
pub(super) struct SceneBuild {
    pub(super) vertices: Vec<EditorWorldVertex>,
    pub(super) indices: Vec<u32>,
    pub(super) mesh_primitives: u32,
    pub(super) pbr_textures: Option<PbrTextureSetRef>,
}

impl SceneBuild {
    fn record_pbr_textures(&mut self, textures: PbrTextureSetRef) -> Result<(), EngineError> {
        if let Some(existing) = &self.pbr_textures {
            if existing != &textures {
                return invalid_asset_scene(
                    "gltf.pbr_textures",
                    format!(
                        "single-pass editor world renderer found multiple material texture sets: base={} normal={} mr={} versus base={} normal={} mr={}",
                        existing.base_color.source_relative_path,
                        existing.normal.source_relative_path,
                        existing.metallic_roughness.source_relative_path,
                        textures.base_color.source_relative_path,
                        textures.normal.source_relative_path,
                        textures.metallic_roughness.source_relative_path
                    ),
                );
            }
            return Ok(());
        }
        self.pbr_textures = Some(textures);
        Ok(())
    }
}

pub(super) fn append_node(
    node: gltf::Node<'_>,
    parent_transform: Mat4,
    buffers: &[gltf::buffer::Data],
    scene_source_relative_path: &str,
    build: &mut SceneBuild,
) -> Result<(), EngineError> {
    let transform = parent_transform * Mat4::from_cols_array_2d(&node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            append_primitive(
                primitive,
                transform,
                buffers,
                scene_source_relative_path,
                build,
            )?;
        }
    }
    for child in node.children() {
        append_node(child, transform, buffers, scene_source_relative_path, build)?;
    }
    Ok(())
}

fn append_primitive(
    primitive: gltf::Primitive<'_>,
    transform: Mat4,
    buffers: &[gltf::buffer::Data],
    scene_source_relative_path: &str,
    build: &mut SceneBuild,
) -> Result<(), EngineError> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return invalid_asset_scene(
            "gltf.primitive.mode",
            format!("unsupported primitive mode {:?}", primitive.mode()),
        );
    }
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    let positions = reader
        .read_positions()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.gltf.positions".to_string(),
            reason: "mesh primitive is missing POSITION data".to_string(),
        })?
        .collect::<Vec<_>>();
    if positions.is_empty() {
        return invalid_asset_scene("gltf.positions", "POSITION accessor is empty");
    }
    let indices = reader
        .read_indices()
        .map(|read| read.into_u32().collect::<Vec<_>>())
        .unwrap_or_else(|| (0..positions.len() as u32).collect());
    let material = primitive.material();
    let pbr_textures = resolve_pbr_texture_set_ref(&material, scene_source_relative_path)?;
    let uses_imported_textures = pbr_textures.uses_imported_textures();
    let uses_imported_normal_texture = pbr_textures.uses_imported_normal_texture();
    let uvs = match reader.read_tex_coords(0) {
        Some(read) => read.into_f32().collect::<Vec<_>>(),
        None if uses_imported_textures => {
            return invalid_asset_scene(
                "gltf.texcoord_0",
                "textured editor world mesh primitive is missing TEXCOORD_0",
            );
        }
        None => vec![[0.0, 0.0]; positions.len()],
    };
    if uvs.len() != positions.len() {
        return invalid_asset_scene(
            "gltf.texcoord_0",
            "TEXCOORD_0 count must match POSITION count",
        );
    }
    build.record_pbr_textures(pbr_textures)?;
    let color = material_color(&material)?;
    let pbr = material_pbr(&material)?;
    let tangents = reader.read_tangents().map(|read| read.collect::<Vec<_>>());
    if let Some(normals) = reader.read_normals().map(|read| read.collect::<Vec<_>>()) {
        append_with_normals(AppendWithNormalsArgs {
            positions,
            normals,
            tangents,
            uvs,
            indices,
            transform,
            color,
            pbr,
            requires_tangent_space: uses_imported_normal_texture,
            build,
        })?;
    } else {
        append_with_flat_normals(AppendWithFlatNormalsArgs {
            positions,
            uvs,
            indices,
            transform,
            color,
            pbr,
            requires_tangent_space: uses_imported_normal_texture,
            build,
        })?;
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn append_with_normals(args: AppendWithNormalsArgs<'_>) -> Result<(), EngineError> {
    let AppendWithNormalsArgs {
        positions,
        normals,
        tangents,
        uvs,
        indices,
        transform,
        color,
        pbr,
        requires_tangent_space,
        build,
    } = args;
    if normals.len() != positions.len() {
        return invalid_asset_scene("gltf.normals", "NORMAL count must match POSITION count");
    }
    if let Some(tangents) = &tangents
        && tangents.len() != positions.len()
    {
        return invalid_asset_scene("gltf.tangents", "TANGENT count must match POSITION count");
    }
    let base = checked_vertex_base(build.vertices.len())?;
    let index_start = build.indices.len();
    for (i, ((position, normal), uv)) in positions
        .iter()
        .zip(normals.iter())
        .zip(uvs.iter())
        .enumerate()
    {
        let normal = transform_normal(transform, *normal)?;
        let tangent = if requires_tangent_space {
            if let Some(tangents) = &tangents {
                transform_tangent(transform, tangents[i], normal)?
            } else {
                [0.0, 0.0, 0.0, 1.0]
            }
        } else {
            orthogonal_tangent(normal)?
        };
        build.vertices.push(vertex(
            transform_point(transform, *position)?,
            normal,
            tangent,
            *uv,
            color,
            pbr,
        )?);
    }
    for index in indices {
        if index as usize >= positions.len() {
            return invalid_asset_scene(
                "gltf.indices",
                format!("index {index} exceeds vertex count"),
            );
        }
        build.indices.push(base + index);
    }
    if requires_tangent_space && tangents.is_none() {
        compute_tangents(
            &mut build.vertices,
            &build.indices[index_start..],
            base as usize,
        )?;
    }
    Ok(())
}

fn append_with_flat_normals(args: AppendWithFlatNormalsArgs<'_>) -> Result<(), EngineError> {
    let AppendWithFlatNormalsArgs {
        positions,
        uvs,
        indices,
        transform,
        color,
        pbr,
        requires_tangent_space,
        build,
    } = args;
    if !indices.len().is_multiple_of(3) {
        return invalid_asset_scene(
            "gltf.indices",
            "triangle primitive index count is not divisible by 3",
        );
    }
    let base = checked_vertex_base(build.vertices.len())?;
    let index_start = build.indices.len();
    for triangle in indices.chunks(3) {
        let points = [
            transform_point(transform, position_at(&positions, triangle[0])?.to_array())?,
            transform_point(transform, position_at(&positions, triangle[1])?.to_array())?,
            transform_point(transform, position_at(&positions, triangle[2])?.to_array())?,
        ];
        let texcoords = [
            uv_at(&uvs, triangle[0])?,
            uv_at(&uvs, triangle[1])?,
            uv_at(&uvs, triangle[2])?,
        ];
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize_or_zero();
        if normal == Vec3::ZERO {
            return invalid_asset_scene(
                "gltf.normals",
                "degenerate triangle cannot produce a normal",
            );
        }
        let local_base = checked_vertex_base(build.vertices.len())?;
        let tangent = if requires_tangent_space {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            orthogonal_tangent(normal)?
        };
        for (point, uv) in points.into_iter().zip(texcoords) {
            build
                .vertices
                .push(vertex(point, normal, tangent, uv, color, pbr)?);
        }
        build
            .indices
            .extend([local_base, local_base + 1, local_base + 2]);
    }
    if requires_tangent_space {
        compute_tangents(
            &mut build.vertices,
            &build.indices[index_start..],
            base as usize,
        )
    } else {
        Ok(())
    }
}

fn compute_tangents(
    vertices: &mut [EditorWorldVertex],
    indices: &[u32],
    vertex_base: usize,
) -> Result<(), EngineError> {
    if !indices.len().is_multiple_of(3) {
        return invalid_asset_scene(
            "gltf.indices",
            "triangle index count must be divisible by 3",
        );
    }
    let vertex_count = vertices.len().saturating_sub(vertex_base);
    let mut tangents = vec![Vec3::ZERO; vertex_count];
    let mut bitangents = vec![Vec3::ZERO; vertex_count];
    for triangle in indices.chunks(3) {
        let local = [
            local_index(triangle[0], vertex_base, vertex_count)?,
            local_index(triangle[1], vertex_base, vertex_count)?,
            local_index(triangle[2], vertex_base, vertex_count)?,
        ];
        let p = [
            Vec3::from_array(vertices[vertex_base + local[0]].position),
            Vec3::from_array(vertices[vertex_base + local[1]].position),
            Vec3::from_array(vertices[vertex_base + local[2]].position),
        ];
        let uv = [
            Vec2::from_array(vertices[vertex_base + local[0]].uv),
            Vec2::from_array(vertices[vertex_base + local[1]].uv),
            Vec2::from_array(vertices[vertex_base + local[2]].uv),
        ];
        let edge1 = p[1] - p[0];
        let edge2 = p[2] - p[0];
        let duv1 = uv[1] - uv[0];
        let duv2 = uv[2] - uv[0];
        let denom = duv1.x * duv2.y - duv2.x * duv1.y;
        if !denom.is_finite() || denom.abs() <= 1.0e-8 {
            return invalid_asset_scene(
                "gltf.tangents",
                "normal-mapped primitive has degenerate UVs and cannot produce tangents",
            );
        }
        let inv = 1.0 / denom;
        let tangent = (edge1 * duv2.y - edge2 * duv1.y) * inv;
        let bitangent = (edge2 * duv1.x - edge1 * duv2.x) * inv;
        if !tangent.is_finite() || !bitangent.is_finite() {
            return invalid_asset_scene("gltf.tangents", "computed tangent basis is not finite");
        }
        for index in local {
            tangents[index] += tangent;
            bitangents[index] += bitangent;
        }
    }
    for local in 0..vertex_count {
        let vertex = &mut vertices[vertex_base + local];
        let normal = Vec3::from_array(vertex.normal).normalize_or_zero();
        let tangent = (tangents[local] - normal * normal.dot(tangents[local])).normalize_or_zero();
        if !tangent.is_finite() || tangent == Vec3::ZERO {
            return invalid_asset_scene("gltf.tangents", "computed tangent is invalid");
        }
        let handedness = if normal.cross(tangent).dot(bitangents[local]) < 0.0 {
            -1.0
        } else {
            1.0
        };
        vertex.tangent = tangent.extend(handedness).to_array();
    }
    Ok(())
}

fn local_index(index: u32, vertex_base: usize, vertex_count: usize) -> Result<usize, EngineError> {
    let index = index as usize;
    if index < vertex_base || index >= vertex_base + vertex_count {
        return invalid_asset_scene(
            "gltf.indices",
            format!("index {index} exceeds vertex range"),
        );
    }
    Ok(index - vertex_base)
}

fn transform_point(transform: Mat4, position: [f32; 3]) -> Result<Vec3, EngineError> {
    let point = transform.transform_point3(Vec3::from_array(position));
    if !point.is_finite() {
        return invalid_asset_scene("gltf.positions", "transformed POSITION is not finite");
    }
    Ok(point)
}

fn transform_normal(transform: Mat4, normal: [f32; 3]) -> Result<Vec3, EngineError> {
    let transformed = transform
        .transform_vector3(Vec3::from_array(normal))
        .normalize_or_zero();
    if !transformed.is_finite() || transformed == Vec3::ZERO {
        return invalid_asset_scene("gltf.normals", "transformed NORMAL is invalid");
    }
    Ok(transformed)
}

fn transform_tangent(
    transform: Mat4,
    tangent: [f32; 4],
    normal: Vec3,
) -> Result<[f32; 4], EngineError> {
    let handedness: f32 = if tangent[3] < 0.0 { -1.0 } else { 1.0 };
    let transformed = transform
        .transform_vector3(Vec3::new(tangent[0], tangent[1], tangent[2]))
        .normalize_or_zero();
    let tangent = (transformed - normal * normal.dot(transformed)).normalize_or_zero();
    if !tangent.is_finite() || tangent == Vec3::ZERO || !handedness.is_finite() {
        return invalid_asset_scene("gltf.tangents", "transformed TANGENT is invalid");
    }
    Ok(tangent.extend(handedness).to_array())
}

fn orthogonal_tangent(normal: Vec3) -> Result<[f32; 4], EngineError> {
    let reference = if normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = reference.cross(normal).normalize_or_zero();
    if !tangent.is_finite() || tangent == Vec3::ZERO {
        return invalid_asset_scene("gltf.tangents", "failed to derive tangent from normal");
    }
    Ok(tangent.extend(1.0).to_array())
}

fn position_at(positions: &[[f32; 3]], index: u32) -> Result<Vec3, EngineError> {
    positions
        .get(index as usize)
        .copied()
        .map(Vec3::from_array)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.gltf.indices".to_string(),
            reason: format!("index {index} exceeds POSITION count"),
        })
}

fn uv_at(uvs: &[[f32; 2]], index: u32) -> Result<[f32; 2], EngineError> {
    uvs.get(index as usize)
        .copied()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.gltf.indices".to_string(),
            reason: format!("index {index} exceeds TEXCOORD_0 count"),
        })
}

fn vertex(
    position: Vec3,
    normal: Vec3,
    tangent: [f32; 4],
    uv: [f32; 2],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<EditorWorldVertex, EngineError> {
    if !Vec2::from_array(uv).is_finite() {
        return invalid_asset_scene("gltf.texcoord_0", "TEXCOORD_0 contains non-finite values");
    }
    Ok(EditorWorldVertex::new(
        position.to_array(),
        normal.to_array(),
        tangent,
        uv,
        color,
        pbr,
    ))
}

fn material_color(material: &gltf::Material<'_>) -> Result<[f32; 4], EngineError> {
    let base = material.pbr_metallic_roughness().base_color_factor();
    let emissive = material
        .emissive_factor()
        .into_iter()
        .fold(0.0_f32, f32::max);
    let color = [base[0], base[1], base[2], emissive];
    if color.iter().any(|value| !value.is_finite()) {
        return invalid_asset_scene("gltf.material", "material color factor is not finite");
    }
    Ok(color)
}

fn material_pbr(material: &gltf::Material<'_>) -> Result<[f32; 4], EngineError> {
    let pbr = material.pbr_metallic_roughness();
    let normal_scale = material
        .normal_texture()
        .map(|normal| normal.scale())
        .unwrap_or(1.0);
    let values = [
        pbr.metallic_factor(),
        pbr.roughness_factor(),
        normal_scale,
        0.0,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return invalid_asset_scene("gltf.material", "material PBR factor is not finite");
    }
    Ok(values)
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.gltf.vertex_count".to_string(),
        reason: "scene exceeds u32 index space".to_string(),
    })
}

fn invalid_asset_scene<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.{}", field.into()),
        reason: reason.into(),
    })
}
