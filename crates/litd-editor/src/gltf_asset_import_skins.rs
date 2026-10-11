use litd_engine::EngineError;

use super::{GltfSkinImport, support::invalid_import};

pub(super) fn import_skin(
    skin: gltf::Skin<'_>,
    buffers: &[gltf::buffer::Data],
) -> Result<GltfSkinImport, EngineError> {
    let joint_names = skin
        .joints()
        .map(|joint| {
            joint
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("node{}", joint.index()))
        })
        .collect::<Vec<_>>();
    if joint_names.is_empty() {
        return invalid_import(
            "skin.joints",
            format!("skin {} has no joints", skin.index()),
        );
    }
    let inverse_bind_matrix_count = match skin
        .reader(|buffer| Some(&buffers[buffer.index()]))
        .read_inverse_bind_matrices()
    {
        Some(read) => {
            let mut count = 0;
            for matrix in read {
                for column in matrix {
                    for value in column {
                        if !value.is_finite() {
                            return invalid_import(
                                "skin.inverse_bind_matrices",
                                format!(
                                    "skin {} contains a non-finite inverse bind matrix",
                                    skin.index()
                                ),
                            );
                        }
                    }
                }
                count += 1;
            }
            if count != joint_names.len() {
                return invalid_import(
                    "skin.inverse_bind_matrices",
                    format!(
                        "skin {} inverse bind matrix count {} != joint count {}",
                        skin.index(),
                        count,
                        joint_names.len()
                    ),
                );
            }
            count
        }
        None => 0,
    };
    Ok(GltfSkinImport {
        skin_index: skin.index(),
        name: skin.name().map(str::to_string),
        skeleton_root: skin.skeleton().map(|node| {
            node.name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("node{}", node.index()))
        }),
        joint_count: joint_names.len(),
        inverse_bind_matrix_count,
        joint_names,
    })
}
