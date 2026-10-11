//! Revolved parametric primitive generators (cylinder, sphere, torus).

use std::f32::consts::{PI, TAU};

use crate::EngineError;

use super::mesh::ProcMesh;
use super::primitives::{normalize3, push_tri, push_vertex};
use super::spec::PrimitiveSpec;

pub(super) fn build(spec: &PrimitiveSpec, mesh_id: &str) -> Result<ProcMesh, EngineError> {
    let mut mesh = ProcMesh::new(mesh_id);
    match spec {
        PrimitiveSpec::Cylinder {
            radius,
            height,
            radial_segments,
            height_segments,
        } => build_cylinder(
            &mut mesh,
            *radius,
            *height,
            *radial_segments,
            *height_segments,
        ),
        PrimitiveSpec::Sphere {
            radius,
            segments,
            rings,
        } => build_sphere(&mut mesh, *radius, *segments, *rings),
        PrimitiveSpec::Torus {
            major_radius,
            minor_radius,
            major_segments,
            minor_segments,
        } => build_torus(
            &mut mesh,
            *major_radius,
            *minor_radius,
            *major_segments,
            *minor_segments,
        ),
        other => {
            return super::mesh::invalid(
                "primitive.kind",
                format!("{} is not built here", other.kind_id()),
            );
        }
    }
    mesh.finalize()
}

fn build_cylinder(mesh: &mut ProcMesh, radius: f32, height: f32, radial: u32, rows: u32) {
    let half = height * 0.5;
    // Side wall: a (radial+1) x (rows+1) grid so UVs seam cleanly.
    let base = mesh.vertices.len() as u32;
    for row in 0..=rows {
        let v = row as f32 / rows as f32;
        let y = -half + v * height;
        for seg in 0..=radial {
            let u = seg as f32 / radial as f32;
            let (s, c) = (u * TAU).sin_cos();
            push_vertex(mesh, [radius * c, y, radius * s], [c, 0.0, s], [u, v]);
        }
    }
    let stride = radial + 1;
    for row in 0..rows {
        for seg in 0..radial {
            let a = base + row * stride + seg;
            push_tri(mesh, a, a + stride, a + stride + 1);
            push_tri(mesh, a, a + stride + 1, a + 1);
        }
    }
    append_disc(mesh, radius, half, radial, true);
    append_disc(mesh, radius, -half, radial, false);
}

/// A capping disc at height `y`. `up` selects the +Y or -Y facing and winding.
fn append_disc(mesh: &mut ProcMesh, radius: f32, y: f32, radial: u32, up: bool) {
    let normal = if up {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, -1.0, 0.0]
    };
    let center = push_vertex(mesh, [0.0, y, 0.0], normal, [0.5, 0.5]);
    let mut ring = Vec::with_capacity(radial as usize + 1);
    for seg in 0..=radial {
        let a = seg as f32 / radial as f32 * TAU;
        let (s, c) = a.sin_cos();
        ring.push(push_vertex(
            mesh,
            [radius * c, y, radius * s],
            normal,
            [0.5 + 0.5 * c, 0.5 + 0.5 * s],
        ));
    }
    for seg in 0..radial as usize {
        if up {
            push_tri(mesh, center, ring[seg], ring[seg + 1]);
        } else {
            push_tri(mesh, center, ring[seg + 1], ring[seg]);
        }
    }
}

fn build_sphere(mesh: &mut ProcMesh, radius: f32, segments: u32, rings: u32) {
    let base = mesh.vertices.len() as u32;
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let theta = PI * v;
        let (sin_t, cos_t) = theta.sin_cos();
        for seg in 0..=segments {
            let u = seg as f32 / segments as f32;
            let (sin_p, cos_p) = (TAU * u).sin_cos();
            let normal = [sin_t * cos_p, cos_t, sin_t * sin_p];
            push_vertex(
                mesh,
                [normal[0] * radius, normal[1] * radius, normal[2] * radius],
                normal,
                [u, v],
            );
        }
    }
    let stride = segments + 1;
    for ring in 0..rings {
        for seg in 0..segments {
            let a = base + ring * stride + seg;
            push_tri(mesh, a, a + stride, a + 1);
            push_tri(mesh, a + 1, a + stride, a + stride + 1);
        }
    }
}

fn build_torus(mesh: &mut ProcMesh, major: f32, minor: f32, major_seg: u32, minor_seg: u32) {
    let base = mesh.vertices.len() as u32;
    for i in 0..=major_seg {
        let u = i as f32 / major_seg as f32;
        let (su, cu) = (TAU * u).sin_cos();
        for j in 0..=minor_seg {
            let v = j as f32 / minor_seg as f32;
            let (sv, cv) = (TAU * v).sin_cos();
            let normal = normalize3([cu * cv, sv, su * cv]);
            let center = [major * cu, 0.0, major * su];
            push_vertex(
                mesh,
                [
                    center[0] + minor * normal[0],
                    minor * normal[1],
                    center[2] + minor * normal[2],
                ],
                normal,
                [u, v],
            );
        }
    }
    let stride = minor_seg + 1;
    for i in 0..major_seg {
        for j in 0..minor_seg {
            let a = base + i * stride + j;
            push_tri(mesh, a, a + stride, a + 1);
            push_tri(mesh, a + 1, a + stride, a + stride + 1);
        }
    }
}
