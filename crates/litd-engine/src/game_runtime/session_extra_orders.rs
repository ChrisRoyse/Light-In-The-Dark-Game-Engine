//! #1854/#1859/#1861: session dispatch for spawn-unit, cinematic sim-gating, and
//! camera-follow presentation intent.
//!
//! Kept out of `session_orders.rs` so the main match stays under the 500-line
//! ratchet. Every order fails loud with a typed error (empty ids, unknown unit
//! keys, out-of-bounds cells, unknown owners) -- never a silent no-op.

use super::super::groups::{OrderTargetKind, UnitCapability};
use super::GameRuntimeError;
use super::{FieldUnit, MatchRuntimeSession, SessionOrder};

/// Deterministic cinematic gate recorded by `PlayCinematic` / cleared by
/// `SkipCinematic`. Presentation host drives the float `CinematicPlayer`; this
/// gate is the only sim-affecting output (`should_advance_sim`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CinematicGate {
    pub timeline_id: String,
    pub pauses_sim: bool,
    pub skipped: bool,
}

impl MatchRuntimeSession {
    /// #1859: spawn a field unit at a cell after validating the economy unit key,
    /// owner, and grid bounds. Registers the unit in match state (point-move)
    /// and the session field list, then recomputes fog so vision updates now.
    pub(super) fn spawn_field_unit(
        &mut self,
        order: &SessionOrder,
    ) -> Result<(), GameRuntimeError> {
        let SessionOrder::SpawnFieldUnit {
            unit_key,
            owner,
            cell_x,
            cell_z,
        } = order
        else {
            return Err(GameRuntimeError::Invalid {
                field: "session.spawn_field_unit".to_string(),
                reason: "dispatched a non-SpawnFieldUnit order".to_string(),
            });
        };
        if unit_key.is_empty() {
            return Err(GameRuntimeError::Invalid {
                field: "session.spawn_field_unit.unit_key".to_string(),
                reason: "unit_key must be non-empty".to_string(),
            });
        }
        if !self.economy.has_unit_type(unit_key) {
            return Err(GameRuntimeError::Invalid {
                field: "session.spawn_field_unit.unit_key".to_string(),
                reason: format!("unit_key {unit_key:?} is absent from the lowered economy"),
            });
        }
        let player_count = self.match_state.roster().player_count();
        if usize::from(*owner) >= player_count {
            return Err(GameRuntimeError::Invalid {
                field: "session.spawn_field_unit.owner".to_string(),
                reason: format!("owner {owner} is out of range for {player_count} players"),
            });
        }
        let w = self.constants.fog_grid_width as i32;
        let h = self.constants.fog_grid_height as i32;
        if *cell_x < 0 || *cell_z < 0 || *cell_x >= w || *cell_z >= h {
            return Err(GameRuntimeError::Invalid {
                field: "session.spawn_field_unit.cell".to_string(),
                reason: format!("cell ({cell_x},{cell_z}) is outside the {w}x{h} fog grid"),
            });
        }
        let unit_id = self.next_spawned_unit_id;
        self.next_spawned_unit_id =
            self.next_spawned_unit_id
                .checked_add(1)
                .ok_or_else(|| GameRuntimeError::Invalid {
                    field: "session.spawn_field_unit.id".to_string(),
                    reason: "spawned unit id space exhausted".to_string(),
                })?;
        // Point-move ability so scripted staging units accept IssueMove/order_unit.
        // #1860: seed max HP from package engagement defaults (field units).
        self.match_state.spawn_unit(
            unit_id,
            *owner,
            vec![UnitCapability {
                ability: 1,
                target_kind: OrderTargetKind::Point,
            }],
            self.constants.field_unit_max_hp_milli,
        )?;
        self.field_units.push(FieldUnit {
            id: unit_id,
            owner: u32::from(*owner),
            cell_x: *cell_x,
            cell_z: *cell_z,
        });
        self.field_units.sort_by_key(|u| u.id);
        self.recompute_fog()?;
        Ok(())
    }

    /// #1854: apply PlayCinematic / SkipCinematic to the session cinematic gate.
    pub(super) fn apply_cinematic_order(
        &mut self,
        order: &SessionOrder,
    ) -> Result<(), GameRuntimeError> {
        match order {
            SessionOrder::PlayCinematic {
                timeline_id,
                pauses_sim,
            } => {
                if timeline_id.is_empty() {
                    return Err(GameRuntimeError::Invalid {
                        field: "session.play_cinematic.timeline_id".to_string(),
                        reason: "timeline_id must be non-empty".to_string(),
                    });
                }
                self.cinematic_gate = Some(CinematicGate {
                    timeline_id: timeline_id.clone(),
                    pauses_sim: *pauses_sim,
                    skipped: false,
                });
                Ok(())
            }
            SessionOrder::SkipCinematic => {
                if let Some(gate) = &mut self.cinematic_gate {
                    gate.skipped = true;
                }
                // Idempotent: no active gate is a deterministic no-op (host skip).
                Ok(())
            }
            _ => Err(GameRuntimeError::Invalid {
                field: "session.cinematic".to_string(),
                reason: "dispatched a non-cinematic order".to_string(),
            }),
        }
    }

    /// #1861: set/clear the presentation camera-follow binding. Fails loud when
    /// binding to a unit that is not a live field unit.
    pub(super) fn set_camera_follow(&mut self, unit: Option<u32>) -> Result<(), GameRuntimeError> {
        match unit {
            None => {
                self.camera_follow_unit = None;
                Ok(())
            }
            Some(id) => {
                if self.field_unit(id).is_none() {
                    return Err(GameRuntimeError::Invalid {
                        field: "session.camera_follow.unit".to_string(),
                        reason: format!("no live field unit {id}"),
                    });
                }
                self.camera_follow_unit = Some(id);
                Ok(())
            }
        }
    }

    /// Whether the host should step the sim this frame (#1854 SimGating).
    /// Only an active, unskipped, `pauses_sim` cinematic freezes it.
    pub fn should_advance_sim(&self) -> bool {
        !matches!(&self.cinematic_gate, Some(gate) if gate.pauses_sim && !gate.skipped)
    }

    /// Active cinematic timeline id, if any (presentation host lookup key).
    pub fn cinematic_timeline_id(&self) -> Option<&str> {
        self.cinematic_gate
            .as_ref()
            .filter(|g| !g.skipped)
            .map(|g| g.timeline_id.as_str())
    }

    /// Presentation camera-follow unit binding (#1861), if set.
    pub fn camera_follow_unit(&self) -> Option<u32> {
        self.camera_follow_unit
    }
}
