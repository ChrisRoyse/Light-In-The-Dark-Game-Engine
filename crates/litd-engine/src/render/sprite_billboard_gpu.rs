use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    gpu::GpuContext,
    render::stylized::{StylizedGpuImage, StylizedGpuMetrics},
};

use super::{
    SpriteBillboardLibrarySelection, build_sprite_billboard_library_readback,
    sprite_billboard_error,
};

#[path = "sprite_billboard_gpu_wgpu.rs"]
mod sprite_billboard_gpu_wgpu;

const MAX_TARGET_PIXELS: u64 = 4096 * 4096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardGpuScene {
    pub width: u32,
    pub height: u32,
    pub library: SpriteBillboardLibrarySelection,
}

impl SpriteBillboardGpuScene {
    pub fn issue868_defaults(width: u32, height: u32) -> Result<Self, EngineError> {
        let scene = Self {
            width,
            height,
            library: SpriteBillboardLibrarySelection::issue868_defaults()?,
        };
        validate_scene(&scene)?;
        Ok(scene)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardGpuFrame {
    pub image: StylizedGpuImage,
    pub stats: SpriteBillboardGpuStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub shader_bytes: usize,
    pub target_texture_format: String,
    pub width: u32,
    pub height: u32,
    pub library_id: String,
    pub selected_billboard_id: String,
    pub texture_count: usize,
    pub material_count: usize,
    pub sheet_count: usize,
    pub billboard_count: usize,
    pub search_row_count: usize,
    pub texture_content_addresses: Vec<String>,
    pub total_vertex_count: usize,
    pub total_index_count: usize,
    pub triangle_count: usize,
    pub draw_calls: u32,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub output_checksum: String,
    pub output_metrics: StylizedGpuMetrics,
    pub billboard_readbacks: Vec<SpriteBillboardGpuBillboardReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardGpuBillboardReadback {
    pub billboard_id: String,
    pub material_id: String,
    pub texture_id: String,
    pub texture_content_address: String,
    pub frame_uv: [f32; 4],
    pub region: [u32; 4],
    pub metrics: StylizedGpuMetrics,
}

pub async fn render_sprite_billboard_library_gpu(
    scene: &SpriteBillboardGpuScene,
) -> Result<SpriteBillboardGpuFrame, EngineError> {
    let context = GpuContext::new_headless().await?;
    render_sprite_billboard_library_gpu_with_context(&context, scene).await
}

pub async fn render_sprite_billboard_library_gpu_with_context(
    context: &GpuContext,
    scene: &SpriteBillboardGpuScene,
) -> Result<SpriteBillboardGpuFrame, EngineError> {
    validate_scene(scene)?;
    let library = build_sprite_billboard_library_readback(&scene.library)?;
    let readback = sprite_billboard_gpu_wgpu::render_sprite_billboards(
        context,
        scene.width,
        scene.height,
        &scene.library,
        &library,
    )?;
    let image = StylizedGpuImage::from_pixels(scene.width, scene.height, readback.pixels);
    let billboard_readbacks = library
        .meshes
        .iter()
        .zip(readback.regions.iter())
        .map(|(mesh, region)| SpriteBillboardGpuBillboardReadback {
            billboard_id: mesh.billboard_id.clone(),
            material_id: mesh.material_id.clone(),
            texture_id: mesh.texture_id.clone(),
            texture_content_address: mesh.texture_content_address.clone(),
            frame_uv: mesh.frame_uv,
            region: *region,
            metrics: measure_region(&image, *region),
        })
        .collect();
    let stats = SpriteBillboardGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        shader_bytes: sprite_billboard_gpu_wgpu::SPRITE_BILLBOARD_WGSL.len(),
        target_texture_format: format!("{:?}", wgpu::TextureFormat::Rgba8Unorm),
        width: scene.width,
        height: scene.height,
        library_id: library.library_id,
        selected_billboard_id: library.selected_billboard_id,
        texture_count: library.texture_count,
        material_count: library.material_count,
        sheet_count: library.sheet_count,
        billboard_count: library.billboard_count,
        search_row_count: library.search_row_count,
        texture_content_addresses: library.texture_content_addresses,
        total_vertex_count: library.total_vertex_count,
        total_index_count: library.total_index_count,
        triangle_count: library.triangle_count,
        draw_calls: readback.draw_calls,
        atlas_width: readback.atlas_width,
        atlas_height: readback.atlas_height,
        readback_bytes: readback.readback_bytes,
        row_pitch: readback.row_pitch,
        output_checksum: image.checksum(),
        output_metrics: measure(&image),
        billboard_readbacks,
    };
    Ok(SpriteBillboardGpuFrame { image, stats })
}

fn validate_scene(scene: &SpriteBillboardGpuScene) -> Result<(), EngineError> {
    if scene.width == 0 || scene.height == 0 {
        return Err(sprite_billboard_error(
            "gpu.viewport",
            "width and height must be non-zero",
        ));
    }
    if scene.width as u64 * scene.height as u64 > MAX_TARGET_PIXELS {
        return Err(sprite_billboard_error(
            "gpu.viewport",
            "width * height must be <= 4096 * 4096",
        ));
    }
    build_sprite_billboard_library_readback(&scene.library)?;
    Ok(())
}

fn measure(image: &StylizedGpuImage) -> StylizedGpuMetrics {
    measure_region(image, [0, 0, image.width, image.height])
}

fn measure_region(image: &StylizedGpuImage, region: [u32; 4]) -> StylizedGpuMetrics {
    let [x0, y0, width, height] = region;
    let mut luma_sum = 0.0;
    let mut edge_sum = 0.0;
    let mut edge_count = 0_u32;
    let mut hot = 0_u32;
    let mut cyan = 0_u32;
    let mut nonblack = 0_u32;
    let mut opaque = 0_u32;
    for y in y0..(y0 + height).min(image.height) {
        for x in x0..(x0 + width).min(image.width) {
            let [r, g, b, a] = image.pixels()[(y * image.width + x) as usize];
            let pixel_luma = luma(r, g, b);
            luma_sum += pixel_luma;
            hot += u32::from(r > 210 && g > 80);
            cyan += u32::from(b > 150 && g > 120);
            nonblack += u32::from(r > 8 || g > 8 || b > 8);
            opaque += u32::from(a == 255);
            if x + 1 < (x0 + width).min(image.width) {
                let [nr, ng, nb, _] = image.pixels()[(y * image.width + x + 1) as usize];
                edge_sum += (pixel_luma - luma(nr, ng, nb)).abs();
                edge_count += 1;
            }
        }
    }
    let total = (width * height).max(1) as f32;
    StylizedGpuMetrics {
        average_luma: luma_sum / total,
        edge_contrast: edge_sum / edge_count.max(1) as f32,
        hot_pixel_ratio: hot as f32 / total,
        cyan_pixel_ratio: cyan as f32 / total,
        nonblack_pixel_ratio: nonblack as f32 / total,
        alpha_opaque_ratio: opaque as f32 / total,
    }
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * r as f32 / 255.0 + 0.7152 * g as f32 / 255.0 + 0.0722 * b as f32 / 255.0
}
