use bytemuck::{Pod, Zeroable};

use super::EditorWorldLayerVisibility;
#[path = "editor_world_post_helpers.rs"]
mod editor_world_post_helpers;

use editor_world_post_helpers::{
    blur_bind_group, blur_bind_group_layout, composite_bind_group, composite_bind_group_layout,
    composite_pass, display_flag, fullscreen_pass, post_pipeline, post_texture,
};

pub(super) const EDITOR_WORLD_HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(super) const EDITOR_WORLD_POST_WGSL: &str = include_str!("editor_world_post.wgsl");
const BLOOM_INTENSITY: f32 = 1.36;
const BLOOM_INTENSITY_LABEL: &str = "1.36";
const EXPOSURE: f32 = 1.02;
const EXPOSURE_LABEL: &str = "1.02";
const COLOR_GRADE_LABEL: &str = "contrast=1.16;saturation=1.10;pivot=0.24;shadow_cool=blue;highlight_warm=gold;ed11_cinematic_veil=smoke_vignette_warm_focus";
pub(super) const BACKGROUND_MODEL_LABEL: &str = "ed11_clear_color_screen_space_atmosphere";
pub(super) const BACKGROUND_PROFILE_LABEL: &str = "clear=0.022,0.030,0.044;top=0.010,0.026,0.052;horizon=0.080,0.135,0.175;mist_band=0.43;mist_strength=0.34;mask=smoothstep_0.004_0.020";

pub(super) struct EditorWorldPost {
    width: u32,
    height: u32,
    _scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    _emissive: wgpu::Texture,
    emissive_view: wgpu::TextureView,
    _blur_a: wgpu::Texture,
    blur_a_view: wgpu::TextureView,
    _blur_b: wgpu::Texture,
    blur_b_view: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    blur_horizontal_bind_group: wgpu::BindGroup,
    blur_vertical_bind_group: wgpu::BindGroup,
    composite_bind_group: wgpu::BindGroup,
    blur_horizontal_pipeline: wgpu::RenderPipeline,
    blur_vertical_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
}

impl EditorWorldPost {
    pub(super) fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        let scene = post_texture(device, "litd-editor-world-hdr-scene", width, height);
        let scene_view = scene.create_view(&wgpu::TextureViewDescriptor::default());
        let emissive = post_texture(device, "litd-editor-world-emissive-additive", width, height);
        let emissive_view = emissive.create_view(&wgpu::TextureViewDescriptor::default());
        let blur_a = post_texture(device, "litd-editor-world-bloom-blur-a", width, height);
        let blur_a_view = blur_a.create_view(&wgpu::TextureViewDescriptor::default());
        let blur_b = post_texture(device, "litd-editor-world-bloom-blur-b", width, height);
        let blur_b_view = blur_b.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("litd-editor-world-post-uniform"),
            size: std::mem::size_of::<EditorWorldPostUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("litd-editor-world-post-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let blur_layout = blur_bind_group_layout(device);
        let composite_layout = composite_bind_group_layout(device);
        let blur_horizontal_bind_group = blur_bind_group(
            device,
            &blur_layout,
            &emissive_view,
            &sampler,
            &uniform_buffer,
            "litd-editor-world-bloom-horizontal-bind-group",
        );
        let blur_vertical_bind_group = blur_bind_group(
            device,
            &blur_layout,
            &blur_a_view,
            &sampler,
            &uniform_buffer,
            "litd-editor-world-bloom-vertical-bind-group",
        );
        let composite_bind_group = composite_bind_group(
            device,
            &composite_layout,
            &scene_view,
            &blur_b_view,
            &sampler,
            &uniform_buffer,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("litd-editor-world-post-shader"),
            source: wgpu::ShaderSource::Wgsl(EDITOR_WORLD_POST_WGSL.into()),
        });
        let blur_horizontal_pipeline = post_pipeline(
            device,
            &shader,
            "litd-editor-world-bloom-horizontal-pipeline",
            "litd_editor_world_blur_horizontal_fs",
            &[&blur_layout],
            EDITOR_WORLD_HDR_FORMAT,
        );
        let blur_vertical_pipeline = post_pipeline(
            device,
            &shader,
            "litd-editor-world-bloom-vertical-pipeline",
            "litd_editor_world_blur_vertical_fs",
            &[&blur_layout],
            EDITOR_WORLD_HDR_FORMAT,
        );
        let composite_pipeline = post_pipeline(
            device,
            &shader,
            "litd-editor-world-composite-pipeline",
            "litd_editor_world_composite_fs",
            &[&blur_layout, &composite_layout],
            output_format,
        );
        Self {
            width,
            height,
            _scene: scene,
            scene_view,
            _emissive: emissive,
            emissive_view,
            _blur_a: blur_a,
            blur_a_view,
            _blur_b: blur_b,
            blur_b_view,
            uniform_buffer,
            blur_horizontal_bind_group,
            blur_vertical_bind_group,
            composite_bind_group,
            blur_horizontal_pipeline,
            blur_vertical_pipeline,
            composite_pipeline,
        }
    }

    pub(super) fn resize_if_needed(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        output_format: wgpu::TextureFormat,
    ) {
        if self.width != width || self.height != height {
            *self = Self::new(device, width, height, output_format);
        }
    }

    pub(super) fn update_uniform(
        &self,
        queue: &wgpu::Queue,
        layers: EditorWorldLayerVisibility,
        visible_index_count: u32,
    ) {
        let grid_strength = if layers.grid {
            if visible_index_count == 0 {
                0.42
            } else {
                0.055
            }
        } else {
            0.0
        };
        let uniform = EditorWorldPostUniform {
            texel_size: [1.0 / self.width as f32, 1.0 / self.height as f32],
            bloom_intensity: BLOOM_INTENSITY,
            exposure: EXPOSURE,
            display: [
                grid_strength,
                display_flag(layers.sky),
                display_flag(layers.weather),
                display_flag(layers.shadows),
            ],
            grid: [32.0, 128.0, 1.25, display_flag(layers.letterbox_mode)],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub(super) fn scene_view(&self) -> &wgpu::TextureView {
        &self.scene_view
    }

    pub(super) fn emissive_view(&self) -> &wgpu::TextureView {
        &self.emissive_view
    }

    pub(super) fn run_postprocess(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        clear_color: wgpu::Color,
    ) {
        fullscreen_pass(
            encoder,
            "litd-editor-world-bloom-horizontal-pass",
            &self.blur_a_view,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            &self.blur_horizontal_pipeline,
            &self.blur_horizontal_bind_group,
            0,
        );
        fullscreen_pass(
            encoder,
            "litd-editor-world-bloom-vertical-pass",
            &self.blur_b_view,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            &self.blur_vertical_pipeline,
            &self.blur_vertical_bind_group,
            0,
        );
        composite_pass(
            encoder,
            "litd-editor-world-composite-pass",
            output,
            wgpu::LoadOp::Clear(clear_color),
            &self.composite_pipeline,
            &self.blur_vertical_bind_group,
            &self.composite_bind_group,
        );
    }

    pub(super) fn postprocess_pass_count(&self) -> u32 {
        3
    }

    pub(super) fn bloom_pass_count(&self) -> u32 {
        2
    }

    pub(super) fn bloom_intensity_label(&self) -> &'static str {
        BLOOM_INTENSITY_LABEL
    }

    pub(super) fn exposure_label(&self) -> &'static str {
        EXPOSURE_LABEL
    }

    pub(super) fn color_grade_label(&self) -> &'static str {
        COLOR_GRADE_LABEL
    }

    pub(super) fn background_model_label(&self) -> &'static str {
        BACKGROUND_MODEL_LABEL
    }

    pub(super) fn background_profile_label(&self) -> &'static str {
        BACKGROUND_PROFILE_LABEL
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EditorWorldPostUniform {
    texel_size: [f32; 2],
    bloom_intensity: f32,
    exposure: f32,
    display: [f32; 4],
    grid: [f32; 4],
}
