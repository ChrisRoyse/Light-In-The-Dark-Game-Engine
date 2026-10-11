//! #1860 engagement model: HP defaults, damage results, and package-level strike tuning.
//!
//! Hold-the-line missions need multi-hit HP reduction, not one-shot `DestroyUnit`.
//! Defaults live here so `match_state` / `datasets` stay under the <500-line ratchet.

use crate::EngineError;

use super::groups::UnitId;
use super::victory::GameOverEvent;

/// Default max HP for structures (milli). Three default strikes (1000 each) kill a structure.
pub const DEFAULT_STRUCTURE_MAX_HP_MILLI: u32 = 3_000;
/// Default max HP for field units (milli).
pub const DEFAULT_FIELD_UNIT_MAX_HP_MILLI: u32 = 1_000;
/// Default damage per engagement strike (milli).
pub const DEFAULT_STRIKE_DAMAGE_MILLI: u32 = 1_000;

/// Engagement HP defaults applied when materializing a match from a lowered blueprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngagementDefaults {
    pub structure_max_hp_milli: u32,
    pub field_unit_max_hp_milli: u32,
}

impl Default for EngagementDefaults {
    fn default() -> Self {
        Self {
            structure_max_hp_milli: DEFAULT_STRUCTURE_MAX_HP_MILLI,
            field_unit_max_hp_milli: DEFAULT_FIELD_UNIT_MAX_HP_MILLI,
        }
    }
}

impl EngagementDefaults {
    pub fn validate(self) -> Result<(), EngineError> {
        if self.structure_max_hp_milli == 0 {
            return Err(EngineError::SimInvalidConfig {
                field: "engagement.structure_max_hp_milli".to_string(),
                reason:
                    "must be > 0 (zero-HP structures cannot participate in hold-the-line combat)"
                        .to_string(),
            });
        }
        if self.field_unit_max_hp_milli == 0 {
            return Err(EngineError::SimInvalidConfig {
                field: "engagement.field_unit_max_hp_milli".to_string(),
                reason:
                    "must be > 0 (zero-HP field units cannot participate in hold-the-line combat)"
                        .to_string(),
            });
        }
        Ok(())
    }

    pub fn max_hp_for(self, is_structure: bool) -> u32 {
        if is_structure {
            self.structure_max_hp_milli
        } else {
            self.field_unit_max_hp_milli
        }
    }
}

/// Result of applying damage (#1860). Readback surface for FSV / AI timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageResult {
    pub target: UnitId,
    pub amount_applied: u32,
    pub hp_before: u32,
    pub hp_after: u32,
    pub destroyed: bool,
    pub game_over: Vec<GameOverEvent>,
}
