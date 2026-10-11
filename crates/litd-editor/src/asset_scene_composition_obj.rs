use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use litd_engine::{
    EngineError,
    math::{Mat4, Quat, Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use crate::asset_vault::AssetVaultRecord;

use super::{
    geometry::{AssetPlacement, CompositionBuild, CompositionStats},
    invalid_composition,
};

#[path = "asset_scene_composition_mtl.rs"]
mod mtl;
#[path = "asset_scene_composition_obj_tokens.rs"]
mod tokens;

use mtl::{ObjMaterial, load_mtl};
use tokens::{
    FaceVertex, invalid_obj, parse_face_vertex, parse_vec2, parse_vec3, validate_face_indices,
};

const WHITE_ATLAS_UV: [f32; 2] = [0.015, 0.015];

pub(super) fn append_obj_asset_tinted(
    obj_record: &AssetVaultRecord,
    mtl_record: &AssetVaultRecord,
    placement: AssetPlacement,
    color_tint: [f32; 4],
    build: &mut CompositionBuild,
) -> Result<CompositionStats, EngineError> {
    validate_color_tint(color_tint)?;
    let materials = load_mtl(mtl_record)?;
    let obj_text = fs::read_to_string(&obj_record.absolute_path).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.obj.read".to_string(),
            reason: format!(
                "{} failed OBJ read: {error}",
                obj_record.absolute_path.display()
            ),
        }
    })?;
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::splat(placement.scale),
        Quat::from_rotation_y(placement.yaw_degrees.to_radians()),
        Vec3::from_array(placement.translation),
    );
    let start_vertex_count = build.vertices.len();
    let start_index_count = build.indices.len();
    let start_primitive_count = build.mesh_primitives;
    let imported = match import_obj_text(
        &obj_text, obj_record, mtl_record, &materials, transform, color_tint, build,
    ) {
        Ok(imported) => imported,
        Err(error) => {
            build.vertices.truncate(start_vertex_count);
            build.indices.truncate(start_index_count);
            build.mesh_primitives = start_primitive_count;
            return Err(error);
        }
    };
    let mut source_paths = BTreeSet::new();
    source_paths.insert(obj_record.relative_path.clone());
    source_paths.insert(mtl_record.relative_path.clone());
    Ok(CompositionStats {
        source_paths,
        material_count: imported.material_count,
        texture_count: 0,
        image_count: 0,
        degenerate_triangle_count: 0,
    })
}

#[derive(Debug, Clone, Copy)]
struct ObjImportStats {
    material_count: usize,
}

fn import_obj_text(
    obj_text: &str,
    obj_record: &AssetVaultRecord,
    mtl_record: &AssetVaultRecord,
    materials: &BTreeMap<String, ObjMaterial>,
    transform: Mat4,
    color_tint: [f32; 4],
    build: &mut CompositionBuild,
) -> Result<ObjImportStats, EngineError> {
    let mut positions = Vec::new();
    let mut texcoords = Vec::new();
    let mut normals = Vec::new();
    let mut current_material: Option<String> = None;
    let mut saw_mtllib = false;
    let mut valid_triangle_count = 0;
    let start_primitives = build.mesh_primitives;

    for (line_index, raw_line) in obj_text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(tag) = fields.next() else {
            continue;
        };
        match tag {
            "mtllib" => {
                let name = fields.collect::<Vec<_>>().join(" ");
                require_expected_mtllib(&name, mtl_record, obj_record, line_number)?;
                saw_mtllib = true;
            }
            "usemtl" => {
                let name = fields.collect::<Vec<_>>().join(" ");
                if !materials.contains_key(&name) {
                    return invalid_obj(
                        "material.use",
                        obj_record,
                        line_number,
                        format!("OBJ references unknown material '{name}'"),
                    );
                }
                current_material = Some(name);
            }
            "v" => positions.push(parse_vec3(fields, obj_record, line_number, "v")?),
            "vn" => normals.push(parse_vec3(fields, obj_record, line_number, "vn")?),
            "vt" => texcoords.push(parse_vec2(fields, obj_record, line_number)?),
            "f" => {
                let Some(material_name) = current_material.as_deref() else {
                    return invalid_obj(
                        "face.material",
                        obj_record,
                        line_number,
                        "face appears before usemtl",
                    );
                };
                let material = materials.get(material_name).ok_or_else(|| {
                    EngineError::UiRenderInvalidConfig {
                        field: "editor.asset_scene.composition.obj.material".to_string(),
                        reason: format!(
                            "{}:{} lost material lookup for {material_name}",
                            obj_record.relative_path, line_number
                        ),
                    }
                })?;
                let face = fields
                    .map(|field| parse_face_vertex(field, obj_record, line_number))
                    .collect::<Result<Vec<_>, _>>()?;
                if face.len() < 3 {
                    return invalid_obj(
                        "face",
                        obj_record,
                        line_number,
                        "face must contain at least three vertices",
                    );
                }
                validate_face_indices(
                    &face,
                    &positions,
                    &texcoords,
                    &normals,
                    obj_record,
                    line_number,
                )?;
                for index in 1..face.len() - 1 {
                    append_obj_triangle(AppendObjTriangleArgs {
                        face: [face[0], face[index], face[index + 1]],
                        positions: &positions,
                        texcoords: &texcoords,
                        normals: &normals,
                        material,
                        transform,
                        color_tint,
                        build,
                        obj_record,
                        line_number,
                    })?;
                    valid_triangle_count += 1;
                }
            }
            "o" | "g" | "s" => {}
            other => {
                return invalid_obj(
                    "token",
                    obj_record,
                    line_number,
                    format!("unsupported OBJ token '{other}'"),
                );
            }
        }
    }

    if !saw_mtllib {
        return invalid_composition(
            "obj.mtllib",
            format!("{} has no mtllib", obj_record.relative_path),
        );
    }
    if valid_triangle_count == 0 {
        return invalid_composition(
            "obj.triangles",
            format!(
                "{} produced no renderable triangles",
                obj_record.relative_path
            ),
        );
    }
    if build.mesh_primitives == start_primitives {
        build.mesh_primitives += 1;
    }
    Ok(ObjImportStats {
        material_count: materials.len(),
    })
}

struct AppendObjTriangleArgs<'a> {
    face: [FaceVertex; 3],
    positions: &'a [Vec3],
    texcoords: &'a [[f32; 2]],
    normals: &'a [Vec3],
    material: &'a ObjMaterial,
    transform: Mat4,
    color_tint: [f32; 4],
    build: &'a mut CompositionBuild,
    obj_record: &'a AssetVaultRecord,
    line_number: usize,
}

fn append_obj_triangle(args: AppendObjTriangleArgs<'_>) -> Result<(), EngineError> {
    let AppendObjTriangleArgs {
        face,
        positions,
        texcoords,
        normals,
        material,
        transform,
        color_tint,
        build,
        obj_record,
        line_number,
    } = args;
    let points = [
        transform_point(transform, positions[face[0].position])?,
        transform_point(transform, positions[face[1].position])?,
        transform_point(transform, positions[face[2].position])?,
    ];
    let Some(flat_normal) = triangle_normal(points)? else {
        return invalid_obj(
            "face.degenerate",
            obj_record,
            line_number,
            "OBJ face produced a degenerate triangle after placement transform; clean or replace the source asset before import",
        );
    };
    let base = checked_vertex_base(build.vertices.len())?;
    for (corner, vertex_ref) in face.into_iter().enumerate() {
        let normal = match vertex_ref.normal {
            Some(index) => transform_normal(transform, normals[index])?,
            None => flat_normal,
        };
        let uv = vertex_ref
            .texcoord
            .map(|index| texcoords[index])
            .unwrap_or(WHITE_ATLAS_UV);
        build.vertices.push(vertex(
            points[corner],
            normal,
            uv,
            apply_color_tint(material.diffuse, material.emissive, color_tint),
            [0.0, material.roughness, 0.0, 0.0],
            obj_record,
        )?);
    }
    build.indices.extend([base, base + 1, base + 2]);
    Ok(())
}

fn require_expected_mtllib(
    actual: &str,
    mtl_record: &AssetVaultRecord,
    obj_record: &AssetVaultRecord,
    line_number: usize,
) -> Result<(), EngineError> {
    let Some(expected) = mtl_record
        .absolute_path
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return invalid_obj(
            "mtllib",
            obj_record,
            line_number,
            "MTL manifest path has no file name",
        );
    };
    if actual != expected {
        return invalid_obj(
            "mtllib",
            obj_record,
            line_number,
            format!("expected mtllib {expected}, got {actual}"),
        );
    }
    Ok(())
}

fn transform_point(transform: Mat4, position: Vec3) -> Result<Vec3, EngineError> {
    let point = transform.transform_point3(position);
    if !point.is_finite() {
        return invalid_composition("obj.position", "transformed OBJ position is not finite");
    }
    Ok(point)
}

fn transform_normal(transform: Mat4, normal: Vec3) -> Result<Vec3, EngineError> {
    let normal = transform.transform_vector3(normal).normalize_or_zero();
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition("obj.normal", "transformed OBJ normal is invalid");
    }
    Ok(normal)
}

fn triangle_normal(points: [Vec3; 3]) -> Result<Option<Vec3>, EngineError> {
    let normal = (points[1] - points[0])
        .cross(points[2] - points[0])
        .normalize_or_zero();
    if !normal.is_finite() {
        return invalid_composition("obj.normal", "OBJ triangle normal is non-finite");
    }
    Ok((normal != Vec3::ZERO).then_some(normal))
}

fn vertex(
    position: Vec3,
    normal: Vec3,
    uv: [f32; 2],
    color: [f32; 4],
    pbr: [f32; 4],
    record: &AssetVaultRecord,
) -> Result<EditorWorldVertex, EngineError> {
    if !position.is_finite() || !normal.is_finite() || !Vec2::from_array(uv).is_finite() {
        return invalid_composition(
            "obj.vertex",
            format!("{} produced invalid vertex", record.relative_path),
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

fn apply_color_tint(diffuse: [f32; 3], emissive: f32, tint: [f32; 4]) -> [f32; 4] {
    [
        diffuse[0] * tint[0],
        diffuse[1] * tint[1],
        diffuse[2] * tint[2],
        (emissive + tint[3]).clamp(0.0, 4.0),
    ]
}

fn validate_color_tint(tint: [f32; 4]) -> Result<(), EngineError> {
    if tint.iter().any(|value| !value.is_finite())
        || tint[..3].iter().any(|value| *value < 0.0 || *value > 4.0)
        || tint[3] < 0.0
        || tint[3] > 4.0
    {
        return invalid_composition(
            "obj.tint",
            "OBJ tint RGB/emissive must be finite and within 0.0..=4.0",
        );
    }
    Ok(())
}

fn orthogonal_tangent(normal: Vec3) -> Result<[f32; 4], EngineError> {
    let reference = if normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = reference.cross(normal).normalize_or_zero();
    if !tangent.is_finite() || tangent == Vec3::ZERO {
        return invalid_composition("obj.tangent", "failed to derive tangent from OBJ normal");
    }
    Ok(tangent.extend(1.0).to_array())
}

fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.obj.vertex_count".to_string(),
        reason: "OBJ composition exceeds u32 index space".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use crate::asset_vault::AssetVaultCatalog;

    use super::*;

    const DIRTY_OBJ: &str = "sources/quaternius/game-kit/ultimatemodularruins/OBJ/Arch_Gothic.obj";
    const DIRTY_MTL: &str = "sources/quaternius/game-kit/ultimatemodularruins/OBJ/Arch_Gothic.mtl";

    #[test]
    fn real_degenerate_obj_fails_loud_without_partial_build_mutation() {
        let catalog = AssetVaultCatalog::load_default_cached().unwrap();
        let obj_record = crate::asset_vault_fixture::expect_asset_vault(
            super::super::sources::manifest_file_record(&catalog, DIRTY_OBJ),
            "editor.asset_scene.composition.obj",
        );
        let mtl_record = crate::asset_vault_fixture::expect_asset_vault(
            super::super::sources::manifest_file_record(&catalog, DIRTY_MTL),
            "editor.asset_scene.composition.mtl",
        );
        let mut build = CompositionBuild::default();
        println!(
            "issue1302 obj_edge before path={} sha={} vertices={} indices={} primitives={}",
            obj_record.relative_path,
            obj_record.sha256,
            build.vertices.len(),
            build.indices.len(),
            build.mesh_primitives
        );

        let error = match append_obj_asset_tinted(
            obj_record,
            mtl_record,
            AssetPlacement {
                file_name: "Arch_Gothic.obj",
                translation: [0.0, 0.0, 0.0],
                scale: 1.0,
                yaw_degrees: 0.0,
            },
            [1.0, 1.0, 1.0, 0.0],
            &mut build,
        ) {
            Ok(_) => panic!("dirty OBJ must fail loud instead of culling degenerate triangles"),
            Err(error) => error,
        };
        let error = error.to_string();
        println!(
            "issue1302 obj_edge after error={} vertices={} indices={} primitives={}",
            error,
            build.vertices.len(),
            build.indices.len(),
            build.mesh_primitives
        );

        assert!(error.contains("editor.asset_scene.composition.obj.face.degenerate"));
        assert!(error.contains(DIRTY_OBJ));
        assert_eq!(build.vertices.len(), 0);
        assert_eq!(build.indices.len(), 0);
        assert_eq!(build.mesh_primitives, 0);
    }
}
