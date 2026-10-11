use std::path::Path;

use litd_engine::{
    EngineError,
    math::{Mat4, Vec3},
    render::editor_world::{
        EditorWorldSceneBuffers, EditorWorldSceneBuffersNewArgs, EditorWorldVertex,
    },
};

use crate::{
    asset_products::{AssetProduct, AssetProductKind},
    asset_scene_composition::{
        load_composed_editor_world_asset_scene,
        load_composed_editor_world_asset_scene_with_foliage_brush_path,
    },
    asset_scene_geometry::{SceneBuild, append_node},
    asset_scene_texture::imported_pbr_textures,
    gltf_validation::validate_gltf_source,
};

#[path = "asset_scene_blank.rs"]
mod asset_scene_blank;

use asset_scene_blank::load_blank_editor_world_asset_scene_impl;

pub const DEFAULT_EDITOR_WORLD_GLTF: &str =
    "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";
const WORLD_TARGET_EXTENT: f32 = 96.0;

pub struct LoadedEditorAssetScene {
    pub scene: EditorWorldSceneBuffers,
    pub report: EditorAssetSceneReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorAssetSceneReport {
    pub product_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub dependency_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub image_count: usize,
    pub base_color_texture_source_path: String,
    pub base_color_texture_width: u32,
    pub base_color_texture_height: u32,
    pub base_color_texture_bytes: usize,
    pub normal_texture_source_path: String,
    pub normal_texture_width: u32,
    pub normal_texture_height: u32,
    pub normal_texture_bytes: usize,
    pub metallic_roughness_texture_source_path: String,
    pub metallic_roughness_texture_width: u32,
    pub metallic_roughness_texture_height: u32,
    pub metallic_roughness_texture_bytes: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub degenerate_triangle_count: usize,
    pub mesh_primitives: u32,
    pub fitted_bounds_min: [f32; 3],
    pub fitted_bounds_max: [f32; 3],
    pub foliage: Option<EditorFoliageReport>,
    pub world_map: EditorWorldMapReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorFoliageReport {
    pub config_id: String,
    pub tileset_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub brush_state_present: bool,
    pub brush_state_source_path: String,
    pub brush_state_sha256: String,
    pub brush_stamp_count: usize,
    pub brush_add_stamp_count: usize,
    pub brush_erase_stamp_count: usize,
    pub brush_added_patch_count: usize,
    pub brush_erased_patch_count: usize,
    pub schema_version: u32,
    pub source_issue: u32,
    pub prototype_count: usize,
    pub patch_count: usize,
    pub target_clump_count: u32,
    pub placed_clump_count: u32,
    pub foliage_index_count: u32,
    pub density_scale: f32,
    pub gpu_instancing_required: bool,
    pub render_mode_summary: String,
    pub cull_range_summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorWorldMapReport {
    pub source_kind: String,
    pub bounds_min: [f32; 2],
    pub bounds_max: [f32; 2],
    pub outline: Vec<[f32; 2]>,
    pub areas: Vec<EditorWorldMapArea>,
    pub points: Vec<EditorWorldMapPoint>,
    pub rings: Vec<EditorWorldMapRing>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorWorldMapArea {
    pub kind: String,
    pub points: [[f32; 2]; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorWorldMapPoint {
    pub kind: String,
    pub world_xz: [f32; 2],
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorWorldMapRing {
    pub kind: String,
    pub center_xz: [f32; 2],
    pub radius: f32,
}

impl EditorWorldMapReport {
    pub fn new(
        source_kind: impl Into<String>,
        bounds_min: [f32; 2],
        bounds_max: [f32; 2],
        outline: Vec<[f32; 2]>,
        areas: Vec<EditorWorldMapArea>,
        points: Vec<EditorWorldMapPoint>,
        rings: Vec<EditorWorldMapRing>,
    ) -> Result<Self, EngineError> {
        let source_kind = source_kind.into();
        validate_xz("map.bounds_min", bounds_min)?;
        validate_xz("map.bounds_max", bounds_max)?;
        if bounds_max[0] <= bounds_min[0] || bounds_max[1] <= bounds_min[1] {
            return invalid_asset_scene("map.bounds", "world-map bounds must be ordered");
        }
        if source_kind.trim().is_empty() {
            return invalid_asset_scene("map.source_kind", "world-map source kind is required");
        }
        if outline.len() < 4 {
            return invalid_asset_scene("map.outline", "world-map outline needs at least 4 points");
        }
        for point in &outline {
            validate_xz("map.outline.point", *point)?;
        }
        for area in &areas {
            if area.kind.trim().is_empty() {
                return invalid_asset_scene("map.area.kind", "map area kind is required");
            }
            for point in area.points {
                validate_xz("map.area.point", point)?;
            }
        }
        for point in &points {
            if point.kind.trim().is_empty() {
                return invalid_asset_scene("map.point.kind", "map point kind is required");
            }
            validate_xz("map.point.world_xz", point.world_xz)?;
            if !point.radius.is_finite() || point.radius <= 0.0 {
                return invalid_asset_scene(
                    "map.point.radius",
                    "map point radius must be positive",
                );
            }
        }
        for ring in &rings {
            if ring.kind.trim().is_empty() {
                return invalid_asset_scene("map.ring.kind", "map ring kind is required");
            }
            validate_xz("map.ring.center_xz", ring.center_xz)?;
            if !ring.radius.is_finite() || ring.radius <= 0.0 {
                return invalid_asset_scene("map.ring.radius", "map ring radius must be positive");
            }
        }
        Ok(Self {
            source_kind,
            bounds_min,
            bounds_max,
            outline,
            areas,
            points,
            rings,
        })
    }

    pub fn from_fitted_bounds(
        source_kind: impl Into<String>,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
    ) -> Result<Self, EngineError> {
        let min = [bounds_min[0], bounds_min[2]];
        let max = [bounds_max[0], bounds_max[2]];
        Self::new(
            source_kind,
            min,
            max,
            vec![
                [min[0], min[1]],
                [max[0], min[1]],
                [max[0], max[1]],
                [min[0], max[1]],
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    pub fn feature_count(&self) -> usize {
        self.outline.len() + self.areas.len() + self.points.len() + self.rings.len()
    }
}

fn validate_xz(field: &str, point: [f32; 2]) -> Result<(), EngineError> {
    if point.iter().any(|value| !value.is_finite()) {
        return invalid_asset_scene(field, "world-map coordinate contains non-finite data");
    }
    Ok(())
}

pub fn load_default_editor_world_asset_scene() -> Result<LoadedEditorAssetScene, EngineError> {
    load_composed_editor_world_asset_scene()
}

pub fn load_default_editor_world_asset_scene_with_foliage_brush(
    foliage_brush_path: Option<&Path>,
) -> Result<LoadedEditorAssetScene, EngineError> {
    load_composed_editor_world_asset_scene_with_foliage_brush_path(foliage_brush_path)
}

pub fn load_blank_editor_world_asset_scene() -> Result<LoadedEditorAssetScene, EngineError> {
    load_blank_editor_world_asset_scene_impl()
}

pub fn load_editor_world_asset_scene(
    product: &AssetProduct,
) -> Result<LoadedEditorAssetScene, EngineError> {
    product.validate()?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_asset_scene("product.kind", "editor world scene requires a mesh product");
    }
    let extension = Path::new(&product.source_relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("gltf" | "glb")) {
        return invalid_asset_scene(
            "source.extension",
            "editor world scene loader currently accepts only glTF/GLB products",
        );
    }
    let source_path = Path::new(&product.source_absolute_path);
    let validation = validate_gltf_source(source_path, &product.source_relative_path)?;
    let (document, buffers, images) =
        gltf::import(source_path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.gltf.import".to_string(),
            reason: format!(
                "{} failed glTF scene import: {error}",
                source_path.display()
            ),
        })?;

    let mut build = SceneBuild::default();
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.gltf.scene".to_string(),
            reason: "glTF product has no scene to render".to_string(),
        })?;
    for node in scene.nodes() {
        append_node(
            node,
            Mat4::IDENTITY,
            &buffers,
            product.source_relative_path.as_str(),
            &mut build,
        )?;
    }
    let pbr_texture_refs =
        build
            .pbr_textures
            .clone()
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.asset_scene.gltf.pbr_textures".to_string(),
                reason: "scene has no primitive with a complete PBR texture set".to_string(),
            })?;
    let pbr_textures = imported_pbr_textures(&pbr_texture_refs, &images)?;
    let base_color_texture_width = pbr_textures.base_color.width;
    let base_color_texture_height = pbr_textures.base_color.height;
    let base_color_texture_bytes = pbr_textures.base_color.rgba8.len();
    let normal_texture_width = pbr_textures.normal.width;
    let normal_texture_height = pbr_textures.normal.height;
    let normal_texture_bytes = pbr_textures.normal.rgba8.len();
    let metallic_roughness_texture_width = pbr_textures.metallic_roughness.width;
    let metallic_roughness_texture_height = pbr_textures.metallic_roughness.height;
    let metallic_roughness_texture_bytes = pbr_textures.metallic_roughness.rgba8.len();
    let (bounds_min, bounds_max) = fit_to_editor_world(&mut build.vertices)?;
    let vertex_count = build.vertices.len();
    let index_count = build.indices.len();
    let mesh_primitives = build.mesh_primitives;
    if validation.index_count != index_count || validation.primitive_count as u32 != mesh_primitives
    {
        return invalid_asset_scene(
            "gltf.buffer_counts",
            "validated glTF counts do not match extracted render buffers",
        );
    }
    let material_count = document.materials().count();
    let texture_count = document.textures().count();
    let image_count = images.len();
    let scene = EditorWorldSceneBuffers::new(EditorWorldSceneBuffersNewArgs {
        scene_name: format!("gltf_asset.{}", clean_scene_segment(&product.title)),
        source_id: product.product_id.clone(),
        source_path: product.source_relative_path.clone(),
        dependency_count: product.dependencies.len() as u32,
        material_count: material_count as u32,
        texture_count: texture_count as u32,
        image_count: image_count as u32,
        vertices: build.vertices,
        indices: build.indices,
        mesh_primitives: build.mesh_primitives,
        base_color_texture: pbr_textures.base_color,
        normal_texture: pbr_textures.normal,
        metallic_roughness_texture: pbr_textures.metallic_roughness,
    })?;
    let world_map = EditorWorldMapReport::from_fitted_bounds(
        "gltf_fitted_bounds",
        bounds_min.to_array(),
        bounds_max.to_array(),
    )?;
    Ok(LoadedEditorAssetScene {
        report: EditorAssetSceneReport {
            product_id: product.product_id.clone(),
            source_relative_path: product.source_relative_path.clone(),
            source_sha256: product.source_sha256.clone(),
            dependency_count: product.dependencies.len(),
            material_count,
            texture_count,
            image_count,
            base_color_texture_source_path: pbr_texture_refs.base_color.source_relative_path,
            base_color_texture_width,
            base_color_texture_height,
            base_color_texture_bytes,
            normal_texture_source_path: pbr_texture_refs.normal.source_relative_path,
            normal_texture_width,
            normal_texture_height,
            normal_texture_bytes,
            metallic_roughness_texture_source_path: pbr_texture_refs
                .metallic_roughness
                .source_relative_path,
            metallic_roughness_texture_width,
            metallic_roughness_texture_height,
            metallic_roughness_texture_bytes,
            vertex_count,
            index_count,
            triangle_count: index_count / 3,
            degenerate_triangle_count: validation.degenerate_triangle_count,
            mesh_primitives,
            fitted_bounds_min: bounds_min.to_array(),
            fitted_bounds_max: bounds_max.to_array(),
            foliage: None,
            world_map,
        },
        scene,
    })
}

fn fit_to_editor_world(vertices: &mut [EditorWorldVertex]) -> Result<(Vec3, Vec3), EngineError> {
    let (min, max) = bounds(vertices)?;
    let size = max - min;
    let longest = size.x.max(size.y).max(size.z);
    if !longest.is_finite() || longest <= 0.0 {
        return invalid_asset_scene(
            "gltf.bounds",
            "mesh bounds must have non-zero finite extent",
        );
    }
    let scale = WORLD_TARGET_EXTENT / longest;
    let origin = Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
    for vertex in &mut *vertices {
        let fitted = (Vec3::from_array(vertex.position) - origin) * scale;
        vertex.position = fitted.to_array();
    }
    bounds(vertices)
}

fn bounds(vertices: &[EditorWorldVertex]) -> Result<(Vec3, Vec3), EngineError> {
    let Some(first) = vertices.first() else {
        return invalid_asset_scene("gltf.geometry", "scene has no renderable vertices");
    };
    let mut min = Vec3::from_array(first.position);
    let mut max = min;
    for vertex in vertices {
        let point = Vec3::from_array(vertex.position);
        if !point.is_finite() {
            return invalid_asset_scene("gltf.positions", "POSITION contains non-finite values");
        }
        min = min.min(point);
        max = max.max(point);
    }
    Ok((min, max))
}

fn clean_scene_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
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

#[cfg(test)]
#[path = "asset_scene_default_composition_tests.rs"]
mod default_composition_tests;
#[cfg(test)]
#[path = "asset_scene_tests.rs"]
mod tests;
