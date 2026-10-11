use std::collections::BTreeSet;

use litd_engine::{
    EngineError,
    math::{Mat4, Quat, Vec3},
    render::editor_world::{
        EditorWorldFoliageInstance, EditorWorldSceneLayer, EditorWorldSceneLayerKind,
        EditorWorldVertex,
    },
};

use crate::{asset_vault::AssetVaultRecord, gltf_validation::validate_gltf_source};

use super::{
    geometry_support::{
        apply_color_tint, checked_vertex_base, material_color, material_pbr, multiply_color,
        position_at, require_same_len, transform_normal, transform_point, triangle_normal,
        validate_color_tint, vertex,
    },
    invalid_composition,
    texture_atlas::TextureAtlasBuilder,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct AssetPlacement {
    pub(super) file_name: &'static str,
    pub(super) translation: [f32; 3],
    pub(super) scale: f32,
    pub(super) yaw_degrees: f32,
}

#[derive(Default)]
pub(super) struct CompositionBuild {
    pub(super) vertices: Vec<EditorWorldVertex>,
    pub(super) indices: Vec<u32>,
    pub(super) layers: Vec<EditorWorldSceneLayer>,
    pub(super) foliage_instances: Vec<EditorWorldFoliageInstance>,
    pub(super) mesh_primitives: u32,
}

impl CompositionBuild {
    pub(super) fn index_cursor(&self) -> usize {
        self.indices.len()
    }

    pub(super) fn mark_layer(
        &mut self,
        kind: EditorWorldSceneLayerKind,
        start_index: usize,
    ) -> Result<(), EngineError> {
        if start_index > self.indices.len() {
            return invalid_composition(
                "layers",
                format!(
                    "{kind:?} layer start {start_index} exceeds current index count {}",
                    self.indices.len()
                ),
            );
        }
        let index_count = self.indices.len() - start_index;
        if index_count == 0 {
            return invalid_composition("layers", format!("{kind:?} layer added no indices"));
        }
        let start = u32::try_from(start_index).map_err(|_| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.layers.start".to_string(),
            reason: format!("{kind:?} layer start exceeds u32 range"),
        })?;
        let count = u32::try_from(index_count).map_err(|_| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.layers.count".to_string(),
            reason: format!("{kind:?} layer index_count exceeds u32 range"),
        })?;
        self.layers
            .push(EditorWorldSceneLayer::new(kind, start, count));
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct CompositionStats {
    pub(super) source_paths: BTreeSet<String>,
    pub(super) material_count: usize,
    pub(super) texture_count: usize,
    pub(super) image_count: usize,
    pub(super) degenerate_triangle_count: usize,
}

pub(super) fn append_asset_tinted(
    record: &AssetVaultRecord,
    placement: AssetPlacement,
    color_tint: [f32; 4],
    build: &mut CompositionBuild,
    texture_atlas: &mut TextureAtlasBuilder,
) -> Result<CompositionStats, EngineError> {
    validate_color_tint(color_tint)?;
    let validation = validate_gltf_source(&record.absolute_path, &record.relative_path)?;
    let (document, buffers, images) = gltf::import(&record.absolute_path).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.gltf.import".to_string(),
            reason: format!(
                "{} failed glTF import: {error}",
                record.absolute_path.display()
            ),
        }
    })?;
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::splat(placement.scale),
        Quat::from_rotation_y(placement.yaw_degrees.to_radians()),
        Vec3::from_array(placement.translation),
    );
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.gltf.scene".to_string(),
            reason: format!("{} has no scene", record.relative_path),
        })?;
    let mut degenerate_triangle_count = 0;
    for node in scene.nodes() {
        degenerate_triangle_count += append_node(AppendNodeArgs {
            node,
            parent_transform: transform,
            buffers: &buffers,
            images: &images,
            build,
            texture_atlas,
            source_relative_path: &record.relative_path,
            color_tint,
        })?;
    }

    Ok(CompositionStats {
        source_paths: validation.dependencies.into_iter().collect(),
        material_count: document.materials().count(),
        texture_count: document.textures().count(),
        image_count: images.len(),
        degenerate_triangle_count,
    })
}

pub(super) fn bounds(vertices: &[EditorWorldVertex]) -> Result<(Vec3, Vec3), EngineError> {
    let Some(first) = vertices.first() else {
        return invalid_composition("geometry", "composition has no renderable vertices");
    };
    let mut min = Vec3::from_array(first.position);
    let mut max = min;
    for vertex in vertices {
        let point = Vec3::from_array(vertex.position);
        if !point.is_finite() {
            return invalid_composition("bounds", "composition contains non-finite bounds");
        }
        min = min.min(point);
        max = max.max(point);
    }
    Ok((min, max))
}

struct AppendNodeArgs<'a> {
    node: gltf::Node<'a>,
    parent_transform: Mat4,
    buffers: &'a [gltf::buffer::Data],
    images: &'a [gltf::image::Data],
    build: &'a mut CompositionBuild,
    texture_atlas: &'a mut TextureAtlasBuilder,
    source_relative_path: &'a str,
    color_tint: [f32; 4],
}

fn append_node(args: AppendNodeArgs<'_>) -> Result<usize, EngineError> {
    let AppendNodeArgs {
        node,
        parent_transform,
        buffers,
        images,
        build,
        texture_atlas,
        source_relative_path,
        color_tint,
    } = args;
    let transform = parent_transform * Mat4::from_cols_array_2d(&node.transform().matrix());
    let mut degenerate_triangle_count = 0;
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            degenerate_triangle_count += append_primitive(AppendPrimitiveArgs {
                primitive,
                transform,
                buffers,
                images,
                build,
                texture_atlas,
                source_relative_path,
                color_tint,
            })?;
        }
    }
    for child in node.children() {
        degenerate_triangle_count += append_node(AppendNodeArgs {
            node: child,
            parent_transform: transform,
            buffers,
            images,
            build,
            texture_atlas,
            source_relative_path,
            color_tint,
        })?;
    }
    Ok(degenerate_triangle_count)
}

struct AppendPrimitiveArgs<'a> {
    primitive: gltf::Primitive<'a>,
    transform: Mat4,
    buffers: &'a [gltf::buffer::Data],
    images: &'a [gltf::image::Data],
    build: &'a mut CompositionBuild,
    texture_atlas: &'a mut TextureAtlasBuilder,
    source_relative_path: &'a str,
    color_tint: [f32; 4],
}

fn append_primitive(args: AppendPrimitiveArgs<'_>) -> Result<usize, EngineError> {
    let AppendPrimitiveArgs {
        primitive,
        transform,
        buffers,
        images,
        build,
        texture_atlas,
        source_relative_path,
        color_tint,
    } = args;
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return invalid_composition(
            "gltf.primitive.mode",
            format!(
                "{source_relative_path} uses unsupported mode {:?}",
                primitive.mode()
            ),
        );
    }
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    let positions = reader
        .read_positions()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.gltf.positions".to_string(),
            reason: format!("{source_relative_path} primitive is missing POSITION"),
        })?
        .collect::<Vec<_>>();
    if positions.is_empty() {
        return invalid_composition("gltf.positions", "POSITION accessor is empty");
    }
    let indices = match reader.read_indices() {
        Some(read) => read.into_u32().collect::<Vec<_>>(),
        None => {
            let count = checked_vertex_base(positions.len())?;
            (0..count).collect::<Vec<_>>()
        }
    };
    if indices.len() % 3 != 0 {
        return invalid_composition(
            "gltf.indices",
            format!("{source_relative_path} triangle index count is not divisible by 3"),
        );
    }
    let normals = reader.read_normals().map(|read| read.collect::<Vec<_>>());
    if let Some(normals) = &normals {
        require_same_len(
            normals.len(),
            positions.len(),
            "NORMAL",
            source_relative_path,
        )?;
    }
    let uvs = reader
        .read_tex_coords(0)
        .map(|read| read.into_f32().collect::<Vec<_>>())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
    require_same_len(
        uvs.len(),
        positions.len(),
        "TEXCOORD_0",
        source_relative_path,
    )?;
    let colors = reader
        .read_colors(0)
        .map(|read| read.into_rgba_f32().collect::<Vec<_>>());
    if let Some(colors) = &colors {
        require_same_len(
            colors.len(),
            positions.len(),
            "COLOR_0",
            source_relative_path,
        )?;
    }
    let material = primitive.material();
    let material_color = material_color(&material)?;
    let material_pbr = material_pbr(&material)?;
    let atlas_region =
        texture_atlas.region_for_material(&material, source_relative_path, images)?;

    let mut valid_triangle_count = 0;
    for (triangle_index, triangle) in indices.chunks(3).enumerate() {
        let world_points = [
            transform_point(transform, position_at(&positions, triangle[0])?)?,
            transform_point(transform, position_at(&positions, triangle[1])?)?,
            transform_point(transform, position_at(&positions, triangle[2])?)?,
        ];
        let Some(flat_normal) = triangle_normal(world_points, source_relative_path)? else {
            return invalid_composition(
                "gltf.triangles",
                format!(
                    "{source_relative_path} produced a degenerate triangle after placement transform at primitive triangle {triangle_index}; clean or replace the source asset before import"
                ),
            );
        };
        let base = checked_vertex_base(build.vertices.len())?;
        for (corner, index) in triangle.iter().copied().enumerate() {
            let normal = match &normals {
                Some(normals) => transform_normal(transform, normals[index as usize])?,
                None => flat_normal,
            };
            let vertex_color = colors
                .as_ref()
                .map(|colors| colors[index as usize])
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            build.vertices.push(vertex(
                world_points[corner],
                normal,
                atlas_region.map_uv(uvs[index as usize]),
                apply_color_tint(multiply_color(material_color, vertex_color), color_tint),
                material_pbr,
            )?);
        }
        build.indices.extend([base, base + 1, base + 2]);
        valid_triangle_count += 1;
    }
    if valid_triangle_count == 0 {
        return invalid_composition(
            "gltf.triangles",
            format!("{source_relative_path} primitive produced no renderable triangles"),
        );
    }
    build.mesh_primitives += 1;
    Ok(0)
}
