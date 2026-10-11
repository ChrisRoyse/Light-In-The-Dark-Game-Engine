use std::collections::BTreeSet;

use super::{
    SpriteBillboardLibrarySelection, SpriteBillboardSearchRow,
    sprite_billboard_validate::ValidatedSpriteBillboardLibrary,
};

pub(super) fn build_search_rows(
    library: &SpriteBillboardLibrarySelection,
    validated: &ValidatedSpriteBillboardLibrary<'_>,
) -> Vec<SpriteBillboardSearchRow> {
    let mut rows = Vec::new();
    for texture in &library.textures {
        rows.push(search_row(
            "texture",
            &texture.texture_id,
            Some(&texture.texture_id),
            None,
            Some(texture.content_address()),
            vec![
                texture.texture_id.clone(),
                texture.source_relative_path.clone(),
            ],
            &texture.tags,
        ));
    }
    for material in &library.materials {
        let texture = validated.textures[material.texture_id.as_str()];
        rows.push(search_row(
            "material",
            &material.material_id,
            Some(&material.texture_id),
            Some(&material.material_id),
            Some(texture.content_address()),
            vec![
                material.material_id.clone(),
                material.texture_id.clone(),
                material.alpha_mode.id().to_string(),
            ],
            &material.tags,
        ));
    }
    for sheet in &library.sheets {
        let texture = validated.textures[sheet.texture_id.as_str()];
        rows.push(search_row(
            "sheet",
            &sheet.sheet_id,
            Some(&sheet.texture_id),
            None,
            Some(texture.content_address()),
            vec![sheet.sheet_id.clone(), sheet.texture_id.clone()],
            &sheet.tags,
        ));
    }
    for billboard in &library.billboards {
        let material = validated.materials[billboard.material_id.as_str()];
        let texture = validated.textures[material.texture_id.as_str()];
        rows.push(search_row(
            "billboard",
            &billboard.billboard_id,
            Some(&texture.texture_id),
            Some(&material.material_id),
            Some(texture.content_address()),
            vec![
                billboard.billboard_id.clone(),
                billboard.material_id.clone(),
                billboard.facing.id().to_string(),
            ],
            &billboard.tags,
        ));
    }
    rows
}

fn search_row(
    kind: &str,
    id: &str,
    texture_id: Option<&str>,
    material_id: Option<&str>,
    content_address: Option<String>,
    raw_terms: Vec<String>,
    tags: &[String],
) -> SpriteBillboardSearchRow {
    let mut terms = BTreeSet::new();
    terms.insert(kind.to_string());
    for raw in raw_terms.iter().chain(tags.iter()) {
        for term in raw.split(|ch: char| !ch.is_ascii_alphanumeric()) {
            if !term.is_empty() {
                terms.insert(term.to_ascii_lowercase());
            }
        }
    }
    SpriteBillboardSearchRow {
        row_id: format!("{kind}:{id}"),
        entity_kind: kind.to_string(),
        primary_id: id.to_string(),
        texture_id: texture_id.map(str::to_string),
        material_id: material_id.map(str::to_string),
        content_address,
        terms: terms.into_iter().collect(),
    }
}
