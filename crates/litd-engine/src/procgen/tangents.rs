//! Per-vertex tangent-space generation, Lengyel's method.
//!
//! Reference: Eric Lengyel, "Computing Tangent Space Basis Vectors for an
//! Arbitrary Mesh" (terathon.com/blog/tangent-space.html). We accumulate a
//! `tan`/`bitan` pair per vertex over every triangle, then Gram-Schmidt
//! orthonormalize the tangent against the (already unit) vertex normal and pack
//! the bitangent handedness sign into `tangent.w` so the shader recovers
//! `B = w·(N×T)`. This is the analytic-friendly, order-independent choice for
//! procedural primitives with continuous UVs; MikkTSpace-parity welding is only
//! needed for imported/baked assets (#1098) and is deliberately out of scope.
//!
//! Determinism: accumulation order follows the index buffer and uses only
//! IEEE-754 `+ - * /` and `sqrt`, so identical inputs yield byte-identical
//! tangents on a given target.

use crate::EngineError;

use super::mesh::ProcVertex;

/// Fill `vertices[*].tangent` from positions, normals, and UVs over `indices`.
pub(super) fn accumulate_tangents(
    vertices: &mut [ProcVertex],
    indices: &[u32],
) -> Result<(), EngineError> {
    let mut tan = vec![[0.0f32; 3]; vertices.len()];
    let mut bitan = vec![[0.0f32; 3]; vertices.len()];

    for triangle in indices.chunks_exact(3) {
        let [i0, i1, i2] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let p0 = vertices[i0].position;
        let p1 = vertices[i1].position;
        let p2 = vertices[i2].position;
        let w0 = vertices[i0].uv;
        let w1 = vertices[i1].uv;
        let w2 = vertices[i2].uv;

        let e1 = sub(p1, p0);
        let e2 = sub(p2, p0);
        let du1 = w1[0] - w0[0];
        let dv1 = w1[1] - w0[1];
        let du2 = w2[0] - w0[0];
        let dv2 = w2[1] - w0[1];

        let det = du1 * dv2 - du2 * dv1;
        // Degenerate UV mapping (collinear/zero-area in texture space): skip the
        // triangle's contribution rather than divide by zero. Vertices touched
        // only by such triangles fall back to a normal-derived tangent below.
        if det.abs() <= f32::EPSILON {
            continue;
        }
        let r = 1.0 / det;
        let sdir = [
            (dv2 * e1[0] - dv1 * e2[0]) * r,
            (dv2 * e1[1] - dv1 * e2[1]) * r,
            (dv2 * e1[2] - dv1 * e2[2]) * r,
        ];
        let tdir = [
            (du1 * e2[0] - du2 * e1[0]) * r,
            (du1 * e2[1] - du2 * e1[1]) * r,
            (du1 * e2[2] - du2 * e1[2]) * r,
        ];
        for &index in &[i0, i1, i2] {
            tan[index] = add(tan[index], sdir);
            bitan[index] = add(bitan[index], tdir);
        }
    }

    for (index, vertex) in vertices.iter_mut().enumerate() {
        let normal = vertex.normal;
        let accumulated = tan[index];
        // Gram-Schmidt: T' = normalize(T - N·(N·T)).
        let ndott = dot(normal, accumulated);
        let mut ortho = [
            accumulated[0] - normal[0] * ndott,
            accumulated[1] - normal[1] * ndott,
            accumulated[2] - normal[2] * ndott,
        ];
        if length(ortho) <= f32::EPSILON {
            // No usable UV gradient reached this vertex — synthesize a stable
            // tangent orthogonal to the normal so the TBN frame stays valid.
            ortho = fallback_tangent(normal);
        }
        let tangent = normalize(ortho)?;
        // Handedness: sign of (N×T)·B tells the shader which way the bitangent
        // points. Default to +1 when the accumulated bitangent is degenerate.
        let handedness = if dot(cross(normal, tangent), bitan[index]) < 0.0 {
            -1.0
        } else {
            1.0
        };
        vertex.tangent = [tangent[0], tangent[1], tangent[2], handedness];
    }
    Ok(())
}

fn fallback_tangent(normal: [f32; 3]) -> [f32; 3] {
    // Pick the world axis least aligned with the normal, then project it out.
    let reference = if normal[0].abs() <= normal[1].abs() && normal[0].abs() <= normal[2].abs() {
        [1.0, 0.0, 0.0]
    } else if normal[1].abs() <= normal[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let ndotr = dot(normal, reference);
    [
        reference[0] - normal[0] * ndotr,
        reference[1] - normal[1] * ndotr,
        reference[2] - normal[2] * ndotr,
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: [f32; 3]) -> Result<[f32; 3], EngineError> {
    let len = length(a);
    if len <= f32::EPSILON {
        return Err(EngineError::ProcGenInvalidConfig {
            field: "procgen.tangent".to_string(),
            reason: "degenerate tangent vector could not be normalized".to_string(),
        });
    }
    Ok([a[0] / len, a[1] / len, a[2] / len])
}
