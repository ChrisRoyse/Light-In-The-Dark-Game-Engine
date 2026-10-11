use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use crate::asset_scene::EditorWorldMapPoint;

use super::{
    geometry::CompositionBuild,
    invalid_composition,
    terrain::{SELECTION_RINGS, TerrainSelectionRing, terrain_height_at},
};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];
const HALO_SEGMENTS: u32 = 96;

pub(super) fn add_selection_halo_layer(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    for ring in SELECTION_RINGS {
        add_halo_band(
            build,
            ring,
            ring.outer_radius + 0.30,
            ring.outer_radius + 0.68,
            ring.y_offset + 0.38,
            [1.80, 1.08, 0.30, 2.35],
        )?;
        add_halo_band(
            build,
            ring,
            (ring.inner_radius * 0.18).max(0.66),
            (ring.inner_radius * 0.26).max(0.92),
            ring.y_offset + 2.18,
            [1.22, 0.82, 0.30, 1.70],
        )?;
        add_cardinal_ticks(build, ring)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition(
            "selection_halo",
            "ED11 selection halo layer added no primitives",
        );
    }
    Ok(())
}

pub(super) fn selection_halo_map_points() -> Vec<EditorWorldMapPoint> {
    SELECTION_RINGS
        .into_iter()
        .map(|ring| EditorWorldMapPoint {
            kind: "ed11_selection_halo".to_string(),
            world_xz: ring.center_xz,
            radius: ring.outer_radius + 0.68,
        })
        .collect()
}

fn add_halo_band(
    build: &mut CompositionBuild,
    ring: TerrainSelectionRing,
    inner: f32,
    outer: f32,
    y_offset: f32,
    color: [f32; 4],
) -> Result<(), EngineError> {
    for segment in 0..HALO_SEGMENTS {
        let a0 = segment as f32 / HALO_SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (segment + 1) as f32 / HALO_SEGMENTS as f32 * std::f32::consts::TAU;
        push_quad(
            build,
            [
                ring_point(ring.center_xz, inner, a0, y_offset),
                ring_point(ring.center_xz, outer, a0, y_offset),
                ring_point(ring.center_xz, outer, a1, y_offset),
                ring_point(ring.center_xz, inner, a1, y_offset),
            ],
            color,
        )?;
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn add_cardinal_ticks(
    build: &mut CompositionBuild,
    ring: TerrainSelectionRing,
) -> Result<(), EngineError> {
    for index in 0..4 {
        let angle = index as f32 * std::f32::consts::FRAC_PI_2;
        let radial = Vec2::new(angle.cos(), angle.sin());
        let tangent = Vec2::new(-angle.sin(), angle.cos());
        let inner = ring.outer_radius + 0.92;
        let outer = ring.outer_radius + 2.20;
        let half_width = ((ring.outer_radius - ring.inner_radius) * 0.45).max(0.26);
        let center = Vec2::from_array(ring.center_xz);
        let points = [
            center + radial * inner - tangent * half_width,
            center + radial * outer - tangent * half_width,
            center + radial * outer + tangent * half_width,
            center + radial * inner + tangent * half_width,
        ];
        push_quad(
            build,
            points.map(|point| tick_point(point.to_array(), ring.y_offset + 0.52)),
            [2.05, 1.30, 0.38, 2.55],
        )?;
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn ring_point(center_xz: [f32; 2], radius: f32, angle: f32, y_offset: f32) -> Vec3 {
    let x = center_xz[0] + angle.cos() * radius;
    let z = center_xz[1] + angle.sin() * radius;
    Vec3::new(x, terrain_height_at(x, z) + y_offset, z)
}

fn tick_point(xz: [f32; 2], y_offset: f32) -> Vec3 {
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
        return invalid_composition(
            "selection_halo.normals",
            "selection halo triangle is degenerate",
        );
    }
    let base = checked_vertex_base(build.vertices.len())?;
    for point in points {
        build
            .vertices
            .push(vertex(point, normal, WHITE_ATLAS_UV, color)?);
    }
    build.indices.extend([base, base + 1, base + 2]);
    Ok(())
}

fn vertex(
    position: Vec3,
    normal: Vec3,
    uv: [f32; 2],
    color: [f32; 4],
) -> Result<EditorWorldVertex, EngineError> {
    if !position.is_finite()
        || !normal.is_finite()
        || !Vec2::from_array(uv).is_finite()
        || color.iter().any(|value| !value.is_finite())
    {
        return invalid_composition(
            "selection_halo.vertex",
            "selection halo vertex contains invalid data",
        );
    }
    Ok(EditorWorldVertex::new(
        position.to_array(),
        normal.to_array(),
        orthogonal_tangent(normal)?,
        uv,
        color,
        [0.0, 0.34, 0.0, 0.0],
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
            "selection_halo.tangent",
            "failed to derive tangent from selection halo normal",
        );
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.selection_halo.vertex_count".to_string(),
        reason: "selection halo exceeds u32 index space".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_halo_adds_bounded_editor_gizmo_primitives() {
        let mut build = CompositionBuild::default();
        add_selection_halo_layer(&mut build).unwrap();

        assert_eq!(build.mesh_primitives, (SELECTION_RINGS.len() * 3) as u32);
        assert_eq!(build.indices.len() % 3, 0);
        assert!(build.vertices.len() < 2_500);
        assert!(
            build
                .vertices
                .iter()
                .any(|vertex| vertex.color_emissive[3] >= 2.50)
        );
        assert_eq!(selection_halo_map_points().len(), SELECTION_RINGS.len());
        assert!(
            selection_halo_map_points()
                .iter()
                .all(|point| point.kind == "ed11_selection_halo")
        );
    }
}
