//! Deterministic lowering of authored terrain (#806/#807/#811) into the frozen
//! runtime artifacts consumed by the merged #1739 CDLOD terrain runtime and the
//! `litd-engine` nav (A*/bake) path.
//!
//! The editor OWNS authoring: the `terrain_*` modules mutate an `EditorDocument`
//! that is persisted through the REAL Calyx world-record path
//! (`world_creation_store`). This module is the lowering VALVE: it turns that
//! authored, Calyx-persisted document into three derived-from-Calyx,
//! fingerprinted, reproducible artifacts (never authoritative -- #1348 sec.6):
//!
//! * `LoweredHeightField` (#807 cliffs/ramps/height layers): a dense
//!   `HeightField` + `TerrainConfig` the CDLOD `TerrainRuntime` streams and draws.
//! * `LoweredTileSplat` (#806 tile/tileset palette + custom tilesets): a per-tile
//!   palette-index buffer the render splat/atlas consumes.
//! * `LoweredNavInput` (#811 pathing / blockers / walkable bridges / doodad
//!   footprints): a `NavBakeInput` the nav bake lowers to a `BakedNavMesh` that
//!   the deterministic A* (`find_path`) queries.
//!
//! Every lowering is a pure function of the document: identical input bytes yield
//! byte-identical artifacts (fingerprinted with the editor's canonical
//! `stable_fingerprint`), so an agent or human can Full-State-Verify the lowered
//! runtime data against the authored source. Failures are loud typed errors.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{
    EngineError,
    math::Fixed,
    nav::{NavBakeInput, NavPathingFlags, NavStaticBlockerInput, NavTerrainCellInput},
    render::terrain::{HeightField, TerrainConfig},
};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    terrain_authoring::{TERRAIN_TILE_X_PROPERTY, TERRAIN_TILE_Z_PROPERTY},
    terrain_bulk_ops::EditorTerrainCell,
    terrain_pathing::{TerrainPathingFlags, effective_pathing_for_cell},
};

#[path = "terrain_lowering_types.rs"]
mod terrain_lowering_types;
pub use terrain_lowering_types::{
    HeightFieldLowerParams, LoweredHeightField, LoweredNavInput, LoweredTerrainBundle,
    LoweredTileSplat, TERRAIN_HEIGHTFIELD_LOWERED_SCHEMA, TERRAIN_NAV_LOWERED_SCHEMA,
    TERRAIN_TILE_SPLAT_LOWERED_SCHEMA,
};

/// One world unit of height quantised to this many fixed-point sub-units when
/// building nav `height_fp`. Deterministic: `round(height * SCALE)` over the same
/// `f32` always yields the same integer.
const NAV_HEIGHT_MILLI: i64 = 1000;

impl HeightFieldLowerParams {
    fn validate(&self) -> Result<(), EngineError> {
        if self.leaf_sample_span < 1 {
            return invalid("params.leaf_sample_span", "leaf_sample_span must be >= 1");
        }
        if !self.cliff_step.is_finite() {
            return invalid("params.cliff_step", "cliff_step must be finite");
        }
        Ok(())
    }
}

/// Integer tile bounds of the authored terrain (inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileBounds {
    min_x: i32,
    max_x: i32,
    min_z: i32,
    max_z: i32,
}

impl TileBounds {
    fn tiles_x(&self) -> u32 {
        (self.max_x - self.min_x + 1) as u32
    }
    fn tiles_z(&self) -> u32 {
        (self.max_z - self.min_z + 1) as u32
    }
}

/// Lower every runtime artifact for `document` in one deterministic pass.
pub fn lower_terrain_bundle(
    document: &EditorDocument,
    params: &HeightFieldLowerParams,
) -> Result<LoweredTerrainBundle, EngineError> {
    let source_fingerprint = stable_fingerprint(document)?;
    let height_field = lower_height_field(document, params)?;
    let tile_splat = lower_tile_splat_map(document)?;
    let nav_input = lower_nav_bake_input(document, params.cliff_step)?;
    Ok(LoweredTerrainBundle {
        source_fingerprint,
        height_field,
        tile_splat,
        nav_input,
    })
}

/// Lower the authored heights + cliffs/ramps into a CDLOD height field (#807).
pub fn lower_height_field(
    document: &EditorDocument,
    params: &HeightFieldLowerParams,
) -> Result<LoweredHeightField, EngineError> {
    document.validate()?;
    params.validate()?;
    let bounds = tile_bounds(document)?;
    let by_coord = dense_cells_by_coord(document, bounds)?;
    let source_fingerprint = stable_fingerprint(document)?;

    let tiles_x = bounds.tiles_x();
    let tiles_z = bounds.tiles_z();
    let size = params
        .leaf_nodes_per_side
        .checked_mul(params.leaf_sample_span)
        .and_then(|v| v.checked_add(1))
        .ok_or_else(|| invalid_err("params.size", "sample grid size overflowed"))?;

    let mut heights = Vec::with_capacity((size as usize) * (size as usize));
    let mut min_height = f32::INFINITY;
    let mut max_height = f32::NEG_INFINITY;
    for sz in 0..size {
        let tz = nearest_tile(sz, size, tiles_z, bounds.min_z);
        for sx in 0..size {
            let tx = nearest_tile(sx, size, tiles_x, bounds.min_x);
            let cell = by_coord
                .get(&(tx, tz))
                .ok_or_else(|| invalid_err("terrain_cells", "dense grid lookup missed a cell"))?;
            let h = effective_height(cell, params.cliff_step);
            min_height = min_height.min(h);
            max_height = max_height.max(h);
            heights.push(h);
        }
    }

    let field = HeightField::new(size, params.cell_size, heights)?;
    let config = TerrainConfig::new(
        params.leaf_nodes_per_side,
        params.grid_dim,
        params.lod0_range,
        params.morph_start_ratio,
        params.flat_error_cutoff,
    )?;
    let sample_count = (size as usize) * (size as usize);
    let fingerprint = stable_fingerprint(&(
        TERRAIN_HEIGHTFIELD_LOWERED_SCHEMA,
        &field,
        &config,
        &source_fingerprint,
    ))?;
    Ok(LoweredHeightField {
        schema: TERRAIN_HEIGHTFIELD_LOWERED_SCHEMA.to_string(),
        source_fingerprint,
        field,
        config,
        tiles_x,
        tiles_z,
        min_height,
        max_height,
        sample_count,
        fingerprint,
    })
}

/// Lower the authored per-tile tileset ids into a palette + index buffer (#806).
pub fn lower_tile_splat_map(document: &EditorDocument) -> Result<LoweredTileSplat, EngineError> {
    document.validate()?;
    let bounds = tile_bounds(document)?;
    let by_coord = dense_cells_by_coord(document, bounds)?;
    let source_fingerprint = stable_fingerprint(document)?;

    let palette: Vec<String> = by_coord
        .values()
        .map(|cell| cell.tile_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if palette.len() > u16::MAX as usize {
        return invalid(
            "tile_splat.palette",
            "authored tile palette exceeds the u16 splat-index budget",
        );
    }
    let palette_index: BTreeMap<&str, u16> = palette
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i as u16))
        .collect();

    let tiles_x = bounds.tiles_x();
    let tiles_z = bounds.tiles_z();
    let mut indices = Vec::with_capacity((tiles_x as usize) * (tiles_z as usize));
    for lz in 0..tiles_z {
        for lx in 0..tiles_x {
            let coord = (bounds.min_x + lx as i32, bounds.min_z + lz as i32);
            let cell = by_coord
                .get(&coord)
                .ok_or_else(|| invalid_err("terrain_cells", "dense grid lookup missed a cell"))?;
            let index = *palette_index.get(cell.tile_id.as_str()).ok_or_else(|| {
                invalid_err("tile_splat.index", "cell tile id missing from palette")
            })?;
            indices.push(index);
        }
    }
    let cell_count = indices.len();
    let fingerprint = stable_fingerprint(&(
        TERRAIN_TILE_SPLAT_LOWERED_SCHEMA,
        &palette,
        &indices,
        &source_fingerprint,
    ))?;
    Ok(LoweredTileSplat {
        schema: TERRAIN_TILE_SPLAT_LOWERED_SCHEMA.to_string(),
        source_fingerprint,
        palette,
        min_x: bounds.min_x,
        min_z: bounds.min_z,
        tiles_x,
        tiles_z,
        indices,
        cell_count,
        fingerprint,
    })
}

/// Lower the authored pathing (overrides, tile rules, doodad footprints) into a
/// `NavBakeInput` for the deterministic nav bake + A* (#811).
pub fn lower_nav_bake_input(
    document: &EditorDocument,
    cliff_step: f32,
) -> Result<LoweredNavInput, EngineError> {
    document.validate()?;
    if document.terrain_cells.is_empty() {
        return invalid(
            "terrain_cells",
            "document has no terrain cells to lower for nav",
        );
    }
    let source_fingerprint = stable_fingerprint(document)?;

    let mut cells = Vec::with_capacity(document.terrain_cells.len());
    let mut walkable_count = 0usize;
    let mut blocked_count = 0usize;
    for cell in document.terrain_cells.values() {
        let flags = effective_pathing_for_cell(document, &cell.id)?;
        if flags.walkable {
            walkable_count += 1;
        }
        if flags.blocked {
            blocked_count += 1;
        }
        cells.push(NavTerrainCellInput {
            id: cell.id.clone(),
            x: cell.tile_x,
            z: cell.tile_z,
            height_fp: nav_height_fp(effective_height(cell, cliff_step))?,
            flags: nav_flags(flags),
        });
    }
    let static_blockers = doodad_blockers(document);
    let blocker_count = static_blockers.len();
    let input = NavBakeInput::new(Fixed::ONE.raw(), cells, static_blockers);
    let cell_count = input.terrain_cells.len();
    let fingerprint =
        stable_fingerprint(&(TERRAIN_NAV_LOWERED_SCHEMA, &input, &source_fingerprint))?;
    Ok(LoweredNavInput {
        schema: TERRAIN_NAV_LOWERED_SCHEMA.to_string(),
        source_fingerprint,
        input,
        cell_count,
        blocker_count,
        walkable_count,
        blocked_count,
        fingerprint,
    })
}

fn effective_height(cell: &EditorTerrainCell, cliff_step: f32) -> f32 {
    cell.height + cell.cliff_level as f32 * cliff_step
}

fn nav_height_fp(height: f32) -> Result<i64, EngineError> {
    let milli = (height as f64 * NAV_HEIGHT_MILLI as f64).round() as i64;
    Ok(Fixed::from_ratio(milli, NAV_HEIGHT_MILLI)?.raw())
}

fn nav_flags(flags: TerrainPathingFlags) -> NavPathingFlags {
    NavPathingFlags {
        walkable: flags.walkable,
        buildable: flags.buildable,
        flyable: flags.flyable,
        blocked: flags.blocked,
    }
}

/// A 1x1 static blocker for every Doodad placed on a terrain tile -- the authored
/// per-doodad pathing footprint (#811).
fn doodad_blockers(document: &EditorDocument) -> Vec<NavStaticBlockerInput> {
    let mut blockers = Vec::new();
    for object in document.objects.values() {
        if object.kind != "Doodad" {
            continue;
        }
        let (Some(x), Some(z)) = (
            object
                .properties
                .get(TERRAIN_TILE_X_PROPERTY)
                .and_then(|v| v.parse::<i32>().ok()),
            object
                .properties
                .get(TERRAIN_TILE_Z_PROPERTY)
                .and_then(|v| v.parse::<i32>().ok()),
        ) else {
            continue;
        };
        blockers.push(NavStaticBlockerInput {
            id: object.id.clone(),
            min_x: x,
            min_z: z,
            max_x: x,
            max_z: z,
        });
    }
    blockers.sort_by(|a, b| a.id.cmp(&b.id));
    blockers
}

fn tile_bounds(document: &EditorDocument) -> Result<TileBounds, EngineError> {
    if document.terrain_cells.is_empty() {
        return Err(invalid_err(
            "terrain_cells",
            "document has no terrain cells",
        ));
    }
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_z = i32::MAX;
    let mut max_z = i32::MIN;
    for cell in document.terrain_cells.values() {
        min_x = min_x.min(cell.tile_x);
        max_x = max_x.max(cell.tile_x);
        min_z = min_z.min(cell.tile_z);
        max_z = max_z.max(cell.tile_z);
    }
    Ok(TileBounds {
        min_x,
        max_x,
        min_z,
        max_z,
    })
}

/// Index the cells by coordinate and require a dense rectangle -- fail loud on any
/// hole so the lowered artifacts never silently guess a missing tile.
fn dense_cells_by_coord(
    document: &EditorDocument,
    bounds: TileBounds,
) -> Result<BTreeMap<(i32, i32), &EditorTerrainCell>, EngineError> {
    let mut by_coord = BTreeMap::new();
    for cell in document.terrain_cells.values() {
        by_coord.insert((cell.tile_x, cell.tile_z), cell);
    }
    let expected = (bounds.tiles_x() as usize) * (bounds.tiles_z() as usize);
    if by_coord.len() != expected {
        return Err(invalid_err(
            "terrain_cells.density",
            format!(
                "lowering needs a dense tile grid: expected {expected} cells over {}x{}, got {}",
                bounds.tiles_x(),
                bounds.tiles_z(),
                by_coord.len()
            ),
        ));
    }
    Ok(by_coord)
}

/// Map a sample index on the `size`-sample axis to the nearest authored tile.
fn nearest_tile(sample: u32, size: u32, tiles: u32, min_tile: i32) -> i32 {
    if size <= 1 || tiles <= 1 {
        return min_tile;
    }
    let frac = sample as f32 / (size - 1) as f32;
    let offset = (frac * (tiles - 1) as f32).round() as i32;
    min_tile + offset.clamp(0, (tiles - 1) as i32)
}

fn invalid<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(invalid_err(field, reason))
}

fn invalid_err(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.terrain_lowering.{field}"),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "terrain_lowering_tests.rs"]
mod tests;
