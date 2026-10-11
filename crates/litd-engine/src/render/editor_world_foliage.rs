use bytemuck::{Pod, Zeroable};

use crate::EngineError;

use super::{
    editor_world_scene::{EditorWorldVertex, validate_scene_budget},
    world_error,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct EditorWorldFoliageInstance {
    pub root_height: [f32; 4],
    pub yaw_width: [f32; 4],
    pub color_emissive: [f32; 4],
    pub cull_wind: [f32; 4],
}

impl EditorWorldFoliageInstance {
    pub fn new(
        root: [f32; 3],
        height: f32,
        yaw_radians: f32,
        width: f32,
        color_emissive: [f32; 4],
        cull: [f32; 2],
        wind_strength: f32,
    ) -> Result<Self, EngineError> {
        if root.iter().any(|value| !value.is_finite())
            || !height.is_finite()
            || !yaw_radians.is_finite()
            || !width.is_finite()
            || color_emissive.iter().any(|value| !value.is_finite())
            || cull.iter().any(|value| !value.is_finite())
            || !wind_strength.is_finite()
            || height <= 0.0
            || width <= 0.0
            || cull[0] < 0.0
            || cull[1] <= cull[0]
        {
            return world_error(
                "reference_scene.foliage_instance",
                "foliage instances require finite positive transform, ordered cull range, and finite color",
            );
        }
        Ok(Self {
            root_height: [root[0], root[1], root[2], height],
            yaw_width: [yaw_radians.cos(), yaw_radians.sin(), width, 0.0],
            color_emissive,
            cull_wind: [cull[0], cull[1], wind_strength, 0.0],
        })
    }
}

pub struct EditorWorldFoliageBuffers {
    vertices: Vec<EditorWorldVertex>,
    indices: Vec<u32>,
    instances: Vec<EditorWorldFoliageInstance>,
}

impl EditorWorldFoliageBuffers {
    pub fn empty() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            instances: Vec::new(),
        }
    }

    pub fn new(
        vertices: Vec<EditorWorldVertex>,
        indices: Vec<u32>,
        instances: Vec<EditorWorldFoliageInstance>,
    ) -> Result<Self, EngineError> {
        let buffers = Self {
            vertices,
            indices,
            instances,
        };
        buffers.validate()?;
        Ok(buffers)
    }

    pub fn instance_count(&self) -> u32 {
        self.instances.len() as u32
    }

    pub fn template_index_count(&self) -> u32 {
        self.indices.len() as u32
    }

    pub fn effective_index_count(&self) -> u32 {
        let effective_index_count = self
            .indices
            .len()
            .checked_mul(self.instances.len())
            .expect("validated foliage effective index count must not overflow usize");
        u32::try_from(effective_index_count)
            .expect("validated foliage effective index count must fit u32 evidence range")
    }

    pub fn effective_triangle_count(&self) -> u32 {
        self.effective_index_count() / 3
    }

    pub fn has_instances(&self) -> bool {
        !self.instances.is_empty()
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.instances.is_empty() {
            if self.vertices.is_empty() && self.indices.is_empty() {
                return Ok(());
            }
            return world_error(
                "reference_scene.foliage_buffers",
                "empty foliage instance set requires zero template vertices and indices",
            );
        }
        if self.vertices.is_empty()
            || self.indices.is_empty()
            || !self.indices.len().is_multiple_of(3)
        {
            return world_error(
                "reference_scene.foliage_buffers",
                "instanced foliage requires non-empty triangle-aligned template geometry",
            );
        }
        let vertex_count = self.vertices.len() as u32;
        for index in &self.indices {
            if *index >= vertex_count {
                return world_error(
                    "reference_scene.foliage_buffers",
                    format!(
                        "foliage template index {index} exceeds template vertex_count {vertex_count}"
                    ),
                );
            }
        }
        validate_foliage_effective_budget(
            self.vertices.len(),
            self.indices.len(),
            self.instances.len(),
        )?;
        Ok(())
    }

    pub(super) fn vertices(&self) -> &[EditorWorldVertex] {
        &self.vertices
    }

    pub(super) fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub(super) fn instances(&self) -> &[EditorWorldFoliageInstance] {
        &self.instances
    }
}

pub(super) fn validate_foliage_effective_budget(
    template_vertex_count: usize,
    template_index_count: usize,
    instance_count: usize,
) -> Result<usize, EngineError> {
    let effective_index_count =
        template_index_count
            .checked_mul(instance_count)
            .ok_or_else(|| EngineError::EditorWorldRendererInvalidConfig {
                field: "reference_scene.foliage_budget".to_string(),
                reason: format!(
                    "foliage effective index count overflowed usize: template_indices={template_index_count}, instances={instance_count}"
                ),
            })?;
    validate_scene_budget(template_vertex_count, effective_index_count)?;
    Ok(effective_index_count)
}
