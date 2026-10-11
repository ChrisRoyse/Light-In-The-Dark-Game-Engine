//! #1855/#1857/#1854/#1859/#1861/#804: plan trigger effect-actions into SessionOrders.
//!
//! Split from `session_triggers.rs` so the evaluator stays under the 500-line
//! ratchet as the action vocabulary grows. Pure 1:1 mapping; owning subsystems
//! fail loud on invalid ids at dispatch.

use super::super::super::trigger_graph::TriggerAction;
use super::super::GameRuntimeError;
use super::super::{MatchRuntimeSession, SessionOrder};

impl MatchRuntimeSession {
    /// Map one runtime-effect / fog / cinematic / spawn / weather / camera
    /// trigger action into its SessionOrder. Returns `true` when the action was
    /// handled here; `false` leaves the #1851 unit-affecting set to the caller.
    pub(super) fn plan_effect_trigger_action(
        &self,
        action: &TriggerAction,
        plan: &mut Vec<SessionOrder>,
    ) -> Result<bool, GameRuntimeError> {
        match action {
            TriggerAction::CreateQuest {
                quest,
                title,
                description,
                icon,
                required,
                required_flags,
            } => {
                plan.push(SessionOrder::QuestCreate {
                    quest: *quest,
                    title: title.clone(),
                    description: description.clone(),
                    icon: icon.clone(),
                    required: *required,
                    required_flags: required_flags.clone(),
                });
            }
            TriggerAction::DiscoverQuest { quest } => {
                plan.push(SessionOrder::QuestDiscover { quest: *quest });
            }
            TriggerAction::SatisfyQuestFlag { flag } => {
                plan.push(SessionOrder::QuestSatisfyFlag { flag: *flag });
            }
            TriggerAction::CompleteQuest { quest } => {
                plan.push(SessionOrder::QuestComplete { quest: *quest });
            }
            TriggerAction::FailQuest { quest } => {
                plan.push(SessionOrder::QuestFail { quest: *quest });
            }
            TriggerAction::StartTimer {
                one_shot,
                period_ticks,
            } => {
                plan.push(SessionOrder::StartTimer {
                    one_shot: *one_shot,
                    period_ticks: *period_ticks,
                });
            }
            TriggerAction::PauseTimer { timer } => {
                plan.push(SessionOrder::PauseTimer { timer: *timer });
            }
            TriggerAction::ResumeTimer { timer } => {
                plan.push(SessionOrder::ResumeTimer { timer: *timer });
            }
            TriggerAction::CreateTimerWindow { timer, title } => {
                plan.push(SessionOrder::CreateTimerWindow {
                    timer: *timer,
                    title: title.clone(),
                });
            }
            TriggerAction::CreateDialog { title } => {
                plan.push(SessionOrder::CreateDialog {
                    title: title.clone(),
                });
            }
            TriggerAction::AddDialogButton {
                dialog,
                text,
                hotkey,
            } => {
                plan.push(SessionOrder::AddDialogButton {
                    dialog: *dialog,
                    text: text.clone(),
                    hotkey: *hotkey,
                });
            }
            TriggerAction::ShowDialog { dialog, players } => {
                plan.push(SessionOrder::ShowDialog {
                    dialog: *dialog,
                    players: *players,
                });
            }
            TriggerAction::HideDialog { dialog } => {
                plan.push(SessionOrder::HideDialog { dialog: *dialog });
            }
            TriggerAction::SpawnFloatingText {
                text,
                position_mm,
                color,
                lifespan_ticks,
            } => {
                plan.push(SessionOrder::SpawnFloatingText {
                    text: text.clone(),
                    position_mm: *position_mm,
                    velocity_mm_per_tick: [0, 0, 40],
                    color: *color,
                    lifespan_ticks: *lifespan_ticks,
                    fade_start_ticks: *lifespan_ticks / 2,
                });
            }
            TriggerAction::DefeatPlayer { player } => {
                plan.push(SessionOrder::DefeatPlayer { player: *player });
            }
            TriggerAction::DamageUnit { unit, amount_milli } => {
                plan.push(SessionOrder::ApplyDamage {
                    target: *unit,
                    amount_milli: *amount_milli,
                });
            }
            TriggerAction::AddUnitToGroup { player, unit } => {
                plan.push(SessionOrder::AddToGroup {
                    player: *player,
                    unit: *unit,
                });
            }
            TriggerAction::SelectUnits { player, units } => {
                plan.push(SessionOrder::SelectUnits {
                    player: *player,
                    units: units.clone(),
                });
            }
            TriggerAction::SetTimeOfDay { tick_in_day } => {
                plan.push(SessionOrder::SetTimeOfDay {
                    tick_in_day: *tick_in_day,
                });
            }
            TriggerAction::RevealRegion {
                player,
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                plan.push(SessionOrder::RevealRegion {
                    player: *player,
                    min_x: *min_x,
                    min_y: *min_y,
                    max_x: *max_x,
                    max_y: *max_y,
                });
            }
            TriggerAction::ConcealRegion {
                player,
                min_x,
                min_y,
                max_x,
                max_y,
            } => {
                plan.push(SessionOrder::ConcealRegion {
                    player: *player,
                    min_x: *min_x,
                    min_y: *min_y,
                    max_x: *max_x,
                    max_y: *max_y,
                });
            }
            TriggerAction::PlayCinematic {
                timeline_id,
                pauses_sim,
            } => {
                plan.push(SessionOrder::PlayCinematic {
                    timeline_id: timeline_id.clone(),
                    pauses_sim: *pauses_sim,
                });
            }
            TriggerAction::SkipCinematic => {
                plan.push(SessionOrder::SkipCinematic);
            }
            TriggerAction::SpawnFieldUnit {
                unit_key,
                owner,
                cell_x,
                cell_z,
            } => {
                plan.push(SessionOrder::SpawnFieldUnit {
                    unit_key: unit_key.clone(),
                    owner: *owner,
                    cell_x: *cell_x,
                    cell_z: *cell_z,
                });
            }
            TriggerAction::SetWeather {
                ordinal,
                intensity_permille,
            } => {
                plan.push(SessionOrder::SetWeather {
                    ordinal: *ordinal,
                    intensity_permille: *intensity_permille,
                });
            }
            TriggerAction::CameraFollowUnit { unit } => {
                plan.push(SessionOrder::CameraFollowUnit { unit: *unit });
            }
            TriggerAction::CameraClearFollow => {
                plan.push(SessionOrder::CameraClearFollow);
            }
            TriggerAction::GrantResource { key, amount } => {
                plan.push(SessionOrder::GrantResource {
                    key: key.clone(),
                    amount: *amount,
                });
            }
            TriggerAction::SpendResource { key, amount } => {
                plan.push(SessionOrder::SpendResource {
                    key: key.clone(),
                    amount: *amount,
                });
            }
            TriggerAction::PlaceStructure {
                structure,
                cell_x,
                cell_z,
            } => {
                plan.push(SessionOrder::PlaceStructure {
                    struct_id: structure.clone(),
                    x: *cell_x,
                    z: *cell_z,
                });
            }
            TriggerAction::CancelStructure { instance_id } => {
                plan.push(SessionOrder::CancelStructure {
                    instance_id: *instance_id,
                });
            }
            TriggerAction::AddWorker { worker_id, node_id } => {
                plan.push(SessionOrder::AddWorker {
                    worker_id: *worker_id,
                    node_id: node_id.clone(),
                });
            }
            TriggerAction::AssignWorker { worker_id, node_id } => {
                plan.push(SessionOrder::AssignWorker {
                    worker_id: *worker_id,
                    node_id: node_id.clone(),
                });
            }
            TriggerAction::SetRally { instance_id, rally } => {
                plan.push(SessionOrder::SetRally {
                    instance_id: *instance_id,
                    rally: *rally,
                });
            }
            TriggerAction::TrainUnit {
                instance_id,
                unit_id,
            } => {
                plan.push(SessionOrder::TrainUnit {
                    instance_id: *instance_id,
                    unit_id: unit_id.clone(),
                });
            }
            TriggerAction::SetAlliance {
                from,
                to,
                ally,
                shared_vision,
                shared_control,
            } => {
                plan.push(SessionOrder::SetAlliance {
                    from: *from,
                    to: *to,
                    ally: *ally,
                    shared_vision: *shared_vision,
                    shared_control: *shared_control,
                });
            }
            // #1851 unit-affecting set is planned by the caller.
            TriggerAction::KillUnit { .. }
            | TriggerAction::MoveUnit { .. }
            | TriggerAction::OrderUnitMove { .. }
            | TriggerAction::OrderUnitAbilityNoTarget { .. }
            | TriggerAction::OrderUnitAbilityPoint { .. }
            | TriggerAction::SetVariable { .. }
            | TriggerAction::FireTrigger { .. } => return Ok(false),
        }
        let _ = self; // reserved for future state-aware effect planning
        Ok(true)
    }
}
