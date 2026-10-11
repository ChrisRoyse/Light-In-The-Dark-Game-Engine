use crate::math::Vec3;

use super::{
    EditorWorldSceneBuffers, EditorWorldSceneBuffersNewArgs, EditorWorldTextureData,
    EditorWorldVertex,
};

struct MeshBuilder {
    vertices: Vec<EditorWorldVertex>,
    indices: Vec<u32>,
    mesh_primitives: u32,
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            mesh_primitives: 0,
        }
    }

    fn finish(self) -> EditorWorldSceneBuffers {
        EditorWorldSceneBuffers::new(EditorWorldSceneBuffersNewArgs {
            scene_name: "ed11_reference_ruined_island_mesh_scene",
            source_id: "procedural.ed11_reference",
            source_path: "crates/litd-engine/src/render/editor_world_reference_scene.rs",
            dependency_count: 0,
            material_count: 4,
            texture_count: 3,
            image_count: 3,
            vertices: self.vertices,
            indices: self.indices,
            mesh_primitives: self.mesh_primitives,
            base_color_texture: reference_base_color_texture(),
            normal_texture: reference_normal_texture(),
            metallic_roughness_texture: reference_metallic_roughness_texture(),
        })
        .expect("hard-coded reference editor world scene must be valid")
    }

    fn push_vertex(&mut self, position: Vec3, normal: Vec3, color: [f32; 3], emissive: f32) -> u32 {
        let index = self.vertices.len() as u32;
        self.vertices.push(EditorWorldVertex {
            position: position.to_array(),
            normal: normal.normalize_or_zero().to_array(),
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
            color_emissive: [color[0], color[1], color[2], emissive],
            material_pbr: [0.0, 0.72, 1.0, 0.0],
        });
        index
    }

    fn add_triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 3], emissive: f32) {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let base = [
            self.push_vertex(a, normal, color, emissive),
            self.push_vertex(b, normal, color, emissive),
            self.push_vertex(c, normal, color, emissive),
        ];
        self.indices.extend(base);
        self.mesh_primitives += 1;
    }

    fn add_quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 3], emissive: f32) {
        self.add_triangle(a, b, c, color, emissive);
        self.add_triangle(a, c, d, color, emissive);
    }
}

pub(super) fn build_reference_scene() -> EditorWorldSceneBuffers {
    let mut b = MeshBuilder::new();
    add_island(&mut b);
    add_ruins(&mut b);
    add_energy_rifts(&mut b);
    add_selection_ring(&mut b, Vec3::new(17.0, 0.35, 14.0), 4.8, 7.2);
    b.finish()
}

fn reference_base_color_texture() -> EditorWorldTextureData {
    EditorWorldTextureData::new(
        "ed11_reference_base_color",
        "crates/litd-engine/src/render/editor_world_reference_scene.rs#reference_base_color_texture",
        2,
        2,
        vec![
            72, 78, 80, 255, 38, 46, 52, 255, 96, 80, 56, 255, 24, 36, 44, 255,
        ],
    )
    .expect("hard-coded reference texture must be valid")
}

fn reference_normal_texture() -> EditorWorldTextureData {
    EditorWorldTextureData::new(
        "ed11_reference_normal",
        "crates/litd-engine/src/render/editor_world_reference_scene.rs#reference_normal_texture",
        2,
        2,
        vec![
            128, 128, 255, 255, 128, 128, 255, 255, 128, 128, 255, 255, 128, 128, 255, 255,
        ],
    )
    .expect("hard-coded reference normal texture must be valid")
}

fn reference_metallic_roughness_texture() -> EditorWorldTextureData {
    EditorWorldTextureData::new(
        "ed11_reference_metallic_roughness",
        "crates/litd-engine/src/render/editor_world_reference_scene.rs#reference_metallic_roughness_texture",
        2,
        2,
        vec![
            255, 184, 0, 255, 255, 184, 0, 255, 255, 184, 0, 255, 255, 184, 0, 255,
        ],
    )
    .expect("hard-coded reference metallic-roughness texture must be valid")
}

fn add_island(b: &mut MeshBuilder) {
    let points = [
        Vec3::new(-56.0, 0.0, -23.0),
        Vec3::new(-34.0, 0.0, 30.0),
        Vec3::new(4.0, 0.0, 43.0),
        Vec3::new(42.0, 0.0, 29.0),
        Vec3::new(61.0, 0.0, -7.0),
        Vec3::new(35.0, 0.0, -36.0),
        Vec3::new(-19.0, 0.0, -42.0),
        Vec3::new(-51.0, 0.0, -31.0),
    ];
    let top = [0.33, 0.36, 0.33];
    for i in 1..points.len() - 1 {
        b.add_triangle(points[0], points[i], points[i + 1], top, 0.0);
    }
    let side = [0.12, 0.14, 0.15];
    for i in 0..points.len() {
        let next = (i + 1) % points.len();
        let a = points[i];
        let d = points[next];
        let b0 = Vec3::new(a.x * 1.08, -14.0, a.z * 1.08);
        let c = Vec3::new(d.x * 1.08, -14.0, d.z * 1.08);
        b.add_quad(a, b0, c, d, side, 0.0);
    }
}

fn add_ruins(b: &mut MeshBuilder) {
    let stone = [0.23, 0.25, 0.26];
    let dark = [0.10, 0.12, 0.15];
    let roof = [0.06, 0.08, 0.11];
    add_box(
        b,
        Vec3::new(0.0, 5.2, -8.0),
        Vec3::new(13.0, 5.2, 8.0),
        stone,
        0.0,
    );
    add_box(
        b,
        Vec3::new(0.0, 3.2, 2.0),
        Vec3::new(4.0, 3.2, 2.5),
        dark,
        0.0,
    );
    for x in [-11.0, -5.0, 5.0, 11.0] {
        add_pyramid(b, Vec3::new(x, 10.2, -8.0), 3.2, 10.8, roof, 0.0);
    }
    for (x, z, h) in [(-30.0, -9.0, 13.0), (28.0, -2.0, 15.0), (10.0, -25.0, 18.0)] {
        add_box(
            b,
            Vec3::new(x, h * 0.5, z),
            Vec3::new(4.0, h * 0.5, 4.0),
            stone,
            0.0,
        );
        add_pyramid(b, Vec3::new(x, h, z), 5.0, 10.0, roof, 0.0);
    }
    for (x, z) in [(-42.0, 10.0), (-23.0, 18.0), (23.0, 19.0), (39.0, 8.0)] {
        add_box(b, Vec3::new(x, 1.1, z), Vec3::new(4.0, 1.1, 2.5), dark, 0.0);
    }
    for (x, z) in [
        (-9.5, -1.8),
        (0.0, -1.2),
        (9.5, -1.8),
        (-30.0, -2.0),
        (28.0, 2.0),
    ] {
        add_box(
            b,
            Vec3::new(x, 5.5, z),
            Vec3::new(0.9, 1.4, 0.2),
            [1.0, 0.34, 0.12],
            1.7,
        );
    }
}

fn add_energy_rifts(b: &mut MeshBuilder) {
    for chain in [
        [
            Vec3::new(-58.0, -6.0, 22.0),
            Vec3::new(-43.0, -5.2, 35.0),
            Vec3::new(-26.0, -5.0, 28.0),
        ],
        [
            Vec3::new(43.0, -5.0, -19.0),
            Vec3::new(62.0, -5.3, -29.0),
            Vec3::new(77.0, -5.4, -18.0),
        ],
    ] {
        add_ribbon(b, &chain, 1.1, [1.0, 0.26, 0.08], 2.2);
    }
    let magic = [
        Vec3::new(-45.0, -4.4, 42.0),
        Vec3::new(-31.0, -4.2, 29.0),
        Vec3::new(-15.0, -4.1, 38.0),
        Vec3::new(3.0, -4.0, 24.0),
    ];
    add_ribbon(b, &magic, 1.4, [0.05, 0.86, 1.0], 2.5);
}

fn add_selection_ring(b: &mut MeshBuilder, center: Vec3, inner: f32, outer: f32) {
    let segments = 48;
    for i in 0..segments {
        let a0 = i as f32 / segments as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / segments as f32 * std::f32::consts::TAU;
        let p0 = center + Vec3::new(a0.cos() * inner, 0.0, a0.sin() * inner);
        let p1 = center + Vec3::new(a1.cos() * inner, 0.0, a1.sin() * inner);
        let p2 = center + Vec3::new(a1.cos() * outer, 0.0, a1.sin() * outer);
        let p3 = center + Vec3::new(a0.cos() * outer, 0.0, a0.sin() * outer);
        b.add_quad(p0, p1, p2, p3, [1.0, 0.70, 0.25], 1.3);
    }
}

fn add_box(b: &mut MeshBuilder, center: Vec3, half: Vec3, color: [f32; 3], emissive: f32) {
    let min = center - half;
    let max = center + half;
    let p = |x, y, z| Vec3::new(x, y, z);
    b.add_quad(
        p(min.x, min.y, max.z),
        p(max.x, min.y, max.z),
        p(max.x, max.y, max.z),
        p(min.x, max.y, max.z),
        color,
        emissive,
    );
    b.add_quad(
        p(max.x, min.y, min.z),
        p(min.x, min.y, min.z),
        p(min.x, max.y, min.z),
        p(max.x, max.y, min.z),
        color,
        emissive,
    );
    b.add_quad(
        p(min.x, min.y, min.z),
        p(min.x, min.y, max.z),
        p(min.x, max.y, max.z),
        p(min.x, max.y, min.z),
        color,
        emissive,
    );
    b.add_quad(
        p(max.x, min.y, max.z),
        p(max.x, min.y, min.z),
        p(max.x, max.y, min.z),
        p(max.x, max.y, max.z),
        color,
        emissive,
    );
    b.add_quad(
        p(min.x, max.y, max.z),
        p(max.x, max.y, max.z),
        p(max.x, max.y, min.z),
        p(min.x, max.y, min.z),
        color,
        emissive,
    );
    b.add_quad(
        p(min.x, min.y, min.z),
        p(max.x, min.y, min.z),
        p(max.x, min.y, max.z),
        p(min.x, min.y, max.z),
        color,
        emissive,
    );
}

fn add_pyramid(
    b: &mut MeshBuilder,
    base_center: Vec3,
    half_width: f32,
    height: f32,
    color: [f32; 3],
    emissive: f32,
) {
    let y = base_center.y;
    let apex = Vec3::new(base_center.x, y + height, base_center.z);
    let a = Vec3::new(base_center.x - half_width, y, base_center.z - half_width);
    let c = Vec3::new(base_center.x + half_width, y, base_center.z + half_width);
    let b0 = Vec3::new(base_center.x + half_width, y, base_center.z - half_width);
    let d = Vec3::new(base_center.x - half_width, y, base_center.z + half_width);
    b.add_triangle(a, b0, apex, color, emissive);
    b.add_triangle(b0, c, apex, color, emissive);
    b.add_triangle(c, d, apex, color, emissive);
    b.add_triangle(d, a, apex, color, emissive);
}

fn add_ribbon(b: &mut MeshBuilder, points: &[Vec3], width: f32, color: [f32; 3], emissive: f32) {
    for pair in points.windows(2) {
        let a = pair[0];
        let d = pair[1];
        let direction = (d - a).normalize_or_zero();
        let side = Vec3::new(-direction.z, 0.0, direction.x) * width;
        b.add_quad(a - side, d - side, d + side, a + side, color, emissive);
    }
}
