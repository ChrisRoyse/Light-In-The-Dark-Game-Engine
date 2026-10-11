use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

const MAX_VARIANTS: u16 = 256;
const MAX_GROUP_MEMBERS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainTileVariation {
    pub id: String,
    pub tile_x: i32,
    pub tile_z: i32,
    pub tileset_id: String,
    pub variation_count: u16,
    pub variation_index: u16,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainVariationResetReport {
    pub seed: u64,
    pub tile_count: usize,
    pub changed_tile_count: usize,
    pub unchanged_tile_count: usize,
    pub before_summary: String,
    pub after_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainRandomGroupMemberKind {
    Tile,
    Doodad,
    Destructible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainRandomGroupMember {
    pub id: String,
    pub kind: TerrainRandomGroupMemberKind,
    pub asset_ref: String,
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainRandomGroup {
    pub id: String,
    pub name: String,
    pub seed: u64,
    pub members: Vec<TerrainRandomGroupMember>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainRandomGroupCycleDirection {
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainRandomGroupPreview {
    pub group_id: String,
    pub current_index: usize,
    pub auto_cycle: bool,
    pub tick: u64,
}

impl TerrainTileVariation {
    pub fn new(
        id: impl Into<String>,
        tile_x: i32,
        tile_z: i32,
        tileset_id: impl Into<String>,
        variation_count: u16,
        variation_index: u16,
        seed: u64,
    ) -> Self {
        Self {
            id: id.into(),
            tile_x,
            tile_z,
            tileset_id: tileset_id.into(),
            variation_count,
            variation_index,
            seed,
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.tileset_id.trim().is_empty() {
            return invalid_terrain_variation("tile", "id and tileset_id must be non-empty");
        }
        if self.variation_count == 0 || self.variation_count > MAX_VARIANTS {
            return invalid_terrain_variation(
                "tile.variation_count",
                format!("variation_count must be 1..={MAX_VARIANTS}"),
            );
        }
        if self.variation_index >= self.variation_count {
            return invalid_terrain_variation(
                "tile.variation_index",
                "variation_index must be less than variation_count",
            );
        }
        Ok(())
    }

    pub fn deterministic_index_for_seed(&self, seed: u64) -> u16 {
        if self.variation_count == 1 {
            return 0;
        }
        let hash = stable_hash(&[
            seed.to_string(),
            self.id.clone(),
            self.tile_x.to_string(),
            self.tile_z.to_string(),
            self.tileset_id.clone(),
            self.variation_count.to_string(),
        ]);
        (hash % u64::from(self.variation_count)) as u16
    }
}

impl TerrainRandomGroupMember {
    pub fn new(
        id: impl Into<String>,
        kind: TerrainRandomGroupMemberKind,
        asset_ref: impl Into<String>,
        weight: u32,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            asset_ref: asset_ref.into(),
            weight,
        }
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.asset_ref.trim().is_empty() {
            return invalid_terrain_variation(
                "random_group.member",
                "id and asset_ref must be non-empty",
            );
        }
        if self.weight == 0 {
            return invalid_terrain_variation(
                "random_group.member.weight",
                "weight must be positive",
            );
        }
        Ok(())
    }
}

impl TerrainRandomGroup {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        seed: u64,
        members: Vec<TerrainRandomGroupMember>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            seed,
            members,
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return invalid_terrain_variation("random_group", "id and name must be non-empty");
        }
        if self.members.is_empty() || self.members.len() > MAX_GROUP_MEMBERS {
            return invalid_terrain_variation(
                "random_group.members",
                format!("member count must be 1..={MAX_GROUP_MEMBERS}"),
            );
        }
        let mut ids = BTreeSet::new();
        let mut total_weight: u64 = 0;
        for member in &self.members {
            member.validate()?;
            if !ids.insert(member.id.clone()) {
                return invalid_terrain_variation(
                    "random_group.member.id",
                    format!("duplicate member id {}", member.id),
                );
            }
            total_weight = total_weight
                .checked_add(u64::from(member.weight))
                .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                    field: "editor.terrain_variation.random_group.weight".to_string(),
                    reason: "total weight overflowed".to_string(),
                })?;
        }
        if total_weight == 0 {
            return invalid_terrain_variation(
                "random_group.weight",
                "total weight must be positive",
            );
        }
        Ok(())
    }

    pub fn select_member_index(
        &self,
        placement_key: &str,
        placement_ordinal: u64,
    ) -> Result<usize, EngineError> {
        self.validate()?;
        if self.members.len() == 1 {
            return Ok(0);
        }
        let total_weight = total_group_weight(self)?;
        let mut target = stable_hash(&[
            self.seed.to_string(),
            self.id.clone(),
            placement_key.to_string(),
            placement_ordinal.to_string(),
        ]) % total_weight;
        for (index, member) in self.members.iter().enumerate() {
            let weight = u64::from(member.weight);
            if target < weight {
                return Ok(index);
            }
            target -= weight;
        }
        invalid_terrain_variation(
            "random_group.selection",
            "weighted selection fell out of range",
        )
    }

    pub fn select_member(
        &self,
        placement_key: &str,
        placement_ordinal: u64,
    ) -> Result<&TerrainRandomGroupMember, EngineError> {
        let index = self.select_member_index(placement_key, placement_ordinal)?;
        Ok(&self.members[index])
    }
}

impl TerrainRandomGroupPreview {
    pub fn new(group: &TerrainRandomGroup) -> Result<Self, EngineError> {
        group.validate()?;
        Ok(Self {
            group_id: group.id.clone(),
            current_index: 0,
            auto_cycle: false,
            tick: 0,
        })
    }

    pub fn current_member<'a>(
        &self,
        group: &'a TerrainRandomGroup,
    ) -> Result<&'a TerrainRandomGroupMember, EngineError> {
        validate_preview_matches_group(self, group)?;
        Ok(&group.members[self.current_index % group.members.len()])
    }
}

pub fn reset_texture_variations(
    tiles: &mut BTreeMap<String, TerrainTileVariation>,
    seed: u64,
) -> Result<TerrainVariationResetReport, EngineError> {
    validate_tile_variations(tiles)?;
    let before_summary = tile_variation_summary(tiles);
    let mut changed_tile_count = 0;
    let mut unchanged_tile_count = 0;
    for tile in tiles.values_mut() {
        let before = tile.variation_index;
        let after = tile.deterministic_index_for_seed(seed);
        tile.seed = seed;
        tile.variation_index = after;
        if before == after {
            unchanged_tile_count += 1;
        } else {
            changed_tile_count += 1;
        }
    }
    validate_tile_variations(tiles)?;
    Ok(TerrainVariationResetReport {
        seed,
        tile_count: tiles.len(),
        changed_tile_count,
        unchanged_tile_count,
        before_summary,
        after_summary: tile_variation_summary(tiles),
    })
}

pub fn cycle_random_group_preview(
    group: &TerrainRandomGroup,
    preview: &TerrainRandomGroupPreview,
    direction: TerrainRandomGroupCycleDirection,
) -> Result<TerrainRandomGroupPreview, EngineError> {
    validate_preview_matches_group(preview, group)?;
    let len = group.members.len();
    let current_index = match direction {
        TerrainRandomGroupCycleDirection::Next => (preview.current_index + 1) % len,
        TerrainRandomGroupCycleDirection::Previous => (preview.current_index + len - 1) % len,
    };
    let tick = preview
        .tick
        .checked_add(1)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.terrain_variation.random_group.preview.tick".to_string(),
            reason: "preview tick overflowed".to_string(),
        })?;
    Ok(TerrainRandomGroupPreview {
        group_id: preview.group_id.clone(),
        current_index,
        auto_cycle: preview.auto_cycle,
        tick,
    })
}

pub fn auto_cycle_random_group_preview(
    group: &TerrainRandomGroup,
    preview: &TerrainRandomGroupPreview,
) -> Result<TerrainRandomGroupPreview, EngineError> {
    let mut next =
        cycle_random_group_preview(group, preview, TerrainRandomGroupCycleDirection::Next)?;
    next.auto_cycle = true;
    Ok(next)
}

pub fn validate_tile_variations(
    tiles: &BTreeMap<String, TerrainTileVariation>,
) -> Result<(), EngineError> {
    let mut coords = BTreeSet::new();
    for (id, tile) in tiles {
        if id != &tile.id {
            return invalid_terrain_variation(
                "tile.id",
                format!("map key {id} does not match tile id {}", tile.id),
            );
        }
        tile.validate()?;
        let coord_key = (tile.tile_x, tile.tile_z);
        if !coords.insert(coord_key) {
            return invalid_terrain_variation(
                "tile.coord",
                format!(
                    "duplicate terrain tile coordinate {},{}",
                    tile.tile_x, tile.tile_z
                ),
            );
        }
    }
    Ok(())
}

pub fn validate_random_groups(
    groups: &BTreeMap<String, TerrainRandomGroup>,
) -> Result<(), EngineError> {
    for (id, group) in groups {
        if id != &group.id {
            return invalid_terrain_variation(
                "random_group.id",
                format!("map key {id} does not match group id {}", group.id),
            );
        }
        group.validate()?;
    }
    Ok(())
}

pub fn tile_variation_summary(tiles: &BTreeMap<String, TerrainTileVariation>) -> String {
    tiles
        .values()
        .map(|tile| {
            format!(
                "{}@{},{}:{}/{}#{}",
                tile.id,
                tile.tile_x,
                tile.tile_z,
                tile.variation_index,
                tile.variation_count,
                tile.seed
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn validate_preview_matches_group(
    preview: &TerrainRandomGroupPreview,
    group: &TerrainRandomGroup,
) -> Result<(), EngineError> {
    group.validate()?;
    if preview.group_id != group.id {
        return invalid_terrain_variation(
            "random_group.preview.group_id",
            format!(
                "preview group {} does not match {}",
                preview.group_id, group.id
            ),
        );
    }
    if preview.current_index >= group.members.len() {
        return invalid_terrain_variation(
            "random_group.preview.current_index",
            format!(
                "preview index {} is outside group member count {}",
                preview.current_index,
                group.members.len()
            ),
        );
    }
    Ok(())
}

fn total_group_weight(group: &TerrainRandomGroup) -> Result<u64, EngineError> {
    let mut total_weight = 0_u64;
    for member in &group.members {
        total_weight = total_weight
            .checked_add(u64::from(member.weight))
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.terrain_variation.random_group.weight".to_string(),
                reason: "total weight overflowed".to_string(),
            })?;
    }
    if total_weight == 0 {
        return invalid_terrain_variation("random_group.weight", "total weight must be positive");
    }
    Ok(total_weight)
}

fn stable_hash(parts: &[String]) -> u64 {
    let mut hasher = Fnv1a64::new();
    for part in parts {
        hasher.update(part.as_bytes()).update_u8(0xff);
    }
    hasher.finish()
}

fn invalid_terrain_variation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.terrain_variation.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "terrain_variation_tests.rs"]
mod tests;
