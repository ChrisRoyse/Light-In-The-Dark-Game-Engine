use litd_engine::EngineError;

use super::{
    GltfPrimitiveImport,
    support::{invalid_import, validate_vec3_iter},
};

pub(super) fn import_primitive(
    mesh_index: usize,
    primitive: gltf::Primitive<'_>,
    buffers: &[gltf::buffer::Data],
) -> Result<GltfPrimitiveImport, EngineError> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return invalid_import(
            "primitive.mode",
            format!(
                "mesh {mesh_index} primitive {} uses unsupported mode {:?}",
                primitive.index(),
                primitive.mode()
            ),
        );
    }
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    let positions = reader
        .read_positions()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.gltf_import.primitive.positions".to_string(),
            reason: format!(
                "mesh {mesh_index} primitive {} is missing POSITION data",
                primitive.index()
            ),
        })?;
    let vertex_count = validate_vec3_iter(positions, "primitive.positions", "POSITION")?;
    if vertex_count == 0 {
        return invalid_import("primitive.positions", "POSITION accessor is empty");
    }
    let index_count = reader
        .read_indices()
        .map(|read| read.into_u32().count())
        .unwrap_or(vertex_count);
    if index_count == 0 || !index_count.is_multiple_of(3) {
        return invalid_import(
            "primitive.indices",
            format!(
                "mesh {mesh_index} primitive {} index count {} is not non-empty triangles",
                primitive.index(),
                index_count
            ),
        );
    }
    let has_normals = match reader.read_normals() {
        Some(read) => {
            let count = validate_vec3_iter(read, "primitive.normals", "NORMAL")?;
            if count != vertex_count {
                return invalid_import(
                    "primitive.normals",
                    "NORMAL count must match POSITION count",
                );
            }
            true
        }
        None => false,
    };
    let has_tangents = match reader.read_tangents() {
        Some(read) => {
            let mut count = 0;
            for tangent in read {
                if tangent.iter().any(|value| !value.is_finite()) {
                    return invalid_import(
                        "primitive.tangents",
                        "TANGENT contains non-finite data",
                    );
                }
                count += 1;
            }
            if count != vertex_count {
                return invalid_import(
                    "primitive.tangents",
                    "TANGENT count must match POSITION count",
                );
            }
            true
        }
        None => false,
    };
    let texcoord_0_count = match reader.read_tex_coords(0) {
        Some(read) => {
            let mut count = 0;
            for texcoord in read.into_f32() {
                if texcoord.iter().any(|value| !value.is_finite()) {
                    return invalid_import(
                        "primitive.texcoord_0",
                        "TEXCOORD_0 contains non-finite data",
                    );
                }
                count += 1;
            }
            if count != vertex_count {
                return invalid_import(
                    "primitive.texcoord_0",
                    "TEXCOORD_0 count must match POSITION count",
                );
            }
            count
        }
        None => 0,
    };
    let (joint_set_count, weight_set_count, skinned_vertex_count, weighted_vertex_count) =
        import_joint_weight_sets(&reader, vertex_count)?;
    let (morph_target_count, morph_position_count, morph_normal_count, morph_tangent_count) =
        import_morph_targets(&reader, vertex_count)?;

    Ok(GltfPrimitiveImport {
        mesh_index,
        primitive_index: primitive.index(),
        material_index: primitive.material().index(),
        mode: format!("{:?}", primitive.mode()),
        vertex_count,
        index_count,
        triangle_count: index_count / 3,
        has_normals,
        has_tangents,
        texcoord_0_count,
        joint_set_count,
        weight_set_count,
        skinned_vertex_count,
        weighted_vertex_count,
        morph_target_count,
        morph_position_count,
        morph_normal_count,
        morph_tangent_count,
    })
}

fn import_joint_weight_sets<'a, 's, F>(
    reader: &gltf::mesh::Reader<'a, 's, F>,
    vertex_count: usize,
) -> Result<(usize, usize, usize, usize), EngineError>
where
    F: Clone + Fn(gltf::Buffer<'a>) -> Option<&'s [u8]>,
{
    let mut joint_set_count = 0;
    let mut weight_set_count = 0;
    let mut skinned_vertex_count = 0;
    let mut weighted_vertex_count = 0;
    for set in 0..4 {
        let joints = reader
            .read_joints(set)
            .map(|read| read.into_u16().collect::<Vec<_>>());
        let weights = reader
            .read_weights(set)
            .map(|read| read.into_f32().collect::<Vec<_>>());
        match (joints, weights) {
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => {
                return invalid_import(
                    "primitive.skinning",
                    format!("JOINTS_{set} and WEIGHTS_{set} must be present together"),
                );
            }
            (Some(joints), Some(weights)) => {
                if joints.len() != vertex_count || weights.len() != vertex_count {
                    return invalid_import(
                        "primitive.skinning",
                        format!(
                            "JOINTS_{set}/WEIGHTS_{set} counts must match POSITION count {vertex_count}"
                        ),
                    );
                }
                joint_set_count += 1;
                weight_set_count += 1;
                for (joint, weight) in joints.iter().zip(weights.iter()) {
                    if weight.iter().any(|value| !value.is_finite()) {
                        return invalid_import(
                            "primitive.skinning",
                            format!("WEIGHTS_{set} contains non-finite data"),
                        );
                    }
                    if joint.iter().any(|value| *value > 0) {
                        skinned_vertex_count += 1;
                    }
                    if weight.iter().any(|value| *value > 0.0) {
                        weighted_vertex_count += 1;
                    }
                }
            }
        }
    }
    Ok((
        joint_set_count,
        weight_set_count,
        skinned_vertex_count,
        weighted_vertex_count,
    ))
}

fn import_morph_targets<'a, 's, F>(
    reader: &gltf::mesh::Reader<'a, 's, F>,
    vertex_count: usize,
) -> Result<(usize, usize, usize, usize), EngineError>
where
    F: Clone + Fn(gltf::Buffer<'a>) -> Option<&'s [u8]>,
{
    let mut morph_target_count = 0;
    let mut morph_position_count = 0;
    let mut morph_normal_count = 0;
    let mut morph_tangent_count = 0;
    for (positions, normals, tangents) in reader.read_morph_targets() {
        morph_target_count += 1;
        if let Some(read) = positions {
            let count = validate_vec3_iter(read, "primitive.morph.positions", "morph POSITION")?;
            if count != vertex_count {
                return invalid_import(
                    "primitive.morph.positions",
                    "morph POSITION count must match POSITION count",
                );
            }
            morph_position_count += count;
        }
        if let Some(read) = normals {
            let count = validate_vec3_iter(read, "primitive.morph.normals", "morph NORMAL")?;
            if count != vertex_count {
                return invalid_import(
                    "primitive.morph.normals",
                    "morph NORMAL count must match POSITION count",
                );
            }
            morph_normal_count += count;
        }
        if let Some(read) = tangents {
            let count = validate_vec3_iter(read, "primitive.morph.tangents", "morph TANGENT")?;
            if count != vertex_count {
                return invalid_import(
                    "primitive.morph.tangents",
                    "morph TANGENT count must match POSITION count",
                );
            }
            morph_tangent_count += count;
        }
    }
    Ok((
        morph_target_count,
        morph_position_count,
        morph_normal_count,
        morph_tangent_count,
    ))
}

pub(super) fn validate_mesh_morph_weights(mesh: &gltf::Mesh<'_>) -> Result<(), EngineError> {
    let morph_target_count = mesh
        .primitives()
        .map(|primitive| primitive.morph_targets().count())
        .max()
        .unwrap_or(0);
    if let Some(weights) = mesh.weights() {
        if weights.iter().any(|value| !value.is_finite()) {
            return invalid_import(
                "mesh.weights",
                format!("mesh {} has non-finite weights", mesh.index()),
            );
        }
        if morph_target_count > 0 && weights.len() != morph_target_count {
            return invalid_import(
                "mesh.weights",
                format!(
                    "mesh {} default weight count {} != morph target count {}",
                    mesh.index(),
                    weights.len(),
                    morph_target_count
                ),
            );
        }
    }
    Ok(())
}
