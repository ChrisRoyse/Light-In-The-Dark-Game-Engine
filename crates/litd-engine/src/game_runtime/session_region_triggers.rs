//! #1856: region enter/leave transition collection for the trigger evaluator.

use super::super::GameRuntimeError;
use super::super::MatchRuntimeSession;
use super::TriggerRuntime;

impl MatchRuntimeSession {
    /// Ensure a latch exists for every (field_unit, watched_region), then record
    /// enter/leave transitions. Returns sorted unique region ids that fired.
    pub(super) fn collect_region_transitions(
        &self,
        state: &mut TriggerRuntime,
    ) -> Result<(Vec<u32>, Vec<u32>), GameRuntimeError> {
        let watched_regions: Vec<u32> = {
            let mut ids: Vec<u32> = state
                .region_latches
                .iter()
                .map(|(_, region, _)| *region)
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        if watched_regions.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let Some(set) = self.regions.as_ref() else {
            return Err(GameRuntimeError::Invalid {
                field: "session.trigger.regions".to_string(),
                reason: "triggers watch regions but the session has no region set".to_string(),
            });
        };
        for unit in &self.field_units {
            for &region_id in &watched_regions {
                let exists = state
                    .region_latches
                    .iter()
                    .any(|(u, r, _)| *u == unit.id && *r == region_id);
                if !exists {
                    // Seed as outside so the first evaluation that finds the unit
                    // inside produces a real outside->inside enter transition
                    // (even if the move happened before the first region tick).
                    state.region_latches.push((unit.id, region_id, false));
                }
            }
        }
        state.region_latches.retain(|(u, _, _)| *u != 0);
        state.region_latches.sort_by_key(|(u, r, _)| (*u, *r));
        let mut enters = Vec::new();
        let mut leaves = Vec::new();
        for (unit_id, region_id, was_inside) in &mut state.region_latches {
            let Some(region) = set.region(*region_id) else {
                continue;
            };
            let Some(unit) = self.field_units.iter().find(|u| u.id == *unit_id) else {
                continue;
            };
            let now_inside = region.contains_cell(unit.cell_x, unit.cell_z);
            if !*was_inside && now_inside {
                enters.push(*region_id);
            } else if *was_inside && !now_inside {
                leaves.push(*region_id);
            }
            *was_inside = now_inside;
        }
        enters.sort_unstable();
        enters.dedup();
        leaves.sort_unstable();
        leaves.dedup();
        Ok((enters, leaves))
    }
}
