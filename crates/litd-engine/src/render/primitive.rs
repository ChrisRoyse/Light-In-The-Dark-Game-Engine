//! Procedural blockout primitive meshes for the Rust/wgpu renderer.

use std::collections::BTreeSet;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::EngineError;

#[path = "primitive_gpu.rs"]
mod primitive_gpu;
#[path = "primitive_mesh.rs"]
mod primitive_mesh;

pub use primitive_gpu::{
    PrimitiveGpuFrame, PrimitiveGpuPrimitiveReadback, PrimitiveGpuScene, PrimitiveGpuStats,
    render_primitive_gallery_gpu, render_primitive_gallery_gpu_with_context,
};
pub use primitive_mesh::build_primitive_mesh;

const MAX_PRIMITIVES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveTopologyKind {
    TriangleList,
    LineList,
    PointList,
}

impl PrimitiveTopologyKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::TriangleList => "triangle_list",
            Self::LineList => "line_list",
            Self::PointList => "point_list",
        }
    }

    pub(crate) fn wgpu(self) -> wgpu::PrimitiveTopology {
        match self {
            Self::TriangleList => wgpu::PrimitiveTopology::TriangleList,
            Self::LineList => wgpu::PrimitiveTopology::LineList,
            Self::PointList => wgpu::PrimitiveTopology::PointList,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrimitiveKind {
    #[serde(rename = "box")]
    Box3d {
        size: [f32; 3],
    },
    Sphere {
        radius: f32,
        segments: u32,
        rings: u32,
    },
    Cylinder {
        radius: f32,
        height: f32,
        radial_segments: u32,
        height_segments: u32,
    },
    Torus {
        major_radius: f32,
        minor_radius: f32,
        major_segments: u32,
        minor_segments: u32,
    },
    Tube {
        outer_radius: f32,
        inner_radius: f32,
        height: f32,
        radial_segments: u32,
    },
    Plane {
        width: f32,
        depth: f32,
        subdivisions_x: u32,
        subdivisions_z: u32,
    },
    Sprite {
        width: f32,
        height: f32,
    },
    Points {
        points: Vec<[f32; 3]>,
    },
    Lines {
        points: Vec<[f32; 3]>,
    },
}

impl PrimitiveKind {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Box3d { .. } => "box",
            Self::Sphere { .. } => "sphere",
            Self::Cylinder { .. } => "cylinder",
            Self::Torus { .. } => "torus",
            Self::Tube { .. } => "tube",
            Self::Plane { .. } => "plane",
            Self::Sprite { .. } => "sprite",
            Self::Points { .. } => "points",
            Self::Lines { .. } => "lines",
        }
    }

    pub fn topology(&self) -> PrimitiveTopologyKind {
        match self {
            Self::Points { .. } => PrimitiveTopologyKind::PointList,
            Self::Lines { .. } => PrimitiveTopologyKind::LineList,
            _ => PrimitiveTopologyKind::TriangleList,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Pod, Zeroable)]
pub struct PrimitiveVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveSpec {
    pub primitive_id: String,
    pub kind: PrimitiveKind,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveMesh {
    pub primitive_id: String,
    pub kind_id: String,
    pub topology: PrimitiveTopologyKind,
    pub vertices: Vec<PrimitiveVertex>,
    pub indices: Vec<u32>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveGallerySelection {
    pub selected_primitive_id: String,
    pub primitives: Vec<PrimitiveSpec>,
}

impl PrimitiveGallerySelection {
    pub fn blockout_defaults() -> Result<Self, EngineError> {
        let primitives = vec![
            spec(
                "blockout.box",
                PrimitiveKind::Box3d { size: [1.0; 3] },
                [0.60, 0.78, 1.00, 1.0],
            ),
            spec(
                "blockout.sphere",
                PrimitiveKind::Sphere {
                    radius: 0.55,
                    segments: 16,
                    rings: 8,
                },
                [0.95, 0.68, 0.25, 1.0],
            ),
            spec(
                "blockout.cylinder",
                PrimitiveKind::Cylinder {
                    radius: 0.45,
                    height: 1.1,
                    radial_segments: 16,
                    height_segments: 1,
                },
                [0.42, 0.86, 0.56, 1.0],
            ),
            spec(
                "blockout.torus",
                PrimitiveKind::Torus {
                    major_radius: 0.38,
                    minor_radius: 0.13,
                    major_segments: 18,
                    minor_segments: 8,
                },
                [0.84, 0.55, 1.00, 1.0],
            ),
            spec(
                "blockout.tube",
                PrimitiveKind::Tube {
                    outer_radius: 0.48,
                    inner_radius: 0.26,
                    height: 1.0,
                    radial_segments: 16,
                },
                [0.35, 0.90, 0.92, 1.0],
            ),
            spec(
                "blockout.plane",
                PrimitiveKind::Plane {
                    width: 1.15,
                    depth: 0.82,
                    subdivisions_x: 1,
                    subdivisions_z: 1,
                },
                [0.78, 0.70, 0.48, 1.0],
            ),
            spec(
                "blockout.sprite",
                PrimitiveKind::Sprite {
                    width: 0.76,
                    height: 1.04,
                },
                [1.00, 0.42, 0.34, 1.0],
            ),
            spec(
                "blockout.points",
                PrimitiveKind::Points {
                    points: point_grid(),
                },
                [1.00, 0.92, 0.28, 1.0],
            ),
            spec(
                "blockout.lines",
                PrimitiveKind::Lines {
                    points: line_pairs(),
                },
                [0.18, 0.82, 1.00, 1.0],
            ),
        ];
        let gallery = Self {
            selected_primitive_id: "blockout.box".to_string(),
            primitives,
        };
        build_primitive_gallery_readback(&gallery)?;
        Ok(gallery)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveGalleryReadback {
    pub selected_primitive_id: String,
    pub primitive_count: usize,
    pub primitive_ids: Vec<String>,
    pub kind_ids: Vec<String>,
    pub topology_ids: Vec<String>,
    pub total_vertex_count: usize,
    pub total_index_count: usize,
    pub triangle_mesh_count: usize,
    pub line_mesh_count: usize,
    pub point_mesh_count: usize,
    pub meshes: Vec<PrimitiveMeshReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveMeshReadback {
    pub primitive_id: String,
    pub kind_id: String,
    pub topology: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub line_count: usize,
    pub point_count: usize,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub color: [f32; 4],
}

pub fn build_primitive_gallery_readback(
    selection: &PrimitiveGallerySelection,
) -> Result<PrimitiveGalleryReadback, EngineError> {
    if selection.primitives.is_empty() {
        return invalid_primitive("gallery.primitives", "at least one primitive is required");
    }
    if selection.primitives.len() > MAX_PRIMITIVES {
        return invalid_primitive("gallery.primitives", "primitive count must stay <= 64");
    }
    validate_id("selected_primitive_id", &selection.selected_primitive_id)?;
    let mut seen = BTreeSet::new();
    let mut primitive_ids = Vec::with_capacity(selection.primitives.len());
    let mut kind_ids = Vec::with_capacity(selection.primitives.len());
    let mut topology_ids = Vec::with_capacity(selection.primitives.len());
    let mut meshes = Vec::with_capacity(selection.primitives.len());
    let mut total_vertex_count = 0;
    let mut total_index_count = 0;
    let mut topology_counts = [0_usize; 3];
    for primitive in &selection.primitives {
        let mesh = build_primitive_mesh(primitive)?;
        if !seen.insert(primitive.primitive_id.clone()) {
            return invalid_primitive(
                "gallery.primitive_id",
                format!("duplicate primitive_id {}", primitive.primitive_id),
            );
        }
        primitive_ids.push(primitive.primitive_id.clone());
        kind_ids.push(primitive.kind.id().to_string());
        topology_ids.push(primitive.kind.topology().id().to_string());
        total_vertex_count += mesh.vertices.len();
        total_index_count += mesh.indices.len();
        match mesh.topology {
            PrimitiveTopologyKind::TriangleList => topology_counts[0] += 1,
            PrimitiveTopologyKind::LineList => topology_counts[1] += 1,
            PrimitiveTopologyKind::PointList => topology_counts[2] += 1,
        }
        meshes.push(mesh_readback(primitive, &mesh));
    }
    if !seen.contains(&selection.selected_primitive_id) {
        return invalid_primitive(
            "gallery.selected_primitive_id",
            "selected primitive is not present in the gallery",
        );
    }
    Ok(PrimitiveGalleryReadback {
        selected_primitive_id: selection.selected_primitive_id.clone(),
        primitive_count: selection.primitives.len(),
        primitive_ids,
        kind_ids,
        topology_ids,
        total_vertex_count,
        total_index_count,
        triangle_mesh_count: topology_counts[0],
        line_mesh_count: topology_counts[1],
        point_mesh_count: topology_counts[2],
        meshes,
    })
}

fn mesh_readback(spec: &PrimitiveSpec, mesh: &PrimitiveMesh) -> PrimitiveMeshReadback {
    let index_count = mesh.indices.len();
    PrimitiveMeshReadback {
        primitive_id: mesh.primitive_id.clone(),
        kind_id: mesh.kind_id.clone(),
        topology: mesh.topology.id().to_string(),
        vertex_count: mesh.vertices.len(),
        index_count,
        triangle_count: if mesh.topology == PrimitiveTopologyKind::TriangleList {
            index_count / 3
        } else {
            0
        },
        line_count: if mesh.topology == PrimitiveTopologyKind::LineList {
            index_count / 2
        } else {
            0
        },
        point_count: if mesh.topology == PrimitiveTopologyKind::PointList {
            index_count
        } else {
            0
        },
        bounds_min: mesh.bounds_min,
        bounds_max: mesh.bounds_max,
        color: spec.color,
    }
}

pub(crate) fn validate_spec(spec: &PrimitiveSpec) -> Result<(), EngineError> {
    validate_id("primitive_id", &spec.primitive_id)?;
    validate_color(spec.color)?;
    primitive_mesh::validate_kind(&spec.kind)
}

pub(crate) fn invalid_primitive<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(primitive_error(field, reason))
}

pub(crate) fn primitive_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("render.primitive.{}", field.into()),
        reason: reason.into(),
    }
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_primitive(field, "id must be a non-empty single line");
    }
    Ok(())
}

fn validate_color(color: [f32; 4]) -> Result<(), EngineError> {
    if !color
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        return invalid_primitive("color", "RGBA color values must be finite inside [0,1]");
    }
    Ok(())
}

fn spec(id: &str, kind: PrimitiveKind, color: [f32; 4]) -> PrimitiveSpec {
    PrimitiveSpec {
        primitive_id: id.to_string(),
        kind,
        color,
    }
}

fn point_grid() -> Vec<[f32; 3]> {
    (0..4)
        .flat_map(|y| (0..4).map(move |x| [(x as f32 - 1.5) * 0.22, (y as f32 - 1.5) * 0.22, 0.0]))
        .collect()
}

fn line_pairs() -> Vec<[f32; 3]> {
    vec![
        [-0.48, -0.36, 0.0],
        [0.48, -0.36, 0.0],
        [0.48, -0.36, 0.0],
        [0.48, 0.36, 0.0],
        [0.48, 0.36, 0.0],
        [-0.48, 0.36, 0.0],
        [-0.48, 0.36, 0.0],
        [-0.48, -0.36, 0.0],
        [-0.48, -0.36, 0.0],
        [0.48, 0.36, 0.0],
        [-0.48, 0.36, 0.0],
        [0.48, -0.36, 0.0],
    ]
}

#[cfg(test)]
#[path = "primitive_tests.rs"]
mod tests;
