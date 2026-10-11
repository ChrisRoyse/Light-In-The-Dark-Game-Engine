use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    gpu::GpuContext,
    render::stylized::{StylizedGpuImage, StylizedGpuMetrics},
};

use super::{
    PrimitiveGallerySelection, PrimitiveMesh, PrimitiveTopologyKind, PrimitiveVertex,
    build_primitive_gallery_readback, build_primitive_mesh, primitive_error,
};

#[path = "primitive_gpu_wgpu.rs"]
mod primitive_gpu_wgpu;

use primitive_gpu_wgpu::render_primitive_meshes;

const MAX_TARGET_PIXELS: u64 = 4096 * 4096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveGpuScene {
    pub width: u32,
    pub height: u32,
    pub gallery: PrimitiveGallerySelection,
}

impl PrimitiveGpuScene {
    pub fn blockout_defaults(width: u32, height: u32) -> Result<Self, EngineError> {
        let scene = Self {
            width,
            height,
            gallery: PrimitiveGallerySelection::blockout_defaults()?,
        };
        validate_scene(&scene)?;
        Ok(scene)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveGpuFrame {
    pub image: StylizedGpuImage,
    pub stats: PrimitiveGpuStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub shader_bytes: usize,
    pub target_texture_format: String,
    pub width: u32,
    pub height: u32,
    pub primitive_count: usize,
    pub selected_primitive_id: String,
    pub total_vertex_count: usize,
    pub total_index_count: usize,
    pub triangle_mesh_count: usize,
    pub line_mesh_count: usize,
    pub point_mesh_count: usize,
    pub draw_calls: u32,
    pub rendered_topologies: Vec<String>,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub output_checksum: String,
    pub output_metrics: StylizedGpuMetrics,
    pub primitive_readbacks: Vec<PrimitiveGpuPrimitiveReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveGpuPrimitiveReadback {
    pub primitive_id: String,
    pub kind_id: String,
    pub topology: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub region: [u32; 4],
    pub metrics: StylizedGpuMetrics,
}

pub async fn render_primitive_gallery_gpu(
    scene: &PrimitiveGpuScene,
) -> Result<PrimitiveGpuFrame, EngineError> {
    let context = GpuContext::new_headless().await?;
    render_primitive_gallery_gpu_with_context(&context, scene).await
}

pub async fn render_primitive_gallery_gpu_with_context(
    context: &GpuContext,
    scene: &PrimitiveGpuScene,
) -> Result<PrimitiveGpuFrame, EngineError> {
    validate_scene(scene)?;
    let gallery = build_primitive_gallery_readback(&scene.gallery)?;
    let meshes = scene
        .gallery
        .primitives
        .iter()
        .map(build_primitive_mesh)
        .collect::<Result<Vec<_>, _>>()?;
    let preview_meshes = preview_meshes(scene.width, scene.height, &meshes)?;
    let readback = render_primitive_meshes(context, scene.width, scene.height, &preview_meshes)?;
    let image = StylizedGpuImage::from_pixels(scene.width, scene.height, readback.pixels);
    let primitive_readbacks = meshes
        .iter()
        .zip(readback.regions.iter())
        .map(|(mesh, region)| PrimitiveGpuPrimitiveReadback {
            primitive_id: mesh.primitive_id.clone(),
            kind_id: mesh.kind_id.clone(),
            topology: mesh.topology.id().to_string(),
            vertex_count: mesh.vertices.len(),
            index_count: mesh.indices.len(),
            region: *region,
            metrics: measure_region(&image, *region),
        })
        .collect();
    let output_metrics = measure(&image);
    let stats = PrimitiveGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        shader_bytes: primitive_gpu_wgpu::PRIMITIVE_GALLERY_WGSL.len(),
        target_texture_format: format!("{:?}", wgpu::TextureFormat::Rgba8Unorm),
        width: scene.width,
        height: scene.height,
        primitive_count: gallery.primitive_count,
        selected_primitive_id: gallery.selected_primitive_id,
        total_vertex_count: gallery.total_vertex_count,
        total_index_count: gallery.total_index_count,
        triangle_mesh_count: gallery.triangle_mesh_count,
        line_mesh_count: gallery.line_mesh_count,
        point_mesh_count: gallery.point_mesh_count,
        draw_calls: readback.draw_calls,
        rendered_topologies: readback.rendered_topologies,
        readback_bytes: readback.readback_bytes,
        row_pitch: readback.row_pitch,
        output_checksum: image.checksum(),
        output_metrics,
        primitive_readbacks,
    };
    Ok(PrimitiveGpuFrame { image, stats })
}

fn validate_scene(scene: &PrimitiveGpuScene) -> Result<(), EngineError> {
    if scene.width == 0 || scene.height == 0 {
        return Err(primitive_error(
            "gpu.viewport",
            "width and height must be non-zero",
        ));
    }
    if scene.width as u64 * scene.height as u64 > MAX_TARGET_PIXELS {
        return Err(primitive_error(
            "gpu.viewport",
            "width * height must be <= 4096 * 4096",
        ));
    }
    build_primitive_gallery_readback(&scene.gallery)?;
    Ok(())
}

fn preview_meshes(
    width: u32,
    height: u32,
    meshes: &[PrimitiveMesh],
) -> Result<Vec<PrimitiveMesh>, EngineError> {
    meshes
        .iter()
        .enumerate()
        .map(|(index, mesh)| {
            preview_mesh(
                width,
                height,
                mesh,
                stripe_region(index, meshes.len(), width, height),
            )
        })
        .collect()
}

fn preview_mesh(
    width: u32,
    height: u32,
    mesh: &PrimitiveMesh,
    region: [u32; 4],
) -> Result<PrimitiveMesh, EngineError> {
    let projected = project_bounds(mesh)?;
    let scale = 0.72 / (projected[2].max(projected[3]).max(0.0001));
    let center = [
        projected[0] + projected[2] * 0.5,
        projected[1] + projected[3] * 0.5,
    ];
    let mut out = mesh.clone();
    for vertex in &mut out.vertices {
        let [px, py] = project_point(mesh, vertex.position);
        let local_x = ((px - center[0]) * scale + 0.5).clamp(0.05, 0.95);
        let local_y = (0.5 - (py - center[1]) * scale).clamp(0.05, 0.95);
        let screen_x = region[0] as f32 + region[2] as f32 * local_x;
        let screen_y = region[1] as f32 + region[3] as f32 * local_y;
        vertex.position = [
            screen_x / width as f32 * 2.0 - 1.0,
            1.0 - screen_y / height as f32 * 2.0,
            0.0,
        ];
        if mesh.topology == PrimitiveTopologyKind::PointList {
            vertex.color = [
                vertex.color[0].max(0.95),
                vertex.color[1].max(0.85),
                vertex.color[2],
                1.0,
            ];
        }
    }
    Ok(out)
}

fn project_bounds(mesh: &PrimitiveMesh) -> Result<[f32; 4], EngineError> {
    let mut min = project_point(mesh, mesh.vertices[0].position);
    let mut max = min;
    for vertex in &mesh.vertices {
        let point = project_point(mesh, vertex.position);
        min[0] = min[0].min(point[0]);
        min[1] = min[1].min(point[1]);
        max[0] = max[0].max(point[0]);
        max[1] = max[1].max(point[1]);
    }
    if !min.iter().chain(max.iter()).all(|value| value.is_finite()) {
        return Err(primitive_error(
            "gpu.preview_bounds",
            "projected bounds must be finite",
        ));
    }
    Ok([min[0], min[1], max[0] - min[0], max[1] - min[1]])
}

fn project_point(mesh: &PrimitiveMesh, position: [f32; 3]) -> [f32; 2] {
    match mesh.kind_id.as_str() {
        "plane" | "torus" => [position[0], position[2]],
        _ => [position[0], position[1]],
    }
}

fn stripe_region(index: usize, count: usize, width: u32, height: u32) -> [u32; 4] {
    let x0 = (index as u32 * width) / count as u32;
    let x1 = ((index as u32 + 1) * width) / count as u32;
    [x0, 0, x1 - x0, height]
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

pub(super) fn vertex_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    const ATTRS: [wgpu::VertexAttribute; 2] = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
            offset: 32,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x4,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<PrimitiveVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    }
}
