use bytemuck::{Pod, Zeroable};

use crate::render::lighting::{
    DEFAULT_TIME_OF_DAY_HOURS, DayNightLightingInput, DayNightLightingPlan, WeatherLightingState,
};

use super::{EditorWorldCameraState, EditorWorldLayerVisibility};

pub(super) const ATMOSPHERE_MODEL: &str = "exponential_squared_distance_plus_height_fog";
pub(super) const ATMOSPHERE_PROFILE: &str = "fog_color=0.018,0.032,0.052;distance_start=96;distance_range=150;fog_max=0.58;height_reference=-12;height_range=50;height_strength=0.26;aerial_strength=0.46;light_power=1.25";

const FOG_START: f32 = 96.0;
const FOG_RANGE: f32 = 150.0;
const FOG_MAX: f32 = 0.58;
const LIGHT_POWER: f32 = 1.25;
const HEIGHT_REFERENCE: f32 = -12.0;
const HEIGHT_RANGE: f32 = 50.0;
const HEIGHT_STRENGTH: f32 = 0.26;
const AERIAL_STRENGTH: f32 = 0.46;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorWorldRenderEvidence {
    pub renderer_kind: &'static str,
    pub visual_preset_id: &'static str,
    pub visual_preset_label: &'static str,
    pub visual_preset_camera_profile: &'static str,
    pub visual_preset_reference_images: String,
    pub visual_preset_contract: String,
    pub scene_name: String,
    pub scene_source_id: String,
    pub scene_source_path: String,
    pub scene_dependency_count: u32,
    pub scene_material_count: u32,
    pub scene_texture_count: u32,
    pub scene_image_count: u32,
    pub scene_is_blank: bool,
    pub blank_grid_visible: bool,
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
    pub shader_label: &'static str,
    pub post_shader_label: &'static str,
    pub hdr_format: &'static str,
    pub additive_blend_mode: &'static str,
    pub lighting_environment_summary: String,
    pub lighting_time_of_day_hours: String,
    pub lighting_phase: &'static str,
    pub lighting_weather_summary: String,
    pub lighting_primary_light: String,
    pub lighting_primary_direction_to_light: String,
    pub lighting_primary_intensity: String,
    pub lighting_weather_attenuation: String,
    pub lighting_ambient: String,
    pub lighting_fog: String,
    pub atmosphere_model: &'static str,
    pub atmosphere_profile: &'static str,
    pub bloom_passes: u32,
    pub bloom_intensity: &'static str,
    pub exposure: &'static str,
    pub color_grade: &'static str,
    pub background_model: &'static str,
    pub background_profile: &'static str,
    pub depth_format: &'static str,
    pub viewport_rect: String,
    pub vertex_count: u32,
    pub index_count: u32,
    pub triangle_count: u32,
    pub mesh_primitives: u32,
    pub layer_state: String,
    pub layer_count: u32,
    pub hidden_layer_count: u32,
    pub terrain_visible: bool,
    pub foliage_visible: bool,
    pub doodads_visible: bool,
    pub units_visible: bool,
    pub water_visible: bool,
    pub blight_visible: bool,
    pub shadows_visible: bool,
    pub lighting_visible: bool,
    pub weather_visible: bool,
    pub sky_visible: bool,
    pub fog_effects_visible: bool,
    pub grid_visible: bool,
    pub camera_bounds_visible: bool,
    pub pathing_ground_visible: bool,
    pub pathing_naval_visible: bool,
    pub regions_visible: bool,
    pub camera_objects_visible: bool,
    pub info_display_visible: bool,
    pub letterbox_mode: bool,
    pub lock_to_game_camera: bool,
    pub foliage_index_count: u32,
    pub foliage_template_index_count: u32,
    pub foliage_instance_count: u32,
    pub foliage_instanced_draw_calls: u32,
    pub foliage_distance_fade: &'static str,
    pub visible_index_count: u32,
    pub visible_triangle_count: u32,
    pub hidden_index_count: u32,
    pub hidden_triangle_count: u32,
    pub draw_calls: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct EditorWorldUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    light_dir: [f32; 4],
    fog_color: [f32; 4],
    params: [f32; 4],
    atmosphere: [f32; 4],
}

impl EditorWorldUniform {
    pub(super) fn new(
        camera: EditorWorldCameraState,
        aspect_ratio: f32,
        layers: EditorWorldLayerVisibility,
        lighting: &DayNightLightingPlan,
    ) -> Self {
        let fog_max = if layers.fog_effects {
            lighting.fog_max
        } else {
            0.0
        };
        let light_power = if layers.lighting {
            lighting.primary_intensity
        } else {
            0.0
        };
        let height_strength = if layers.fog_effects {
            HEIGHT_STRENGTH * (1.0 + lighting.input.weather.fog_density * 0.35)
        } else {
            0.0
        };
        let aerial_strength = if layers.fog_effects {
            AERIAL_STRENGTH * (0.75 + lighting.day_factor * 0.35)
        } else {
            0.0
        };
        Self {
            view_proj: camera.view_projection(aspect_ratio).to_cols_array_2d(),
            camera_pos: camera.eye().extend(1.0).to_array(),
            light_dir: (-lighting.primary_direction_to_light.normalize())
                .extend(0.0)
                .to_array(),
            fog_color: lighting.fog_color.extend(1.0).to_array(),
            params: [FOG_START, FOG_RANGE, fog_max, light_power],
            atmosphere: [
                HEIGHT_REFERENCE,
                HEIGHT_RANGE,
                height_strength,
                aerial_strength,
            ],
        }
    }
}

pub(super) fn default_editor_world_lighting_input(
    layers: EditorWorldLayerVisibility,
) -> DayNightLightingInput {
    DayNightLightingInput {
        time_of_day_hours: DEFAULT_TIME_OF_DAY_HOURS,
        weather: if layers.weather {
            WeatherLightingState::AUTHORING_WEATHER
        } else {
            WeatherLightingState::CLEAR
        },
        sun_base_intensity: LIGHT_POWER,
        fog_max_base: FOG_MAX,
        ..DayNightLightingInput::default()
    }
}
