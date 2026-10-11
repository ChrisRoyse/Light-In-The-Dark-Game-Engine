use crate::EngineError;

use bytemuck::{Pod, Zeroable};

use super::{
    EditorWorldSceneMetrics,
    editor_world_foliage::EditorWorldFoliageBuffers,
    editor_world_layers::{EditorWorldSceneLayer, validate_scene_layers},
    world_error,
};

pub(super) const MAX_EDITOR_WORLD_SCENE_TRIANGLES: usize = 1_048_576;
pub(super) const MAX_EDITOR_WORLD_VERTEX_BUFFER_BYTES: usize = 256 * 1024 * 1024;
const MAX_EDITOR_WORLD_SCENE_INDICES: usize = MAX_EDITOR_WORLD_SCENE_TRIANGLES * 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorWorldSceneMetadata {
    pub scene_name: String,
    pub source_id: String,
    pub source_path: String,
    pub dependency_count: u32,
    pub material_count: u32,
    pub texture_count: u32,
    pub image_count: u32,
    pub base_color_texture_source_path: String,
    pub base_color_texture_width: u32,
    pub base_color_texture_height: u32,
    pub base_color_texture_bytes: u32,
    pub normal_texture_source_path: String,
    pub normal_texture_width: u32,
    pub normal_texture_height: u32,
    pub normal_texture_bytes: u32,
    pub metallic_roughness_texture_source_path: String,
    pub metallic_roughness_texture_width: u32,
    pub metallic_roughness_texture_height: u32,
    pub metallic_roughness_texture_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorWorldTextureData {
    pub label: String,
    pub source_path: String,
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

impl EditorWorldTextureData {
    pub fn new(
        label: impl Into<String>,
        source_path: impl Into<String>,
        width: u32,
        height: u32,
        rgba8: Vec<u8>,
    ) -> Result<Self, EngineError> {
        let label = label.into();
        let source_path = source_path.into();
        if label.trim().is_empty() || source_path.trim().is_empty() {
            return world_error(
                "editor_world_texture",
                "texture label and source_path must be non-empty",
            );
        }
        if width == 0 || height == 0 {
            return world_error(
                "editor_world_texture",
                "texture dimensions must be non-zero",
            );
        }
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| EngineError::EditorWorldRendererInvalidConfig {
                field: "editor_world_texture".to_string(),
                reason: "texture dimensions overflow rgba8 byte count".to_string(),
            })?;
        if rgba8.len() != expected_len {
            return world_error(
                "editor_world_texture",
                format!(
                    "rgba8 byte count {} does not match {width}x{height}x4={expected_len}",
                    rgba8.len()
                ),
            );
        }
        Ok(Self {
            label,
            source_path,
            width,
            height,
            rgba8,
        })
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct EditorWorldVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    pub color_emissive: [f32; 4],
    pub material_pbr: [f32; 4],
}

impl EditorWorldVertex {
    pub fn new(
        position: [f32; 3],
        normal: [f32; 3],
        tangent: [f32; 4],
        uv: [f32; 2],
        color_emissive: [f32; 4],
        material_pbr: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            tangent,
            uv,
            color_emissive,
            material_pbr,
        }
    }
}

pub struct EditorWorldSceneBuffers {
    metadata: EditorWorldSceneMetadata,
    pub(super) vertices: Vec<EditorWorldVertex>,
    pub(super) indices: Vec<u32>,
    pub(super) layers: Vec<EditorWorldSceneLayer>,
    pub(super) mesh_primitives: u32,
    pub(super) foliage: EditorWorldFoliageBuffers,
    pub(super) base_color_texture: EditorWorldTextureData,
    pub(super) normal_texture: EditorWorldTextureData,
    pub(super) metallic_roughness_texture: EditorWorldTextureData,
}

pub struct EditorWorldSceneBuffersFullArgs<T0: Into<String>, T1: Into<String>, T2: Into<String>> {
    pub scene_name: T0,
    pub source_id: T1,
    pub source_path: T2,
    pub dependency_count: u32,
    pub material_count: u32,
    pub texture_count: u32,
    pub image_count: u32,
    pub vertices: Vec<EditorWorldVertex>,
    pub indices: Vec<u32>,
    pub layers: Vec<EditorWorldSceneLayer>,
    pub foliage: EditorWorldFoliageBuffers,
    pub mesh_primitives: u32,
    pub base_color_texture: EditorWorldTextureData,
    pub normal_texture: EditorWorldTextureData,
    pub metallic_roughness_texture: EditorWorldTextureData,
}

pub struct EditorWorldSceneBuffersWithLayersArgs<
    T0: Into<String>,
    T1: Into<String>,
    T2: Into<String>,
> {
    pub scene_name: T0,
    pub source_id: T1,
    pub source_path: T2,
    pub dependency_count: u32,
    pub material_count: u32,
    pub texture_count: u32,
    pub image_count: u32,
    pub vertices: Vec<EditorWorldVertex>,
    pub indices: Vec<u32>,
    pub layers: Vec<EditorWorldSceneLayer>,
    pub mesh_primitives: u32,
    pub base_color_texture: EditorWorldTextureData,
    pub normal_texture: EditorWorldTextureData,
    pub metallic_roughness_texture: EditorWorldTextureData,
}

pub struct EditorWorldSceneBuffersNewArgs<T0: Into<String>, T1: Into<String>, T2: Into<String>> {
    pub scene_name: T0,
    pub source_id: T1,
    pub source_path: T2,
    pub dependency_count: u32,
    pub material_count: u32,
    pub texture_count: u32,
    pub image_count: u32,
    pub vertices: Vec<EditorWorldVertex>,
    pub indices: Vec<u32>,
    pub mesh_primitives: u32,
    pub base_color_texture: EditorWorldTextureData,
    pub normal_texture: EditorWorldTextureData,
    pub metallic_roughness_texture: EditorWorldTextureData,
}

impl EditorWorldSceneBuffers {
    pub fn new(
        args: EditorWorldSceneBuffersNewArgs<
            impl Into<String>,
            impl Into<String>,
            impl Into<String>,
        >,
    ) -> Result<Self, crate::EngineError> {
        let EditorWorldSceneBuffersNewArgs {
            scene_name,
            source_id,
            source_path,
            dependency_count,
            material_count,
            texture_count,
            image_count,
            vertices,
            indices,
            mesh_primitives,
            base_color_texture,
            normal_texture,
            metallic_roughness_texture,
        } = args;
        Self::new_with_layers(EditorWorldSceneBuffersWithLayersArgs {
            scene_name,
            source_id,
            source_path,
            dependency_count,
            material_count,
            texture_count,
            image_count,
            vertices,
            indices,
            layers: Vec::new(),
            mesh_primitives,
            base_color_texture,
            normal_texture,
            metallic_roughness_texture,
        })
    }

    pub fn new_with_layers(
        args: EditorWorldSceneBuffersWithLayersArgs<
            impl Into<String>,
            impl Into<String>,
            impl Into<String>,
        >,
    ) -> Result<Self, crate::EngineError> {
        let EditorWorldSceneBuffersWithLayersArgs {
            scene_name,
            source_id,
            source_path,
            dependency_count,
            material_count,
            texture_count,
            image_count,
            vertices,
            indices,
            layers,
            mesh_primitives,
            base_color_texture,
            normal_texture,
            metallic_roughness_texture,
        } = args;
        Self::new_with_layers_and_foliage(EditorWorldSceneBuffersFullArgs {
            scene_name,
            source_id,
            source_path,
            dependency_count,
            material_count,
            texture_count,
            image_count,
            vertices,
            indices,
            layers,
            foliage: EditorWorldFoliageBuffers::empty(),
            mesh_primitives,
            base_color_texture,
            normal_texture,
            metallic_roughness_texture,
        })
    }

    pub fn new_with_layers_and_foliage(
        args: EditorWorldSceneBuffersFullArgs<
            impl Into<String>,
            impl Into<String>,
            impl Into<String>,
        >,
    ) -> Result<Self, crate::EngineError> {
        let EditorWorldSceneBuffersFullArgs {
            scene_name,
            source_id,
            source_path,
            dependency_count,
            material_count,
            texture_count,
            image_count,
            vertices,
            indices,
            layers,
            foliage,
            mesh_primitives,
            base_color_texture,
            normal_texture,
            metallic_roughness_texture,
        } = args;
        let base_color_texture_bytes = texture_byte_count(&base_color_texture)?;
        let normal_texture_bytes = texture_byte_count(&normal_texture)?;
        let metallic_roughness_texture_bytes = texture_byte_count(&metallic_roughness_texture)?;
        validate_scene_budget(vertices.len(), indices.len())?;
        let combined_index_count = indices
            .len()
            .checked_add(foliage.effective_index_count() as usize)
            .ok_or_else(|| EngineError::EditorWorldRendererInvalidConfig {
                field: "reference_scene.budget".to_string(),
                reason: "scene plus foliage effective index count overflowed usize".to_string(),
            })?;
        validate_scene_budget(vertices.len(), combined_index_count)?;
        let total_index_count = u32::try_from(indices.len()).map_err(|_| {
            EngineError::EditorWorldRendererInvalidConfig {
                field: "reference_scene.layers".to_string(),
                reason: "scene index_count exceeds u32 layer range".to_string(),
            }
        })?;
        validate_scene_layers(&layers, total_index_count)?;
        let scene = Self {
            metadata: EditorWorldSceneMetadata {
                scene_name: scene_name.into(),
                source_id: source_id.into(),
                source_path: source_path.into(),
                dependency_count,
                material_count,
                texture_count,
                image_count,
                base_color_texture_source_path: base_color_texture.source_path.clone(),
                base_color_texture_width: base_color_texture.width,
                base_color_texture_height: base_color_texture.height,
                base_color_texture_bytes,
                normal_texture_source_path: normal_texture.source_path.clone(),
                normal_texture_width: normal_texture.width,
                normal_texture_height: normal_texture.height,
                normal_texture_bytes,
                metallic_roughness_texture_source_path: metallic_roughness_texture
                    .source_path
                    .clone(),
                metallic_roughness_texture_width: metallic_roughness_texture.width,
                metallic_roughness_texture_height: metallic_roughness_texture.height,
                metallic_roughness_texture_bytes,
            },
            vertices,
            indices,
            layers,
            mesh_primitives,
            foliage,
            base_color_texture,
            normal_texture,
            metallic_roughness_texture,
        };
        scene.metrics()?;
        if scene.metadata.scene_name.trim().is_empty() || scene.metadata.source_id.trim().is_empty()
        {
            return world_error("scene_source", "scene_name and source_id must be non-empty");
        }
        Ok(scene)
    }

    pub(super) fn metrics(&self) -> Result<EditorWorldSceneMetrics, EngineError> {
        if self.vertices.is_empty() || self.indices.is_empty() {
            if self.vertices.is_empty() && self.indices.is_empty() && self.mesh_primitives == 0 {
                return Ok(EditorWorldSceneMetrics {
                    vertex_count: 0,
                    index_count: 0,
                    triangle_count: 0,
                    mesh_primitives: 0,
                });
            }
            return world_error(
                "reference_scene",
                "empty scene requires zero vertices, zero indices, and zero mesh primitives",
            );
        }
        if !self.indices.len().is_multiple_of(3) || self.mesh_primitives == 0 {
            return world_error(
                "reference_scene",
                "non-empty scene buffers must contain indexed triangles and mesh primitives",
            );
        }
        validate_scene_budget(self.vertices.len(), self.indices.len())?;
        Ok(EditorWorldSceneMetrics {
            vertex_count: self.vertices.len() as u32,
            index_count: self.indices.len() as u32,
            triangle_count: (self.indices.len() / 3) as u32,
            mesh_primitives: self.mesh_primitives,
        })
    }

    pub fn vertices(&self) -> &[EditorWorldVertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// #1891 FSV: package play identifies maps via `package_map:<document_id>`.
    pub fn scene_name(&self) -> &str {
        self.metadata.scene_name.as_str()
    }

    pub(super) fn metadata(&self) -> EditorWorldSceneMetadata {
        self.metadata.clone()
    }

    pub(super) fn layers(&self) -> &[EditorWorldSceneLayer] {
        &self.layers
    }

    pub(super) fn foliage(&self) -> &EditorWorldFoliageBuffers {
        &self.foliage
    }

    pub(super) fn base_color_texture(&self) -> &EditorWorldTextureData {
        &self.base_color_texture
    }

    pub(super) fn normal_texture(&self) -> &EditorWorldTextureData {
        &self.normal_texture
    }

    pub(super) fn metallic_roughness_texture(&self) -> &EditorWorldTextureData {
        &self.metallic_roughness_texture
    }
}

pub(super) fn validate_scene_budget(
    vertex_count: usize,
    index_count: usize,
) -> Result<(), EngineError> {
    let triangle_count = index_count / 3;
    let vertex_buffer_bytes = vertex_count
        .checked_mul(std::mem::size_of::<EditorWorldVertex>())
        .ok_or_else(|| EngineError::EditorWorldRendererInvalidConfig {
            field: "reference_scene.budget".to_string(),
            reason: "vertex buffer byte count overflowed usize".to_string(),
        })?;
    if index_count > MAX_EDITOR_WORLD_SCENE_INDICES
        || triangle_count > MAX_EDITOR_WORLD_SCENE_TRIANGLES
        || vertex_buffer_bytes > MAX_EDITOR_WORLD_VERTEX_BUFFER_BYTES
    {
        return world_error(
            "reference_scene.budget",
            format!(
                "scene exceeds editor renderer budget: vertices={vertex_count}, indices={index_count}, triangles={triangle_count}, vertex_buffer_bytes={vertex_buffer_bytes}; caps vertices_bytes<={MAX_EDITOR_WORLD_VERTEX_BUFFER_BYTES}, indices<={MAX_EDITOR_WORLD_SCENE_INDICES}, triangles<={MAX_EDITOR_WORLD_SCENE_TRIANGLES}"
            ),
        );
    }
    Ok(())
}

fn texture_byte_count(texture: &EditorWorldTextureData) -> Result<u32, EngineError> {
    u32::try_from(texture.rgba8.len()).map_err(|_| EngineError::EditorWorldRendererInvalidConfig {
        field: "editor_world_texture".to_string(),
        reason: format!(
            "{} texture byte count exceeds u32 evidence range",
            texture.source_path
        ),
    })
}
