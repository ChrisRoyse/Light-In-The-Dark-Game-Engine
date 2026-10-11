use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
};

use crate::asset_vault::AssetVaultRecord;

use super::invalid_composition;

#[derive(Debug, Clone, Copy)]
pub(super) struct FaceVertex {
    pub(super) position: usize,
    pub(super) texcoord: Option<usize>,
    pub(super) normal: Option<usize>,
}

pub(super) fn parse_face_vertex(
    value: &str,
    record: &AssetVaultRecord,
    line_number: usize,
) -> Result<FaceVertex, EngineError> {
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 3 || parts[0].is_empty() {
        return invalid_obj(
            "face.vertex",
            record,
            line_number,
            format!("invalid face vertex '{value}'"),
        );
    }
    Ok(FaceVertex {
        position: parse_positive_index(parts[0], record, line_number, "position")?,
        texcoord: optional_index(parts.get(1).copied(), record, line_number, "texcoord")?,
        normal: optional_index(parts.get(2).copied(), record, line_number, "normal")?,
    })
}

pub(super) fn validate_face_indices(
    face: &[FaceVertex],
    positions: &[Vec3],
    texcoords: &[[f32; 2]],
    normals: &[Vec3],
    record: &AssetVaultRecord,
    line_number: usize,
) -> Result<(), EngineError> {
    for vertex in face {
        if vertex.position >= positions.len() {
            return invalid_obj(
                "face.position",
                record,
                line_number,
                "face POSITION index exceeds vertex count",
            );
        }
        if let Some(index) = vertex.texcoord
            && index >= texcoords.len()
        {
            return invalid_obj(
                "face.texcoord",
                record,
                line_number,
                "face TEXCOORD index exceeds texcoord count",
            );
        }
        if let Some(index) = vertex.normal
            && index >= normals.len()
        {
            return invalid_obj(
                "face.normal",
                record,
                line_number,
                "face NORMAL index exceeds normal count",
            );
        }
    }
    Ok(())
}

pub(super) fn parse_vec3<'a>(
    mut fields: impl Iterator<Item = &'a str>,
    record: &AssetVaultRecord,
    line_number: usize,
    tag: &str,
) -> Result<Vec3, EngineError> {
    let values = [
        next_f32(&mut fields, record, line_number, tag)?,
        next_f32(&mut fields, record, line_number, tag)?,
        next_f32(&mut fields, record, line_number, tag)?,
    ];
    let vector = Vec3::from_array(values);
    if !vector.is_finite() {
        return invalid_obj(
            tag,
            record,
            line_number,
            "vector contains non-finite values",
        );
    }
    Ok(vector)
}

pub(super) fn parse_vec2<'a>(
    mut fields: impl Iterator<Item = &'a str>,
    record: &AssetVaultRecord,
    line_number: usize,
) -> Result<[f32; 2], EngineError> {
    let uv = [
        next_f32(&mut fields, record, line_number, "vt")?,
        next_f32(&mut fields, record, line_number, "vt")?,
    ];
    if !Vec2::from_array(uv).is_finite() {
        return invalid_obj(
            "vt",
            record,
            line_number,
            "texcoord contains non-finite values",
        );
    }
    Ok(uv)
}

pub(super) fn invalid_obj<T>(
    field: impl Into<String>,
    record: &AssetVaultRecord,
    line_number: usize,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    invalid_composition(
        format!("obj.{}", field.into()),
        format!("{}:{line_number}: {}", record.relative_path, reason.into()),
    )
}

fn optional_index(
    value: Option<&str>,
    record: &AssetVaultRecord,
    line_number: usize,
    field: &str,
) -> Result<Option<usize>, EngineError> {
    match value {
        Some(value) if !value.is_empty() => Ok(Some(parse_positive_index(
            value,
            record,
            line_number,
            field,
        )?)),
        _ => Ok(None),
    }
}

fn parse_positive_index(
    value: &str,
    record: &AssetVaultRecord,
    line_number: usize,
    field: &str,
) -> Result<usize, EngineError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.composition.obj.face.{field}"),
            reason: format!(
                "{}:{} invalid positive OBJ index '{value}': {error}",
                record.relative_path, line_number
            ),
        })?;
    if parsed == 0 {
        return invalid_obj(
            "face.index",
            record,
            line_number,
            "OBJ indices are 1-based and must be positive",
        );
    }
    Ok(parsed - 1)
}

fn next_f32<'a>(
    fields: &mut impl Iterator<Item = &'a str>,
    record: &AssetVaultRecord,
    line_number: usize,
    tag: &str,
) -> Result<f32, EngineError> {
    let value = fields
        .next()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.composition.obj.{tag}"),
            reason: format!(
                "{}:{} missing numeric field",
                record.relative_path, line_number
            ),
        })?;
    value
        .parse::<f32>()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.composition.obj.{tag}"),
            reason: format!(
                "{}:{} invalid number '{value}': {error}",
                record.relative_path, line_number
            ),
        })
}
