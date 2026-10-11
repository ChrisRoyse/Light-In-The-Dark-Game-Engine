//! Editor world viewport renderer: real wgpu mesh/depth/fog pass for the
//! center editor viewport. Egui may compose chrome after this pass, but it does
//! not paint the world.

use wgpu::util::DeviceExt;

use crate::{
    EngineError,
    math::{Mat4, Vec3},
};

#[path = "editor_world_evidence.rs"]
mod editor_world_evidence;
#[path = "editor_world_fog.rs"]
mod editor_world_fog;
#[path = "editor_world_foliage.rs"]
mod editor_world_foliage;
#[path = "editor_world_frame.rs"]
mod editor_world_frame;
#[path = "editor_world_gpu.rs"]
mod editor_world_gpu;
#[path = "editor_world_layers.rs"]
mod editor_world_layers;
#[path = "editor_world_package_scene.rs"]
mod editor_world_package_scene;
#[cfg(test)]
#[path = "editor_world_package_scene_tests.rs"]
mod editor_world_package_scene_tests;
#[path = "editor_world_post.rs"]
mod editor_world_post;
#[path = "editor_world_reference_scene.rs"]
mod editor_world_reference_scene;
#[path = "editor_world_scene.rs"]
mod editor_world_scene;
#[path = "editor_world_visual_preset.rs"]
mod editor_world_visual_preset;

pub use editor_world_evidence::EditorWorldRenderEvidence;
use editor_world_evidence::EditorWorldUniform;
pub use editor_world_fog::{EditorWorldFogCell, EditorWorldFogFrame};
pub use editor_world_foliage::{EditorWorldFoliageBuffers, EditorWorldFoliageInstance};
use editor_world_gpu::{
    DepthTarget, GpuEditorWorldTexture, TextureColorSpace, additive_pipeline, bind_group,
    bind_group_layout, foliage_pipeline, pipeline,
};
pub use editor_world_layers::{
    EditorWorldLayerVisibility, EditorWorldSceneLayer, EditorWorldSceneLayerKind,
};
pub use editor_world_package_scene::{PackageWorldSceneBuild, build_package_world_scene};
use editor_world_post::{EDITOR_WORLD_HDR_FORMAT, EditorWorldPost};
use editor_world_reference_scene::build_reference_scene;
pub use editor_world_scene::{
    EditorWorldSceneBuffers, EditorWorldSceneBuffersFullArgs, EditorWorldSceneBuffersNewArgs,
    EditorWorldSceneBuffersWithLayersArgs, EditorWorldSceneMetadata, EditorWorldTextureData,
    EditorWorldVertex,
};
pub use editor_world_visual_preset::{
    DEFAULT_CAMERA_PROFILE_ID, DEFAULT_VISUAL_PRESET_ID, EditorWorldVisualPreset,
    default_editor_world_visual_preset,
};

pub const EDITOR_WORLD_WGSL: &str = include_str!("editor_world.wgsl");
pub const EDITOR_WORLD_POST_WGSL: &str = editor_world_post::EDITOR_WORLD_POST_WGSL;
pub(super) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorViewportRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl EditorViewportRect {
    pub fn from_float_bounds(
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
        surface_width: u32,
        surface_height: u32,
    ) -> Result<Self, EngineError> {
        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            return world_error("viewport_rect", "bounds must be finite");
        }
        if min_x < 0.0 || min_y < 0.0 || max_x <= min_x || max_y <= min_y {
            return world_error("viewport_rect", "bounds must be positive and ordered");
        }
        let x = min_x.ceil() as u32;
        let y = min_y.ceil() as u32;
        let right = max_x.floor() as u32;
        let bottom = max_y.floor() as u32;
        if right > surface_width || bottom > surface_height {
            return world_error(
                "viewport_rect",
                format!(
                    "rect {x},{y}->{right},{bottom} exceeds surface {surface_width}x{surface_height}"
                ),
            );
        }
        Self::new(x, y, right - x, bottom - y)
    }

    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return world_error("viewport_rect", "width and height must be non-zero");
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn aspect_ratio(self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorWorldCameraState {
    pub target_world: Vec3,
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
}

impl EditorWorldCameraState {
    pub fn validate(self) -> Result<(), EngineError> {
        if !self.target_world.is_finite()
            || !self.distance.is_finite()
            || !self.yaw_degrees.is_finite()
            || !self.pitch_degrees.is_finite()
            || self.distance <= 0.0
            || self.pitch_degrees < -89.0
            || self.pitch_degrees > 89.0
        {
            return world_error(
                "camera",
                "target, positive distance, yaw, and pitch -89..89 must be finite and valid",
            );
        }
        Ok(())
    }

    fn eye(self) -> Vec3 {
        let yaw = self.yaw_degrees.to_radians();
        let pitch = self.pitch_degrees.to_radians();
        let horizontal = self.distance * pitch.cos();
        self.target_world
            + Vec3::new(
                horizontal * yaw.sin(),
                -self.distance * pitch.sin(),
                horizontal * yaw.cos(),
            )
    }

    fn view_projection(self, aspect_ratio: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target_world, Vec3::Y);
        let projection = Mat4::perspective_rh(55.0_f32.to_radians(), aspect_ratio, 0.1, 900.0);
        projection * view
    }

    /// Camera matrix shared by the world and packaged-runtime overlay passes.
    pub fn checked_view_projection(self, aspect_ratio: f32) -> Result<Mat4, EngineError> {
        self.validate()?;
        if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
            return world_error(
                "camera.aspect_ratio",
                "aspect ratio must be finite and greater than zero",
            );
        }
        Ok(self.view_projection(aspect_ratio))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorWorldSceneMetrics {
    pub vertex_count: u32,
    pub index_count: u32,
    pub triangle_count: u32,
    pub mesh_primitives: u32,
}

pub struct EditorWorldRenderer {
    pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    foliage_pipeline: wgpu::RenderPipeline,
    fog: editor_world_fog::EditorWorldFogGpu,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    _base_color_texture: GpuEditorWorldTexture,
    _normal_texture: GpuEditorWorldTexture,
    _metallic_roughness_texture: GpuEditorWorldTexture,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    foliage_vertex_buffer: Option<wgpu::Buffer>,
    foliage_index_buffer: Option<wgpu::Buffer>,
    foliage_instance_buffer: Option<wgpu::Buffer>,
    scene: EditorWorldSceneMetrics,
    scene_metadata: EditorWorldSceneMetadata,
    scene_layers: Vec<EditorWorldSceneLayer>,
    foliage_template_index_count: u32,
    foliage_instance_count: u32,
    depth: DepthTarget,
    post: EditorWorldPost,
    surface_format: wgpu::TextureFormat,
    // #1485 finding 3: inputs from the last `prepare_frame_metrics`, so the allocation-heavy
    // `EditorWorldRenderEvidence` is rebuilt on demand (`last_frame_evidence`) at snapshot cadence
    // instead of every frame. `None` until the first frame is prepared.
    last_frame: Option<editor_world_frame::LastFramePrep>,
}

pub use editor_world_frame::{EditorWorldFrameMetrics, PrepareFrameArgs, PrepareFrameMetricsArgs};

impl EditorWorldRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        surface_width: u32,
        surface_height: u32,
    ) -> Result<Self, EngineError> {
        if surface_width == 0 || surface_height == 0 {
            return world_error("surface", "surface size must be non-zero");
        }
        Self::new_with_scene(
            device,
            queue,
            surface_format,
            surface_width,
            surface_height,
            build_reference_scene(),
        )
    }

    pub fn new_with_scene(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        surface_width: u32,
        surface_height: u32,
        scene_buffers: EditorWorldSceneBuffers,
    ) -> Result<Self, EngineError> {
        if surface_width == 0 || surface_height == 0 {
            return world_error("surface", "surface size must be non-zero");
        }
        let scene = scene_buffers.metrics()?;
        let scene_metadata = scene_buffers.metadata();
        default_editor_world_visual_preset().validate_scene_metadata(&scene_metadata)?;
        let scene_layers = scene_buffers.layers().to_vec();
        let foliage_template_index_count = scene_buffers.foliage().template_index_count();
        let foliage_instance_count = scene_buffers.foliage().instance_count();
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("litd-editor-world-uniform"),
            size: std::mem::size_of::<EditorWorldUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout = bind_group_layout(device);
        let base_color_texture = GpuEditorWorldTexture::new(
            device,
            queue,
            scene_buffers.base_color_texture(),
            TextureColorSpace::Srgb,
        )?;
        let normal_texture = GpuEditorWorldTexture::new(
            device,
            queue,
            scene_buffers.normal_texture(),
            TextureColorSpace::Linear,
        )?;
        let metallic_roughness_texture = GpuEditorWorldTexture::new(
            device,
            queue,
            scene_buffers.metallic_roughness_texture(),
            TextureColorSpace::Linear,
        )?;
        let uniform_bind_group = bind_group(
            device,
            &bind_group_layout,
            &uniform_buffer,
            &base_color_texture,
            &normal_texture,
            &metallic_roughness_texture,
        );
        let fog = editor_world_fog::EditorWorldFogGpu::new(device, queue);
        let pipeline = pipeline(
            device,
            &bind_group_layout,
            fog.layout(),
            EDITOR_WORLD_HDR_FORMAT,
        );
        let additive_pipeline = additive_pipeline(
            device,
            &bind_group_layout,
            fog.layout(),
            EDITOR_WORLD_HDR_FORMAT,
        );
        let foliage_pipeline = foliage_pipeline(
            device,
            &bind_group_layout,
            fog.layout(),
            EDITOR_WORLD_HDR_FORMAT,
        );
        let post = EditorWorldPost::new(device, surface_width, surface_height, surface_format);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("litd-editor-world-vertices"),
            contents: bytemuck::cast_slice(&scene_buffers.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("litd-editor-world-indices"),
            contents: bytemuck::cast_slice(&scene_buffers.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let foliage = scene_buffers.foliage();
        let foliage_vertex_buffer = foliage.has_instances().then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("litd-editor-world-foliage-template-vertices"),
                contents: bytemuck::cast_slice(foliage.vertices()),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let foliage_index_buffer = foliage.has_instances().then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("litd-editor-world-foliage-template-indices"),
                contents: bytemuck::cast_slice(foliage.indices()),
                usage: wgpu::BufferUsages::INDEX,
            })
        });
        let foliage_instance_buffer = foliage.has_instances().then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("litd-editor-world-foliage-instances"),
                contents: bytemuck::cast_slice(foliage.instances()),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        Ok(Self {
            pipeline,
            additive_pipeline,
            foliage_pipeline,
            fog,
            uniform_buffer,
            uniform_bind_group,
            _base_color_texture: base_color_texture,
            _normal_texture: normal_texture,
            _metallic_roughness_texture: metallic_roughness_texture,
            vertex_buffer,
            index_buffer,
            foliage_vertex_buffer,
            foliage_index_buffer,
            foliage_instance_buffer,
            scene,
            scene_metadata,
            scene_layers,
            foliage_template_index_count,
            foliage_instance_count,
            depth: DepthTarget::new(device, surface_width, surface_height),
            post,
            surface_format,
            last_frame: None,
        })
    }
}

pub fn reference_editor_world_scene_metrics() -> Result<EditorWorldSceneMetrics, EngineError> {
    build_reference_scene().metrics()
}

fn world_error<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::EditorWorldRendererInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "editor_world_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "editor_world_visual_preset_tests.rs"]
mod visual_preset_tests;
