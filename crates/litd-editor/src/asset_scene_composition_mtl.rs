use std::{collections::BTreeMap, fs};

use litd_engine::{EngineError, math::Vec3};

use crate::asset_vault::AssetVaultRecord;

use super::invalid_composition;

#[derive(Debug, Clone)]
pub(super) struct ObjMaterial {
    pub(super) diffuse: [f32; 3],
    pub(super) emissive: f32,
    pub(super) roughness: f32,
}

pub(super) fn load_mtl(
    record: &AssetVaultRecord,
) -> Result<BTreeMap<String, ObjMaterial>, EngineError> {
    let text = fs::read_to_string(&record.absolute_path).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.mtl.read".to_string(),
            reason: format!(
                "{} failed MTL read: {error}",
                record.absolute_path.display()
            ),
        }
    })?;
    let mut materials = BTreeMap::new();
    let mut current_name: Option<String> = None;
    let mut current = default_material();
    for (line_index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(tag) = fields.next() else {
            continue;
        };
        match tag {
            "newmtl" => {
                if let Some(name) = current_name.replace(fields.collect::<Vec<_>>().join(" ")) {
                    materials.insert(name, current.clone());
                    current = default_material();
                }
                if current_name
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    return invalid_mtl("newmtl", record, line_index + 1, "material name is empty");
                }
            }
            "Kd" => current.diffuse = parse_color3(fields, record, line_index + 1, "Kd")?,
            "Ke" => {
                let color = parse_color3(fields, record, line_index + 1, "Ke")?;
                current.emissive = color.into_iter().fold(0.0_f32, f32::max);
            }
            "Ns" => {
                let shininess = next_f32(&mut fields, record, line_index + 1, "Ns")?;
                current.roughness = (1.0 - (shininess / 256.0).clamp(0.0, 1.0)).clamp(0.18, 0.95);
            }
            "Ka" | "Ks" | "Ni" | "d" | "illum" => {}
            other => {
                return invalid_mtl(
                    "token",
                    record,
                    line_index + 1,
                    format!("unsupported MTL token '{other}'"),
                );
            }
        }
    }
    if let Some(name) = current_name {
        materials.insert(name, current);
    }
    if materials.is_empty() {
        return invalid_composition(
            "mtl.materials",
            format!("{} has no materials", record.relative_path),
        );
    }
    Ok(materials)
}

fn default_material() -> ObjMaterial {
    ObjMaterial {
        diffuse: [1.0, 1.0, 1.0],
        emissive: 0.0,
        roughness: 0.65,
    }
}

fn parse_color3<'a>(
    fields: impl Iterator<Item = &'a str>,
    record: &AssetVaultRecord,
    line_number: usize,
    tag: &str,
) -> Result<[f32; 3], EngineError> {
    let mut fields = fields;
    let values = [
        next_f32(&mut fields, record, line_number, tag)?,
        next_f32(&mut fields, record, line_number, tag)?,
        next_f32(&mut fields, record, line_number, tag)?,
    ];
    let color = Vec3::from_array(values);
    if !color.is_finite() {
        return invalid_mtl(tag, record, line_number, "color contains non-finite values");
    }
    Ok(values)
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
            field: format!("editor.asset_scene.composition.mtl.{tag}"),
            reason: format!(
                "{}:{} missing numeric field",
                record.relative_path, line_number
            ),
        })?;
    value
        .parse::<f32>()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: format!("editor.asset_scene.composition.mtl.{tag}"),
            reason: format!(
                "{}:{} invalid number '{value}': {error}",
                record.relative_path, line_number
            ),
        })
}

fn invalid_mtl<T>(
    field: impl Into<String>,
    record: &AssetVaultRecord,
    line_number: usize,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    invalid_composition(
        format!("mtl.{}", field.into()),
        format!("{}:{line_number}: {}", record.relative_path, reason.into()),
    )
}
