use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use super::{
    geometry::CompositionBuild,
    invalid_composition,
    terrain::{ISLAND_X_RADIUS, ISLAND_Z_RADIUS},
};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];

#[derive(Debug, Clone, Copy)]
struct CliffSpire {
    angle_degrees: f32,
    radial_scale: f32,
    width: f32,
    depth: f32,
    top_y: f32,
    apex_y: f32,
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct RiftFall {
    angle_degrees: f32,
    radial_scale: f32,
    width: f32,
    top_y: f32,
    bottom_y: f32,
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct LightSlit {
    center: [f32; 3],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
}

const CLIFF_SPIRES: [CliffSpire; 20] = [
    spire(212.0, 1.01, 7.4, 7.0, -5.0, -43.0),
    spire(229.0, 0.99, 5.8, 5.0, -7.0, -35.0),
    spire(246.0, 1.03, 8.2, 7.6, -8.0, -49.0),
    spire(271.0, 0.97, 6.6, 5.4, -8.5, -41.0),
    spire(296.0, 1.02, 8.8, 7.2, -6.5, -47.0),
    spire(318.0, 1.00, 5.5, 5.0, -7.0, -36.0),
    spire(338.0, 1.04, 8.5, 7.6, -8.0, -52.0),
    spire(12.0, 0.99, 6.8, 5.8, -6.0, -39.0),
    spire(34.0, 1.03, 8.0, 6.8, -8.0, -48.0),
    spire(58.0, 1.00, 5.9, 5.2, -7.5, -37.0),
    spire(83.0, 1.03, 8.5, 7.4, -7.0, -51.0),
    spire(105.0, 0.98, 6.3, 5.2, -6.5, -38.0),
    spire(128.0, 1.04, 8.1, 7.0, -8.0, -46.0),
    spire(151.0, 1.01, 5.7, 5.0, -7.0, -34.0),
    spire(174.0, 1.03, 8.4, 7.4, -8.5, -50.0),
    spire(195.0, 0.98, 6.0, 5.4, -7.0, -37.0),
    spire(25.0, 1.16, 5.0, 4.4, -14.0, -36.0),
    spire(142.0, 1.18, 5.4, 4.8, -15.0, -39.0),
    spire(268.0, 1.15, 4.9, 4.0, -14.5, -35.0),
    spire(322.0, 1.18, 5.2, 4.6, -15.0, -38.0),
];

const RIFT_FALLS: [RiftFall; 8] = [
    fall(214.0, 1.045, 1.15, -4.0, -36.0, [0.05, 0.72, 1.55, 2.25]),
    fall(246.0, 1.035, 0.95, -5.5, -29.0, [1.55, 0.42, 0.10, 1.95]),
    fall(298.0, 1.040, 1.10, -5.0, -38.0, [0.05, 0.64, 1.38, 2.05]),
    fall(336.0, 1.035, 0.95, -6.0, -31.0, [1.50, 0.36, 0.10, 1.85]),
    fall(38.0, 1.040, 1.05, -5.0, -34.0, [0.05, 0.66, 1.44, 2.10]),
    fall(84.0, 1.035, 0.90, -6.0, -32.0, [1.48, 0.42, 0.12, 1.90]),
    fall(128.0, 1.045, 1.05, -5.0, -37.0, [0.05, 0.58, 1.28, 1.95]),
    fall(174.0, 1.035, 0.90, -6.0, -30.0, [1.35, 0.34, 0.10, 1.75]),
];

const LIGHT_SLITS: [LightSlit; 12] = [
    light([-15.0, 23.0, -31.5], 18.0, 1.0, 5.8),
    light([-9.5, 34.0, -30.8], 12.0, 0.9, 7.2),
    light([9.8, 24.0, -30.2], -10.0, 1.0, 6.2),
    light([14.5, 34.5, -29.5], -14.0, 0.9, 7.4),
    light([-28.0, 17.5, -22.0], 36.0, 0.9, 5.2),
    light([29.0, 17.0, -20.5], -34.0, 0.9, 5.0),
    light([-3.5, 15.5, -47.0], 0.0, 1.2, 4.6),
    light([8.0, 14.5, -46.0], 0.0, 1.2, 4.2),
    light([-34.0, 7.0, -7.0], 58.0, 1.2, 3.6),
    light([34.5, 7.0, -7.5], -58.0, 1.2, 3.6),
    light([-8.0, 9.0, 10.0], 92.0, 1.0, 3.2),
    light([18.0, 9.0, 12.0], 86.0, 1.0, 3.2),
];

pub(super) fn add_floating_cliff_silhouette(
    build: &mut CompositionBuild,
) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    for spire in CLIFF_SPIRES {
        add_spire(build, spire)?;
    }
    for fall in RIFT_FALLS {
        add_rift_fall(build, fall)?;
    }
    for slit in LIGHT_SLITS {
        add_light_slit(build, slit)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition(
            "silhouette",
            "floating cliff silhouette added no primitives",
        );
    }
    Ok(())
}

fn add_spire(build: &mut CompositionBuild, spire: CliffSpire) -> Result<(), EngineError> {
    let basis = edge_basis(spire.angle_degrees, spire.radial_scale);
    let center = Vec3::new(basis.center.x, spire.top_y, basis.center.z);
    let left = center - basis.tangent * spire.width;
    let right = center + basis.tangent * spire.width;
    let inner = center - basis.outward * spire.depth;
    let outer = center + basis.outward * (spire.depth * 0.42);
    let apex =
        center + basis.outward * (spire.depth * 0.18) + Vec3::Y * (spire.apex_y - spire.top_y);
    push_triangle(
        build,
        [left, inner, apex],
        spire.color,
        [0.0, 0.92, 0.0, 0.0],
    )?;
    push_triangle(
        build,
        [inner, right, apex],
        spire.color,
        [0.0, 0.92, 0.0, 0.0],
    )?;
    push_triangle(
        build,
        [right, outer, apex],
        spire.color,
        [0.0, 0.92, 0.0, 0.0],
    )?;
    push_triangle(
        build,
        [outer, left, apex],
        spire.color,
        [0.0, 0.92, 0.0, 0.0],
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn add_rift_fall(build: &mut CompositionBuild, fall: RiftFall) -> Result<(), EngineError> {
    let basis = edge_basis(fall.angle_degrees, fall.radial_scale);
    let top = Vec3::new(basis.center.x, fall.top_y, basis.center.z) + basis.outward * 0.8;
    let bottom = Vec3::new(basis.center.x, fall.bottom_y, basis.center.z) + basis.outward * 2.3;
    let side = basis.tangent * fall.width;
    push_quad(
        build,
        [
            top - side,
            top + side,
            bottom + side * 0.42,
            bottom - side * 0.42,
        ],
        fall.color,
        [0.0, 0.34, 0.0, 0.0],
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn add_light_slit(build: &mut CompositionBuild, slit: LightSlit) -> Result<(), EngineError> {
    let yaw = slit.yaw_degrees.to_radians();
    let side = Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * (slit.width * 0.52);
    let up = Vec3::Y * (slit.height * 0.52);
    let center = Vec3::from_array(slit.center);
    push_quad(
        build,
        [
            center - side - up,
            center + side - up,
            center + side + up,
            center - side + up,
        ],
        slit.color,
        [0.0, 0.38, 0.0, 0.0],
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn edge_basis(angle_degrees: f32, radial_scale: f32) -> EdgeBasis {
    let angle = angle_degrees.to_radians();
    let radius_scale = island_radius_scale(angle) * radial_scale;
    let center = Vec3::new(
        angle.cos() * ISLAND_X_RADIUS * radius_scale,
        0.0,
        angle.sin() * ISLAND_Z_RADIUS * radius_scale,
    );
    let outward = Vec3::new(center.x, 0.0, center.z).normalize_or_zero();
    let tangent = Vec3::new(-outward.z, 0.0, outward.x).normalize_or_zero();
    EdgeBasis {
        center,
        outward,
        tangent,
    }
}

fn island_radius_scale(angle: f32) -> f32 {
    let broad_breaks = (angle * 3.0 + 0.45).sin() * 0.085;
    let cliff_notches = (angle * 5.0 - 0.25).cos() * 0.055;
    let small_chips = (angle * 9.0 + 1.7).sin() * 0.035;
    (1.0 + broad_breaks + cliff_notches + small_chips).clamp(0.84, 1.07)
}

fn push_quad(
    build: &mut CompositionBuild,
    points: [Vec3; 4],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<(), EngineError> {
    push_triangle(build, [points[0], points[1], points[2]], color, pbr)?;
    push_triangle(build, [points[0], points[2], points[3]], color, pbr)
}

fn push_triangle(
    build: &mut CompositionBuild,
    mut points: [Vec3; 3],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<(), EngineError> {
    let mut normal = (points[1] - points[0])
        .cross(points[2] - points[0])
        .normalize_or_zero();
    if normal.y < 0.0 {
        points.swap(1, 2);
        normal = -normal;
    }
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition("silhouette.normals", "silhouette triangle is degenerate");
    }
    let base = checked_vertex_base(build.vertices.len())?;
    for point in points {
        build
            .vertices
            .push(vertex(point, normal, WHITE_ATLAS_UV, color, pbr)?);
    }
    build.indices.extend([base, base + 1, base + 2]);
    Ok(())
}

fn vertex(
    position: Vec3,
    normal: Vec3,
    uv: [f32; 2],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<EditorWorldVertex, EngineError> {
    if !position.is_finite()
        || !normal.is_finite()
        || !Vec2::from_array(uv).is_finite()
        || color.iter().any(|value| !value.is_finite())
        || pbr.iter().any(|value| !value.is_finite())
    {
        return invalid_composition(
            "silhouette.vertex",
            "silhouette vertex contains invalid data",
        );
    }
    Ok(EditorWorldVertex::new(
        position.to_array(),
        normal.to_array(),
        orthogonal_tangent(normal)?,
        uv,
        color,
        pbr,
    ))
}

fn orthogonal_tangent(normal: Vec3) -> Result<[f32; 4], EngineError> {
    let reference = if normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = reference.cross(normal).normalize_or_zero();
    if !tangent.is_finite() || tangent == Vec3::ZERO {
        return invalid_composition("silhouette.tangent", "failed to derive tangent from normal");
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.silhouette.vertex_count".to_string(),
        reason: "silhouette exceeds u32 index space".to_string(),
    })
}

const fn spire(
    angle_degrees: f32,
    radial_scale: f32,
    width: f32,
    depth: f32,
    top_y: f32,
    apex_y: f32,
) -> CliffSpire {
    CliffSpire {
        angle_degrees,
        radial_scale,
        width,
        depth,
        top_y,
        apex_y,
        color: [0.016, 0.024, 0.038, 0.0],
    }
}

const fn fall(
    angle_degrees: f32,
    radial_scale: f32,
    width: f32,
    top_y: f32,
    bottom_y: f32,
    color: [f32; 4],
) -> RiftFall {
    RiftFall {
        angle_degrees,
        radial_scale,
        width,
        top_y,
        bottom_y,
        color,
    }
}

const fn light(center: [f32; 3], yaw_degrees: f32, width: f32, height: f32) -> LightSlit {
    LightSlit {
        center,
        yaw_degrees,
        width,
        height,
        color: [1.36, 0.62, 0.24, 1.85],
    }
}

struct EdgeBasis {
    center: Vec3,
    outward: Vec3,
    tangent: Vec3,
}
