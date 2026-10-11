//! Deterministic doodad scatter brush (#809).
//!
//! WC3-style scenery brush: a footprint of tiles plus variation/scale/rotation/
//! density knobs. Given a fixed `seed`, [`DoodadScatterBrush::scatter`] is a pure,
//! integer-only function -- the same brush always yields the same placements, on
//! any platform, so the result lowers cleanly into a frozen sim package and can be
//! reproduced from the persisted Aster row.
//!
//! All authored quantities are integers on purpose: scale is expressed in
//! thousandths (`scale_milli`, 1000 == 1.0x) and rotation in whole degrees. The
//! Calyx lowering valve forbids floating point, so integer authoring is what makes
//! the persist->lower->sim path deterministic and total.

use litd_hash::{Fnv1a64, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::{
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
    world_authoring::{WorldAuthoringError, codec_failed, invalid},
};

pub const DOODAD_SCATTER_SCHEMA: &str = "litd.world.doodad_scatter.v1";
pub const DOODAD_SCATTER_SURFACE_KIND: &str = "doodad_scatter";

const MAX_VARIANTS: usize = 256;
const MAX_CELLS: usize = 4096;
const MAX_SCALE_MILLI: u32 = 100_000;

/// One weighted member of the scatter palette (e.g. `pine_a`, `pine_b`, `boulder`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoodadVariant {
    pub variant_id: String,
    pub weight: u32,
}

/// A single tile in the brush footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScatterCell {
    pub tile_x: i32,
    pub tile_z: i32,
}

/// The authored brush: palette + variation/scale/rotation/density knobs + footprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoodadScatterBrush {
    pub brush_id: String,
    pub display_name: String,
    pub variants: Vec<DoodadVariant>,
    /// Percent chance (1..=100) that each footprint tile receives a doodad.
    pub density_percent: u32,
    /// Uniform scale range in thousandths (min<=max, 1000 == 1.0x).
    pub min_scale_milli: u32,
    pub max_scale_milli: u32,
    /// Yaw range in whole degrees (min<=max, 0..=359).
    pub min_yaw_deg: u32,
    pub max_yaw_deg: u32,
    pub cells: Vec<ScatterCell>,
    pub seed: u64,
}

/// One resolved doodad. Everything is integer so it lowers 1:1 into sim data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoodadPlacement {
    pub placement_id: String,
    pub variant_id: String,
    pub variant_index: u32,
    pub tile_x: i32,
    pub tile_z: i32,
    pub scale_milli: u32,
    pub yaw_deg: u32,
}

/// The integer-only content payload persisted to Calyx and lowered for the sim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoodadScatterPayload {
    pub schema: String,
    pub surface_kind: String,
    pub brush_id: String,
    pub seed: u64,
    pub density_percent: u32,
    pub variant_count: u32,
    pub min_scale_milli: u32,
    pub max_scale_milli: u32,
    pub min_yaw_deg: u32,
    pub max_yaw_deg: u32,
    pub placement_count: u32,
    pub placements: Vec<DoodadPlacement>,
}

impl DoodadScatterBrush {
    pub fn validate(&self) -> Result<(), WorldAuthoringError> {
        if self.brush_id.trim().is_empty() || !is_id_safe(&self.brush_id) {
            return invalid(
                "WORLD_SCATTER_BRUSH_ID",
                "brush_id",
                "brush_id must be non-empty ASCII alnum, '_', '-', or '.'",
            );
        }
        if self.display_name.trim().is_empty() {
            return invalid(
                "WORLD_SCATTER_DISPLAY_NAME",
                "display_name",
                "display_name must be non-empty",
            );
        }
        if self.variants.is_empty() || self.variants.len() > MAX_VARIANTS {
            return invalid(
                "WORLD_SCATTER_VARIANTS",
                "variants",
                format!("variant count must be 1..={MAX_VARIANTS}"),
            );
        }
        let mut total_weight = 0_u64;
        for variant in &self.variants {
            if variant.variant_id.trim().is_empty() || !is_id_safe(&variant.variant_id) {
                return invalid(
                    "WORLD_SCATTER_VARIANT_ID",
                    "variants.variant_id",
                    "variant_id must be non-empty ASCII alnum, '_', '-', or '.'",
                );
            }
            if variant.weight == 0 {
                return invalid(
                    "WORLD_SCATTER_VARIANT_WEIGHT",
                    "variants.weight",
                    "variant weight must be positive",
                );
            }
            total_weight += u64::from(variant.weight);
        }
        if total_weight == 0 {
            return invalid(
                "WORLD_SCATTER_TOTAL_WEIGHT",
                "variants.weight",
                "total variant weight must be positive",
            );
        }
        if self.density_percent == 0 || self.density_percent > 100 {
            return invalid(
                "WORLD_SCATTER_DENSITY",
                "density_percent",
                "density_percent must be 1..=100",
            );
        }
        if self.min_scale_milli == 0
            || self.min_scale_milli > self.max_scale_milli
            || self.max_scale_milli > MAX_SCALE_MILLI
        {
            return invalid(
                "WORLD_SCATTER_SCALE",
                "scale_milli",
                format!("require 1 <= min_scale_milli <= max_scale_milli <= {MAX_SCALE_MILLI}"),
            );
        }
        if self.min_yaw_deg > self.max_yaw_deg || self.max_yaw_deg > 359 {
            return invalid(
                "WORLD_SCATTER_YAW",
                "yaw_deg",
                "require 0 <= min_yaw_deg <= max_yaw_deg <= 359",
            );
        }
        if self.cells.is_empty() || self.cells.len() > MAX_CELLS {
            return invalid(
                "WORLD_SCATTER_CELLS",
                "cells",
                format!("cell count must be 1..={MAX_CELLS}"),
            );
        }
        let mut seen = std::collections::BTreeSet::new();
        for cell in &self.cells {
            if !seen.insert((cell.tile_x, cell.tile_z)) {
                return invalid(
                    "WORLD_SCATTER_DUP_CELL",
                    "cells",
                    format!("duplicate footprint cell {},{}", cell.tile_x, cell.tile_z),
                );
            }
        }
        Ok(())
    }

    /// Deterministically resolve the brush footprint into placements. Cells are
    /// visited in sorted order; a per-cell density roll decides placement, then a
    /// weighted variant, a scale, and a yaw are drawn from independent stable hash
    /// streams. Empty results are rejected: a brush that places nothing is a
    /// misconfiguration, not a valid authored artifact.
    pub fn scatter(&self) -> Result<Vec<DoodadPlacement>, WorldAuthoringError> {
        self.validate()?;
        let mut cells = self.cells.clone();
        cells.sort_unstable();
        let total_weight = self
            .variants
            .iter()
            .map(|variant| u64::from(variant.weight))
            .sum::<u64>();
        let mut placements = Vec::new();
        for cell in &cells {
            let roll = hash_stream(self.seed, &self.brush_id, "density", cell) % 100;
            if roll >= u64::from(self.density_percent) {
                continue;
            }
            let variant_index = select_variant(
                hash_stream(self.seed, &self.brush_id, "variant", cell),
                total_weight,
                &self.variants,
            )?;
            let scale_milli = pick_range(
                hash_stream(self.seed, &self.brush_id, "scale", cell),
                self.min_scale_milli,
                self.max_scale_milli,
            );
            let yaw_deg = pick_range(
                hash_stream(self.seed, &self.brush_id, "yaw", cell),
                self.min_yaw_deg,
                self.max_yaw_deg,
            );
            placements.push(DoodadPlacement {
                placement_id: placement_id(&self.brush_id, cell),
                variant_id: self.variants[variant_index as usize].variant_id.clone(),
                variant_index,
                tile_x: cell.tile_x,
                tile_z: cell.tile_z,
                scale_milli,
                yaw_deg,
            });
        }
        if placements.is_empty() {
            return invalid(
                "WORLD_SCATTER_EMPTY",
                "placements",
                "brush resolved to zero placements; raise density_percent or footprint",
            );
        }
        Ok(placements)
    }

    /// Build the integer-only Calyx content record for this brush.
    pub fn to_content_record(
        &self,
        author: &str,
    ) -> Result<EngineContentRecord, WorldAuthoringError> {
        if author.trim().is_empty() {
            return invalid("WORLD_SCATTER_AUTHOR", "author", "author must be non-empty");
        }
        let payload = self.to_payload()?;
        let payload_value = serde_json::to_value(&payload).map_err(codec_failed)?;
        let canonical = serde_json::to_vec(&payload).map_err(codec_failed)?;
        Ok(EngineContentRecord {
            kind: EngineContentKind::Terrain,
            id: self.brush_id.clone(),
            display_name: self.display_name.clone(),
            payload: payload_value,
            source: EngineContentSource {
                path: format!("editor://terrain/doodad-scatter/{}", self.brush_id),
                author: author.to_string(),
                source_hash: sha256_hex(&canonical),
            },
            anchors: Vec::new(),
        })
    }

    pub fn to_payload(&self) -> Result<DoodadScatterPayload, WorldAuthoringError> {
        let placements = self.scatter()?;
        Ok(DoodadScatterPayload {
            schema: DOODAD_SCATTER_SCHEMA.to_string(),
            surface_kind: DOODAD_SCATTER_SURFACE_KIND.to_string(),
            brush_id: self.brush_id.clone(),
            seed: self.seed,
            density_percent: self.density_percent,
            variant_count: self.variants.len() as u32,
            min_scale_milli: self.min_scale_milli,
            max_scale_milli: self.max_scale_milli,
            min_yaw_deg: self.min_yaw_deg,
            max_yaw_deg: self.max_yaw_deg,
            placement_count: placements.len() as u32,
            placements,
        })
    }
}

fn select_variant(
    ticket_source: u64,
    total_weight: u64,
    variants: &[DoodadVariant],
) -> Result<u32, WorldAuthoringError> {
    let mut ticket = ticket_source % total_weight;
    for (index, variant) in variants.iter().enumerate() {
        let weight = u64::from(variant.weight);
        if ticket < weight {
            return Ok(index as u32);
        }
        ticket -= weight;
    }
    invalid(
        "WORLD_SCATTER_SELECT",
        "variants",
        "weighted variant selection fell out of range",
    )
}

fn pick_range(source: u64, min: u32, max: u32) -> u32 {
    if min == max {
        return min;
    }
    let span = u64::from(max - min) + 1;
    min + (source % span) as u32
}

fn placement_id(brush_id: &str, cell: &ScatterCell) -> String {
    format!(
        "{brush_id}__{}_{}",
        fmt_coord(cell.tile_x),
        fmt_coord(cell.tile_z)
    )
}

fn fmt_coord(value: i32) -> String {
    if value < 0 {
        format!("n{}", value.unsigned_abs())
    } else {
        value.to_string()
    }
}

fn hash_stream(seed: u64, brush_id: &str, stream: &str, cell: &ScatterCell) -> u64 {
    let mut hasher = Fnv1a64::new();
    hasher
        .update(&seed.to_le_bytes())
        .update_u8(0xff)
        .update(brush_id.as_bytes())
        .update_u8(0xff)
        .update(stream.as_bytes())
        .update_u8(0xff)
        .update(&cell.tile_x.to_le_bytes())
        .update_u8(0xff)
        .update(&cell.tile_z.to_le_bytes());
    let mut value = hasher.finish();
    // One SplitMix64 diffusion round so adjacent tiles/streams decorrelate.
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn is_id_safe(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[cfg(test)]
#[path = "world_scatter_tests.rs"]
mod tests;
