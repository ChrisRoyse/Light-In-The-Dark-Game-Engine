//! Flat-and-conic parametric primitive generators (box, plane, cone).
//!
//! All generators emit outward normals, `[0,1]` UVs, and counter-clockwise
//! front faces. Tangents are filled by [`ProcMesh::finalize`]; callers reach
//! these through [`super::generate_primitive`].

use std::f32::consts::TAU;

use crate::EngineError;

use super::mesh::ProcMesh;
use super::spec::PrimitiveSpec;

/// Push one vertex, returning its index.
pub(super) fn push_vertex(
    mesh: &mut ProcMesh,
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
) -> u32 {
    let index = mesh.vertices.len() as u32;
    mesh.vertices
        .push(super::mesh::ProcVertex::new(position, normal, uv));
    index
}

/// Push a triangle by existing vertex indices (CCW as given).
pub(super) fn push_tri(mesh: &mut ProcMesh, a: u32, b: u32, c: u32) {
    mesh.indices.extend_from_slice(&[a, b, c]);
}

/// Push a planar quad (4 corners CCW) with a shared face normal and corner UVs
/// (0,0)-(1,0)-(1,1)-(0,1).
pub(super) fn push_quad(mesh: &mut ProcMesh, corners: [[f32; 3]; 4], normal: [f32; 3]) {
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let base = mesh.vertices.len() as u32;
    for (corner, uv) in corners.iter().zip(uvs) {
        push_vertex(mesh, *corner, normal, uv);
    }
    push_tri(mesh, base, base + 1, base + 2);
    push_tri(mesh, base, base + 2, base + 3);
}

pub(super) fn build(spec: &PrimitiveSpec, mesh_id: &str) -> Result<ProcMesh, EngineError> {
    let mut mesh = ProcMesh::new(mesh_id);
    match spec {
        PrimitiveSpec::Box { size } => build_box(&mut mesh, *size),
        PrimitiveSpec::Plane {
            width,
            depth,
            subdivisions_x,
            subdivisions_z,
        } => build_plane(&mut mesh, *width, *depth, *subdivisions_x, *subdivisions_z),
        PrimitiveSpec::Cone {
            radius,
            height,
            radial_segments,
        } => build_cone(&mut mesh, *radius, *height, *radial_segments),
        other => {
            return super::mesh::invalid(
                "primitive.kind",
                format!("{} is not built here", other.kind_id()),
            );
        }
    }
    mesh.finalize()
}

fn build_box(mesh: &mut ProcMesh, size: [f32; 3]) {
    let [hx, hy, hz] = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    // Each face: 4 CCW corners viewed from outside + outward normal.
    let faces: [([[f32; 3]; 4], [f32; 3]); 6] = [
        (
            [[hx, -hy, -hz], [hx, -hy, hz], [hx, hy, hz], [hx, hy, -hz]],
            [1.0, 0.0, 0.0],
        ),
        (
            [
                [-hx, -hy, hz],
                [-hx, -hy, -hz],
                [-hx, hy, -hz],
                [-hx, hy, hz],
            ],
            [-1.0, 0.0, 0.0],
        ),
        (
            [[-hx, hy, -hz], [hx, hy, -hz], [hx, hy, hz], [-hx, hy, hz]],
            [0.0, 1.0, 0.0],
        ),
        (
            [
                [-hx, -hy, hz],
                [hx, -hy, hz],
                [hx, -hy, -hz],
                [-hx, -hy, -hz],
            ],
            [0.0, -1.0, 0.0],
        ),
        (
            [[-hx, -hy, hz], [-hx, hy, hz], [hx, hy, hz], [hx, -hy, hz]],
            [0.0, 0.0, 1.0],
        ),
        (
            [
                [hx, -hy, -hz],
                [hx, hy, -hz],
                [-hx, hy, -hz],
                [-hx, -hy, -hz],
            ],
            [0.0, 0.0, -1.0],
        ),
    ];
    for (corners, normal) in faces {
        push_quad(mesh, corners, normal);
    }
}

fn build_plane(mesh: &mut ProcMesh, width: f32, depth: f32, sx: u32, sz: u32) {
    for z in 0..=sz {
        for x in 0..=sx {
            let u = x as f32 / sx as f32;
            let v = z as f32 / sz as f32;
            push_vertex(
                mesh,
                [(u - 0.5) * width, 0.0, (v - 0.5) * depth],
                [0.0, 1.0, 0.0],
                [u, v],
            );
        }
    }
    let row = sx + 1;
    for z in 0..sz {
        for x in 0..sx {
            let a = z * row + x;
            push_tri(mesh, a, a + row, a + row + 1);
            push_tri(mesh, a, a + row + 1, a + 1);
        }
    }
}

fn build_cone(mesh: &mut ProcMesh, radius: f32, height: f32, radial: u32) {
    let half = height * 0.5;
    let apex_y = half;
    let base_y = -half;
    // Slant side: per segment, two base ring vertices with their own outward
    // normal and one apex vertex carrying the mid-angle normal (flat facets).
    for seg in 0..radial {
        let t0 = seg as f32 / radial as f32;
        let t1 = (seg + 1) as f32 / radial as f32;
        let a0 = t0 * TAU;
        let a1 = t1 * TAU;
        let am = (a0 + a1) * 0.5;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        let n0 = normalize3([height * c0, radius, height * s0]);
        let n1 = normalize3([height * c1, radius, height * s1]);
        let nm = {
            let (sm, cm) = am.sin_cos();
            normalize3([height * cm, radius, height * sm])
        };
        let v0 = push_vertex(mesh, [radius * c0, base_y, radius * s0], n0, [t0, 0.0]);
        let v1 = push_vertex(mesh, [radius * c1, base_y, radius * s1], n1, [t1, 0.0]);
        let apex = push_vertex(mesh, [0.0, apex_y, 0.0], nm, [(t0 + t1) * 0.5, 1.0]);
        push_tri(mesh, v0, apex, v1);
    }
    // Base cap facing down.
    let center = push_vertex(mesh, [0.0, base_y, 0.0], [0.0, -1.0, 0.0], [0.5, 0.5]);
    let mut ring = Vec::with_capacity(radial as usize + 1);
    for seg in 0..=radial {
        let a = seg as f32 / radial as f32 * TAU;
        let (s, c) = a.sin_cos();
        ring.push(push_vertex(
            mesh,
            [radius * c, base_y, radius * s],
            [0.0, -1.0, 0.0],
            [0.5 + 0.5 * c, 0.5 + 0.5 * s],
        ));
    }
    for seg in 0..radial as usize {
        push_tri(mesh, center, ring[seg + 1], ring[seg]);
    }
}

pub(super) fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}
