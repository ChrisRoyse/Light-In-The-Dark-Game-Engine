use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::Aabb,
    render::{
        batching::{
            BatchPlan, BatchingSettings, Frustum, HzbOccluder, RenderInstance, ScreenRect,
            build_batch_plan,
        },
        vfx::{VfxEffectAsset, VfxOutputModule, invalid_vfx, validate_effect},
    },
};

const SPRITE_INDEX_COUNT: u32 = 6;
const TRAIL_INDEX_COUNT: u32 = 12;
const MESH_INDEX_COUNT: u32 = 36;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VfxRenderWorkloadSettings {
    pub target_width: u32,
    pub target_height: u32,
    pub max_effect_instances: usize,
    pub max_simulated_particles: u64,
    pub max_particle_buffer_bytes: u64,
    pub particle_attribute_stride_bytes: u32,
    pub far_lod_start_meters: f32,
    pub far_lod_particle_scale: f32,
    pub batching: BatchingSettings,
}

impl Default for VfxRenderWorkloadSettings {
    fn default() -> Self {
        Self {
            target_width: 3840,
            target_height: 2160,
            max_effect_instances: 1024,
            max_simulated_particles: 2_000_000,
            max_particle_buffer_bytes: 256 * 1024 * 1024,
            particle_attribute_stride_bytes: 64,
            far_lod_start_meters: 80.0,
            far_lod_particle_scale: 0.25,
            batching: BatchingSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxSceneEffectInstance {
    pub instance_id: u64,
    pub effect: VfxEffectAsset,
    pub world_bounds: Aabb,
    pub screen_rect: ScreenRect,
    pub nearest_depth: f32,
    pub distance_meters: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VfxRenderLod {
    Near,
    Far,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxRenderWorkloadStats {
    pub target_width: u32,
    pub target_height: u32,
    pub input_effect_instances: usize,
    pub input_particle_capacity: u64,
    pub visible_effect_instances: usize,
    pub visible_render_instances: usize,
    pub frustum_culled_effect_ids: Vec<u64>,
    pub occlusion_culled_effect_ids: Vec<u64>,
    pub far_lod_effect_ids: Vec<u64>,
    pub simulated_particles: u64,
    pub lod_saved_particles: u64,
    pub particle_buffer_bytes: u64,
    pub draw_batches: usize,
    pub indirect_commands: usize,
    pub batch_plan: BatchPlan,
}

pub fn build_vfx_render_workload(
    settings: VfxRenderWorkloadSettings,
    frustum: &Frustum,
    occluders: &[HzbOccluder],
    effects: &[VfxSceneEffectInstance],
) -> Result<VfxRenderWorkloadStats, EngineError> {
    settings.validate()?;
    if effects.len() > settings.max_effect_instances {
        return invalid_vfx(
            "render_workload.effect_instances",
            format!(
                "has {} instances but limit is {}",
                effects.len(),
                settings.max_effect_instances
            ),
        );
    }

    let mut render_instances = Vec::new();
    let mut sources = BTreeMap::new();
    let mut effect_particles = BTreeMap::new();
    let mut far_lod_effects = BTreeSet::new();
    let mut input_particle_capacity = 0_u64;

    for (index, scene_effect) in effects.iter().enumerate() {
        scene_effect.validate(index)?;
        let validation = validate_effect(&scene_effect.effect)?;
        let capacity = u64::from(validation.total_max_particles);
        input_particle_capacity =
            input_particle_capacity
                .checked_add(capacity)
                .ok_or_else(|| {
                    crate::render::vfx::invalid_vfx_error(
                        "render_workload.input_particle_capacity",
                        "particle capacity overflowed u64",
                    )
                })?;
        effect_particles.insert(scene_effect.instance_id, capacity);
        let lod = scene_effect.lod(settings);
        if lod == VfxRenderLod::Far {
            far_lod_effects.insert(scene_effect.instance_id);
        }

        let mut draw_output_index = 0_usize;
        for emitter in &scene_effect.effect.emitters {
            for output in &emitter.output {
                let Some((mesh_id, material_id, index_count)) = draw_contract(output) else {
                    continue;
                };
                let render_id = render_instance_id(scene_effect.instance_id, draw_output_index)?;
                draw_output_index += 1;
                render_instances.push(RenderInstance {
                    id: render_id,
                    mesh_id,
                    material_id,
                    world_bounds: scene_effect.world_bounds,
                    screen_rect: scene_effect.screen_rect,
                    nearest_depth: scene_effect.nearest_depth,
                    index_count,
                });
                sources.insert(
                    render_id,
                    RenderSource {
                        effect_id: scene_effect.instance_id,
                    },
                );
            }
        }
    }

    let batch_plan = build_batch_plan(settings.batching, frustum, occluders, &render_instances)?;
    let visible_effects = effect_set(&batch_plan.visible_instance_ids, &sources);
    let frustum_culled_effects = only_culled_effects(
        &batch_plan.frustum_culled_ids,
        &batch_plan.visible_instance_ids,
        &batch_plan.occlusion_culled_ids,
        &sources,
    );
    let occlusion_culled_effects = only_culled_effects(
        &batch_plan.occlusion_culled_ids,
        &batch_plan.visible_instance_ids,
        &batch_plan.frustum_culled_ids,
        &sources,
    );

    let mut simulated_particles = 0_u64;
    let mut lod_saved_particles = 0_u64;
    for effect_id in &visible_effects {
        let capacity = *effect_particles.get(effect_id).ok_or_else(|| {
            crate::render::vfx::invalid_vfx_error(
                "render_workload.effect_particles",
                format!("missing particle capacity for effect instance {effect_id}"),
            )
        })?;
        let scale = if far_lod_effects.contains(effect_id) {
            settings.far_lod_particle_scale
        } else {
            1.0
        };
        let scaled = scaled_particles(capacity, scale)?;
        simulated_particles = simulated_particles.checked_add(scaled).ok_or_else(|| {
            crate::render::vfx::invalid_vfx_error(
                "render_workload.simulated_particles",
                "visible particle count overflowed u64",
            )
        })?;
        lod_saved_particles = lod_saved_particles
            .checked_add(capacity.saturating_sub(scaled))
            .ok_or_else(|| {
                crate::render::vfx::invalid_vfx_error(
                    "render_workload.lod_saved_particles",
                    "LOD saved particle count overflowed u64",
                )
            })?;
    }
    if simulated_particles > settings.max_simulated_particles {
        return invalid_vfx(
            "render_workload.simulated_particles",
            format!(
                "visible workload has {simulated_particles} particles but limit is {}",
                settings.max_simulated_particles
            ),
        );
    }
    let particle_buffer_bytes = simulated_particles
        .checked_mul(u64::from(settings.particle_attribute_stride_bytes))
        .ok_or_else(|| {
            crate::render::vfx::invalid_vfx_error(
                "render_workload.particle_buffer_bytes",
                "particle buffer byte count overflowed u64",
            )
        })?;
    if particle_buffer_bytes > settings.max_particle_buffer_bytes {
        return invalid_vfx(
            "render_workload.particle_buffer_bytes",
            format!(
                "requires {particle_buffer_bytes} bytes but limit is {}",
                settings.max_particle_buffer_bytes
            ),
        );
    }

    Ok(VfxRenderWorkloadStats {
        target_width: settings.target_width,
        target_height: settings.target_height,
        input_effect_instances: effects.len(),
        input_particle_capacity,
        visible_effect_instances: visible_effects.len(),
        visible_render_instances: batch_plan.visible_instance_ids.len(),
        frustum_culled_effect_ids: frustum_culled_effects,
        occlusion_culled_effect_ids: occlusion_culled_effects,
        far_lod_effect_ids: far_lod_effects.into_iter().collect(),
        simulated_particles,
        lod_saved_particles,
        particle_buffer_bytes,
        draw_batches: batch_plan.draw_batches.len(),
        indirect_commands: batch_plan.indirect_commands.len(),
        batch_plan,
    })
}

impl VfxRenderWorkloadSettings {
    fn validate(&self) -> Result<(), EngineError> {
        if self.target_width == 0 || self.target_height == 0 {
            return invalid_vfx("render_workload.target_size", "dimensions must be non-zero");
        }
        if self.max_effect_instances == 0 {
            return invalid_vfx("render_workload.max_effect_instances", "must be non-zero");
        }
        if self.max_simulated_particles == 0 {
            return invalid_vfx(
                "render_workload.max_simulated_particles",
                "must be non-zero",
            );
        }
        if self.max_particle_buffer_bytes == 0 {
            return invalid_vfx(
                "render_workload.max_particle_buffer_bytes",
                "must be non-zero",
            );
        }
        if self.particle_attribute_stride_bytes == 0 {
            return invalid_vfx(
                "render_workload.particle_attribute_stride_bytes",
                "must be non-zero",
            );
        }
        validate_positive(
            "render_workload.far_lod_start_meters",
            self.far_lod_start_meters,
        )?;
        if !self.far_lod_particle_scale.is_finite()
            || !(0.0..=1.0).contains(&self.far_lod_particle_scale)
            || self.far_lod_particle_scale == 0.0
        {
            return invalid_vfx(
                "render_workload.far_lod_particle_scale",
                "must be finite inside 0..=1 and non-zero",
            );
        }
        self.batching.validate()
    }
}

impl VfxSceneEffectInstance {
    fn validate(&self, index: usize) -> Result<(), EngineError> {
        if self.instance_id == 0 {
            return invalid_vfx(
                format!("render_workload.instances[{index}].id"),
                "must be non-zero",
            );
        }
        if !self.world_bounds.min.is_finite()
            || !self.world_bounds.max.is_finite()
            || self.world_bounds.min.x > self.world_bounds.max.x
            || self.world_bounds.min.y > self.world_bounds.max.y
            || self.world_bounds.min.z > self.world_bounds.max.z
        {
            return invalid_vfx(
                format!("render_workload.instances[{index}].world_bounds"),
                "must be finite with min <= max",
            );
        }
        validate_screen_rect(index, self.screen_rect)?;
        if !self.nearest_depth.is_finite() || !(0.0..=1.0).contains(&self.nearest_depth) {
            return invalid_vfx(
                format!("render_workload.instances[{index}].nearest_depth"),
                "must be finite inside 0..=1",
            );
        }
        validate_positive(
            format!("render_workload.instances[{index}].distance_meters"),
            self.distance_meters,
        )
    }

    fn lod(&self, settings: VfxRenderWorkloadSettings) -> VfxRenderLod {
        if self.distance_meters >= settings.far_lod_start_meters {
            VfxRenderLod::Far
        } else {
            VfxRenderLod::Near
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderSource {
    effect_id: u64,
}

fn draw_contract(output: &VfxOutputModule) -> Option<(String, String, u32)> {
    match output {
        VfxOutputModule::Sprite { material_id, .. } => Some((
            "vfx.sprite.quad".to_string(),
            material_id.clone(),
            SPRITE_INDEX_COUNT,
        )),
        VfxOutputModule::Mesh {
            mesh_id,
            material_id,
            ..
        } => Some((mesh_id.clone(), material_id.clone(), MESH_INDEX_COUNT)),
        VfxOutputModule::Trail { material_id, .. } => Some((
            "vfx.trail.ribbon".to_string(),
            material_id.clone(),
            TRAIL_INDEX_COUNT,
        )),
        VfxOutputModule::Light { .. } => None,
    }
}

fn render_instance_id(instance_id: u64, output_index: usize) -> Result<u64, EngineError> {
    let offset = u64::try_from(output_index + 1).map_err(|_| {
        crate::render::vfx::invalid_vfx_error("render_workload.output_index", "does not fit in u64")
    })?;
    instance_id
        .checked_mul(100)
        .and_then(|base| base.checked_add(offset))
        .ok_or_else(|| {
            crate::render::vfx::invalid_vfx_error(
                "render_workload.render_instance_id",
                "render instance id overflowed u64",
            )
        })
}

fn effect_set(ids: &[u64], sources: &BTreeMap<u64, RenderSource>) -> BTreeSet<u64> {
    ids.iter()
        .filter_map(|id| sources.get(id).map(|source| source.effect_id))
        .collect()
}

fn only_culled_effects(
    culled_ids: &[u64],
    visible_ids: &[u64],
    other_culled_ids: &[u64],
    sources: &BTreeMap<u64, RenderSource>,
) -> Vec<u64> {
    let culled = effect_set(culled_ids, sources);
    let visible = effect_set(visible_ids, sources);
    let other = effect_set(other_culled_ids, sources);
    culled
        .difference(&visible)
        .copied()
        .filter(|effect_id| !other.contains(effect_id))
        .collect()
}

fn scaled_particles(capacity: u64, scale: f32) -> Result<u64, EngineError> {
    let scaled = (capacity as f64 * f64::from(scale)).ceil();
    if !scaled.is_finite() || scaled > u64::MAX as f64 {
        return invalid_vfx(
            "render_workload.lod_particles",
            "scaled particle count overflowed u64",
        );
    }
    Ok(scaled as u64)
}

fn validate_screen_rect(index: usize, rect: ScreenRect) -> Result<(), EngineError> {
    if !rect.min.is_finite()
        || !rect.max.is_finite()
        || rect.min.x < 0.0
        || rect.min.y < 0.0
        || rect.max.x > 1.0
        || rect.max.y > 1.0
        || rect.min.x > rect.max.x
        || rect.min.y > rect.max.y
    {
        return invalid_vfx(
            format!("render_workload.instances[{index}].screen_rect"),
            "must be finite, normalized, and ordered inside 0..=1",
        );
    }
    Ok(())
}

fn validate_positive(field: impl Into<String>, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value <= 0.0 {
        return invalid_vfx(field, "must be finite and positive");
    }
    Ok(())
}
