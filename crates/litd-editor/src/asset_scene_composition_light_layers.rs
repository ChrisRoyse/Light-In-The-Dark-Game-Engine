use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use crate::asset_scene::EditorWorldMapPoint;

use super::{geometry::CompositionBuild, invalid_composition, terrain::terrain_height_at};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];
const POOL_SEGMENTS: u32 = 32;

#[derive(Debug, Clone, Copy)]
struct LightPool {
    center_xz: [f32; 2],
    radius_x: f32,
    radius_z: f32,
    yaw_degrees: f32,
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct FogVeil {
    center: [f32; 3],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
}

const LIGHT_POOLS: [LightPool; 12] = [
    warm_pool([1.0, -18.0], 14.0, 5.2, 8.0, 0.34),
    warm_pool([-23.0, -29.0], 8.0, 3.4, -18.0, 0.24),
    warm_pool([24.0, -28.0], 8.0, 3.4, 20.0, 0.24),
    warm_pool([-7.0, -46.0], 7.0, 2.9, -4.0, 0.20),
    warm_pool([-17.0, 12.0], 7.5, 3.0, 86.0, 0.18),
    warm_pool([18.0, 13.0], 7.5, 3.0, 92.0, 0.18),
    warm_pool([7.0, -4.0], 8.0, 3.1, 31.0, 0.22),
    warm_pool([0.0, -34.0], 9.0, 3.2, 0.0, 0.18),
    cool_pool([-45.0, -42.0], 9.0, 3.0, -35.0, 0.28),
    cool_pool([44.0, -43.0], 9.0, 3.0, 35.0, 0.28),
    cool_pool([-58.0, 8.0], 7.5, 2.7, 70.0, 0.22),
    cool_pool([57.0, 6.0], 7.5, 2.7, -72.0, 0.22),
];

const FOG_VEILS: [FogVeil; 9] = [
    veil([-52.0, 15.0, -63.0], 7.0, 22.0, 32.0, 0.20),
    veil([-27.0, 20.0, -71.0], -4.0, 28.0, 40.0, 0.24),
    veil([0.0, 22.0, -76.0], 0.0, 34.0, 44.0, 0.27),
    veil([30.0, 19.0, -70.0], 4.0, 30.0, 39.0, 0.24),
    veil([55.0, 14.0, -60.0], -7.0, 24.0, 31.0, 0.20),
    veil([-77.0, 2.0, 7.0], 72.0, 20.0, 26.0, 0.18),
    veil([78.0, 2.0, 4.0], -70.0, 20.0, 26.0, 0.18),
    veil([-46.0, -8.0, 56.0], -18.0, 18.0, 22.0, 0.17),
    veil([48.0, -8.0, 54.0], 18.0, 18.0, 22.0, 0.17),
];

const DEPTH_FOG_CARDS: [FogVeil; 12] = [
    depth_card([-68.0, 28.0, -52.0], 12.0, 30.0, 26.0, 0.18),
    depth_card([-34.0, 30.0, -58.0], -8.0, 34.0, 30.0, 0.22),
    depth_card([2.0, 34.0, -62.0], 0.0, 42.0, 34.0, 0.24),
    depth_card([38.0, 28.0, -56.0], 8.0, 34.0, 30.0, 0.21),
    depth_card([70.0, 22.0, -46.0], -14.0, 30.0, 25.0, 0.18),
    depth_card([-76.0, 6.0, -8.0], 64.0, 24.0, 22.0, 0.16),
    depth_card([76.0, 7.0, -6.0], -62.0, 24.0, 22.0, 0.16),
    depth_card([-54.0, 4.0, 32.0], -28.0, 26.0, 18.0, 0.14),
    depth_card([55.0, 4.0, 30.0], 28.0, 26.0, 18.0, 0.14),
    depth_card([-18.0, 7.0, 44.0], -6.0, 28.0, 18.0, 0.13),
    depth_card([18.0, 7.0, 44.0], 6.0, 28.0, 18.0, 0.13),
    depth_card([0.0, 13.0, 18.0], 0.0, 36.0, 20.0, 0.12),
];

pub(super) fn add_light_pool_layers(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    for pool in LIGHT_POOLS {
        add_light_pool(build, pool)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition("light_layers", "light pools added no primitives");
    }
    Ok(())
}

pub(super) fn add_fog_effect_layers(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    for veil in FOG_VEILS {
        add_fog_veil(build, veil)?;
    }
    for card in DEPTH_FOG_CARDS {
        add_fog_veil(build, card)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition("light_layers", "fog effect layers added no primitives");
    }
    Ok(())
}

pub(super) fn light_layer_map_points() -> Vec<EditorWorldMapPoint> {
    let mut points =
        Vec::with_capacity(LIGHT_POOLS.len() + FOG_VEILS.len() + DEPTH_FOG_CARDS.len());
    points.extend(LIGHT_POOLS.iter().map(|pool| EditorWorldMapPoint {
        kind: light_pool_kind(*pool).to_string(),
        world_xz: pool.center_xz,
        radius: pool.radius_x.max(pool.radius_z),
    }));
    points.extend(FOG_VEILS.iter().map(|veil| EditorWorldMapPoint {
        kind: "atmospheric_fog_veil".to_string(),
        world_xz: [veil.center[0], veil.center[2]],
        radius: (veil.width.max(veil.height) * 0.25).max(1.0),
    }));
    points.extend(DEPTH_FOG_CARDS.iter().map(|card| EditorWorldMapPoint {
        kind: "ed11_depth_fog_card".to_string(),
        world_xz: [card.center[0], card.center[2]],
        radius: (card.width.max(card.height) * 0.25).max(1.0),
    }));
    points
}

fn add_light_pool(build: &mut CompositionBuild, pool: LightPool) -> Result<(), EngineError> {
    let center = pool_point(pool.center_xz, pool, 0.36);
    for segment in 0..POOL_SEGMENTS {
        let a0 = segment as f32 / POOL_SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (segment + 1) as f32 / POOL_SEGMENTS as f32 * std::f32::consts::TAU;
        push_triangle(
            build,
            [center, pool_edge(pool, a0), pool_edge(pool, a1)],
            pool.color,
            glow_pbr(),
        )?;
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn add_fog_veil(build: &mut CompositionBuild, veil: FogVeil) -> Result<(), EngineError> {
    let yaw = veil.yaw_degrees.to_radians();
    let side = Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * (veil.width * 0.5);
    let bottom = Vec3::from_array(veil.center) - Vec3::Y * (veil.height * 0.5);
    let top = bottom + Vec3::Y * veil.height;
    let corners = [bottom - side, bottom + side, top + side, top - side];
    let center = (bottom + top) * 0.5;
    let center_color = fog_center_color(veil.color);
    let edge_color = fog_edge_color(veil.color);
    for index in 0..4 {
        push_triangle_colored(
            build,
            [center, corners[index], corners[(index + 1) % 4]],
            [center_color, edge_color, edge_color],
            fog_pbr(),
        )?;
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn pool_edge(pool: LightPool, angle: f32) -> Vec3 {
    let yaw = pool.yaw_degrees.to_radians();
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    let side = Vec2::new(forward.y, -forward.x);
    let offset = side * angle.cos() * pool.radius_x + forward * angle.sin() * pool.radius_z;
    let center = Vec2::from_array(pool.center_xz);
    pool_point((center + offset).to_array(), pool, 0.30)
}

fn pool_point(xz: [f32; 2], pool: LightPool, y_offset: f32) -> Vec3 {
    let y = terrain_height_at(xz[0], xz[1]) + y_offset + pool.color[3] * 0.035;
    Vec3::new(xz[0], y, xz[1])
}

fn push_triangle(
    build: &mut CompositionBuild,
    points: [Vec3; 3],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<(), EngineError> {
    push_triangle_colored(build, points, [color; 3], pbr)
}

fn push_triangle_colored(
    build: &mut CompositionBuild,
    mut points: [Vec3; 3],
    mut colors: [[f32; 4]; 3],
    pbr: [f32; 4],
) -> Result<(), EngineError> {
    let mut normal = (points[1] - points[0])
        .cross(points[2] - points[0])
        .normalize_or_zero();
    if normal.y < 0.0 {
        points.swap(1, 2);
        colors.swap(1, 2);
        normal = -normal;
    }
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition("light_layers.normals", "light layer triangle is degenerate");
    }
    let base = checked_vertex_base(build.vertices.len())?;
    for index in 0..3 {
        build.vertices.push(vertex(
            points[index],
            normal,
            WHITE_ATLAS_UV,
            colors[index],
            pbr,
        )?);
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
        return invalid_composition("light_layers.vertex", "light layer vertex is invalid");
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
            "light_layers.tangent",
            "failed to derive tangent from normal",
        );
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.light_layers.vertex_count".to_string(),
        reason: "light layers exceed u32 index space".to_string(),
    })
}

const fn warm_pool(
    center_xz: [f32; 2],
    radius_x: f32,
    radius_z: f32,
    yaw_degrees: f32,
    strength: f32,
) -> LightPool {
    LightPool {
        center_xz,
        radius_x,
        radius_z,
        yaw_degrees,
        color: [0.74, 0.30, 0.09, strength],
    }
}

const fn cool_pool(
    center_xz: [f32; 2],
    radius_x: f32,
    radius_z: f32,
    yaw_degrees: f32,
    strength: f32,
) -> LightPool {
    LightPool {
        center_xz,
        radius_x,
        radius_z,
        yaw_degrees,
        color: [0.07, 0.48, 0.90, strength],
    }
}

const fn veil(
    center: [f32; 3],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    strength: f32,
) -> FogVeil {
    FogVeil {
        center,
        yaw_degrees,
        width,
        height,
        color: [0.035, 0.105, 0.210, strength],
    }
}

const fn depth_card(
    center: [f32; 3],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    strength: f32,
) -> FogVeil {
    FogVeil {
        center,
        yaw_degrees,
        width,
        height,
        color: [0.060, 0.125, 0.190, strength],
    }
}

fn light_pool_kind(pool: LightPool) -> &'static str {
    if pool.color[2] > pool.color[0] {
        "cool_light_pool"
    } else {
        "warm_light_pool"
    }
}

fn fog_center_color(color: [f32; 4]) -> [f32; 4] {
    [
        (color[0] * 0.92).min(0.10),
        (color[1] * 0.88).min(0.16),
        (color[2] * 0.82).min(0.26),
        (color[3] * 0.52).min(0.22),
    ]
}

fn fog_edge_color(color: [f32; 4]) -> [f32; 4] {
    [
        (color[0] * 0.38).max(0.010),
        (color[1] * 0.32).max(0.020),
        (color[2] * 0.24).max(0.035),
        color[3] * 0.0,
    ]
}

const fn glow_pbr() -> [f32; 4] {
    [0.0, 0.50, 0.0, 0.0]
}

const fn fog_pbr() -> [f32; 4] {
    [0.0, 0.82, 0.0, -1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atmospheric_light_layers_add_bounded_renderable_primitives() {
        let mut build = CompositionBuild::default();
        add_light_pool_layers(&mut build).unwrap();
        add_fog_effect_layers(&mut build).unwrap();

        assert_eq!(
            build.mesh_primitives,
            (LIGHT_POOLS.len() + FOG_VEILS.len() + DEPTH_FOG_CARDS.len()) as u32
        );
        assert_eq!(build.indices.len() % 3, 0);
        assert!(build.vertices.len() < 1_800);
        assert!(
            build
                .vertices
                .iter()
                .any(|vertex| vertex.color_emissive[3] >= 0.30)
        );
        assert_eq!(
            light_layer_map_points().len(),
            LIGHT_POOLS.len() + FOG_VEILS.len() + DEPTH_FOG_CARDS.len()
        );
        assert!(
            light_layer_map_points()
                .iter()
                .any(|point| point.kind == "cool_light_pool")
        );
        assert!(
            light_layer_map_points()
                .iter()
                .any(|point| point.kind == "atmospheric_fog_veil")
        );
        assert_eq!(
            light_layer_map_points()
                .iter()
                .filter(|point| point.kind == "ed11_depth_fog_card")
                .count(),
            DEPTH_FOG_CARDS.len()
        );
    }
}
