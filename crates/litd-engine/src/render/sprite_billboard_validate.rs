use std::collections::{BTreeMap, BTreeSet};

use litd_hash::sha256_hex;

use crate::EngineError;

use super::{
    ATLAS_ROW_ALIGNMENT, MAX_ATLAS_DIMENSION, MAX_BILLBOARDS, MAX_MATERIALS, MAX_SHEETS,
    MAX_TEXTURES, SPRITE_BILLBOARD_SCHEMA, SpriteBillboardInstance,
    SpriteBillboardLibrarySelection, SpriteMaterialRecord, SpriteSheetRecord, SpriteTextureRecord,
    invalid_sprite_billboard, sprite_billboard_error,
};

pub(super) struct ValidatedSpriteBillboardLibrary<'a> {
    pub textures: BTreeMap<&'a str, &'a SpriteTextureRecord>,
    pub materials: BTreeMap<&'a str, &'a SpriteMaterialRecord>,
    pub sheets: BTreeMap<&'a str, &'a SpriteSheetRecord>,
}

pub(super) fn validate_library<'a>(
    library: &'a SpriteBillboardLibrarySelection,
) -> Result<ValidatedSpriteBillboardLibrary<'a>, EngineError> {
    validate_library_identity(library)?;
    let textures = validate_textures(&library.textures)?;
    validate_combined_atlas(&library.textures)?;
    let materials = validate_materials(&library.materials, &textures)?;
    let sheets = validate_sheets(&library.sheets, &textures)?;
    validate_billboards(library, &materials, &sheets)?;
    Ok(ValidatedSpriteBillboardLibrary {
        textures,
        materials,
        sheets,
    })
}

pub(super) fn frame_uv(
    sheet: &SpriteSheetRecord,
    frame_index: u32,
) -> Result<[f32; 4], EngineError> {
    if frame_index >= sheet.frame_count {
        return invalid_sprite_billboard(
            "billboard.frame_index",
            "frame index is outside the sheet frame_count",
        );
    }
    let column = frame_index % sheet.columns;
    let row = frame_index / sheet.columns;
    let inv_w = 1.0 / sheet.columns as f32;
    let inv_h = 1.0 / sheet.rows as f32;
    Ok([
        column as f32 * inv_w,
        row as f32 * inv_h,
        (column + 1) as f32 * inv_w,
        (row + 1) as f32 * inv_h,
    ])
}

fn validate_library_identity(library: &SpriteBillboardLibrarySelection) -> Result<(), EngineError> {
    if library.schema != SPRITE_BILLBOARD_SCHEMA {
        return invalid_sprite_billboard("schema", "unexpected sprite billboard schema");
    }
    validate_id("library_id", &library.library_id)?;
    validate_id("selected_billboard_id", &library.selected_billboard_id)
}

fn validate_textures(
    records: &[SpriteTextureRecord],
) -> Result<BTreeMap<&str, &SpriteTextureRecord>, EngineError> {
    if records.is_empty() || records.len() > MAX_TEXTURES {
        return invalid_sprite_billboard("textures", "texture count must be 1..=128");
    }
    let mut by_id = BTreeMap::new();
    let mut content = BTreeSet::new();
    for texture in records {
        validate_id("texture.texture_id", &texture.texture_id)?;
        validate_id(
            "texture.source_relative_path",
            &texture.source_relative_path,
        )?;
        validate_tags("texture.tags", &texture.tags)?;
        validate_sha256("texture.rgba8_sha256", &texture.rgba8_sha256)?;
        if texture.width == 0 || texture.height == 0 {
            return invalid_sprite_billboard(
                "texture.dimensions",
                "width and height must be non-zero",
            );
        }
        let expected = rgba8_len(texture.width, texture.height)?;
        if texture.rgba8.len() != expected {
            return invalid_sprite_billboard(
                "texture.rgba8",
                format!(
                    "{} expected {expected} bytes, found {}",
                    texture.texture_id,
                    texture.rgba8.len()
                ),
            );
        }
        let actual = sha256_hex(&texture.rgba8);
        if actual != texture.rgba8_sha256 {
            return invalid_sprite_billboard(
                "texture.rgba8_sha256",
                format!(
                    "{} manifest hash {} != actual {actual}",
                    texture.texture_id, texture.rgba8_sha256
                ),
            );
        }
        if !content.insert(texture.content_address()) {
            return invalid_sprite_billboard(
                "texture.content_address",
                format!("duplicate content address {}", texture.content_address()),
            );
        }
        if by_id.insert(texture.texture_id.as_str(), texture).is_some() {
            return invalid_sprite_billboard(
                "texture.texture_id",
                format!("duplicate texture_id {}", texture.texture_id),
            );
        }
    }
    Ok(by_id)
}

/// Bound the *combined* horizontal atlas that `build_atlas` packs from every
/// library texture. Per-texture validation only bounds each texture's own
/// `width*height*4` to u32; without this check a set of individually-valid
/// textures (e.g. a 1_000_000x1 beside a 1x4096) sums to a combined atlas whose
/// `width*height*4` wraps u32 in release, producing an undersized buffer and a
/// `copy_from_slice` panic instead of a structured error (#1484-1). All checked
/// arithmetic; the dimension bound also guarantees the atlas fits a GPU 2D
/// texture on the #928 min-spec target.
fn validate_combined_atlas(records: &[SpriteTextureRecord]) -> Result<(), EngineError> {
    let mut source_width: u32 = 0;
    let mut max_height: u32 = 0;
    for texture in records {
        source_width = source_width.checked_add(texture.width).ok_or_else(|| {
            sprite_billboard_error("atlas.width", "combined atlas width overflows u32")
        })?;
        max_height = max_height.max(texture.height);
    }
    let atlas_width = source_width
        .max(1)
        .checked_next_multiple_of(ATLAS_ROW_ALIGNMENT)
        .ok_or_else(|| {
            sprite_billboard_error("atlas.width", "row-aligned atlas width overflows u32")
        })?;
    let atlas_height = max_height.max(1);
    if atlas_width > MAX_ATLAS_DIMENSION || atlas_height > MAX_ATLAS_DIMENSION {
        return invalid_sprite_billboard(
            "atlas.dimensions",
            format!(
                "combined atlas {atlas_width}x{atlas_height} exceeds max GPU 2D texture dimension {MAX_ATLAS_DIMENSION}"
            ),
        );
    }
    // Guaranteed to fit by the dimension bound above; assert it explicitly so a
    // future bump to MAX_ATLAS_DIMENSION can never silently reintroduce the wrap.
    atlas_width
        .checked_mul(atlas_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            sprite_billboard_error("atlas.bytes", "combined atlas byte length overflows u32")
        })?;
    Ok(())
}

fn validate_materials<'a>(
    records: &'a [SpriteMaterialRecord],
    textures: &BTreeMap<&str, &SpriteTextureRecord>,
) -> Result<BTreeMap<&'a str, &'a SpriteMaterialRecord>, EngineError> {
    if records.is_empty() || records.len() > MAX_MATERIALS {
        return invalid_sprite_billboard("materials", "material count must be 1..=256");
    }
    let mut by_id = BTreeMap::new();
    for material in records {
        validate_id("material.material_id", &material.material_id)?;
        validate_id("material.texture_id", &material.texture_id)?;
        validate_tags("material.tags", &material.tags)?;
        if !textures.contains_key(material.texture_id.as_str()) {
            return invalid_sprite_billboard(
                "material.texture_id",
                format!(
                    "{} references missing texture {}",
                    material.material_id, material.texture_id
                ),
            );
        }
        validate_color4("material.tint", material.tint)?;
        validate_color3("material.emissive_rgb", material.emissive_rgb)?;
        if !material.alpha_cutoff.is_finite() || !(0.0..=1.0).contains(&material.alpha_cutoff) {
            return invalid_sprite_billboard(
                "material.alpha_cutoff",
                "alpha cutoff must be finite inside [0,1]",
            );
        }
        if by_id
            .insert(material.material_id.as_str(), material)
            .is_some()
        {
            return invalid_sprite_billboard(
                "material.material_id",
                format!("duplicate material_id {}", material.material_id),
            );
        }
    }
    Ok(by_id)
}

fn validate_sheets<'a>(
    records: &'a [SpriteSheetRecord],
    textures: &BTreeMap<&str, &SpriteTextureRecord>,
) -> Result<BTreeMap<&'a str, &'a SpriteSheetRecord>, EngineError> {
    if records.len() > MAX_SHEETS {
        return invalid_sprite_billboard("sheets", "sheet count must stay <= 128");
    }
    let mut by_id = BTreeMap::new();
    for sheet in records {
        validate_id("sheet.sheet_id", &sheet.sheet_id)?;
        validate_id("sheet.texture_id", &sheet.texture_id)?;
        validate_tags("sheet.tags", &sheet.tags)?;
        if !textures.contains_key(sheet.texture_id.as_str()) {
            return invalid_sprite_billboard(
                "sheet.texture_id",
                "sheet references missing texture",
            );
        }
        if sheet.columns == 0 || sheet.rows == 0 || sheet.frame_count == 0 {
            return invalid_sprite_billboard(
                "sheet.grid",
                "columns, rows, and frame_count must be non-zero",
            );
        }
        if sheet.frame_count > sheet.columns.saturating_mul(sheet.rows) {
            return invalid_sprite_billboard(
                "sheet.frame_count",
                "frame_count exceeds columns * rows",
            );
        }
        if by_id.insert(sheet.sheet_id.as_str(), sheet).is_some() {
            return invalid_sprite_billboard(
                "sheet.sheet_id",
                format!("duplicate sheet_id {}", sheet.sheet_id),
            );
        }
    }
    Ok(by_id)
}

fn validate_billboards(
    library: &SpriteBillboardLibrarySelection,
    materials: &BTreeMap<&str, &SpriteMaterialRecord>,
    sheets: &BTreeMap<&str, &SpriteSheetRecord>,
) -> Result<(), EngineError> {
    if library.billboards.is_empty() || library.billboards.len() > MAX_BILLBOARDS {
        return invalid_sprite_billboard("billboards", "billboard count must be 1..=2048");
    }
    let mut seen = BTreeSet::new();
    for billboard in &library.billboards {
        validate_billboard_shape(billboard)?;
        if !seen.insert(billboard.billboard_id.clone()) {
            return invalid_sprite_billboard(
                "billboard.billboard_id",
                format!("duplicate billboard_id {}", billboard.billboard_id),
            );
        }
        if !materials.contains_key(billboard.material_id.as_str()) {
            return invalid_sprite_billboard(
                "billboard.material_id",
                format!(
                    "{} references missing material {}",
                    billboard.billboard_id, billboard.material_id
                ),
            );
        }
        if let Some(sheet_id) = &billboard.sheet_id
            && !sheets.contains_key(sheet_id.as_str())
        {
            return invalid_sprite_billboard(
                "billboard.sheet_id",
                format!(
                    "{} references missing sheet {sheet_id}",
                    billboard.billboard_id
                ),
            );
        }
    }
    if !seen.contains(&library.selected_billboard_id) {
        return invalid_sprite_billboard(
            "selected_billboard_id",
            "selected billboard is not present in the library",
        );
    }
    Ok(())
}

fn validate_billboard_shape(billboard: &SpriteBillboardInstance) -> Result<(), EngineError> {
    validate_id("billboard.billboard_id", &billboard.billboard_id)?;
    validate_id("billboard.material_id", &billboard.material_id)?;
    if let Some(sheet_id) = &billboard.sheet_id {
        validate_id("billboard.sheet_id", sheet_id)?;
    }
    validate_tags("billboard.tags", &billboard.tags)?;
    if !billboard.center.iter().all(|value| value.is_finite()) {
        return invalid_sprite_billboard("billboard.center", "center must be finite");
    }
    if !billboard
        .size
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
    {
        return invalid_sprite_billboard(
            "billboard.size",
            "size values must be finite and positive",
        );
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_sprite_billboard(field, "value must be a non-empty single line");
    }
    Ok(())
}

fn validate_tags(field: &str, tags: &[String]) -> Result<(), EngineError> {
    for tag in tags {
        validate_id(field, tag)?;
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    if !litd_hash::is_sha256_hex(value) {
        return invalid_sprite_billboard(field, "sha256 must be 64 lowercase hex characters");
    }
    Ok(())
}

fn validate_color4(field: &str, color: [f32; 4]) -> Result<(), EngineError> {
    if !color
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        return invalid_sprite_billboard(field, "RGBA values must be finite inside [0,1]");
    }
    Ok(())
}

fn validate_color3(field: &str, color: [f32; 3]) -> Result<(), EngineError> {
    if !color.iter().all(|value| value.is_finite() && *value >= 0.0) {
        return invalid_sprite_billboard(
            field,
            "RGB emissive values must be finite and non-negative",
        );
    }
    Ok(())
}

fn rgba8_len(width: u32, height: u32) -> Result<usize, EngineError> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|bytes| bytes as usize)
        .ok_or_else(|| {
            sprite_billboard_error(
                "texture.dimensions",
                "texture dimensions overflow RGBA8 byte length",
            )
        })
}
