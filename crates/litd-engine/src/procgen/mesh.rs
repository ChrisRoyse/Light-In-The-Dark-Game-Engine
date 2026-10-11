//! Frozen, content-addressed mesh representation for procedural geometry.
//!
//! Procedural generators build a [`ProcMesh`] (position/normal/tangent/UV
//! vertices + a `u32` index buffer). [`ProcMesh::freeze`] lowers it to a
//! [`FrozenMesh`]: two byte buffers (interleaved vertices as `bytemuck` POD
//! bytes, indices as little-endian `u32`) each carrying a SHA-256, plus a
//! single `content_hash` over a canonical block. That byte layout is the frozen
//! format the runtime renderer consumes (a superset of `render::primitive`'s
//! `PrimitiveVertex`, adding a Lengyel 4-component tangent), and the content
//! hash is the address used by the content-addressed store (#1348).

use bytemuck::{Pod, Zeroable};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::tangents::accumulate_tangents;

/// Schema/magic prefix folded into the content hash so meshes from a different
/// layout version can never collide with these under the same address.
pub const FROZEN_MESH_SCHEMA: &str = "litd.procgen.frozen_mesh.v1";

/// Interleaved vertex, `#[repr(C)]` + `Pod` so `bytemuck::cast_slice` yields a
/// deterministic little-endian byte image on this target. 12 × f32 = 48 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Pod, Zeroable)]
pub struct ProcVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// xyz tangent + handedness sign in `w` (Lengyel): bitangent = w·(N×T).
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
}

impl ProcVertex {
    /// Vertex with a zeroed tangent (filled later by [`accumulate_tangents`]).
    pub fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            tangent: [0.0; 4],
            uv,
        }
    }
}

/// In-memory triangle-list mesh prior to freezing.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcMesh {
    pub mesh_id: String,
    pub vertices: Vec<ProcVertex>,
    pub indices: Vec<u32>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

impl ProcMesh {
    pub fn new(mesh_id: impl Into<String>) -> Self {
        Self {
            mesh_id: mesh_id.into(),
            vertices: Vec::new(),
            indices: Vec::new(),
            bounds_min: [0.0; 3],
            bounds_max: [0.0; 3],
        }
    }

    /// Validate, compute bounds, and fill per-vertex tangents. Every generator
    /// funnels through here so the frozen output is uniform and fail-loud.
    pub fn finalize(mut self) -> Result<Self, EngineError> {
        if self.vertices.is_empty() {
            return invalid("mesh.vertices", "a mesh must have at least one vertex");
        }
        if self.indices.is_empty() || !self.indices.len().is_multiple_of(3) {
            return invalid(
                "mesh.indices",
                "triangle-list index count must be non-zero and a multiple of 3",
            );
        }
        let vertex_count = self.vertices.len() as u32;
        for (triangle, window) in self.indices.chunks_exact(3).enumerate() {
            for &index in window {
                if index >= vertex_count {
                    return invalid(
                        "mesh.indices",
                        format!(
                            "triangle {triangle} references vertex {index} but only \
                             {vertex_count} vertices exist"
                        ),
                    );
                }
            }
        }
        for (position, vertex) in self.vertices.iter().enumerate() {
            if !finite3(vertex.position) || !finite3(vertex.normal) || !finite2(vertex.uv) {
                return invalid(
                    "mesh.vertex",
                    format!("vertex {position} has non-finite position/normal/uv"),
                );
            }
        }
        self.compute_bounds();
        accumulate_tangents(&mut self.vertices, &self.indices)?;
        Ok(self)
    }

    fn compute_bounds(&mut self) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in &self.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.position[axis]);
                max[axis] = max[axis].max(vertex.position[axis]);
            }
        }
        self.bounds_min = min;
        self.bounds_max = max;
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Lower to the content-addressed [`FrozenMesh`].
    pub fn freeze(&self) -> Result<FrozenMesh, EngineError> {
        let vertex_bytes = bytemuck::cast_slice::<ProcVertex, u8>(&self.vertices).to_vec();
        let mut index_bytes = Vec::with_capacity(self.indices.len() * 4);
        for index in &self.indices {
            index_bytes.extend_from_slice(&index.to_le_bytes());
        }
        let vertex_sha256 = sha256_hex(&vertex_bytes);
        let index_sha256 = sha256_hex(&index_bytes);
        let content_hash = content_hash(&self.mesh_id, &vertex_bytes, &index_bytes);
        Ok(FrozenMesh {
            schema: FROZEN_MESH_SCHEMA.to_string(),
            mesh_id: self.mesh_id.clone(),
            vertex_count: self.vertices.len(),
            index_count: self.indices.len(),
            triangle_count: self.triangle_count(),
            vertex_stride: std::mem::size_of::<ProcVertex>(),
            bounds_min: self.bounds_min,
            bounds_max: self.bounds_max,
            vertex_bytes,
            index_bytes,
            vertex_sha256,
            index_sha256,
            content_hash,
        })
    }
}

/// Content-addressed frozen mesh: the exact bytes the runtime uploads to the
/// GPU, plus integrity digests. `content_hash` is the store address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenMesh {
    pub schema: String,
    pub mesh_id: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub vertex_stride: usize,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    #[serde(skip)]
    pub vertex_bytes: Vec<u8>,
    #[serde(skip)]
    pub index_bytes: Vec<u8>,
    pub vertex_sha256: String,
    pub index_sha256: String,
    pub content_hash: String,
}

impl FrozenMesh {
    /// The first `count` vertex positions, for FSV assertions against known
    /// geometry without re-parsing the whole byte buffer.
    pub fn sample_positions(&self, count: usize) -> Vec<[f32; 3]> {
        let vertices: &[ProcVertex] = bytemuck::cast_slice(&self.vertex_bytes);
        vertices
            .iter()
            .take(count)
            .map(|vertex| vertex.position)
            .collect()
    }

    /// Decode the interleaved vertices back from the frozen bytes.
    pub fn vertices(&self) -> &[ProcVertex] {
        bytemuck::cast_slice(&self.vertex_bytes)
    }

    /// Decode the `u32` index buffer back from the frozen bytes.
    pub fn indices(&self) -> Vec<u32> {
        self.index_bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    /// Re-verify the digests match the carried bytes (fail-loud FSV).
    pub fn verify(&self) -> Result<(), EngineError> {
        let vertex_actual = sha256_hex(&self.vertex_bytes);
        if vertex_actual != self.vertex_sha256 {
            return Err(EngineError::ProcGenReadbackMismatch {
                field: "frozen_mesh.vertex_sha256".to_string(),
                expected: self.vertex_sha256.clone(),
                actual: vertex_actual,
            });
        }
        let index_actual = sha256_hex(&self.index_bytes);
        if index_actual != self.index_sha256 {
            return Err(EngineError::ProcGenReadbackMismatch {
                field: "frozen_mesh.index_sha256".to_string(),
                expected: self.index_sha256.clone(),
                actual: index_actual,
            });
        }
        let hash_actual = content_hash(&self.mesh_id, &self.vertex_bytes, &self.index_bytes);
        if hash_actual != self.content_hash {
            return Err(EngineError::ProcGenReadbackMismatch {
                field: "frozen_mesh.content_hash".to_string(),
                expected: self.content_hash.clone(),
                actual: hash_actual,
            });
        }
        Ok(())
    }
}

/// SHA-256 over a canonical block: schema magic, id, counts, then both byte
/// buffers. Folding `mesh_id` in keeps otherwise-identical geometry addressed
/// separately when the caller wants distinct logical meshes.
fn content_hash(mesh_id: &str, vertex_bytes: &[u8], index_bytes: &[u8]) -> String {
    let mut block = Vec::with_capacity(vertex_bytes.len() + index_bytes.len() + 64);
    block.extend_from_slice(FROZEN_MESH_SCHEMA.as_bytes());
    block.push(0);
    block.extend_from_slice(mesh_id.as_bytes());
    block.push(0);
    block.extend_from_slice(&(vertex_bytes.len() as u64).to_le_bytes());
    block.extend_from_slice(&(index_bytes.len() as u64).to_le_bytes());
    block.extend_from_slice(vertex_bytes);
    block.extend_from_slice(index_bytes);
    sha256_hex(&block)
}

fn finite3(value: [f32; 3]) -> bool {
    value.iter().all(|component| component.is_finite())
}

fn finite2(value: [f32; 2]) -> bool {
    value.iter().all(|component| component.is_finite())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::ProcGenInvalidConfig {
        field: format!("procgen.{}", field.into()),
        reason: reason.into(),
    })
}
