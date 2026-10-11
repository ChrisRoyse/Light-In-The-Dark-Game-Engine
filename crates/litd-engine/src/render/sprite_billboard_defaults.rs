use litd_hash::sha256_hex;

use crate::EngineError;

use super::{
    BillboardFacingMode, SPRITE_BILLBOARD_SCHEMA, SpriteAlphaMode, SpriteBillboardInstance,
    SpriteBillboardLibrarySelection, SpriteMaterialRecord, SpriteSheetRecord, SpriteTextureRecord,
    build_sprite_billboard_library_readback,
};

pub(super) fn issue868_defaults() -> Result<SpriteBillboardLibrarySelection, EngineError> {
    let textures = vec![
        texture(
            "texture.issue868.hero_rune_sheet",
            "generated/issue868/hero-rune-sheet.rgba8",
            ["hero", "rune", "sheet"],
            hero_sheet_pixel,
        ),
        texture(
            "texture.issue868.fire_orb",
            "generated/issue868/fire-orb.rgba8",
            ["fire", "spell", "glow"],
            fire_orb_pixel,
        ),
        texture(
            "texture.issue868.ground_marker",
            "generated/issue868/ground-marker.rgba8",
            ["marker", "targeting", "transparent"],
            marker_pixel,
        ),
    ];
    let library = SpriteBillboardLibrarySelection {
        schema: SPRITE_BILLBOARD_SCHEMA.to_string(),
        library_id: "library.issue868.sprite_billboards".to_string(),
        selected_billboard_id: "billboard.issue868.hero_idle".to_string(),
        materials: vec![
            material(
                "mat.sprite.issue868.hero_rune",
                "texture.issue868.hero_rune_sheet",
                [0.92, 0.96, 1.00, 1.0],
                SpriteAlphaMode::Blended,
                ["hero", "unit"],
            ),
            material(
                "mat.sprite.issue868.fire_orb",
                "texture.issue868.fire_orb",
                [1.00, 0.78, 0.32, 1.0],
                SpriteAlphaMode::Additive,
                ["projectile", "spell"],
            ),
            material(
                "mat.sprite.issue868.ground_marker",
                "texture.issue868.ground_marker",
                [0.38, 0.82, 1.00, 0.72],
                SpriteAlphaMode::Blended,
                ["selection", "marker"],
            ),
        ],
        sheets: vec![SpriteSheetRecord {
            sheet_id: "sheet.issue868.hero_rune_2x2".to_string(),
            texture_id: "texture.issue868.hero_rune_sheet".to_string(),
            columns: 2,
            rows: 2,
            frame_count: 4,
            tags: tag_vec(["hero", "animation"]),
        }],
        billboards: vec![
            billboard(BillboardArgs {
                billboard_id: "billboard.issue868.hero_idle",
                material_id: "mat.sprite.issue868.hero_rune",
                sheet_id: Some("sheet.issue868.hero_rune_2x2"),
                frame_index: 0,
                center: [-1.4, 0.8, 0.0],
                size: [0.72, 1.08],
                facing: BillboardFacingMode::CameraFacing,
                sort_layer: 10,
                tags: ["unit", "idle"],
            }),
            billboard(BillboardArgs {
                billboard_id: "billboard.issue868.hero_cast",
                material_id: "mat.sprite.issue868.hero_rune",
                sheet_id: Some("sheet.issue868.hero_rune_2x2"),
                frame_index: 2,
                center: [-0.35, 0.9, 0.1],
                size: [0.78, 1.16],
                facing: BillboardFacingMode::AxisLockedY,
                sort_layer: 12,
                tags: ["unit", "cast"],
            }),
            billboard(BillboardArgs {
                billboard_id: "billboard.issue868.fireball",
                material_id: "mat.sprite.issue868.fire_orb",
                sheet_id: None,
                frame_index: 0,
                center: [0.72, 0.72, 0.2],
                size: [0.62, 0.62],
                facing: BillboardFacingMode::CameraFacing,
                sort_layer: 20,
                tags: ["spell", "projectile"],
            }),
            billboard(BillboardArgs {
                billboard_id: "billboard.issue868.ground_marker",
                material_id: "mat.sprite.issue868.ground_marker",
                sheet_id: None,
                frame_index: 0,
                center: [1.55, 0.25, -0.1],
                size: [1.12, 0.44],
                facing: BillboardFacingMode::ScreenAligned,
                sort_layer: 1,
                tags: ["marker", "target"],
            }),
        ],
        textures,
    };
    build_sprite_billboard_library_readback(&library)?;
    Ok(library)
}

fn texture(
    texture_id: &str,
    source_relative_path: &str,
    tags: [&str; 3],
    pixel: fn(u32, u32) -> [u8; 4],
) -> SpriteTextureRecord {
    let width = 16;
    let height = 16;
    let mut rgba8 = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba8.extend_from_slice(&pixel(x, y));
        }
    }
    SpriteTextureRecord {
        texture_id: texture_id.to_string(),
        source_relative_path: source_relative_path.to_string(),
        width,
        height,
        rgba8_sha256: sha256_hex(&rgba8),
        rgba8,
        tags: tag_vec(tags),
    }
}

fn material(
    material_id: &str,
    texture_id: &str,
    tint: [f32; 4],
    alpha_mode: SpriteAlphaMode,
    tags: [&str; 2],
) -> SpriteMaterialRecord {
    SpriteMaterialRecord {
        material_id: material_id.to_string(),
        texture_id: texture_id.to_string(),
        tint,
        alpha_mode,
        alpha_cutoff: 0.0,
        emissive_rgb: [0.0, 0.0, 0.0],
        unlit: true,
        tags: tag_vec(tags),
    }
}

struct BillboardArgs<'a> {
    billboard_id: &'a str,
    material_id: &'a str,
    sheet_id: Option<&'a str>,
    frame_index: u32,
    center: [f32; 3],
    size: [f32; 2],
    facing: BillboardFacingMode,
    sort_layer: i32,
    tags: [&'a str; 2],
}

fn billboard(args: BillboardArgs<'_>) -> SpriteBillboardInstance {
    let BillboardArgs {
        billboard_id,
        material_id,
        sheet_id,
        frame_index,
        center,
        size,
        facing,
        sort_layer,
        tags,
    } = args;
    SpriteBillboardInstance {
        billboard_id: billboard_id.to_string(),
        material_id: material_id.to_string(),
        sheet_id: sheet_id.map(str::to_string),
        frame_index,
        center,
        size,
        facing,
        sort_layer,
        tags: tag_vec(tags),
    }
}

fn tag_vec<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_string).collect()
}

fn hero_sheet_pixel(x: u32, y: u32) -> [u8; 4] {
    let frame_x = x / 8;
    let frame_y = y / 8;
    let local_x = x % 8;
    let local_y = y % 8;
    let edge = local_x == 0 || local_x == 7 || local_y == 0 || local_y == 7;
    let rune = local_x == local_y || local_x + local_y == 7;
    match (frame_x, frame_y, edge, rune) {
        (_, _, true, _) => [28, 52, 74, 220],
        (0, 0, _, true) => [78, 208, 255, 255],
        (1, 0, _, true) => [255, 208, 88, 255],
        (0, 1, _, true) => [168, 98, 255, 255],
        (1, 1, _, true) => [116, 240, 168, 255],
        _ => [38 + frame_x as u8 * 26, 44 + frame_y as u8 * 32, 62, 210],
    }
}

fn fire_orb_pixel(x: u32, y: u32) -> [u8; 4] {
    let dx = x as f32 - 7.5;
    let dy = y as f32 - 7.5;
    let dist = (dx * dx + dy * dy).sqrt() / 7.5;
    if dist > 1.0 {
        return [0, 0, 0, 0];
    }
    let heat = ((1.0 - dist) * 255.0) as u8;
    [
        255,
        heat.max(72),
        (heat / 5).max(12),
        (220.0 * (1.0 - dist * 0.45)) as u8,
    ]
}

fn marker_pixel(x: u32, y: u32) -> [u8; 4] {
    let border = x < 2 || y < 2 || x > 13 || y > 13;
    let cross = x == 7 || x == 8 || y == 7 || y == 8;
    if border || cross {
        [70, 205, 255, 180]
    } else {
        [20, 88, 110, 42]
    }
}
