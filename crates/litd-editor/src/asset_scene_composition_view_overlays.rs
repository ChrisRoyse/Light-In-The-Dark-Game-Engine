use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use super::{
    geometry::CompositionBuild,
    invalid_composition,
    terrain::{PATH_SURFACES, terrain_height_at},
};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];

pub(super) fn add_water_layer(build: &mut CompositionBuild) -> Result<(), EngineError> {
    add_surface_quad(
        build,
        [-60.0, 38.0],
        -18.0,
        17.0,
        7.5,
        [0.035, 0.18, 0.34, 0.82],
    )?;
    add_surface_quad(
        build,
        [58.0, -28.0],
        24.0,
        16.0,
        6.5,
        [0.028, 0.15, 0.30, 0.74],
    )
}

pub(super) fn add_units_layer(build: &mut CompositionBuild) -> Result<(), EngineError> {
    add_marker_diamond(build, [3.0, -6.0], 3.4, [0.18, 0.48, 1.10, 2.35])?;
    add_marker_diamond(build, [-10.0, 4.0], 2.4, [0.74, 0.32, 0.12, 1.90])
}

pub(super) fn add_ground_pathing_overlay(build: &mut CompositionBuild) -> Result<(), EngineError> {
    for surface in PATH_SURFACES {
        add_path_ribbon(
            build,
            surface.center_xz,
            surface.yaw_degrees,
            surface.half_length,
            0.62,
            [0.12, 0.72, 0.28, 1.85],
        )?;
    }
    Ok(())
}

pub(super) fn add_naval_pathing_overlay(build: &mut CompositionBuild) -> Result<(), EngineError> {
    add_path_ribbon(
        build,
        [-61.0, 38.0],
        -20.0,
        20.0,
        0.54,
        [0.10, 0.44, 1.25, 1.90],
    )?;
    add_path_ribbon(
        build,
        [58.0, -28.0],
        24.0,
        18.0,
        0.54,
        [0.08, 0.38, 1.05, 1.72],
    )
}

pub(super) fn add_camera_bounds_layer(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let min = Vec2::new(-72.0, -54.0);
    let max = Vec2::new(72.0, 48.0);
    let points = [
        Vec2::new(min.x, min.y),
        Vec2::new(max.x, min.y),
        Vec2::new(max.x, max.y),
        Vec2::new(min.x, max.y),
    ];
    for index in 0..4 {
        add_line_segment(
            build,
            points[index],
            points[(index + 1) % 4],
            0.52,
            [1.22, 0.94, 0.30, 2.15],
        )?;
    }
    Ok(())
}

pub(super) fn add_regions_layer(build: &mut CompositionBuild) -> Result<(), EngineError> {
    add_surface_quad(build, [0.0, 0.0], 0.0, 13.0, 9.0, [0.40, 0.22, 0.74, 0.92])?;
    add_surface_quad(build, [5.0, 28.0], 8.0, 12.0, 7.0, [0.54, 0.28, 0.82, 1.05])
}

pub(super) fn add_camera_objects_layer(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let origin = point_on_terrain([0.0, -42.0], 5.0);
    let left = point_on_terrain([-8.0, -26.0], 3.8);
    let right = point_on_terrain([8.0, -26.0], 3.8);
    let nose = point_on_terrain([0.0, -34.0], 5.8);
    push_triangle(build, [origin, left, nose], [0.16, 0.72, 1.10, 2.20])?;
    push_triangle(build, [origin, nose, right], [0.16, 0.72, 1.10, 2.20])
}

pub(super) fn add_info_display_layer(build: &mut CompositionBuild) -> Result<(), EngineError> {
    add_marker_diamond(build, [0.0, -34.0], 1.8, [2.10, 1.46, 0.34, 2.85])?;
    add_marker_diamond(build, [-34.0, -22.0], 1.5, [2.10, 1.46, 0.34, 2.65])?;
    add_marker_diamond(build, [30.0, 15.0], 1.5, [2.10, 1.46, 0.34, 2.65])
}

fn add_surface_quad(
    build: &mut CompositionBuild,
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    let yaw = yaw_degrees.to_radians();
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    let side = Vec2::new(forward.y, -forward.x);
    let center = Vec2::from_array(center_xz);
    let points = [
        center - forward * half_length - side * half_width,
        center + forward * half_length - side * half_width,
        center + forward * half_length + side * half_width,
        center - forward * half_length + side * half_width,
    ];
    push_quad(
        build,
        points.map(|point| point_on_terrain(point.to_array(), 0.54)),
        color,
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn add_path_ribbon(
    build: &mut CompositionBuild,
    center_xz: [f32; 2],
    yaw_degrees: f32,
    half_length: f32,
    half_width: f32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    let yaw = yaw_degrees.to_radians();
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    let side = Vec2::new(forward.y, -forward.x);
    let center = Vec2::from_array(center_xz);
    let start = center - forward * half_length;
    let end = center + forward * half_length;
    add_line_segment(
        build,
        start + side * half_width,
        end + side * half_width,
        half_width,
        color,
    )?;
    add_line_segment(
        build,
        start - side * half_width,
        end - side * half_width,
        half_width,
        color,
    )
}

fn add_line_segment(
    build: &mut CompositionBuild,
    start: Vec2,
    end: Vec2,
    half_width: f32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    let delta = end - start;
    let length = delta.length();
    if !length.is_finite() || length <= 0.001 {
        return invalid_composition("view_overlay.line", "overlay line segment is degenerate");
    }
    let side = Vec2::new(-delta.y, delta.x) * (half_width / length);
    let points = [start + side, end + side, end - side, start - side];
    push_quad(
        build,
        points.map(|point| point_on_terrain(point.to_array(), 0.82)),
        color,
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn add_marker_diamond(
    build: &mut CompositionBuild,
    center_xz: [f32; 2],
    radius: f32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    let center = point_on_terrain(center_xz, 3.6);
    let points = [
        point_on_terrain([center_xz[0], center_xz[1] - radius], 1.1),
        point_on_terrain([center_xz[0] + radius, center_xz[1]], 1.1),
        point_on_terrain([center_xz[0], center_xz[1] + radius], 1.1),
        point_on_terrain([center_xz[0] - radius, center_xz[1]], 1.1),
    ];
    for index in 0..4 {
        push_triangle(
            build,
            [center, points[index], points[(index + 1) % 4]],
            color,
        )?;
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn point_on_terrain(xz: [f32; 2], y_offset: f32) -> Vec3 {
    Vec3::new(xz[0], terrain_height_at(xz[0], xz[1]) + y_offset, xz[1])
}

fn push_quad(
    build: &mut CompositionBuild,
    points: [Vec3; 4],
    color: [f32; 4],
) -> Result<(), EngineError> {
    push_triangle(build, [points[0], points[1], points[2]], color)?;
    push_triangle(build, [points[0], points[2], points[3]], color)
}

fn push_triangle(
    build: &mut CompositionBuild,
    mut points: [Vec3; 3],
    color: [f32; 4],
) -> Result<(), EngineError> {
    let mut normal = (points[1] - points[0])
        .cross(points[2] - points[0])
        .normalize_or_zero();
    if normal.y < 0.0 {
        points.swap(1, 2);
        normal = -normal;
    }
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition("view_overlay.normals", "overlay triangle is degenerate");
    }
    let base =
        u32::try_from(build.vertices.len()).map_err(|_| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.view_overlay.vertex_count".to_string(),
            reason: "view overlays exceed u32 index space".to_string(),
        })?;
    for point in points {
        build.vertices.push(vertex(point, normal, color)?);
    }
    build.indices.extend([base, base + 1, base + 2]);
    Ok(())
}

fn vertex(position: Vec3, normal: Vec3, color: [f32; 4]) -> Result<EditorWorldVertex, EngineError> {
    if !position.is_finite() || !normal.is_finite() || color.iter().any(|value| !value.is_finite())
    {
        return invalid_composition(
            "view_overlay.vertex",
            "overlay vertex contains invalid data",
        );
    }
    Ok(EditorWorldVertex::new(
        position.to_array(),
        normal.to_array(),
        orthogonal_tangent(normal)?,
        WHITE_ATLAS_UV,
        color,
        [0.0, 0.48, 0.0, -1.0],
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
            "view_overlay.tangent",
            "failed to derive tangent from overlay normal",
        );
    }
    Ok(tangent.extend(1.0).to_array())
}
