use std::f32::consts::TAU;

use crate::EngineError;

use super::super::{PrimitiveMesh, PrimitiveTopologyKind, PrimitiveVertex, invalid_primitive};

pub(super) fn append_cylinder_side(
    mesh: &mut PrimitiveMesh,
    radius: f32,
    height: f32,
    radial: u32,
    rows: u32,
    outward: bool,
    color: [f32; 4],
) -> Result<(), EngineError> {
    let base = mesh.vertices.len() as u32;
    for row in 0..=rows {
        let y = -height * 0.5 + height * row as f32 / rows as f32;
        for seg in 0..=radial {
            let u = seg as f32 / radial as f32;
            let (s, c) = (TAU * u).sin_cos();
            let normal = if outward { [c, 0.0, s] } else { [-c, 0.0, -s] };
            mesh.vertices.push(vertex(
                [radius * c, y, radius * s],
                normal,
                [u, row as f32 / rows as f32],
                color,
            ));
        }
    }
    let stride = radial + 1;
    for row in 0..rows {
        for seg in 0..radial {
            let a = base + row * stride + seg;
            if outward {
                mesh.indices.extend_from_slice(&[
                    a,
                    a + stride,
                    a + 1,
                    a + 1,
                    a + stride,
                    a + stride + 1,
                ]);
            } else {
                mesh.indices.extend_from_slice(&[
                    a,
                    a + 1,
                    a + stride,
                    a + 1,
                    a + stride + 1,
                    a + stride,
                ]);
            }
        }
    }
    Ok(())
}

pub(super) fn append_disc(
    mesh: &mut PrimitiveMesh,
    radius: f32,
    y: f32,
    radial: u32,
    top: bool,
    color: [f32; 4],
) -> Result<(), EngineError> {
    let center = mesh.vertices.len() as u32;
    let normal = if top {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, -1.0, 0.0]
    };
    mesh.vertices
        .push(vertex([0.0, y, 0.0], normal, [0.5, 0.5], color));
    for seg in 0..radial {
        let u = seg as f32 / radial as f32;
        let (s, c) = (TAU * u).sin_cos();
        mesh.vertices.push(vertex(
            [radius * c, y, radius * s],
            normal,
            [(c + 1.0) * 0.5, (s + 1.0) * 0.5],
            color,
        ));
    }
    for seg in 0..radial {
        let a = center + 1 + seg;
        let b = center + 1 + ((seg + 1) % radial);
        if top {
            mesh.indices.extend_from_slice(&[center, a, b]);
        } else {
            mesh.indices.extend_from_slice(&[center, b, a]);
        }
    }
    Ok(())
}

pub(super) fn append_annulus(
    mesh: &mut PrimitiveMesh,
    outer: f32,
    inner: f32,
    y: f32,
    radial: u32,
    top: bool,
    color: [f32; 4],
) -> Result<(), EngineError> {
    let base = mesh.vertices.len() as u32;
    let normal = if top {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, -1.0, 0.0]
    };
    for seg in 0..=radial {
        let u = seg as f32 / radial as f32;
        let (s, c) = (TAU * u).sin_cos();
        mesh.vertices
            .push(vertex([outer * c, y, outer * s], normal, [u, 1.0], color));
        mesh.vertices
            .push(vertex([inner * c, y, inner * s], normal, [u, 0.0], color));
    }
    for seg in 0..radial {
        let a = base + seg * 2;
        if top {
            mesh.indices
                .extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        } else {
            mesh.indices
                .extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
    }
    Ok(())
}

pub(super) fn quad(
    mesh: &mut PrimitiveMesh,
    points: [[f32; 3]; 4],
    normal: [f32; 3],
    color: [f32; 4],
) -> Result<(), EngineError> {
    let base = mesh.vertices.len() as u32;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for (point, uv) in points.into_iter().zip(uvs) {
        mesh.vertices.push(vertex(point, normal, uv, color));
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    Ok(())
}

pub(super) fn finalize(mut mesh: PrimitiveMesh) -> Result<PrimitiveMesh, EngineError> {
    if mesh.vertices.is_empty() || mesh.vertices.len() > 131_072 {
        return invalid_primitive("mesh.vertices", "mesh vertex count is outside 1..=131072");
    }
    if mesh
        .indices
        .iter()
        .any(|index| *index as usize >= mesh.vertices.len())
    {
        return invalid_primitive("mesh.indices", "mesh contains an out-of-range index");
    }
    if mesh.topology == PrimitiveTopologyKind::TriangleList && !mesh.indices.len().is_multiple_of(3)
    {
        return invalid_primitive("mesh.indices", "triangle-list index count must divide by 3");
    }
    if mesh.topology == PrimitiveTopologyKind::LineList && !mesh.indices.len().is_multiple_of(2) {
        return invalid_primitive("mesh.indices", "line-list index count must divide by 2");
    }
    let (min, max) = bounds(&mesh.vertices)?;
    mesh.bounds_min = min;
    mesh.bounds_max = max;
    Ok(mesh)
}

fn bounds(vertices: &[PrimitiveVertex]) -> Result<([f32; 3], [f32; 3]), EngineError> {
    let mut min = vertices[0].position;
    let mut max = min;
    for vertex in vertices {
        for axis in 0..3 {
            let value = vertex.position[axis];
            if !value.is_finite() {
                return invalid_primitive("mesh.position", "vertex position must be finite");
            }
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    Ok((min, max))
}

pub(super) fn vertex(
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
) -> PrimitiveVertex {
    PrimitiveVertex {
        position,
        normal,
        uv,
        color,
    }
}

pub(super) fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}
