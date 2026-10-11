use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use crate::asset_scene::{EditorWorldMapArea, EditorWorldMapPoint};

use super::{geometry::CompositionBuild, invalid_composition, terrain::terrain_height_at};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];

#[derive(Debug, Clone, Copy)]
struct RiftBeacon {
    kind: &'static str,
    base_xz: [f32; 2],
    yaw_degrees: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct RiftFissure {
    kind: &'static str,
    start_xz: [f32; 2],
    end_xz: [f32; 2],
    width: f32,
    color: [f32; 4],
}

const RIFT_BEACONS: [RiftBeacon; 6] = [
    blue_beacon([-68.0, -24.0], -32.0, 2.0, 34.0),
    lava_beacon([70.0, 28.0], 38.0, 1.7, 28.0),
    blue_beacon([-49.0, 47.0], 18.0, 1.5, 24.0),
    lava_beacon([75.0, -38.0], -20.0, 1.4, 22.0),
    blue_beacon([5.0, -63.0], 4.0, 1.6, 27.0),
    lava_beacon([-4.0, 58.0], -9.0, 1.5, 25.0),
];

const RIFT_FISSURES: [RiftFissure; 12] = [
    blue_fissure([-76.0, -29.0], [-66.0, -23.0], 3.1),
    blue_fissure([-66.0, -23.0], [-56.0, -27.0], 2.2),
    blue_fissure([-64.0, -22.0], [-59.0, -12.0], 1.7),
    blue_fissure([-52.0, 41.0], [-43.0, 49.0], 2.5),
    blue_fissure([-45.0, 48.0], [-35.0, 45.0], 1.8),
    blue_fissure([-3.0, -65.0], [10.0, -58.0], 2.4),
    lava_fissure([61.0, 24.0], [73.0, 31.0], 3.0),
    lava_fissure([71.0, 31.0], [83.0, 26.0], 2.1),
    lava_fissure([68.0, 30.0], [65.0, 40.0], 1.6),
    lava_fissure([66.0, -39.0], [78.0, -34.0], 2.5),
    lava_fissure([76.0, -34.0], [84.0, -43.0], 1.8),
    lava_fissure([-12.0, 55.0], [0.0, 62.0], 2.2),
];

pub(super) fn add_rift_beacon_layers(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    for beacon in RIFT_BEACONS {
        add_rift_beacon(build, beacon)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition("rift_beacons", "rift beacons added no primitives");
    }
    Ok(())
}

pub(super) fn add_rift_fissure_layers(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    for fissure in RIFT_FISSURES {
        add_rift_fissure(build, fissure)?;
    }
    if build.mesh_primitives <= start_primitives {
        return invalid_composition("rift_fissures", "rift fissures added no primitives");
    }
    Ok(())
}

pub(super) fn rift_beacon_map_points() -> Vec<EditorWorldMapPoint> {
    RIFT_BEACONS
        .iter()
        .map(|beacon| EditorWorldMapPoint {
            kind: beacon.kind.to_string(),
            world_xz: beacon.base_xz,
            radius: (beacon.width * 2.8).max(3.5),
        })
        .collect()
}

pub(super) fn rift_fissure_map_areas() -> Result<Vec<EditorWorldMapArea>, EngineError> {
    RIFT_FISSURES
        .iter()
        .map(|fissure| {
            Ok(EditorWorldMapArea {
                kind: fissure.kind.to_string(),
                points: fissure_points_xz(*fissure, fissure.width * 1.35)?,
            })
        })
        .collect()
}

fn add_rift_beacon(build: &mut CompositionBuild, beacon: RiftBeacon) -> Result<(), EngineError> {
    let base_y = terrain_height_at(beacon.base_xz[0], beacon.base_xz[1]) + 0.6;
    let base = Vec3::new(beacon.base_xz[0], base_y, beacon.base_xz[1]);
    let yaw = beacon.yaw_degrees.to_radians();
    let side = Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * (beacon.width * 0.5);
    let up = Vec3::Y * beacon.height;
    let center = base + up * 0.48;
    let inner = beacon_inner_color(beacon.color);
    let outer = beacon_outer_color(beacon.color);

    push_triangle_colored(
        build,
        [base - side, base + side, center],
        [outer, outer, inner],
        glow_pbr(),
    )?;
    push_triangle_colored(
        build,
        [base + side, base - side, base + up],
        [outer, outer, outer],
        glow_pbr(),
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn add_rift_fissure(build: &mut CompositionBuild, fissure: RiftFissure) -> Result<(), EngineError> {
    let outer = fissure_points_3d(fissure, fissure.width * 1.25, 0.16)?;
    let core = fissure_points_3d(fissure, fissure.width * 0.42, 0.19)?;
    push_quad_colored(
        build,
        outer,
        [fissure_outer_color(fissure.color); 4],
        glow_pbr(),
    )?;
    build.mesh_primitives += 1;
    push_quad_colored(
        build,
        core,
        [fissure_core_color(fissure.color); 4],
        [0.0, 0.34, 0.0, 0.0],
    )?;
    build.mesh_primitives += 1;
    Ok(())
}

fn push_quad_colored(
    build: &mut CompositionBuild,
    points: [Vec3; 4],
    colors: [[f32; 4]; 4],
    pbr: [f32; 4],
) -> Result<(), EngineError> {
    push_triangle_colored(
        build,
        [points[0], points[1], points[2]],
        [colors[0], colors[1], colors[2]],
        pbr,
    )?;
    push_triangle_colored(
        build,
        [points[0], points[2], points[3]],
        [colors[0], colors[2], colors[3]],
        pbr,
    )
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
    if normal.y < -0.05 {
        points.swap(1, 2);
        colors.swap(1, 2);
        normal = -normal;
    }
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition("ed11_vfx.normals", "ED11 VFX triangle is degenerate");
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
        return invalid_composition("ed11_vfx.vertex", "ED11 VFX vertex is invalid");
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
            "ed11_vfx.tangent",
            "failed to derive tangent from ED11 VFX normal",
        );
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.rift_beacons.vertex_count".to_string(),
        reason: "rift beacons exceed u32 index space".to_string(),
    })
}

const fn blue_beacon(base_xz: [f32; 2], yaw_degrees: f32, width: f32, height: f32) -> RiftBeacon {
    RiftBeacon {
        kind: "ed11_blue_rift_beacon",
        base_xz,
        yaw_degrees,
        width,
        height,
        color: [0.05, 0.68, 1.55, 3.20],
    }
}

const fn lava_beacon(base_xz: [f32; 2], yaw_degrees: f32, width: f32, height: f32) -> RiftBeacon {
    RiftBeacon {
        kind: "ed11_lava_rift_beacon",
        base_xz,
        yaw_degrees,
        width,
        height,
        color: [1.68, 0.42, 0.08, 2.85],
    }
}

const fn blue_fissure(start_xz: [f32; 2], end_xz: [f32; 2], width: f32) -> RiftFissure {
    RiftFissure {
        kind: "ed11_blue_branch_fissure",
        start_xz,
        end_xz,
        width,
        color: [0.05, 0.62, 1.48, 2.65],
    }
}

const fn lava_fissure(start_xz: [f32; 2], end_xz: [f32; 2], width: f32) -> RiftFissure {
    RiftFissure {
        kind: "ed11_lava_branch_fissure",
        start_xz,
        end_xz,
        width,
        color: [1.62, 0.36, 0.06, 2.45],
    }
}

fn beacon_inner_color(color: [f32; 4]) -> [f32; 4] {
    [
        (color[0] * 1.18).min(2.0),
        (color[1] * 1.16).min(2.0),
        (color[2] * 1.12).min(2.0),
        (color[3] * 1.08).min(4.0),
    ]
}

fn beacon_outer_color(color: [f32; 4]) -> [f32; 4] {
    [color[0] * 0.48, color[1] * 0.50, color[2] * 0.52, 0.0]
}

fn fissure_core_color(color: [f32; 4]) -> [f32; 4] {
    [
        (color[0] * 1.18).min(2.0),
        (color[1] * 1.12).min(2.0),
        (color[2] * 1.10).min(2.0),
        (color[3] * 1.04).min(3.4),
    ]
}

fn fissure_outer_color(color: [f32; 4]) -> [f32; 4] {
    [
        color[0] * 0.30,
        color[1] * 0.32,
        color[2] * 0.34,
        color[3] * 0.32,
    ]
}

fn fissure_points_3d(
    fissure: RiftFissure,
    width: f32,
    height_offset: f32,
) -> Result<[Vec3; 4], EngineError> {
    let points = fissure_points_xz(fissure, width)?;
    Ok(points.map(|point| {
        Vec3::new(
            point[0],
            terrain_height_at(point[0], point[1]) + height_offset,
            point[1],
        )
    }))
}

fn fissure_points_xz(fissure: RiftFissure, width: f32) -> Result<[[f32; 2]; 4], EngineError> {
    if !width.is_finite() || width <= 0.0 {
        return invalid_composition("rift_fissures.width", "rift fissure width must be positive");
    }
    let start = Vec2::from_array(fissure.start_xz);
    let end = Vec2::from_array(fissure.end_xz);
    if !start.is_finite() || !end.is_finite() {
        return invalid_composition(
            "rift_fissures.points",
            "rift fissure endpoints must be finite",
        );
    }
    let delta = end - start;
    let length = delta.length();
    if !length.is_finite() || length <= 0.01 {
        return invalid_composition(
            "rift_fissures.length",
            "rift fissure segment length must be positive",
        );
    }
    let side = Vec2::new(-delta.y, delta.x) * (width * 0.5 / length);
    Ok([
        (start + side).to_array(),
        (start - side).to_array(),
        (end - side).to_array(),
        (end + side).to_array(),
    ])
}

const fn glow_pbr() -> [f32; 4] {
    [0.0, 0.42, 0.0, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rift_beacon_layers_add_blue_and_lava_focal_primitives() {
        let mut build = CompositionBuild::default();
        add_rift_beacon_layers(&mut build).unwrap();

        assert_eq!(build.mesh_primitives, RIFT_BEACONS.len() as u32);
        assert_eq!(build.indices.len(), RIFT_BEACONS.len() * 6);
        assert!(build.vertices.iter().any(|vertex| {
            vertex.color_emissive[2] > vertex.color_emissive[0] && vertex.color_emissive[3] >= 3.0
        }));
        assert!(build.vertices.iter().any(|vertex| {
            vertex.color_emissive[0] > vertex.color_emissive[2] && vertex.color_emissive[3] >= 2.8
        }));
        let points = rift_beacon_map_points();
        assert_eq!(points.len(), RIFT_BEACONS.len());
        assert!(
            points
                .iter()
                .any(|point| point.kind == "ed11_blue_rift_beacon")
        );
        assert!(
            points
                .iter()
                .any(|point| point.kind == "ed11_lava_rift_beacon")
        );
    }

    #[test]
    fn rift_fissure_layers_add_branching_ground_ribbons() {
        let mut build = CompositionBuild::default();
        add_rift_fissure_layers(&mut build).unwrap();

        assert_eq!(build.mesh_primitives, (RIFT_FISSURES.len() * 2) as u32);
        assert_eq!(build.indices.len(), RIFT_FISSURES.len() * 12);
        assert!(build.vertices.iter().any(|vertex| {
            vertex.color_emissive[2] > vertex.color_emissive[0] && vertex.color_emissive[3] >= 2.6
        }));
        assert!(build.vertices.iter().any(|vertex| {
            vertex.color_emissive[0] > vertex.color_emissive[2] && vertex.color_emissive[3] >= 2.4
        }));

        let areas = rift_fissure_map_areas().unwrap();
        assert_eq!(areas.len(), RIFT_FISSURES.len());
        assert_eq!(
            areas
                .iter()
                .filter(|area| area.kind == "ed11_blue_branch_fissure")
                .count(),
            6
        );
        assert_eq!(
            areas
                .iter()
                .filter(|area| area.kind == "ed11_lava_branch_fissure")
                .count(),
            6
        );
    }
}
