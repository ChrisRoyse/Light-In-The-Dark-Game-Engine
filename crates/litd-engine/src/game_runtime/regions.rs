//! #1856: spatial regions — integer cell rects on the fog grid + enter/leave evaluation.
//!
//! Authored regions lower into a fingerprinted [`LoweredRegionSet`] carried in the
//! runtime datasets. Membership is pure integer AABB (inclusive min/max cell
//! coordinates), matching fog grid geometry. The session evaluator latches each
//! (unit, region) pair to fire `UnitEntersRegion` / `UnitLeavesRegion` transitions.

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::EngineError;

pub const LITD_REGION_SET_SCHEMA: &str = "litd.game.region_set.v1";

fn region_error<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("litd_game.package.region_set.{field}"),
        reason: reason.into(),
    })
}

/// One lowered spatial region: inclusive cell rect on the fog/grid axes (x,z).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredRegion {
    pub region_id: u32,
    pub name: String,
    pub min_x: i32,
    pub min_z: i32,
    pub max_x: i32,
    pub max_z: i32,
}

impl LoweredRegion {
    /// Inclusive cell membership (WC3 rect semantics on integer cells).
    pub fn contains_cell(&self, cell_x: i32, cell_z: i32) -> bool {
        cell_x >= self.min_x && cell_x <= self.max_x && cell_z >= self.min_z && cell_z <= self.max_z
    }
}

/// Fingerprinted set of regions embedded in runtime datasets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredRegionSet {
    pub schema: String,
    /// Strictly sorted by `region_id`, unique ids.
    pub regions: Vec<LoweredRegion>,
    pub region_set_fingerprint: String,
}

impl LoweredRegionSet {
    pub fn compute_fingerprint(&self) -> Result<String, EngineError> {
        let mut preimage = self.clone();
        preimage.region_set_fingerprint.clear();
        let ron = litd_hash::to_stable_ron(&preimage).map_err(|error| {
            EngineError::SerializationFailed {
                format: "ron",
                reason: format!("encode region set RON failed: {error}"),
            }
        })?;
        Ok(litd_hash::fnv1a64_hex(ron.as_bytes()))
    }

    pub fn region(&self, region_id: u32) -> Option<&LoweredRegion> {
        self.regions
            .binary_search_by_key(&region_id, |r| r.region_id)
            .ok()
            .map(|index| &self.regions[index])
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema != LITD_REGION_SET_SCHEMA {
            return region_error(
                "schema",
                format!("expected {LITD_REGION_SET_SCHEMA}, got {}", self.schema),
            );
        }
        for pair in self.regions.windows(2) {
            if pair[0].region_id >= pair[1].region_id {
                return region_error(
                    "regions",
                    format!(
                        "regions must be strictly sorted by region_id; {} >= {}",
                        pair[0].region_id, pair[1].region_id
                    ),
                );
            }
        }
        for region in &self.regions {
            if region.name.trim().is_empty() {
                return region_error("name", "region name must be non-empty");
            }
            if region.min_x > region.max_x || region.min_z > region.max_z {
                return region_error(
                    &region.region_id.to_string(),
                    format!(
                        "region {} inverted bounds min=({},{}) max=({},{})",
                        region.name, region.min_x, region.min_z, region.max_x, region.max_z
                    ),
                );
            }
        }
        let expected = self.compute_fingerprint()?;
        if self.region_set_fingerprint != expected {
            return Err(EngineError::SerializedFingerprintMismatch {
                expected,
                actual: self.region_set_fingerprint.clone(),
            });
        }
        Ok(())
    }

    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update(self.region_set_fingerprint.as_bytes());
        hasher.update_u64_le(self.regions.len() as u64);
        for region in &self.regions {
            hasher.update_u64_le(u64::from(region.region_id));
            hasher.update(region.name.as_bytes());
            hasher.update_u64_le(region.min_x as u64);
            hasher.update_u64_le(region.min_z as u64);
            hasher.update_u64_le(region.max_x as u64);
            hasher.update_u64_le(region.max_z as u64);
        }
    }
}

/// Author + fingerprint a region set. Regions must already carry unique ids.
pub fn lower_region_set(mut regions: Vec<LoweredRegion>) -> Result<LoweredRegionSet, EngineError> {
    regions.sort_by_key(|r| r.region_id);
    for pair in regions.windows(2) {
        if pair[0].region_id == pair[1].region_id {
            return region_error(
                "regions",
                format!("duplicate region_id {}", pair[0].region_id),
            );
        }
    }
    let mut set = LoweredRegionSet {
        schema: LITD_REGION_SET_SCHEMA.to_string(),
        regions,
        region_set_fingerprint: String::new(),
    };
    set.region_set_fingerprint = set.compute_fingerprint()?;
    set.validate()?;
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_cell_inclusive_rect() {
        let region = LoweredRegion {
            region_id: 1,
            name: "gate".into(),
            min_x: 2,
            min_z: 2,
            max_x: 4,
            max_z: 5,
        };
        assert!(region.contains_cell(2, 2));
        assert!(region.contains_cell(4, 5));
        assert!(region.contains_cell(3, 3));
        assert!(!region.contains_cell(1, 3));
        assert!(!region.contains_cell(3, 6));
    }

    #[test]
    fn inverted_bounds_fail_loud() {
        let err = lower_region_set(vec![LoweredRegion {
            region_id: 1,
            name: "bad".into(),
            min_x: 5,
            min_z: 0,
            max_x: 1,
            max_z: 2,
        }])
        .unwrap_err()
        .to_string();
        assert!(err.contains("inverted"), "error was {err}");
    }

    #[test]
    fn fingerprint_detects_mutation() {
        let set = lower_region_set(vec![LoweredRegion {
            region_id: 1,
            name: "a".into(),
            min_x: 0,
            min_z: 0,
            max_x: 1,
            max_z: 1,
        }])
        .unwrap();
        let mut mutated = set.clone();
        mutated.regions[0].max_x = 9;
        assert_ne!(
            set.region_set_fingerprint,
            mutated.compute_fingerprint().unwrap()
        );
    }
}
