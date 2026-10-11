use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use super::{geometry::CompositionBuild, invalid_composition};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];

#[derive(Debug, Clone, Copy)]
struct Pinnacle {
    center: [f32; 3],
    radius: f32,
    height: f32,
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct Buttress {
    center: [f32; 3],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    depth: f32,
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct WindowCut {
    center: [f32; 3],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
}

const PINNACLES: [Pinnacle; 15] = [
    pinnacle([0.0, 48.0, -29.0], 4.8, 38.0),
    pinnacle([-11.5, 40.0, -30.5], 3.6, 28.0),
    pinnacle([12.0, 39.0, -29.0], 3.6, 27.0),
    pinnacle([-25.5, 27.0, -24.0], 3.1, 24.0),
    pinnacle([26.0, 26.5, -23.0], 3.0, 23.0),
    pinnacle([-4.0, 33.0, -43.0], 3.1, 25.0),
    pinnacle([8.0, 31.5, -42.0], 2.8, 22.0),
    pinnacle([-35.0, 17.0, -8.0], 2.5, 18.0),
    pinnacle([35.5, 17.0, -8.0], 2.5, 18.0),
    pinnacle([-19.0, 15.0, 12.0], 2.3, 15.0),
    pinnacle([20.0, 15.0, 13.0], 2.3, 15.0),
    pinnacle([-43.0, 9.5, -38.0], 2.4, 16.0),
    pinnacle([43.0, 9.5, -37.0], 2.4, 16.0),
    pinnacle([-52.0, 4.5, -6.0], 2.0, 13.0),
    pinnacle([52.0, 4.5, -6.0], 2.0, 13.0),
];

const BUTTRESSES: [Buttress; 18] = [
    buttress([-16.0, 18.0, -31.0], 20.0, 1.4, 26.0, 2.8),
    buttress([-7.0, 19.0, -32.0], 8.0, 1.2, 28.0, 2.5),
    buttress([7.0, 18.5, -31.0], -8.0, 1.2, 27.0, 2.5),
    buttress([16.0, 17.5, -30.0], -20.0, 1.4, 25.0, 2.8),
    buttress([-28.0, 12.5, -20.0], 40.0, 1.5, 20.0, 3.0),
    buttress([28.5, 12.5, -20.0], -40.0, 1.5, 20.0, 3.0),
    buttress([-33.0, 8.5, -5.5], 62.0, 1.2, 15.0, 2.6),
    buttress([33.0, 8.5, -5.5], -62.0, 1.2, 15.0, 2.6),
    buttress([-20.0, 8.0, 11.0], 104.0, 1.1, 12.0, 2.3),
    buttress([21.0, 8.0, 11.0], 78.0, 1.1, 12.0, 2.3),
    buttress([-12.0, 7.5, -44.0], 0.0, 1.0, 12.0, 2.2),
    buttress([14.0, 7.5, -43.0], 0.0, 1.0, 12.0, 2.2),
    buttress([-42.0, 2.5, -33.0], 18.0, 1.1, 10.0, 2.1),
    buttress([42.0, 2.5, -32.0], -18.0, 1.1, 10.0, 2.1),
    buttress([-46.0, 1.5, 5.0], 76.0, 1.0, 9.0, 2.0),
    buttress([46.0, 1.5, 5.0], -76.0, 1.0, 9.0, 2.0),
    buttress([-6.0, 6.5, 24.0], 92.0, 1.0, 9.0, 2.0),
    buttress([8.0, 6.5, 24.0], 88.0, 1.0, 9.0, 2.0),
];

const WINDOWS: [WindowCut; 32] = [
    warm([-12.5, 26.0, -32.0], 18.0, 0.85, 3.8),
    warm([-8.0, 33.0, -31.7], 12.0, 0.75, 4.2),
    warm([-3.5, 41.0, -31.2], 6.0, 0.65, 4.4),
    warm([5.0, 39.0, -30.5], -8.0, 0.65, 4.2),
    warm([10.5, 31.0, -30.0], -14.0, 0.75, 4.0),
    warm([15.5, 25.0, -29.4], -20.0, 0.85, 3.8),
    warm([-25.5, 18.0, -22.0], 42.0, 0.80, 3.6),
    warm([-29.5, 12.0, -17.0], 48.0, 0.75, 3.4),
    warm([25.5, 17.5, -21.5], -42.0, 0.80, 3.6),
    warm([30.0, 11.5, -16.5], -48.0, 0.75, 3.4),
    warm([-4.0, 20.0, -45.0], 0.0, 0.85, 3.7),
    warm([5.5, 19.0, -44.2], 0.0, 0.85, 3.7),
    warm([-15.0, 9.5, -7.5], 92.0, 0.70, 3.2),
    warm([-8.0, 9.0, -7.0], 92.0, 0.70, 3.2),
    warm([8.0, 9.0, -7.0], 88.0, 0.70, 3.2),
    warm([15.0, 9.5, -7.5], 88.0, 0.70, 3.2),
    warm([-22.0, 6.0, 14.0], 8.0, 0.65, 2.8),
    warm([-14.0, 6.0, 16.0], 8.0, 0.65, 2.8),
    warm([14.0, 6.0, 16.0], -8.0, 0.65, 2.8),
    warm([22.0, 6.0, 14.0], -8.0, 0.65, 2.8),
    cold([-18.0, 31.0, -34.0], 20.0, 0.55, 3.2),
    cold([18.0, 30.0, -33.0], -20.0, 0.55, 3.2),
    cold([-37.0, 13.0, -16.0], 52.0, 0.55, 2.8),
    cold([37.0, 13.0, -16.0], -52.0, 0.55, 2.8),
    cold([-10.0, 14.5, -48.0], 0.0, 0.55, 2.8),
    cold([12.0, 14.5, -48.0], 0.0, 0.55, 2.8),
    warm([-41.0, 4.5, -40.0], 14.0, 0.70, 2.8),
    warm([42.0, 4.5, -39.0], -14.0, 0.70, 2.8),
    warm([-43.0, 5.0, 9.0], 78.0, 0.65, 2.6),
    warm([43.0, 5.0, 9.0], -78.0, 0.65, 2.6),
    cold([-55.0, 5.5, -8.0], 28.0, 0.55, 2.6),
    cold([55.0, 5.5, -8.0], -28.0, 0.55, 2.6),
];

pub(super) fn add_citadel_gothic_detail(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    for pinnacle in PINNACLES {
        add_pinnacle(build, pinnacle)?;
    }
    for buttress in BUTTRESSES {
        add_buttress(build, buttress)?;
    }
    for window in WINDOWS {
        add_window(build, window)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition(
            "citadel_detail",
            "citadel gothic detail added no primitives",
        );
    }
    Ok(())
}

fn add_pinnacle(build: &mut CompositionBuild, pinnacle: Pinnacle) -> Result<(), EngineError> {
    let center = Vec3::from_array(pinnacle.center);
    let apex = center + Vec3::Y * pinnacle.height;
    let north = center + Vec3::new(0.0, 0.0, -pinnacle.radius);
    let east = center + Vec3::new(pinnacle.radius, 0.0, 0.0);
    let south = center + Vec3::new(0.0, 0.0, pinnacle.radius);
    let west = center + Vec3::new(-pinnacle.radius, 0.0, 0.0);
    push_triangle(build, [north, east, apex], pinnacle.color, rough_stone())?;
    push_triangle(build, [east, south, apex], pinnacle.color, rough_stone())?;
    push_triangle(build, [south, west, apex], pinnacle.color, rough_stone())?;
    push_triangle(build, [west, north, apex], pinnacle.color, rough_stone())?;
    build.mesh_primitives += 1;
    Ok(())
}

fn add_buttress(build: &mut CompositionBuild, buttress: Buttress) -> Result<(), EngineError> {
    let yaw = buttress.yaw_degrees.to_radians();
    let side = Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * buttress.width;
    let outward = Vec3::new(yaw.sin(), 0.0, yaw.cos()) * buttress.depth;
    let bottom = Vec3::from_array(buttress.center);
    let top = bottom + Vec3::Y * buttress.height - outward * 0.45;
    let base_back = bottom - outward * 0.35;
    let base_front = bottom + outward;
    push_quad(
        build,
        [base_back - side, base_front - side, top - side * 0.42, top],
        buttress.color,
        rough_stone(),
    )?;
    push_quad(
        build,
        [base_front + side, base_back + side, top, top + side * 0.42],
        buttress.color,
        rough_stone(),
    )?;
    push_triangle(
        build,
        [base_front - side, base_front + side, top],
        buttress.color,
        rough_stone(),
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn add_window(build: &mut CompositionBuild, window: WindowCut) -> Result<(), EngineError> {
    let yaw = window.yaw_degrees.to_radians();
    let side = Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * window.width;
    let up = Vec3::Y * window.height;
    let center = Vec3::from_array(window.center);
    push_quad(
        build,
        [
            center - side - up,
            center + side - up,
            center + side + up,
            center - side + up,
        ],
        window.color,
        light_pbr(),
    )?;
    build.mesh_primitives += 1;
    Ok(())
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
        return invalid_composition(
            "citadel_detail.normals",
            "citadel detail triangle is degenerate",
        );
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
            "citadel_detail.vertex",
            "citadel detail vertex contains invalid data",
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
        return invalid_composition(
            "citadel_detail.tangent",
            "failed to derive tangent from normal",
        );
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.citadel_detail.vertex_count".to_string(),
        reason: "citadel detail exceeds u32 index space".to_string(),
    })
}

const fn pinnacle(center: [f32; 3], radius: f32, height: f32) -> Pinnacle {
    Pinnacle {
        center,
        radius,
        height,
        color: [0.025, 0.033, 0.052, 0.0],
    }
}

const fn buttress(
    center: [f32; 3],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    depth: f32,
) -> Buttress {
    Buttress {
        center,
        yaw_degrees,
        width,
        height,
        depth,
        color: [0.055, 0.070, 0.098, 0.0],
    }
}

const fn warm(center: [f32; 3], yaw_degrees: f32, width: f32, height: f32) -> WindowCut {
    WindowCut {
        center,
        yaw_degrees,
        width,
        height,
        color: [1.18, 0.52, 0.18, 1.55],
    }
}

const fn cold(center: [f32; 3], yaw_degrees: f32, width: f32, height: f32) -> WindowCut {
    WindowCut {
        center,
        yaw_degrees,
        width,
        height,
        color: [0.10, 0.58, 1.25, 1.45],
    }
}

const fn rough_stone() -> [f32; 4] {
    [0.0, 0.92, 0.0, 0.0]
}

const fn light_pbr() -> [f32; 4] {
    [0.0, 0.38, 0.0, 0.0]
}
