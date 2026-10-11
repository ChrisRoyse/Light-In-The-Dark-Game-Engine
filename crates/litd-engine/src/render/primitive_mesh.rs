use std::f32::consts::{PI, TAU};

use crate::EngineError;

use super::{PrimitiveKind, PrimitiveMesh, PrimitiveSpec, validate_spec};

#[path = "primitive_mesh_support.rs"]
mod primitive_mesh_support;
#[path = "primitive_mesh_validate.rs"]
mod primitive_mesh_validate;

use primitive_mesh_support::{
    append_annulus, append_cylinder_side, append_disc, finalize, quad, scale, vertex,
};
pub(super) use primitive_mesh_validate::validate_kind;

pub fn build_primitive_mesh(spec: &PrimitiveSpec) -> Result<PrimitiveMesh, EngineError> {
    validate_spec(spec)?;
    let mut mesh = PrimitiveMesh {
        primitive_id: spec.primitive_id.clone(),
        kind_id: spec.kind.id().to_string(),
        topology: spec.kind.topology(),
        vertices: Vec::new(),
        indices: Vec::new(),
        bounds_min: [0.0; 3],
        bounds_max: [0.0; 3],
    };
    match &spec.kind {
        PrimitiveKind::Box3d { size } => append_box(&mut mesh, *size, spec.color)?,
        PrimitiveKind::Sphere {
            radius,
            segments,
            rings,
        } => append_sphere(&mut mesh, *radius, *segments, *rings, spec.color)?,
        PrimitiveKind::Cylinder {
            radius,
            height,
            radial_segments,
            height_segments,
        } => append_cylinder(
            &mut mesh,
            *radius,
            *height,
            *radial_segments,
            *height_segments,
            spec.color,
        )?,
        PrimitiveKind::Torus {
            major_radius,
            minor_radius,
            major_segments,
            minor_segments,
        } => append_torus(
            &mut mesh,
            *major_radius,
            *minor_radius,
            *major_segments,
            *minor_segments,
            spec.color,
        )?,
        PrimitiveKind::Tube {
            outer_radius,
            inner_radius,
            height,
            radial_segments,
        } => append_tube(
            &mut mesh,
            *outer_radius,
            *inner_radius,
            *height,
            *radial_segments,
            spec.color,
        )?,
        PrimitiveKind::Plane {
            width,
            depth,
            subdivisions_x,
            subdivisions_z,
        } => append_plane(
            &mut mesh,
            *width,
            *depth,
            *subdivisions_x,
            *subdivisions_z,
            spec.color,
        )?,
        PrimitiveKind::Sprite { width, height } => {
            append_sprite(&mut mesh, *width, *height, spec.color)?
        }
        PrimitiveKind::Points { points } => append_points(&mut mesh, points, spec.color)?,
        PrimitiveKind::Lines { points } => append_lines(&mut mesh, points, spec.color)?,
    }
    finalize(mesh)
}

fn append_box(
    mesh: &mut PrimitiveMesh,
    size: [f32; 3],
    color: [f32; 4],
) -> Result<(), EngineError> {
    let [hx, hy, hz] = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    let faces = [
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
    for (points, normal) in faces {
        quad(mesh, points, normal, color)?;
    }
    Ok(())
}

fn append_plane(
    mesh: &mut PrimitiveMesh,
    width: f32,
    depth: f32,
    sx: u32,
    sz: u32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    for z in 0..=sz {
        for x in 0..=sx {
            let u = x as f32 / sx as f32;
            let v = z as f32 / sz as f32;
            mesh.vertices.push(vertex(
                [(u - 0.5) * width, 0.0, (v - 0.5) * depth],
                [0.0, 1.0, 0.0],
                [u, v],
                color,
            ));
        }
    }
    let row = sx + 1;
    for z in 0..sz {
        for x in 0..sx {
            let a = z * row + x;
            mesh.indices
                .extend_from_slice(&[a, a + 1, a + row + 1, a, a + row + 1, a + row]);
        }
    }
    Ok(())
}

fn append_sprite(
    mesh: &mut PrimitiveMesh,
    width: f32,
    height: f32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    quad(
        mesh,
        [
            [-width * 0.5, -height * 0.5, 0.0],
            [width * 0.5, -height * 0.5, 0.0],
            [width * 0.5, height * 0.5, 0.0],
            [-width * 0.5, height * 0.5, 0.0],
        ],
        [0.0, 0.0, 1.0],
        color,
    )
}

fn append_sphere(
    mesh: &mut PrimitiveMesh,
    radius: f32,
    segments: u32,
    rings: u32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let theta = PI * v;
        let (sin_t, cos_t) = theta.sin_cos();
        for seg in 0..=segments {
            let u = seg as f32 / segments as f32;
            let (sin_p, cos_p) = (TAU * u).sin_cos();
            let normal = [sin_t * cos_p, cos_t, sin_t * sin_p];
            mesh.vertices
                .push(vertex(scale(normal, radius), normal, [u, v], color));
        }
    }
    let row = segments + 1;
    for ring in 0..rings {
        for seg in 0..segments {
            let a = ring * row + seg;
            mesh.indices
                .extend_from_slice(&[a, a + row, a + 1, a + 1, a + row, a + row + 1]);
        }
    }
    Ok(())
}

fn append_cylinder(
    mesh: &mut PrimitiveMesh,
    radius: f32,
    height: f32,
    radial: u32,
    rows: u32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    append_cylinder_side(mesh, radius, height, radial, rows, true, color)?;
    append_disc(mesh, radius, height * 0.5, radial, true, color)?;
    append_disc(mesh, radius, -height * 0.5, radial, false, color)
}

fn append_torus(
    mesh: &mut PrimitiveMesh,
    major: f32,
    minor: f32,
    major_segments: u32,
    minor_segments: u32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    for i in 0..=major_segments {
        let u = i as f32 / major_segments as f32;
        let (su, cu) = (TAU * u).sin_cos();
        for j in 0..=minor_segments {
            let v = j as f32 / minor_segments as f32;
            let (sv, cv) = (TAU * v).sin_cos();
            let normal = [cu * cv, sv, su * cv];
            let center = [major * cu, 0.0, major * su];
            mesh.vertices.push(vertex(
                [
                    center[0] + minor * normal[0],
                    minor * normal[1],
                    center[2] + minor * normal[2],
                ],
                normal,
                [u, v],
                color,
            ));
        }
    }
    let row = minor_segments + 1;
    for i in 0..major_segments {
        for j in 0..minor_segments {
            let a = i * row + j;
            mesh.indices
                .extend_from_slice(&[a, a + row, a + 1, a + 1, a + row, a + row + 1]);
        }
    }
    Ok(())
}

fn append_tube(
    mesh: &mut PrimitiveMesh,
    outer: f32,
    inner: f32,
    height: f32,
    radial: u32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    append_cylinder_side(mesh, outer, height, radial, 1, true, color)?;
    append_cylinder_side(mesh, inner, height, radial, 1, false, color)?;
    append_annulus(mesh, outer, inner, height * 0.5, radial, true, color)?;
    append_annulus(mesh, outer, inner, -height * 0.5, radial, false, color)
}

fn append_points(
    mesh: &mut PrimitiveMesh,
    points: &[[f32; 3]],
    color: [f32; 4],
) -> Result<(), EngineError> {
    for point in points {
        mesh.vertices
            .push(vertex(*point, [0.0, 0.0, 1.0], [0.0, 0.0], color));
    }
    mesh.indices.extend(0..points.len() as u32);
    Ok(())
}

fn append_lines(
    mesh: &mut PrimitiveMesh,
    points: &[[f32; 3]],
    color: [f32; 4],
) -> Result<(), EngineError> {
    append_points(mesh, points, color)
}
