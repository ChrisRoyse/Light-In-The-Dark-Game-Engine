use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{
    EngineError,
    math::{Vec3, Vec4},
    render::{lighting::cluster_lights, material::PbrMaterial},
};

use super::{
    McpRenderState, RenderAnimationBinding, RenderLightingPlan, RenderMaterialBinding,
    RenderSceneComposition, invalid_mcp_render,
};

pub(super) fn validate_material_binding(
    binding: &RenderMaterialBinding,
) -> Result<(), EngineError> {
    validate_single_line("material.material_id", &binding.material_id)?;
    validate_single_line("material.target_asset_id", &binding.target_asset_id)?;
    validate_single_line("material.name", &binding.material.name)?;
    binding.material.validate()?;
    for note in &binding.channel_notes {
        validate_single_line("material.channel_notes", note)?;
    }
    Ok(())
}

pub(super) fn validate_lighting_plan(
    lighting: &RenderLightingPlan,
) -> Result<litd_engine::render::lighting::ClusteredLightAssignment, EngineError> {
    lighting.ambient.validate()?;
    if lighting.lights.is_empty() {
        return Err(invalid_mcp_render(
            "lighting.lights",
            "at least one explicit light is required",
        ));
    }
    cluster_lights(lighting.cluster_grid, &lighting.lights)
}

pub(super) fn validate_animation_binding(
    animation: &RenderAnimationBinding,
) -> Result<(), EngineError> {
    validate_single_line("animation.target_entity_id", &animation.target_entity_id)?;
    validate_single_line("animation.rig_id", &animation.rig_id)?;
    if animation.clips.is_empty() {
        return Err(invalid_mcp_render(
            "animation.clips",
            "at least one explicit animation clip is required",
        ));
    }
    let mut total = 0.0_f32;
    for clip in &animation.clips {
        validate_single_line("animation.clip_id", &clip.clip_id)?;
        validate_single_line("animation.source_relative_path", &clip.source_relative_path)?;
        if !clip.weight.is_finite() || clip.weight < 0.0 {
            return Err(invalid_mcp_render(
                "animation.weight",
                "clip weight must be finite and non-negative",
            ));
        }
        if !clip.normalized_time.is_finite() || !(0.0..=1.0).contains(&clip.normalized_time) {
            return Err(invalid_mcp_render(
                "animation.normalized_time",
                "clip normalized time must be finite and inside 0..=1",
            ));
        }
        total += clip.weight;
    }
    if (total - 1.0).abs() > 0.001 {
        return Err(invalid_mcp_render(
            "animation.weight_sum",
            format!("clip weights must sum to 1.0 exactly enough; observed {total}"),
        ));
    }
    Ok(())
}

pub(super) fn validate_scene_composition(
    scene: &RenderSceneComposition,
    state: &McpRenderState,
) -> Result<(), EngineError> {
    validate_single_line("scene.scene_id", &scene.scene_id)?;
    validate_single_line("scene.source_scene_id", &scene.source_scene_id)?;
    validate_single_line("scene.camera_mode", &scene.camera_mode)?;
    if scene.layers.is_empty() || scene.placements.is_empty() {
        return Err(invalid_mcp_render(
            "scene.composition",
            "scene composition requires at least one layer and one placement",
        ));
    }
    let mut layer_ids = BTreeSet::new();
    for layer in &scene.layers {
        validate_single_line("scene.layer_id", &layer.layer_id)?;
        validate_single_line("scene.layer_kind", &layer.kind)?;
        if !layer_ids.insert(layer.layer_id.as_str()) {
            return Err(invalid_mcp_render(
                "scene.layer_id",
                format!("duplicate layer id {}", layer.layer_id),
            ));
        }
    }
    let mut placement_ids = BTreeSet::new();
    for placement in &scene.placements {
        validate_single_line("scene.placement_id", &placement.placement_id)?;
        validate_single_line("scene.asset_id", &placement.asset_id)?;
        validate_single_line("scene.placement_layer_id", &placement.layer_id)?;
        if !placement_ids.insert(placement.placement_id.as_str()) {
            return Err(invalid_mcp_render(
                "scene.placement_id",
                format!("duplicate placement id {}", placement.placement_id),
            ));
        }
        if !scene
            .layers
            .iter()
            .any(|layer| layer.layer_id == placement.layer_id)
        {
            return Err(invalid_mcp_render(
                "scene.placement_layer_id",
                format!(
                    "{} does not reference a declared layer",
                    placement.placement_id
                ),
            ));
        }
        validate_vec3("scene.translation", placement.translation)?;
        if !placement.rotation_y_rad.is_finite() {
            return Err(invalid_mcp_render(
                "scene.rotation_y_rad",
                "placement rotation must be finite",
            ));
        }
        if let Some(parent) = &placement.parent_placement_id {
            validate_single_line("scene.parent_placement_id", parent)?;
            if parent == &placement.placement_id {
                return Err(invalid_mcp_render(
                    "scene.parent_placement_id",
                    "placement cannot parent itself",
                ));
            }
            if !scene
                .placements
                .iter()
                .any(|candidate| candidate.placement_id == *parent)
            {
                return Err(invalid_mcp_render(
                    "scene.parent_placement_id",
                    format!("{} does not reference a placement", parent),
                ));
            }
        }
        validate_positive_vec3("scene.scale", placement.scale)?;
    }
    validate_scene_parent_chains(scene)?;
    for assignment in &scene.material_assignments {
        validate_single_line("scene.assignment.placement_id", &assignment.placement_id)?;
        validate_single_line("scene.assignment.material_id", &assignment.material_id)?;
        if !scene
            .placements
            .iter()
            .any(|placement| placement.placement_id == assignment.placement_id)
        {
            return Err(invalid_mcp_render(
                "scene.assignment.placement_id",
                format!("{} does not reference a placement", assignment.placement_id),
            ));
        }
        if !state
            .materials
            .iter()
            .any(|material| material.material_id == assignment.material_id)
        {
            return Err(invalid_mcp_render(
                "scene.assignment.material_id",
                format!(
                    "{} has no persisted material binding",
                    assignment.material_id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_scene_parent_chains(scene: &RenderSceneComposition) -> Result<(), EngineError> {
    let parents = scene
        .placements
        .iter()
        .map(|placement| {
            (
                placement.placement_id.as_str(),
                placement.parent_placement_id.as_deref(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for placement in &scene.placements {
        let mut seen = BTreeSet::new();
        let mut cursor = placement.parent_placement_id.as_deref();
        while let Some(parent) = cursor {
            if parent == placement.placement_id {
                return Err(invalid_mcp_render(
                    "scene.parent_placement_id",
                    format!("{} parent chain contains a cycle", placement.placement_id),
                ));
            }
            if !seen.insert(parent) {
                return Err(invalid_mcp_render(
                    "scene.parent_placement_id",
                    format!("parent chain repeats {parent}"),
                ));
            }
            cursor = parents.get(parent).and_then(|next| *next);
        }
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_render(
            field,
            "value must be non-empty single-line text",
        ));
    }
    Ok(())
}

fn validate_vec3(field: &str, value: Vec3) -> Result<(), EngineError> {
    if !value.is_finite() {
        return Err(invalid_mcp_render(
            field,
            "vector components must be finite",
        ));
    }
    Ok(())
}

fn validate_positive_vec3(field: &str, value: Vec3) -> Result<(), EngineError> {
    validate_vec3(field, value)?;
    if value.cmple(Vec3::ZERO).any() {
        return Err(invalid_mcp_render(
            field,
            "vector components must be greater than zero",
        ));
    }
    Ok(())
}

pub fn sample_issue923_material() -> RenderMaterialBinding {
    let mut material = PbrMaterial::new("issue923 rune tempered steel");
    material.base_color_factor = Vec4::new(0.42, 0.47, 0.52, 1.0);
    material.metallic_factor = 0.85;
    material.roughness_factor = 0.31;
    material.normal_scale = 1.15;
    material.occlusion_strength = 0.9;
    material.emissive_factor = Vec3::new(0.0, 0.62, 0.94);
    RenderMaterialBinding {
        material_id: "mat.issue923.rune_tempered_steel".to_string(),
        target_asset_id: "asset.issue923.hero_helm".to_string(),
        material,
        channel_notes: vec![
            "base_color metallic roughness normal ambient_occlusion emissive".to_string(),
            "stylized dark fantasy rim rune glow".to_string(),
        ],
    }
}
