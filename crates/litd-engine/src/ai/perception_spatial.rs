use std::collections::HashSet;

use crate::{EngineError, math::FixedVec3, nav::NavCellCoord};

use super::{PerceptionBlocker, blackboard::invalid_config};

const MAX_LINE_OF_SIGHT_CELLS: i64 = 4096;
/// A single perception blocker rasterises to at most this many occupancy cells. Blockers
/// are static obstacles (a pillar is one cell); anything spanning more than a 64x64 area
/// is almost certainly malformed input, so we fail loud rather than rasterise an
/// unbounded region into the occupancy grid (#1486 #3).
const MAX_BLOCKER_CELLS: i64 = MAX_LINE_OF_SIGHT_CELLS;

/// Occupancy grid over the perception blockers, built ONCE per perception run. Replaces
/// the per-line, per-cell linear scan of every blocker AABB (`O(cells x blockers)` per
/// line-of-sight check, run for every agent x entity pair) with an `O(1)` set lookup
/// (#1486 #3). Cell membership is identical to the old `blockers.iter().any(contains)`, so
/// line-of-sight results — and therefore the whole perception run — stay byte-identical.
pub(super) struct PerceptionBlockerGrid {
    blocked: HashSet<NavCellCoord>,
}

impl PerceptionBlockerGrid {
    /// Rasterise every blocker AABB into the occupancy set. Assumes `min <= max` (checked
    /// by validate_inputs); fails loud if a single blocker spans an implausible area.
    pub(super) fn build(blockers: &[PerceptionBlocker]) -> Result<Self, EngineError> {
        let mut blocked = HashSet::new();
        for blocker in blockers {
            let width = i64::from(blocker.max.x) - i64::from(blocker.min.x) + 1;
            let depth = i64::from(blocker.max.z) - i64::from(blocker.min.z) + 1;
            if width <= 0 || depth <= 0 {
                return invalid_config(
                    "perception.blocker.bounds",
                    format!("blocker {} has non-positive extent", blocker.id),
                );
            }
            let area = width.saturating_mul(depth);
            if area > MAX_BLOCKER_CELLS {
                return invalid_config(
                    "perception.blocker.area",
                    format!(
                        "blocker {} spans {area} cells, max {MAX_BLOCKER_CELLS}",
                        blocker.id
                    ),
                );
            }
            for x in blocker.min.x..=blocker.max.x {
                for z in blocker.min.z..=blocker.max.z {
                    blocked.insert(NavCellCoord::new(x, z));
                }
            }
        }
        Ok(Self { blocked })
    }

    fn contains(&self, cell: NavCellCoord) -> bool {
        self.blocked.contains(&cell)
    }
}

pub(super) fn manhattan_xz_fp(left: FixedVec3, right: FixedVec3) -> Result<i64, EngineError> {
    let dx = (left.x.raw() as i128 - right.x.raw() as i128).abs();
    let dz = (left.z.raw() as i128 - right.z.raw() as i128).abs();
    fit_i64(dx + dz, "perception.distance_fp")
}

pub(super) fn line_of_sight(
    from: NavCellCoord,
    to: NavCellCoord,
    blockers: &PerceptionBlockerGrid,
) -> Result<bool, EngineError> {
    let target_x = i64::from(to.x);
    let target_z = i64::from(to.z);
    let mut x = i64::from(from.x);
    let mut z = i64::from(from.z);
    let dx = (target_x - x).abs();
    let dz_abs = (target_z - z).abs();
    let cell_count = dx.max(dz_abs) + 1;
    if cell_count > MAX_LINE_OF_SIGHT_CELLS {
        return invalid_config(
            "perception.line_of_sight",
            format!("line spans {cell_count} cells, max {MAX_LINE_OF_SIGHT_CELLS}"),
        );
    }
    let dz = -dz_abs;
    let sx = if x < target_x { 1 } else { -1 };
    let sz = if z < target_z { 1 } else { -1 };
    let mut error = dx + dz;
    loop {
        let cell = NavCellCoord::new(x as i32, z as i32);
        if cell == from || cell == to {
            if x == target_x && z == target_z {
                break;
            }
            step_line(&mut x, &mut z, sx, sz, dx, dz, &mut error);
            continue;
        }
        if blockers.contains(cell) {
            return Ok(false);
        }
        if x == target_x && z == target_z {
            break;
        }
        step_line(&mut x, &mut z, sx, sz, dx, dz, &mut error);
    }
    Ok(true)
}

fn fit_i64(value: i128, field: &'static str) -> Result<i64, EngineError> {
    if value > i64::MAX as i128 {
        return invalid_config(field, "fixed-point distance exceeded i64 range");
    }
    Ok(value as i64)
}

fn step_line(x: &mut i64, z: &mut i64, sx: i64, sz: i64, dx: i64, dz: i64, error: &mut i64) {
    let doubled = *error * 2;
    if doubled >= dz {
        *error += dz;
        *x += sx;
    }
    if doubled <= dx {
        *error += dx;
        *z += sz;
    }
}
