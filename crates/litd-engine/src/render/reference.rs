//! README-quality reference scene planning and visual-target comparison.

#[path = "reference_canvas.rs"]
mod reference_canvas;
#[path = "reference_gpu.rs"]
mod reference_gpu;
#[path = "reference_image.rs"]
mod reference_image;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Aabb, Vec2, Vec3, Vec4},
};

use super::{
    batching::{
        BatchPlan, BatchingSettings, HzbOccluder, RenderInstance, ScreenRect, build_batch_plan,
    },
    camera::{RtsCamera, RtsCameraConfig, RtsCameraState},
    font::{FontAtlas, ui_reference_text_atlas_request},
    lighting::{ClusterGrid, ClusteredLightAssignment, RenderLight, SpotCone, cluster_lights},
    material::{IblEnvironment, PbrLight, PbrMaterial, PbrSample, PbrShadeResult, shade},
    post::{PostProcessPlan, PostProcessSettings, build_post_process_plan},
    screen_space::{ScreenSpacePlan, ScreenSpaceSettings, build_screen_space_plan},
    shader::ShaderSystemSettings,
    shadow::{ShadowCamera, ShadowPlan, ShadowSettings, build_shadow_plan},
    stylized::{StylizedShaderLibrary, build_stylized_shader_library},
    ui::{
        FloatingText, FogOfWarGrid, HudLayoutPreset, HudRegionKind, HudWidget, HudWidgetKind,
        MapOverlayPolicy, MinimapConfig, TextRun, UiRect, UiRenderInputs, UiRenderPlan,
        UiRenderSettings, VisionSource, build_ui_render_plan, reference_hud_layout,
    },
};

pub use reference_gpu::{
    GPU_REFERENCE_SCENE_WGSL, GpuReferenceSceneFrame, render_reference_scene_gpu,
    render_reference_scene_gpu_with_context,
};
pub use reference_image::{
    ReferenceSceneComparison, ReferenceSceneImage, ReferenceSceneMetrics, VisualMetricBand,
    compare_reference_scene_image, render_reference_scene_image,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceSceneSettings {
    pub width: u32,
    pub height: u32,
}

impl Default for ReferenceSceneSettings {
    fn default() -> Self {
        Self {
            width: 960,
            height: 540,
        }
    }
}

impl ReferenceSceneSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.width < 320 || self.height < 180 {
            return invalid_reference_scene(
                "visual_target",
                "width/height must be at least 320x180 for comparison metrics",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceScenePlan {
    pub settings: ReferenceSceneSettings,
    pub reference_images: Vec<String>,
    pub materials: Vec<PbrMaterial>,
    pub shade_results: Vec<PbrShadeResult>,
    pub lights: Vec<RenderLight>,
    pub light_assignment: ClusteredLightAssignment,
    pub shadow_plan: ShadowPlan,
    pub post_plan: PostProcessPlan,
    pub screen_space_plan: ScreenSpacePlan,
    pub stylized_library: StylizedShaderLibrary,
    pub batch_plan: BatchPlan,
    pub ui_plan: UiRenderPlan,
    pub camera_eye: Vec3,
    pub camera_target: Vec3,
}

impl ReferenceScenePlan {
    pub fn reference_image_list(&self) -> String {
        self.reference_images.join("|")
    }

    pub fn pbr_material_count(&self) -> usize {
        self.materials.len()
    }
}

pub fn build_reference_scene_plan(
    settings: ReferenceSceneSettings,
) -> Result<ReferenceScenePlan, EngineError> {
    settings.validate()?;
    let world = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(72.0))?;
    let camera = RtsCamera::new(
        RtsCameraConfig::standard(settings.width, settings.height, world),
        RtsCameraState::isometric(Vec3::ZERO),
    )?;
    let materials = reference_materials();
    let lights = reference_lights();
    let light_assignment = cluster_lights(
        ClusterGrid::new(
            4,
            3,
            4,
            Vec3::new(-48.0, -24.0, -96.0),
            Vec3::new(48.0, 48.0, 24.0),
            8,
        ),
        &lights,
    )?;
    let shadow_plan =
        build_shadow_plan(ShadowSettings::default(), shadow_camera(&camera), &lights)?;
    let post_plan = build_post_process_plan(
        PostProcessSettings::default(),
        settings.width,
        settings.height,
    )?;
    let screen_space_plan = build_screen_space_plan(
        ScreenSpaceSettings::default(),
        settings.width,
        settings.height,
    )?;
    let stylized_library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let batch_plan = build_batch_plan(
        BatchingSettings::default(),
        &camera.frustum()?.to_batching_frustum(),
        &reference_occluders()?,
        &reference_instances()?,
    )?;
    let ui_plan = build_reference_ui_plan(settings, world, camera.view_projection_matrix())?;
    let shade_results = shade_reference_materials(&materials, &lights, &camera)?;
    Ok(ReferenceScenePlan {
        settings,
        // Composition/quality references ONLY — never a HUD/behavior spec.
        // ED12's minimap, quest trackers, and player marker are OBSOLETE per
        // ADR-944 / #944 / #1510 and must not ship in packaged player runtime.
        reference_images: vec![
            "docs/readme/GP03-combat.png".to_string(),
            "docs/readme/ED12-play-topdown.png".to_string(),
            "docs/readme/ED11-world-view.png".to_string(),
            "docs/readme/GP09-inventory.png".to_string(),
        ],
        materials,
        shade_results,
        lights,
        light_assignment,
        shadow_plan,
        post_plan,
        screen_space_plan,
        stylized_library,
        batch_plan,
        ui_plan,
        camera_eye: camera.eye(),
        camera_target: camera.state.target,
    })
}

fn reference_materials() -> Vec<PbrMaterial> {
    let mut basalt = PbrMaterial::new("ashen_basalt");
    basalt.base_color_factor = Vec4::new(0.28, 0.30, 0.34, 1.0);
    basalt.roughness_factor = 0.78;
    let mut hero_gold = PbrMaterial::new("hero_gold_trim");
    hero_gold.base_color_factor = Vec4::new(0.96, 0.68, 0.24, 1.0);
    hero_gold.metallic_factor = 0.72;
    hero_gold.roughness_factor = 0.34;
    let mut ember = PbrMaterial::new("ember_magic");
    ember.base_color_factor = Vec4::new(1.0, 0.22, 0.04, 1.0);
    ember.roughness_factor = 0.42;
    ember.emissive_factor = Vec3::new(2.4, 0.55, 0.12);
    let mut arcane = PbrMaterial::new("arcane_blue");
    arcane.base_color_factor = Vec4::new(0.1, 0.46, 0.95, 1.0);
    arcane.roughness_factor = 0.5;
    arcane.emissive_factor = Vec3::new(0.16, 0.8, 2.6);
    vec![basalt, hero_gold, ember, arcane]
}

fn reference_lights() -> Vec<RenderLight> {
    vec![
        RenderLight::directional(
            "storm_sun_key",
            Vec3::new(-0.35, 0.78, -0.52),
            Vec3::new(1.0, 0.82, 0.62),
            4.5,
        ),
        RenderLight::point(
            "ember_brazier",
            Vec3::new(-10.0, 4.0, 6.0),
            26.0,
            Vec3::new(1.0, 0.36, 0.08),
            9.0,
        ),
        RenderLight::spot(
            "arcane_rim",
            Vec3::new(18.0, 20.0, -22.0),
            Vec3::new(-0.5, -0.7, 0.5),
            44.0,
            SpotCone::new(18.0_f32.to_radians(), 36.0_f32.to_radians()),
            Vec3::new(0.25, 0.75, 1.0),
            5.0,
        ),
    ]
}

fn shade_reference_materials(
    materials: &[PbrMaterial],
    lights: &[RenderLight],
    camera: &RtsCamera,
) -> Result<Vec<PbrShadeResult>, EngineError> {
    let key = &lights[0];
    let direction_to_light = match key.kind {
        super::lighting::LightKind::Directional { direction_to_light } => direction_to_light,
        _ => Vec3::Y,
    };
    let sample = PbrSample {
        normal: Vec3::Y,
        view: -camera.forward(),
        light: PbrLight {
            direction_to_light,
            radiance: key.color * key.intensity,
        },
        environment: IblEnvironment {
            diffuse_irradiance: Vec3::splat(0.08),
            specular_prefilter: Vec3::splat(0.12),
            brdf_scale: 0.9,
            brdf_bias: 0.04,
        },
    };
    materials
        .iter()
        .map(|material| shade(material, sample))
        .collect()
}

fn shadow_camera(camera: &RtsCamera) -> ShadowCamera {
    ShadowCamera {
        position: camera.eye(),
        forward: camera.forward(),
        up: Vec3::Y,
        fov_y_rad: camera.config.fov_y_rad,
        aspect: camera.config.aspect_ratio(),
        near: camera.config.near,
        far: 128.0,
    }
}

fn reference_instances() -> Result<Vec<RenderInstance>, EngineError> {
    Ok(vec![
        instance(
            1,
            "ruin_floor",
            "ashen_basalt",
            Vec3::ZERO,
            Vec3::new(30.0, 2.0, 30.0),
            ([0.22, 0.26], [0.78, 0.72]),
            0.42,
        )?,
        instance(
            2,
            "hero",
            "hero_gold_trim",
            Vec3::new(0.0, 3.0, 0.0),
            Vec3::new(2.0, 5.0, 2.0),
            ([0.47, 0.38], [0.53, 0.55]),
            0.31,
        )?,
        instance(
            3,
            "ember_vfx",
            "ember_magic",
            Vec3::new(-10.0, 3.0, 5.0),
            Vec3::splat(4.0),
            ([0.34, 0.34], [0.45, 0.52]),
            0.35,
        )?,
        instance(
            4,
            "arcane_rune",
            "arcane_blue",
            Vec3::new(12.0, 1.0, -12.0),
            Vec3::splat(5.0),
            ([0.56, 0.28], [0.68, 0.43]),
            0.33,
        )?,
        instance(
            5,
            "hidden_enemy",
            "ashen_basalt",
            Vec3::new(8.0, 2.0, 10.0),
            Vec3::splat(3.0),
            ([0.50, 0.44], [0.56, 0.53]),
            0.63,
        )?,
    ])
}

fn instance(
    id: u64,
    mesh_id: &str,
    material_id: &str,
    center: Vec3,
    half: Vec3,
    screen: ([f32; 2], [f32; 2]),
    depth: f32,
) -> Result<RenderInstance, EngineError> {
    let (min, max) = screen;
    Ok(RenderInstance {
        id,
        mesh_id: mesh_id.to_string(),
        material_id: material_id.to_string(),
        world_bounds: Aabb::from_center_half_extents(center, half)?,
        screen_rect: ScreenRect::new(Vec2::new(min[0], min[1]), Vec2::new(max[0], max[1]))?,
        nearest_depth: depth,
        index_count: 180,
    })
}

fn reference_occluders() -> Result<Vec<HzbOccluder>, EngineError> {
    Ok(vec![HzbOccluder::new(
        ScreenRect::new(Vec2::new(0.49, 0.43), Vec2::new(0.57, 0.54))?,
        0.48,
    )])
}

fn build_reference_ui_plan(
    settings: ReferenceSceneSettings,
    world: Aabb,
    view_projection: glam::Mat4,
) -> Result<UiRenderPlan, EngineError> {
    let ui_settings = UiRenderSettings {
        viewport_width: settings.width as f32,
        viewport_height: settings.height as f32,
    };
    let layout = reference_hud_layout(ui_settings, HudLayoutPreset::PlayTopDown)?;
    let command_bar = layout
        .region_rect(HudRegionKind::CommandBar)
        .ok_or_else(|| {
            reference_scene_error("ui.command_bar", "play preset is missing command bar")
        })?;
    build_ui_render_plan(
        ui_settings,
        &FontAtlas::from_font_file(ui_reference_text_atlas_request())?.atlas,
        UiRenderInputs {
            text_runs: vec![TextRun {
                id: "scene_label".to_string(),
                text: "Ashen Reach".to_string(),
                origin_px: Vec2::new(18.0, 18.0),
                font_px: 22.0,
                color: Vec4::new(1.0, 0.84, 0.35, 1.0),
            }],
            shaped_text_runs: Vec::new(),
            widgets: vec![
                HudWidget {
                    id: "health".to_string(),
                    rect_px: UiRect::new(
                        command_bar.min + Vec2::new(18.0, 86.0),
                        command_bar.min + Vec2::new(260.0, 116.0),
                    )?,
                    kind: HudWidgetKind::HealthBar {
                        current: 92.0,
                        max: 100.0,
                    },
                },
                HudWidget {
                    id: "cleave".to_string(),
                    rect_px: UiRect::new(
                        command_bar.min + Vec2::new(330.0, 42.0),
                        command_bar.min + Vec2::new(386.0, 98.0),
                    )?,
                    kind: HudWidgetKind::CommandButton {
                        enabled: true,
                        cooldown: 0.0,
                    },
                },
            ],
            floating_texts: vec![FloatingText {
                text: "245".to_string(),
                world_position: Vec3::ZERO,
                age_seconds: 0.2,
                lifetime_seconds: 1.2,
                rise_px_per_second: 34.0,
                color: Vec4::new(1.0, 0.82, 0.12, 1.0),
            }],
            // #1537 / #944 / ADR-944: packaged RPG/ARPG play reference must NOT
            // depict omniscient navigation (minimap, GPS player-dot, quest pins,
            // route ribbons). Policy is runtime_rpg_no_map_access — the minimap
            // pass is suppressed even if a rect is present in layout.
            minimap: MinimapConfig {
                world_bounds: world,
                rect_px: layout.minimap_rect_px,
            },
            overlay_policy: MapOverlayPolicy::runtime_rpg_no_map_access(),
            markers: Vec::new(),
            viewport_world_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(20.0))?,
            fog: FogOfWarGrid::new(8, 8, world)?,
            vision_sources: vec![VisionSource {
                world_position: Vec3::ZERO,
                radius: 30.0,
            }],
            view_projection,
        },
    )
}

fn invalid_reference_scene<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(reference_scene_error(field, reason))
}

fn reference_scene_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ReferenceSceneInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "reference_tests.rs"]
mod tests;
