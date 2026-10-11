//! Procedural geometry generation for the Rust/wgpu renderer (#1482).
//!
//! In-engine construction of native geometry — parametric primitives, terrain
//! heightfields, and deterministic doodad scatter — with normals, Lengyel
//! tangents, and UVs, lowered to a content-addressed frozen mesh format the
//! runtime renderer consumes. This is the "build primitives from parameters"
//! complement to authored-asset generation (#782) and glTF import (#1099); it
//! requires no external DCC pipeline.
//!
//! ## Determinism
//! Every output is a pure function of its spec (scatter also of an integer
//! `seed`): only IEEE-754 `f32` arithmetic and a `splitmix64` bit-mixer are
//! used — no wall-clock, no OS RNG — so identical inputs yield byte-identical
//! buffers on a given target and are safe to run inside the deterministic sim
//! and to content-address (#1348).
//!
//! ## Layout
//! - [`spec`] / [`generate_primitive`] — parametric box/plane/cylinder/sphere/
//!   cone/torus.
//! - [`heightfield`] — terrain heightfield -> mesh with per-tile LOD + splat.
//! - [`scatter`] — deterministic instance placement.
//! - [`mesh`] — the frozen, content-addressed [`FrozenMesh`] format.
//! - [`store`] — content-addressed on-disk persistence + verifying readback.

pub mod heightfield;
pub mod mesh;
mod primitives;
mod primitives_round;
pub mod scatter;
pub mod spec;
pub mod store;
mod tangents;

pub use heightfield::{HeightfieldProduct, HeightfieldSpec, SplatBand, generate_heightfield};
pub use mesh::{FROZEN_MESH_SCHEMA, FrozenMesh, ProcMesh, ProcVertex};
pub use scatter::{
    SCATTER_SCHEMA, ScatterInstance, ScatterProduct, ScatterPrototype, ScatterSpec,
    generate_scatter,
};
pub use spec::{MAX_SUBDIVISIONS, MIN_EXTENT, PrimitiveSpec};
pub use store::{ProcGenStore, StoredMeshRef};

use crate::EngineError;

/// Generate a parametric primitive mesh from `spec`, addressed as `mesh_id`.
///
/// The spec is validated first (zero-size extents, out-of-range subdivisions,
/// and degenerate parameters are rejected loudly), then geometry is emitted and
/// finalized (bounds + tangents). Call [`ProcMesh::freeze`] to lower the result
/// to the content-addressed [`FrozenMesh`].
pub fn generate_primitive(spec: &PrimitiveSpec, mesh_id: &str) -> Result<ProcMesh, EngineError> {
    if mesh_id.trim().is_empty() {
        return mesh::invalid("primitive.mesh_id", "mesh_id must be non-empty");
    }
    spec.validate()?;
    match spec {
        PrimitiveSpec::Box { .. } | PrimitiveSpec::Plane { .. } | PrimitiveSpec::Cone { .. } => {
            primitives::build(spec, mesh_id)
        }
        PrimitiveSpec::Cylinder { .. }
        | PrimitiveSpec::Sphere { .. }
        | PrimitiveSpec::Torus { .. } => primitives_round::build(spec, mesh_id),
    }
}

/// Convenience: generate a primitive and immediately freeze it.
pub fn generate_primitive_frozen(
    spec: &PrimitiveSpec,
    mesh_id: &str,
) -> Result<FrozenMesh, EngineError> {
    generate_primitive(spec, mesh_id)?.freeze()
}

#[cfg(test)]
#[path = "procgen/tests.rs"]
mod tests;
