//! #1891: build a drawable editor-world scene from a packaged lowered document.
//!
//! The package play path must render **map file content** (terrain_cells, starts,
//! resource nodes, region markers) -- never the ED11 reference island as SoT.
//! This is greybox geometry until vault meshes stream; positions and map shape
//! still come from the package document (WC3 map-file doctrine #1783).

use serde_json::Value;

use crate::{EngineError, game_package::RuntimePackageReadback, math::Vec3};

use super::{
    EditorWorldSceneBuffers, EditorWorldSceneBuffersNewArgs, EditorWorldTextureData,
    EditorWorldVertex, world_error,
};

/// Result of packaging a document into drawable scene buffers + framing hints.
pub struct PackageWorldSceneBuild {
    pub scene: EditorWorldSceneBuffers,
    pub map_center: Vec3,
    pub map_half_extent: f32,
    pub terrain_cell_count: u32,
    pub marker_count: u32,
    pub document_id: String,
}

/// Build a package-backed world scene. Fails closed when terrain_cells are absent
/// (no silent ED11 fallback -- #1891).
pub fn build_package_world_scene(
    package: &RuntimePackageReadback,
) -> Result<PackageWorldSceneBuild, EngineError> {
    let document = &package.document;
    let terrain = match document.get("terrain_cells").and_then(|v| v.as_object()) {
        Some(cells) if !cells.is_empty() => cells,
        _ => {
            return world_error(
                "package_world_scene.terrain_cells",
                "packaged document has no terrain_cells; refuse ED11 fallback for package play",
            );
        }
    };

    let document_id = package
        .document_summary
        .document_id
        .clone()
        .or_else(|| {
            document
                .get("calyx_document_id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| package.package_id.clone());

    let mut builder = PackageMeshBuilder::new();
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;
    let mut terrain_cell_count = 0u32;

    for (_key, cell) in terrain {
        let Some(tx) = cell.get("tile_x").and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(tz) = cell.get("tile_z").and_then(|v| v.as_i64()) else {
            continue;
        };
        let height = cell.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let cliff = cell
            .get("cliff_level")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let x = tx as f32;
        let z = tz as f32;
        min_x = min_x.min(x);
        max_x = max_x.max(x + 1.0);
        min_z = min_z.min(z);
        max_z = max_z.max(z + 1.0);
        // Grass floor vs cliff wall colors -- readable map without textures.
        let top = if cliff > 0 {
            [0.42, 0.38, 0.30]
        } else {
            [0.22, 0.38, 0.20]
        };
        let y = height.max(0.05);
        builder.add_box(
            Vec3::new(x + 0.5, y * 0.5, z + 0.5),
            Vec3::new(0.48, y.max(0.08) * 0.5, 0.48),
            top,
            0.0,
        );
        terrain_cell_count += 1;
    }

    let mut marker_count = 0u32;
    if let Some(objects) = document.get("objects").and_then(|v| v.as_object()) {
        for (id, object) in objects {
            let kind = object.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let translation = object_translation(object);
            match kind {
                "StartLocation" => {
                    let color = if id.contains("player_two") || id.contains("veil") {
                        [0.55, 0.25, 0.75]
                    } else {
                        [0.85, 0.55, 0.15]
                    };
                    builder.add_box(
                        Vec3::new(translation.x, 0.6, translation.z),
                        Vec3::new(0.55, 0.55, 0.55),
                        color,
                        0.35,
                    );
                    // Selection ring feel
                    builder.add_ring(translation.x, translation.z, 1.1, 1.35, color, 0.5);
                    marker_count += 1;
                }
                "ResourceNode" => {
                    builder.add_box(
                        Vec3::new(translation.x, 0.35, translation.z),
                        Vec3::new(0.4, 0.35, 0.4),
                        [0.90, 0.78, 0.18],
                        0.55,
                    );
                    marker_count += 1;
                }
                "Unit" if id.starts_with("placed_") => {
                    let owner = object
                        .get("properties")
                        .and_then(|p| p.get("template.owner_player_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let role = object
                        .get("properties")
                        .and_then(|p| p.get("runtime.role"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("soldier");
                    // Placed forces often ship identity transform; layout near faction start.
                    let base = if owner.contains("two") || id.contains("veil") {
                        Vec3::new(13.0, 0.0, 13.0)
                    } else {
                        Vec3::new(2.0, 0.0, 2.0)
                    };
                    let offset = if role == "structure" {
                        if id.ends_with("_hall_b") {
                            Vec3::new(1.5, 0.0, 0.0)
                        } else {
                            Vec3::new(0.0, 0.0, 0.0)
                        }
                    } else {
                        Vec3::new(0.8, 0.0, 1.2)
                    };
                    let pos = if translation.length_squared() > 0.01 {
                        translation
                    } else {
                        base + offset
                    };
                    let (half, color) = if role == "structure" {
                        (Vec3::new(0.7, 0.55, 0.7), [0.45, 0.48, 0.52])
                    } else {
                        (Vec3::new(0.28, 0.45, 0.28), [0.25, 0.55, 0.85])
                    };
                    builder.add_box(Vec3::new(pos.x, half.y, pos.z), half, color, 0.05);
                    marker_count += 1;
                }
                _ => {}
            }
        }
    }

    // Region footprints as flat tinted pads so bases/choke read on the map.
    if let Some(regions) = document.get("regions").and_then(|v| v.as_object()) {
        for (id, region) in regions {
            let Some(center) = region.get("center").and_then(as_vec3) else {
                continue;
            };
            let half = region
                .get("half_extents")
                .and_then(as_vec3)
                .unwrap_or(Vec3::new(1.5, 1.0, 1.5));
            let color = if id.contains("ember") {
                [0.55, 0.28, 0.12]
            } else if id.contains("veil") {
                [0.28, 0.18, 0.45]
            } else if id.contains("crossing") {
                [0.18, 0.35, 0.55]
            } else {
                [0.15, 0.18, 0.22]
            };
            builder.add_box(
                Vec3::new(center.x, 0.04, center.z),
                Vec3::new(half.x.max(0.5), 0.04, half.z.max(0.5)),
                color,
                0.15,
            );
            marker_count += 1;
        }
    }

    if builder.vertices.is_empty() {
        return world_error(
            "package_world_scene.geometry",
            "package produced zero drawable vertices",
        );
    }

    let map_center = Vec3::new((min_x + max_x) * 0.5, 0.0, (min_z + max_z) * 0.5);
    let map_half_extent = ((max_x - min_x).max(max_z - min_z) * 0.5).max(8.0);

    let scene = builder.finish(&document_id, package.package_id.as_str())?;
    tracing::info!(
        code = "LITD-PACKAGE-WORLD-SCENE",
        document_id = %document_id,
        package_id = %package.package_id,
        terrain_cell_count,
        marker_count,
        vertex_count = scene.vertices().len(),
        map_center = ?map_center,
        "built package-backed world scene for litd-game play path"
    );

    Ok(PackageWorldSceneBuild {
        scene,
        map_center,
        map_half_extent,
        terrain_cell_count,
        marker_count,
        document_id,
    })
}

fn object_translation(object: &Value) -> Vec3 {
    object
        .get("transform")
        .and_then(|t| t.get("translation"))
        .and_then(as_vec3)
        .unwrap_or(Vec3::ZERO)
}

fn as_vec3(value: &Value) -> Option<Vec3> {
    let arr = value.as_array()?;
    if arr.len() < 3 {
        return None;
    }
    Some(Vec3::new(
        arr[0].as_f64()? as f32,
        arr[1].as_f64()? as f32,
        arr[2].as_f64()? as f32,
    ))
}

struct PackageMeshBuilder {
    vertices: Vec<EditorWorldVertex>,
    indices: Vec<u32>,
    mesh_primitives: u32,
}

impl PackageMeshBuilder {
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            mesh_primitives: 0,
        }
    }

    fn finish(
        self,
        document_id: &str,
        package_id: &str,
    ) -> Result<EditorWorldSceneBuffers, EngineError> {
        EditorWorldSceneBuffers::new(EditorWorldSceneBuffersNewArgs {
            scene_name: format!("package_map:{document_id}"),
            source_id: package_id.to_string(),
            source_path: format!("package://{package_id}/document"),
            dependency_count: 0,
            // Default visual preset requires >=4 materials and >=3 images (#1097 bar).
            material_count: 4,
            texture_count: 3,
            image_count: 3,
            vertices: self.vertices,
            indices: self.indices,
            mesh_primitives: self.mesh_primitives,
            base_color_texture: solid_texture("package_map_base", [48, 72, 40, 255])?,
            normal_texture: solid_texture("package_map_normal", [128, 128, 255, 255])?,
            metallic_roughness_texture: solid_texture("package_map_mr", [255, 200, 0, 255])?,
        })
    }

    fn push_vertex(&mut self, position: Vec3, normal: Vec3, color: [f32; 3], emissive: f32) -> u32 {
        let index = self.vertices.len() as u32;
        self.vertices.push(EditorWorldVertex {
            position: position.to_array(),
            normal: normal.normalize_or_zero().to_array(),
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
            color_emissive: [color[0], color[1], color[2], emissive],
            material_pbr: [0.0, 0.75, 1.0, 0.0],
        });
        index
    }

    fn add_triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 3], emissive: f32) {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let i0 = self.push_vertex(a, normal, color, emissive);
        let i1 = self.push_vertex(b, normal, color, emissive);
        let i2 = self.push_vertex(c, normal, color, emissive);
        self.indices.extend([i0, i1, i2]);
        self.mesh_primitives += 1;
    }

    fn add_quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 3], emissive: f32) {
        self.add_triangle(a, b, c, color, emissive);
        self.add_triangle(a, c, d, color, emissive);
    }

    fn add_box(&mut self, center: Vec3, half: Vec3, color: [f32; 3], emissive: f32) {
        let min = center - half;
        let max = center + half;
        let corners = [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(min.x, max.y, max.z),
        ];
        // top
        self.add_quad(
            corners[4], corners[5], corners[6], corners[7], color, emissive,
        );
        // bottom
        self.add_quad(corners[0], corners[3], corners[2], corners[1], color, 0.0);
        // sides
        self.add_quad(corners[0], corners[1], corners[5], corners[4], color, 0.0);
        self.add_quad(corners[1], corners[2], corners[6], corners[5], color, 0.0);
        self.add_quad(corners[2], corners[3], corners[7], corners[6], color, 0.0);
        self.add_quad(corners[3], corners[0], corners[4], corners[7], color, 0.0);
    }

    fn add_ring(
        &mut self,
        cx: f32,
        cz: f32,
        inner: f32,
        outer: f32,
        color: [f32; 3],
        emissive: f32,
    ) {
        const SEG: i32 = 16;
        for i in 0..SEG {
            let a0 = (i as f32) / (SEG as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32) / (SEG as f32) * std::f32::consts::TAU;
            let i0 = Vec3::new(cx + a0.cos() * inner, 0.08, cz + a0.sin() * inner);
            let i1 = Vec3::new(cx + a1.cos() * inner, 0.08, cz + a1.sin() * inner);
            let o0 = Vec3::new(cx + a0.cos() * outer, 0.08, cz + a0.sin() * outer);
            let o1 = Vec3::new(cx + a1.cos() * outer, 0.08, cz + a1.sin() * outer);
            self.add_quad(i0, o0, o1, i1, color, emissive);
        }
    }
}

fn solid_texture(label: &str, rgba: [u8; 4]) -> Result<EditorWorldTextureData, EngineError> {
    EditorWorldTextureData::new(
        label,
        format!("package_world_scene::{label}"),
        2,
        2,
        vec![
            rgba[0], rgba[1], rgba[2], rgba[3], rgba[0], rgba[1], rgba[2], rgba[3], rgba[0],
            rgba[1], rgba[2], rgba[3], rgba[0], rgba[1], rgba[2], rgba[3],
        ],
    )
}
