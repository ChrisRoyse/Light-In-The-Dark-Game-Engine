use std::path::Path;

use image::{Rgba, RgbaImage};
use litd_editor::{
    history::EditorDocument,
    terrain_pathing::{TerrainPathMovement, effective_pathing_for_cell},
};

pub fn write_preview_png(
    document: &EditorDocument,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let cell_px = 24_u32;
    let width = extent(document, |cell| cell.tile_x) + 1;
    let height = extent(document, |cell| cell.tile_z) + 1;
    let mut image =
        RgbaImage::from_pixel(width * cell_px, height * cell_px, Rgba([19, 23, 28, 255]));
    for cell in document.terrain_cells.values() {
        let flags = effective_pathing_for_cell(document, &cell.id)?;
        let color = if !flags.allows(TerrainPathMovement::GroundUnit) {
            Rgba([32, 34, 39, 255])
        } else if cell.tile_id.contains("moss") {
            Rgba([61, 135, 70, 255])
        } else {
            Rgba([87, 113, 57, 255])
        };
        fill_rect(
            &mut image,
            cell.tile_x as u32 * cell_px,
            cell.tile_z as u32 * cell_px,
            cell_px,
            cell_px,
            color,
        );
    }
    draw_issue_region(document, &mut image, cell_px);
    draw_objects(document, &mut image, cell_px);
    image.save(path)?;
    Ok(())
}

fn draw_issue_region(document: &EditorDocument, image: &mut RgbaImage, cell_px: u32) {
    for region in document
        .regions
        .values()
        .filter(|region| region.id == "region.issue734.spawn")
    {
        draw_border(
            image,
            (region.center.x - region.half_extents.x).floor().max(0.0) as u32 * cell_px,
            (region.center.z - region.half_extents.z).floor().max(0.0) as u32 * cell_px,
            (region.half_extents.x * 2.0).ceil() as u32 * cell_px,
            (region.half_extents.z * 2.0).ceil() as u32 * cell_px,
            Rgba([238, 193, 82, 255]),
        );
    }
}

fn draw_objects(document: &EditorDocument, image: &mut RgbaImage, cell_px: u32) {
    for object in document.objects.values() {
        let color = match object.kind.as_str() {
            "Unit" => Some(Rgba([190, 62, 54, 255])),
            "StartLocation" => Some(Rgba([72, 148, 219, 255])),
            "Doodad" => Some(Rgba([160, 94, 194, 255])),
            _ => None,
        };
        if let Some(color) = color {
            let x = (object.transform.translation.x.floor().max(0.0) as u32 * cell_px) + 7;
            let z = (object.transform.translation.z.floor().max(0.0) as u32 * cell_px) + 7;
            fill_rect(image, x, z, 10, 10, color);
        }
    }
}

fn extent(
    document: &EditorDocument,
    pick: fn(&litd_editor::terrain_bulk_ops::EditorTerrainCell) -> i32,
) -> u32 {
    document.terrain_cells.values().map(pick).max().unwrap_or(0) as u32
}

fn fill_rect(image: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    for yy in y..(y + h).min(image.height()) {
        for xx in x..(x + w).min(image.width()) {
            image.put_pixel(xx, yy, color);
        }
    }
}

fn draw_border(image: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    fill_rect(image, x, y, w, 2, color);
    fill_rect(image, x, y + h.saturating_sub(2), w, 2, color);
    fill_rect(image, x, y, 2, h, color);
    fill_rect(image, x + w.saturating_sub(2), y, 2, h, color);
}
