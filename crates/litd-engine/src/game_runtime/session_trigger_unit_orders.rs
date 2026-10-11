//! State-aware planning for trigger-authored unit orders.

use super::super::super::{groups::OrderTargetKind, trigger_graph::TriggerAction};
use super::super::{GameRuntimeError, MatchRuntimeSession, SessionOrder};

impl MatchRuntimeSession {
    pub(super) fn plan_unit_order_trigger_action(
        &self,
        action: &TriggerAction,
        plan: &mut Vec<SessionOrder>,
    ) -> Result<bool, GameRuntimeError> {
        match action {
            TriggerAction::OrderUnitMove {
                unit,
                cell_x,
                cell_z,
            } => {
                let owner = live_owner(self, *unit, "session.trigger.order_unit")?;
                let ability = self
                    .match_state()
                    .capabilities(*unit)?
                    .iter()
                    .find(|capability| capability.target_kind == OrderTargetKind::Point)
                    .map(|capability| capability.ability)
                    .ok_or_else(|| GameRuntimeError::Invalid {
                        field: "session.trigger.order_unit".to_string(),
                        reason: format!("unit {unit} has no point-move ability"),
                    })?;
                plan.push(SessionOrder::IssueMove {
                    player: owner,
                    units: vec![*unit],
                    ability,
                    x: *cell_x,
                    z: *cell_z,
                    queued: false,
                });
                plan.push(SessionOrder::MoveFieldUnit {
                    field_id: *unit,
                    cell_x: *cell_x,
                    cell_z: *cell_z,
                });
            }
            TriggerAction::OrderUnitAbilityNoTarget {
                unit,
                ability,
                queued,
            } => {
                plan.push(SessionOrder::IssueAbilityNoTarget {
                    player: live_owner(
                        self,
                        *unit,
                        "session.trigger.order_unit_ability_no_target",
                    )?,
                    units: vec![*unit],
                    ability: *ability,
                    queued: *queued,
                });
            }
            TriggerAction::OrderUnitAbilityPoint {
                unit,
                ability,
                cell_x,
                cell_z,
                queued,
            } => {
                plan.push(SessionOrder::IssueMove {
                    player: live_owner(self, *unit, "session.trigger.order_unit_ability_point")?,
                    units: vec![*unit],
                    ability: *ability,
                    x: *cell_x,
                    z: *cell_z,
                    queued: *queued,
                });
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn live_owner(
    session: &MatchRuntimeSession,
    unit: u32,
    field: &'static str,
) -> Result<u8, GameRuntimeError> {
    let owner = session
        .field_unit(unit)
        .map(|unit| unit.owner)
        .ok_or_else(|| GameRuntimeError::Invalid {
            field: field.to_string(),
            reason: format!("unit {unit} is not a live field unit"),
        })?;
    u8::try_from(owner).map_err(|_| GameRuntimeError::Invalid {
        field: field.to_string(),
        reason: format!("unit {unit} owner {owner} exceeds player id range"),
    })
}
