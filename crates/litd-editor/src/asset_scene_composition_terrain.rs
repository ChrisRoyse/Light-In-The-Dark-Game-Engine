use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use super::{geometry::CompositionBuild, invalid_composition};

#[path = "asset_scene_composition_terrain_args.rs"]
mod asset_scene_composition_terrain_args;
use asset_scene_composition_terrain_args::*;

const RINGS: u32 = 30;
const SECTORS: u32 = 112;
pub(super) const ISLAND_X_RADIUS: f32 = 88.0;
pub(super) const ISLAND_Z_RADIUS: f32 = 66.0;
const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TerrainSurfaceQuad {
    pub center_xz: [f32; 2],
    pub yaw_degrees: f32,
    pub half_length: f32,
    pub half_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TerrainRiftQuad {
    pub surface: TerrainSurfaceQuad,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TerrainSelectionRing {
    pub center_xz: [f32; 2],
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub y_offset: f32,
}

pub(super) const PATH_SURFACES: [TerrainSurfaceQuad; 4] = [
    TerrainSurfaceQuad::new([-34.0, -22.0], -30.0, 25.0, 5.8),
    TerrainSurfaceQuad::new([-14.0, -11.0], -13.0, 25.0, 5.4),
    TerrainSurfaceQuad::new([9.0, 0.0], 28.0, 28.0, 5.9),
    TerrainSurfaceQuad::new([30.0, 15.0], 39.0, 22.0, 5.4),
];

pub(super) const RIFT_SURFACES: [TerrainRiftQuad; 8] = [
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([-70.0, -19.0], -33.0, 26.0, 0.34),
        color: [0.05, 0.64, 1.36, 2.15],
    },
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([68.0, 25.0], 41.0, 24.0, 0.32),
        color: [1.56, 0.42, 0.10, 1.95],
    },
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([-47.0, 48.0], 22.0, 19.0, 0.29),
        color: [0.05, 0.54, 1.18, 1.82],
    },
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([73.0, -36.0], -22.0, 18.0, 0.29),
        color: [1.38, 0.34, 0.10, 1.72],
    },
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([4.0, -62.0], 5.0, 21.0, 0.30),
        color: [0.05, 0.66, 1.44, 2.05],
    },
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([-5.0, 58.0], -8.0, 20.0, 0.30),
        color: [1.50, 0.46, 0.12, 1.90],
    },
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([-31.0, -50.0], -16.0, 16.0, 0.25),
        color: [0.05, 0.56, 1.24, 1.86],
    },
    TerrainRiftQuad {
        surface: TerrainSurfaceQuad::new([36.0, -46.0], 18.0, 15.0, 0.25),
        color: [1.42, 0.36, 0.10, 1.72],
    },
];

pub(super) const SELECTION_RINGS: [TerrainSelectionRing; 2] = [
    TerrainSelectionRing {
        center_xz: [4.0, 34.0],
        inner_radius: 8.65,
        outer_radius: 9.55,
        y_offset: 0.70,
    },
    TerrainSelectionRing {
        center_xz: [6.0, 16.0],
        inner_radius: 4.05,
        outer_radius: 4.50,
        y_offset: 0.72,
    },
];

pub(super) fn add_island_terrain(build: &mut CompositionBuild) -> Result<(), EngineError> {
    let start_primitives = build.mesh_primitives;
    add_island_top(build)?;
    add_cliff_skirt(build)?;
    add_energy_rifts(build)?;
    if build.mesh_primitives <= start_primitives {
        return invalid_composition("terrain", "island terrain added no primitives");
    }
    Ok(())
}

pub(super) fn add_path(build: &mut CompositionBuild) -> Result<(), EngineError> {
    for surface in PATH_SURFACES {
        add_surface_quad(AddSurfaceQuadArgs {
            build,
            center: surface.center_vec3(),
            half_length: surface.half_length,
            half_width: surface.half_width,
            yaw_degrees: surface.yaw_degrees,
            color: [0.075, 0.070, 0.072, 0.0],
            pbr: [0.0, 0.76, 0.0, 0.0],
            y_offset: 0.22,
        })?;
    }
    Ok(())
}

pub(super) fn add_selection_ring(build: &mut CompositionBuild) -> Result<(), EngineError> {
    for ring in SELECTION_RINGS {
        add_ring(
            build,
            Vec3::new(ring.center_xz[0], 0.0, ring.center_xz[1]),
            ring.inner_radius,
            ring.outer_radius,
            ring.y_offset,
        )?;
    }
    Ok(())
}

pub(super) fn island_outline_xz(segments: usize) -> Result<Vec<[f32; 2]>, EngineError> {
    if segments < 8 {
        return invalid_composition(
            "map.outline",
            "island map outline needs at least 8 segments",
        );
    }
    let mut points = Vec::with_capacity(segments);
    for segment in 0..segments {
        let angle = segment as f32 / segments as f32 * std::f32::consts::TAU;
        let scale = island_radius_scale(angle);
        points.push([
            angle.cos() * ISLAND_X_RADIUS * scale,
            angle.sin() * ISLAND_Z_RADIUS * scale,
        ]);
    }
    Ok(points)
}

fn add_island_top(build: &mut CompositionBuild) -> Result<(), EngineError> {
    for ring in 0..RINGS {
        for sector in 0..SECTORS {
            let s0 = sector;
            let s1 = (sector + 1) % SECTORS;
            let inner = ring as f32 / RINGS as f32;
            let outer = (ring + 1) as f32 / RINGS as f32;
            if ring == 0 {
                push_terrain_triangle(
                    build,
                    [
                        terrain_point(outer, s1),
                        terrain_point(inner, s0),
                        terrain_point(outer, s0),
                    ],
                    terrain_color(outer * 0.66),
                    [0.0, 0.88, 0.0, 0.0],
                )?;
            } else {
                let p00 = terrain_point(inner, s0);
                let p01 = terrain_point(inner, s1);
                let p10 = terrain_point(outer, s0);
                let p11 = terrain_point(outer, s1);
                let color = terrain_color((inner + outer) * 0.5);
                push_terrain_quad(build, [p01, p00, p10, p11], color, [0.0, 0.88, 0.0, 0.0])?;
            }
        }
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn add_cliff_skirt(build: &mut CompositionBuild) -> Result<(), EngineError> {
    for sector in 0..SECTORS {
        let s0 = sector;
        let s1 = (sector + 1) % SECTORS;
        let top0 = terrain_point(1.0, s0);
        let top1 = terrain_point(1.0, s1);
        let bottom0 = Vec3::new(top0.x * 0.93, -22.0, top0.z * 0.93);
        let bottom1 = Vec3::new(top1.x * 0.93, -22.0, top1.z * 0.93);
        push_quad(
            build,
            [top0, top1, bottom1, bottom0],
            [0.026, 0.034, 0.050, 0.0],
            [0.0, 0.82, 0.0, 0.0],
        )?;
    }
    build.mesh_primitives += 1;
    Ok(())
}

fn add_energy_rifts(build: &mut CompositionBuild) -> Result<(), EngineError> {
    for rift in RIFT_SURFACES {
        add_surface_quad(AddSurfaceQuadArgs {
            build,
            center: rift.surface.center_vec3(),
            half_length: rift.surface.half_length,
            half_width: rift.surface.half_width,
            yaw_degrees: rift.surface.yaw_degrees,
            color: rift.color,
            pbr: [0.0, 0.34, 0.0, 0.0],
            y_offset: 0.18,
        })?;
    }
    Ok(())
}

impl TerrainSurfaceQuad {
    pub const fn new(
        center_xz: [f32; 2],
        yaw_degrees: f32,
        half_length: f32,
        half_width: f32,
    ) -> Self {
        Self {
            center_xz,
            yaw_degrees,
            half_length,
            half_width,
        }
    }

    pub(super) fn points_xz(self) -> [[f32; 2]; 4] {
        let yaw = self.yaw_degrees.to_radians();
        let forward = Vec2::new(yaw.sin(), yaw.cos());
        let side = Vec2::new(forward.y, -forward.x);
        let center = Vec2::from_array(self.center_xz);
        [
            (center - forward * self.half_length - side * self.half_width).to_array(),
            (center + forward * self.half_length - side * self.half_width).to_array(),
            (center + forward * self.half_length + side * self.half_width).to_array(),
            (center - forward * self.half_length + side * self.half_width).to_array(),
        ]
    }

    fn center_vec3(self) -> Vec3 {
        Vec3::new(self.center_xz[0], 0.0, self.center_xz[1])
    }
}

fn add_ring(
    build: &mut CompositionBuild,
    center: Vec3,
    inner: f32,
    outer: f32,
    y_offset: f32,
) -> Result<(), EngineError> {
    let segments = 96_u32;
    let halo_pad = ((outer - inner) * 0.38).max(0.20);
    for segment in 0..segments {
        let a0 = segment as f32 / segments as f32 * std::f32::consts::TAU;
        let a1 = (segment + 1) as f32 / segments as f32 * std::f32::consts::TAU;
        push_quad(
            build,
            [
                ring_point(center, inner - halo_pad, a0, y_offset),
                ring_point(center, outer + halo_pad, a0, y_offset),
                ring_point(center, outer + halo_pad, a1, y_offset),
                ring_point(center, inner - halo_pad, a1, y_offset),
            ],
            [0.54, 0.30, 0.12, 0.34],
            [0.0, 0.64, 0.0, 0.0],
        )?;
        push_quad(
            build,
            [
                ring_point(center, inner, a0, y_offset + 0.10),
                ring_point(center, outer, a0, y_offset + 0.10),
                ring_point(center, outer, a1, y_offset + 0.10),
                ring_point(center, inner, a1, y_offset + 0.10),
            ],
            [1.62, 0.98, 0.34, 2.05],
            [0.0, 0.40, 0.0, 0.0],
        )?;
    }
    build.mesh_primitives += 2;
    Ok(())
}

fn add_surface_quad(args: AddSurfaceQuadArgs<'_>) -> Result<(), EngineError> {
    let AddSurfaceQuadArgs {
        build,
        center,
        half_length,
        half_width,
        yaw_degrees,
        color,
        pbr,
        y_offset,
    } = args;
    let yaw = yaw_degrees.to_radians();
    let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let side = Vec3::new(forward.z, 0.0, -forward.x);
    let mut points = [
        center - forward * half_length - side * half_width,
        center + forward * half_length - side * half_width,
        center + forward * half_length + side * half_width,
        center - forward * half_length + side * half_width,
    ];
    for point in &mut points {
        point.y = terrain_height_at(point.x, point.z) + y_offset;
    }
    push_quad(build, points, color, pbr)?;
    build.mesh_primitives += 1;
    Ok(())
}

fn terrain_point(radius: f32, sector: u32) -> Vec3 {
    let angle = sector as f32 / SECTORS as f32 * std::f32::consts::TAU;
    let edge_scale = island_radius_scale(angle);
    let x = angle.cos() * ISLAND_X_RADIUS * edge_scale * radius;
    let z = angle.sin() * ISLAND_Z_RADIUS * edge_scale * radius;
    Vec3::new(x, terrain_height(radius, x, z), z)
}

fn island_radius_scale(angle: f32) -> f32 {
    let broad_breaks = (angle * 3.0 + 0.45).sin() * 0.085;
    let cliff_notches = (angle * 5.0 - 0.25).cos() * 0.055;
    let small_chips = (angle * 9.0 + 1.7).sin() * 0.035;
    (1.0 + broad_breaks + cliff_notches + small_chips).clamp(0.84, 1.07)
}

pub(super) fn terrain_height_at(x: f32, z: f32) -> f32 {
    let radius = ((x / ISLAND_X_RADIUS).powi(2) + (z / ISLAND_Z_RADIUS).powi(2))
        .sqrt()
        .clamp(0.0, 1.0);
    terrain_height(radius, x, z)
}

fn terrain_height(radius: f32, x: f32, z: f32) -> f32 {
    let ridge = (x * 0.095).sin() * 0.55 + (z * 0.12).cos() * 0.38;
    let mound = (1.0 - radius * radius).max(0.0) * 2.5;
    let edge_drop = if radius > 0.78 {
        -13.0 * ((radius - 0.78) / 0.22).powf(1.35)
    } else {
        0.0
    };
    mound + ridge + edge_drop
}

fn terrain_color(radius: f32) -> [f32; 4] {
    let basalt = Vec3::new(0.020, 0.028, 0.044);
    let ash = Vec3::new(0.045, 0.052, 0.062);
    let cold_rock = Vec3::new(0.040, 0.060, 0.090);
    let rim = ((radius - 0.70) / 0.28).clamp(0.0, 1.0);
    let variation = ((radius * 18.0).sin() * 0.5 + 0.5) * 0.18;
    let interior = basalt.lerp(ash, variation);
    let color = interior.lerp(cold_rock, rim);
    [color.x, color.y, color.z, 0.0]
}

fn ring_point(center: Vec3, radius: f32, angle: f32, y_offset: f32) -> Vec3 {
    let x = center.x + angle.cos() * radius;
    let z = center.z + angle.sin() * radius;
    Vec3::new(x, terrain_height_at(x, z) + y_offset, z)
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

fn push_terrain_quad(
    build: &mut CompositionBuild,
    points: [Vec3; 4],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<(), EngineError> {
    push_terrain_triangle(build, [points[0], points[1], points[2]], color, pbr)?;
    push_terrain_triangle(build, [points[0], points[2], points[3]], color, pbr)
}

fn push_terrain_triangle(
    build: &mut CompositionBuild,
    points: [Vec3; 3],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<(), EngineError> {
    let base = checked_vertex_base(build.vertices.len())?;
    for point in points {
        let normal = terrain_normal_at(point.x, point.z)?;
        build
            .vertices
            .push(vertex(point, normal, WHITE_ATLAS_UV, color, pbr)?);
    }
    build.indices.extend([base, base + 1, base + 2]);
    Ok(())
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
        return invalid_composition("terrain.normals", "terrain triangle is degenerate");
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

fn terrain_normal_at(x: f32, z: f32) -> Result<Vec3, EngineError> {
    let step = 1.25_f32;
    let left = terrain_height_at(x - step, z);
    let right = terrain_height_at(x + step, z);
    let back = terrain_height_at(x, z - step);
    let front = terrain_height_at(x, z + step);
    let normal = Vec3::new(left - right, step * 2.0, back - front).normalize_or_zero();
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition("terrain.normals", "terrain normal is invalid");
    }
    Ok(normal)
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
        return invalid_composition("terrain.vertex", "terrain vertex contains invalid data");
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
        return invalid_composition("terrain.tangent", "failed to derive tangent from normal");
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.terrain.vertex_count".to_string(),
        reason: "terrain exceeds u32 index space".to_string(),
    })
}
