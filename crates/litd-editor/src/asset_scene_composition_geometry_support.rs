use litd_engine::{
    EngineError,
    math::{Mat4, Vec2, Vec3},
    render::editor_world::EditorWorldVertex,
};

use super::invalid_composition;

pub(super) fn vertex(
    position: Vec3,
    normal: Vec3,
    uv: [f32; 2],
    color: [f32; 4],
    pbr: [f32; 4],
) -> Result<EditorWorldVertex, EngineError> {
    let normal = normal.normalize_or_zero();
    if !position.is_finite()
        || !normal.is_finite()
        || normal == Vec3::ZERO
        || !Vec2::from_array(uv).is_finite()
        || color.iter().any(|value| !value.is_finite())
        || pbr.iter().any(|value| !value.is_finite())
    {
        return invalid_composition("vertex", "composition vertex contains invalid data");
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

pub(super) fn transform_point(transform: Mat4, position: [f32; 3]) -> Result<Vec3, EngineError> {
    let point = transform.transform_point3(Vec3::from_array(position));
    if !point.is_finite() {
        return invalid_composition("gltf.positions", "transformed POSITION is not finite");
    }
    Ok(point)
}

pub(super) fn transform_normal(transform: Mat4, normal: [f32; 3]) -> Result<Vec3, EngineError> {
    let normal = transform
        .transform_vector3(Vec3::from_array(normal))
        .normalize_or_zero();
    if !normal.is_finite() || normal == Vec3::ZERO {
        return invalid_composition("gltf.normals", "transformed NORMAL is invalid");
    }
    Ok(normal)
}

pub(super) fn triangle_normal(
    points: [Vec3; 3],
    source_relative_path: &str,
) -> Result<Option<Vec3>, EngineError> {
    let cross = (points[1] - points[0]).cross(points[2] - points[0]);
    if !cross.is_finite() {
        return invalid_composition(
            "gltf.normals",
            format!("{source_relative_path} triangle normal is non-finite"),
        );
    }
    let normal = cross.normalize_or_zero();
    if normal == Vec3::ZERO {
        return Ok(None);
    }
    Ok(Some(normal))
}

pub(super) fn position_at(positions: &[[f32; 3]], index: u32) -> Result<[f32; 3], EngineError> {
    positions
        .get(index as usize)
        .copied()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.gltf.indices".to_string(),
            reason: format!("index {index} exceeds POSITION count"),
        })
}

pub(super) fn material_color(material: &gltf::Material<'_>) -> Result<[f32; 4], EngineError> {
    let base = material.pbr_metallic_roughness().base_color_factor();
    let emissive = material
        .emissive_factor()
        .into_iter()
        .fold(0.0_f32, f32::max);
    let color = [base[0], base[1], base[2], emissive];
    if color.iter().any(|value| !value.is_finite()) {
        return invalid_composition("gltf.material", "material color factor is not finite");
    }
    Ok(color)
}

pub(super) fn material_pbr(material: &gltf::Material<'_>) -> Result<[f32; 4], EngineError> {
    let pbr = material.pbr_metallic_roughness();
    let normal_scale = material
        .normal_texture()
        .map(|normal| normal.scale())
        .unwrap_or(0.0);
    let alpha_cutoff = match material.alpha_mode() {
        gltf::material::AlphaMode::Mask => material.alpha_cutoff().unwrap_or(0.5),
        _ => 0.0,
    };
    let values = [
        pbr.metallic_factor(),
        pbr.roughness_factor(),
        normal_scale,
        alpha_cutoff,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return invalid_composition("gltf.material", "material PBR factor is not finite");
    }
    Ok(values)
}

pub(super) fn multiply_color(material_color: [f32; 4], vertex_color: [f32; 4]) -> [f32; 4] {
    [
        material_color[0] * vertex_color[0],
        material_color[1] * vertex_color[1],
        material_color[2] * vertex_color[2],
        material_color[3],
    ]
}

pub(super) fn apply_color_tint(color: [f32; 4], tint: [f32; 4]) -> [f32; 4] {
    [
        color[0] * tint[0],
        color[1] * tint[1],
        color[2] * tint[2],
        (color[3] + tint[3]).clamp(0.0, 4.0),
    ]
}

pub(super) fn validate_color_tint(color_tint: [f32; 4]) -> Result<(), EngineError> {
    if color_tint.iter().any(|value| !value.is_finite()) {
        return invalid_composition("look.tint", "asset color tint contains non-finite values");
    }
    if color_tint[..3]
        .iter()
        .any(|value| *value < 0.0 || *value > 4.0)
        || color_tint[3] < 0.0
        || color_tint[3] > 4.0
    {
        return invalid_composition(
            "look.tint",
            "asset color tint RGB and emissive bias must be within 0.0..=4.0",
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
        return invalid_composition("tangent", "failed to derive tangent from normal");
    }
    Ok(tangent.extend(1.0).to_array())
}

pub(super) fn require_same_len(
    actual: usize,
    expected: usize,
    attribute: &'static str,
    source_relative_path: &str,
) -> Result<(), EngineError> {
    if actual != expected {
        return invalid_composition(
            "gltf.attribute_count",
            format!(
                "{source_relative_path} {attribute} count {actual} != POSITION count {expected}"
            ),
        );
    }
    Ok(())
}

pub(super) fn checked_vertex_base(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_scene.composition.vertex_count".to_string(),
        reason: "composition exceeds u32 index space".to_string(),
    })
}
