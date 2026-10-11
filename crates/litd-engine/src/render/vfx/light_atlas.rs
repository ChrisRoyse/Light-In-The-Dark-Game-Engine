use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Vec2, Vec3},
    render::{
        lighting::{ClusterGrid, ClusteredLightAssignment, RenderLight, cluster_lights},
        vfx::{
            VfxBlendMode, VfxColor, VfxEffectAsset, VfxOutputModule, invalid_vfx, validate_effect,
        },
    },
};

pub const VFX_MAX_LOCAL_LIGHTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxAtlasCatalog {
    pub atlas_id: String,
    pub columns: u32,
    pub rows: u32,
    pub slots: Vec<VfxAtlasSlot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxAtlasSlot {
    pub slot_id: String,
    pub first_frame: u32,
    pub frame_count: u32,
    pub frames_per_second: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VfxPresentationTerms {
    pub world_position: Vec3,
    pub elapsed_seconds: f32,
    pub team_color: VfxColor,
    pub fog_visibility: f32,
}

impl VfxPresentationTerms {
    pub const fn new(
        world_position: Vec3,
        elapsed_seconds: f32,
        team_color: VfxColor,
        fog_visibility: f32,
    ) -> Self {
        Self {
            world_position,
            elapsed_seconds,
            team_color,
            fog_visibility,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxOutputPlan {
    pub effect_id: String,
    pub atlas_id: String,
    pub presentation_terms: VfxPresentationTerms,
    pub sprite_samples: Vec<VfxAtlasSample>,
    pub local_lights: Vec<RenderLight>,
}

impl VfxOutputPlan {
    pub fn sprite_count(&self) -> usize {
        self.sprite_samples.len()
    }

    pub fn local_light_count(&self) -> usize {
        self.local_lights.len()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxAtlasSample {
    pub emitter_id: String,
    pub material_id: String,
    pub texture_atlas_slot: String,
    pub blend_mode: VfxBlendMode,
    pub frame_index: u32,
    pub frame_column: u32,
    pub frame_row: u32,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    pub team_color: VfxColor,
    pub fog_alpha_multiplier: f32,
}

pub fn resolve_vfx_output_plan(
    effect: &VfxEffectAsset,
    atlas: &VfxAtlasCatalog,
    terms: VfxPresentationTerms,
) -> Result<VfxOutputPlan, EngineError> {
    validate_effect(effect)?;
    atlas.validate()?;
    validate_terms(terms)?;

    let mut sprite_samples = Vec::new();
    let mut local_lights = Vec::new();
    for emitter in &effect.emitters {
        for output in &emitter.output {
            match output {
                VfxOutputModule::Sprite {
                    material_id,
                    texture_atlas_slot,
                    blend_mode,
                } => {
                    let slot = atlas.slot(texture_atlas_slot)?;
                    sprite_samples.push(resolve_atlas_sample(
                        atlas,
                        slot,
                        &emitter.emitter_id,
                        material_id,
                        *blend_mode,
                        terms,
                    )?);
                }
                VfxOutputModule::Light {
                    color,
                    intensity_lumens,
                    radius_meters,
                } => {
                    if local_lights.len() >= VFX_MAX_LOCAL_LIGHTS {
                        return invalid_vfx(
                            "vfxlights.count",
                            format!(
                                "has more than {VFX_MAX_LOCAL_LIGHTS} local VFX lights; split or reduce effect outputs"
                            ),
                        );
                    }
                    let light = resolve_light(
                        &effect.effect_id,
                        &emitter.emitter_id,
                        local_lights.len(),
                        *color,
                        *intensity_lumens,
                        *radius_meters,
                        terms,
                    );
                    light.validate(local_lights.len())?;
                    local_lights.push(light);
                }
                VfxOutputModule::Mesh { .. } | VfxOutputModule::Trail { .. } => {}
            }
        }
    }

    Ok(VfxOutputPlan {
        effect_id: effect.effect_id.clone(),
        atlas_id: atlas.atlas_id.clone(),
        presentation_terms: terms,
        sprite_samples,
        local_lights,
    })
}

pub fn cluster_vfx_output_lights(
    plan: &VfxOutputPlan,
    grid: ClusterGrid,
) -> Result<ClusteredLightAssignment, EngineError> {
    cluster_lights(grid, &plan.local_lights)
}

impl VfxAtlasCatalog {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_non_empty("vfxatlas.atlas_id", &self.atlas_id)?;
        if self.columns == 0 {
            return invalid_vfx("vfxatlas.columns", "atlas columns must be non-zero");
        }
        if self.rows == 0 {
            return invalid_vfx("vfxatlas.rows", "atlas rows must be non-zero");
        }
        if self.slots.is_empty() {
            return invalid_vfx("vfxatlas.slots", "atlas must contain at least one slot");
        }
        let capacity = self.frame_capacity()?;
        let mut seen = HashSet::new();
        for (index, slot) in self.slots.iter().enumerate() {
            let field = format!("vfxatlas.slots[{index}]");
            validate_non_empty(format!("{field}.slot_id"), &slot.slot_id)?;
            if !seen.insert(slot.slot_id.as_str()) {
                return invalid_vfx(format!("{field}.slot_id"), "slot ids must be unique");
            }
            if slot.frame_count == 0 {
                return invalid_vfx(format!("{field}.frame_count"), "must be non-zero");
            }
            validate_positive(format!("{field}.frames_per_second"), slot.frames_per_second)?;
            let exclusive_end =
                slot.first_frame
                    .checked_add(slot.frame_count)
                    .ok_or_else(|| {
                        crate::render::vfx::invalid_vfx_error(
                            format!("{field}.frame_range"),
                            "first_frame + frame_count overflowed u32",
                        )
                    })?;
            if exclusive_end > capacity {
                return invalid_vfx(
                    format!("{field}.frame_range"),
                    format!(
                        "frames {}..{} exceed atlas capacity {capacity}",
                        slot.first_frame, exclusive_end
                    ),
                );
            }
        }
        Ok(())
    }

    pub fn slot(&self, slot_id: &str) -> Result<&VfxAtlasSlot, EngineError> {
        self.slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .ok_or_else(|| {
                crate::render::vfx::invalid_vfx_error(
                    "vfxatlas.slot",
                    format!(
                        "atlas slot '{slot_id}' was not found in '{}'",
                        self.atlas_id
                    ),
                )
            })
    }

    fn frame_capacity(&self) -> Result<u32, EngineError> {
        self.columns.checked_mul(self.rows).ok_or_else(|| {
            crate::render::vfx::invalid_vfx_error("vfxatlas.capacity", "columns * rows overflowed")
        })
    }
}

fn resolve_atlas_sample(
    atlas: &VfxAtlasCatalog,
    slot: &VfxAtlasSlot,
    emitter_id: &str,
    material_id: &str,
    blend_mode: VfxBlendMode,
    terms: VfxPresentationTerms,
) -> Result<VfxAtlasSample, EngineError> {
    let frame_cursor = (terms.elapsed_seconds * slot.frames_per_second).floor();
    if !frame_cursor.is_finite() || frame_cursor > u32::MAX as f32 {
        return invalid_vfx("vfxatlas.frame_cursor", "frame cursor exceeded u32 range");
    }
    let local_frame = frame_cursor as u32 % slot.frame_count;
    let frame_index = slot.first_frame + local_frame;
    let column = frame_index % atlas.columns;
    let row = frame_index / atlas.columns;
    let inv_columns = 1.0 / atlas.columns as f32;
    let inv_rows = 1.0 / atlas.rows as f32;

    Ok(VfxAtlasSample {
        emitter_id: emitter_id.to_string(),
        material_id: material_id.to_string(),
        texture_atlas_slot: slot.slot_id.clone(),
        blend_mode,
        frame_index,
        frame_column: column,
        frame_row: row,
        uv_min: Vec2::new(column as f32 * inv_columns, row as f32 * inv_rows),
        uv_max: Vec2::new(
            (column + 1) as f32 * inv_columns,
            (row + 1) as f32 * inv_rows,
        ),
        team_color: terms.team_color,
        fog_alpha_multiplier: terms.fog_visibility,
    })
}

fn resolve_light(
    effect_id: &str,
    emitter_id: &str,
    index: usize,
    color: VfxColor,
    intensity_lumens: f32,
    radius_meters: f32,
    terms: VfxPresentationTerms,
) -> RenderLight {
    RenderLight::point(
        format!("vfx.{effect_id}.{emitter_id}.light{index}"),
        terms.world_position,
        radius_meters,
        Vec3::new(
            color.r * terms.team_color.r * terms.fog_visibility,
            color.g * terms.team_color.g * terms.fog_visibility,
            color.b * terms.team_color.b * terms.fog_visibility,
        ),
        intensity_lumens * color.a * terms.team_color.a * terms.fog_visibility,
    )
}

fn validate_terms(terms: VfxPresentationTerms) -> Result<(), EngineError> {
    if !terms.world_position.is_finite() {
        return invalid_vfx("presentation.world_position", "position must be finite");
    }
    validate_finite_non_negative("presentation.elapsed_seconds", terms.elapsed_seconds)?;
    validate_color("presentation.team_color", terms.team_color)?;
    validate_normalized("presentation.fog_visibility", terms.fog_visibility)
}

fn validate_color(field: impl Into<String>, color: VfxColor) -> Result<(), EngineError> {
    let field = field.into();
    validate_normalized(format!("{field}.r"), color.r)?;
    validate_normalized(format!("{field}.g"), color.g)?;
    validate_normalized(format!("{field}.b"), color.b)?;
    validate_normalized(format!("{field}.a"), color.a)
}

fn validate_non_empty(field: impl Into<String>, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_vfx(field, "must be non-empty");
    }
    Ok(())
}

fn validate_positive(field: impl Into<String>, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value <= 0.0 {
        return invalid_vfx(field, "must be finite and positive");
    }
    Ok(())
}

fn validate_finite_non_negative(field: impl Into<String>, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value < 0.0 {
        return invalid_vfx(field, "must be finite and non-negative");
    }
    Ok(())
}

fn validate_normalized(field: impl Into<String>, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return invalid_vfx(field, "must be finite inside 0..=1");
    }
    Ok(())
}
