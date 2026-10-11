//! #1851/#1855: the lowered trigger ACTION set (the THEN of an ECA trigger).
//!
//! Split out of `trigger_graph.rs` (#1855) as the action vocabulary grew past the
//! first-cut #1851 unit-affecting set to the WC3-parity runtime-effect set: quest
//! state (#839), timers (#841), dialogs (#842), floating text (#843), victory/defeat
//! (#838), day/night + weather (#848), fog reveal/conceal (#847/#1857), scripted
//! cinematics (#845/#1854), unit spawning (#1859), camera follow (#846/#1861),
//! and persistent unit groups (#851/#804).
//!
//! Every action dispatches inside the deterministic tick as a typed
//! [`super::SessionOrder`] (see `session_triggers::plan_trigger_action`), so replaying
//! the `Tick` stream re-runs the evaluator identically (#85 lockstep). Actions that
//! name a blueprint unit are cross-checked against the verified blueprint at package
//! load by [`validate_action_against_blueprint`]; the rest carry their own id-spaces
//! (quest/flag/timer/dialog ids) validated fail-loud at runtime by the owning subsystem.

use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::super::blueprint::MatchBlueprint;
use super::super::groups::OrderTargetKind;
use super::super::structures::RallyPoint;
use super::TriggerValue;

/// A runtime trigger action (the THEN). Unit-affecting actions dispatch as typed
/// [`super::SessionOrder`]s inside the deterministic tick (#85 lockstep).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerAction {
    // ---- #1851 first-cut unit-affecting set. ----
    /// Destroy the blueprint unit (WC3 `KillUnit` -> `SessionOrder::DestroyUnit`).
    KillUnit { unit: u32 },
    /// Apply deterministic combat damage through the real HP/death/victory path.
    DamageUnit { unit: u32, amount_milli: u32 },
    /// Teleport the field unit to a cell (`SessionOrder::MoveFieldUnit`).
    MoveUnit { unit: u32, cell_x: i32, cell_z: i32 },
    /// Issue a point-target move order (`SessionOrder::IssueMove`).
    OrderUnitMove { unit: u32, cell_x: i32, cell_z: i32 },
    /// Issue one no-target ability order through the unit's live owner.
    OrderUnitAbilityNoTarget {
        unit: u32,
        ability: u16,
        queued: bool,
    },
    /// Issue one exact point-target ability order through the unit's live owner.
    OrderUnitAbilityPoint {
        unit: u32,
        ability: u16,
        cell_x: i32,
        cell_z: i32,
        queued: bool,
    },
    /// Add an authored unit to one player's persistent deterministic group.
    AddUnitToGroup { player: u8, unit: u32 },
    /// Replace one player's deterministic selection with authored units.
    SelectUnits { player: u8, units: Vec<u32> },
    /// Assign a typed value to a declared variable (type-checked at lowering).
    SetVariable { name: String, value: TriggerValue },
    /// Fire a victory custom-condition trigger (#838 `VictoryCondition::CustomTrigger`).
    FireTrigger { trigger: u32 },
    /// Grant an authored resource reward (#837). The resource key is checked against
    /// the package economy dataset before play.
    GrantResource { key: String, amount: u64 },
    /// Spend an authored resource amount through the deterministic economy.
    SpendResource { key: String, amount: u64 },
    /// Place an authored structure type; the runtime allocates its deterministic instance id.
    PlaceStructure {
        structure: String,
        cell_x: i32,
        cell_z: i32,
    },
    /// Cancel an incomplete deterministic structure instance and refund its costs.
    CancelStructure { instance_id: u32 },
    /// Spawn a harvesting worker with an authored deterministic id.
    AddWorker { worker_id: u32, node_id: String },
    /// Reassign an authored worker to another authored resource node.
    AssignWorker { worker_id: u32, node_id: String },
    /// Set a deterministic placed-structure instance's rally target.
    SetRally { instance_id: u32, rally: RallyPoint },
    /// Queue a lowered economy unit on a deterministic structure instance.
    TrainUnit { instance_id: u32, unit_id: String },
    /// Change one directed player-alliance cell (#836), including the fog/control
    /// sharing flags consumed by the real runtime roster.
    SetAlliance {
        from: u8,
        to: u8,
        ally: bool,
        shared_vision: bool,
        shared_control: bool,
    },

    // ---- #1855 runtime-effect set (quest/timer/dialog/floating/defeat/tod). ----
    /// Create a quest in the runtime quest log (#839).
    CreateQuest {
        quest: u32,
        title: String,
        description: String,
        icon: String,
        required: bool,
        required_flags: Vec<u32>,
    },
    /// Mark a quest discovered (#839 `QuestSetDiscovered`).
    DiscoverQuest { quest: u32 },
    /// Record a required quest flag satisfied (#839).
    SatisfyQuestFlag { flag: u32 },
    /// Complete a quest once its required flags are satisfied (#839).
    CompleteQuest { quest: u32 },
    /// Fail an active quest (#839 `QuestSetFailed`).
    FailQuest { quest: u32 },
    /// Start a deterministic countdown timer (#841). Ids are assigned in order.
    StartTimer { one_shot: bool, period_ticks: u64 },
    /// Pause a running countdown timer (#841).
    PauseTimer { timer: u32 },
    /// Resume a paused countdown timer (#841).
    ResumeTimer { timer: u32 },
    /// Bind or retitle the visible window for an existing timer (#841).
    CreateTimerWindow { timer: u32, title: String },
    /// Create a deterministic modal dialog (#842). Ids are assigned in order.
    CreateDialog { title: String },
    /// Add a button to an existing dialog (#842). Button ids are assigned in order.
    AddDialogButton {
        dialog: u32,
        text: String,
        hotkey: u32,
    },
    /// Show a modal dialog to a player-mask set (#842 `DialogDisplay`).
    ShowDialog { dialog: u32, players: u64 },
    /// Hide a modal dialog from everyone (#842).
    HideDialog { dialog: u32 },
    /// Spawn a world-anchored floating text tag (#843 `CreateTextTag`).
    SpawnFloatingText {
        text: String,
        position_mm: [i64; 3],
        color: [u8; 4],
        lifespan_ticks: u64,
    },
    /// Force a player to Defeat (#838), the `fire_trigger`-victory counterpart.
    DefeatPlayer { player: u8 },
    /// Set the day/night position within the current day (#848).
    SetTimeOfDay { tick_in_day: u64 },

    // ---- #1857 fog reveal/conceal (#847). ----
    /// Reveal a rectangular region for a player (WC3 fog modifier).
    RevealRegion {
        player: u32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    },
    /// Conceal a rectangular region for a player (conceal wins over reveal).
    ConcealRegion {
        player: u32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    },

    // ---- #1854 cinematics / #1859 spawn / #1861 weather + camera. ----
    /// Start a scripted cinematic (#1854); `pauses_sim` drives SimGating.
    PlayCinematic {
        timeline_id: String,
        pauses_sim: bool,
    },
    /// Skip / end the active cinematic gate (#1854).
    SkipCinematic,
    /// Spawn a field unit at a cell (#1859), validated against lowered economy unit.
    SpawnFieldUnit {
        unit_key: String,
        owner: u8,
        cell_x: i32,
        cell_z: i32,
    },
    /// Set weather ordinal + intensity (#1861 / #848).
    SetWeather {
        ordinal: u32,
        intensity_permille: u16,
    },
    /// Bind host camera to follow a live unit (#1861).
    CameraFollowUnit { unit: u32 },
    /// Clear the camera follow binding (#1861).
    CameraClearFollow,
}

/// Cross-dataset reference check for one action, run by
/// `validate_trigger_graph_against_blueprint` with the verified blueprint in hand.
/// Only the unit-affecting actions constrain the blueprint; the rest carry their own
/// id-spaces validated at runtime by the owning subsystem, so they always pass here.
pub(super) fn validate_action_against_blueprint(
    id: &str,
    action: &TriggerAction,
    blueprint: &MatchBlueprint,
) -> Result<(), EngineError> {
    let error = |reason: String| -> EngineError {
        EngineError::GamePackageInvalidConfig {
            field: format!("litd_game.package.trigger_graph.{id}"),
            reason,
        }
    };
    let require = |unit: u32, role: &str| {
        blueprint
            .units
            .iter()
            .find(|u| u.unit == unit)
            .ok_or_else(|| {
                error(format!(
                    "{role} references unit {unit} absent from the match blueprint"
                ))
            })
    };
    match action {
        TriggerAction::KillUnit { unit } => {
            require(*unit, "kill_unit action")?;
        }
        TriggerAction::DamageUnit { unit, amount_milli } => {
            require(*unit, "damage_unit action")?;
            if *amount_milli == 0 {
                return Err(error(format!(
                    "damage_unit against unit {unit} must have amount_milli > 0"
                )));
            }
        }
        TriggerAction::MoveUnit { unit, .. } => {
            let authored = require(*unit, "move_unit action")?;
            if authored.is_structure {
                return Err(error(format!(
                    "move_unit targets structure {unit} (not a field unit)"
                )));
            }
        }
        TriggerAction::OrderUnitMove { unit, .. } => {
            let authored = require(*unit, "order_unit action")?;
            if authored.is_structure {
                return Err(error(format!(
                    "order_unit targets structure {unit} (not a field unit)"
                )));
            }
            if !authored
                .capabilities
                .iter()
                .any(|c| c.target_kind == OrderTargetKind::Point)
            {
                return Err(error(format!(
                    "order_unit targets unit {unit} with no point-move ability"
                )));
            }
        }
        TriggerAction::OrderUnitAbilityNoTarget { unit, ability, .. } => {
            let authored = require(*unit, "order_unit_ability_no_target action")?;
            if authored.is_structure {
                return Err(error(format!(
                    "order_unit_ability_no_target targets structure {unit}"
                )));
            }
            let capability = authored
                .capabilities
                .iter()
                .find(|capability| capability.ability == *ability)
                .ok_or_else(|| {
                    error(format!(
                        "order_unit_ability_no_target targets unit {unit} without ability {ability}"
                    ))
                })?;
            if capability.target_kind != OrderTargetKind::None {
                return Err(error(format!(
                    "order_unit_ability_no_target ability {ability} on unit {unit} requires target kind {:?}",
                    capability.target_kind
                )));
            }
        }
        TriggerAction::OrderUnitAbilityPoint { unit, ability, .. } => {
            let authored = require(*unit, "order_unit_ability_point action")?;
            if authored.is_structure {
                return Err(error(format!(
                    "order_unit_ability_point targets structure {unit}"
                )));
            }
            let capability = authored
                .capabilities
                .iter()
                .find(|capability| capability.ability == *ability)
                .ok_or_else(|| {
                    error(format!(
                        "order_unit_ability_point targets unit {unit} without ability {ability}"
                    ))
                })?;
            if capability.target_kind != OrderTargetKind::Point {
                return Err(error(format!(
                    "order_unit_ability_point ability {ability} on unit {unit} requires target kind {:?}",
                    capability.target_kind
                )));
            }
        }
        TriggerAction::AddUnitToGroup { player, unit } => {
            require(*unit, "add_unit_to_group action")?;
            if !blueprint.slots.iter().any(|slot| slot.id == *player) {
                return Err(error(format!(
                    "add_unit_to_group player {player} is absent from the match roster"
                )));
            }
        }
        TriggerAction::SelectUnits { player, units } => {
            if !blueprint.slots.iter().any(|slot| slot.id == *player) {
                return Err(error(format!(
                    "select_units player {player} is absent from the match roster"
                )));
            }
            for unit in units {
                require(*unit, "select_units action")?;
            }
        }
        TriggerAction::SetAlliance { from, to, .. } => {
            if from == to {
                return Err(error(format!(
                    "set_alliance cannot target player {from}'s fixed self-alliance"
                )));
            }
            for (player, role) in [(*from, "from"), (*to, "to")] {
                if !blueprint.slots.iter().any(|slot| slot.id == player) {
                    return Err(error(format!(
                        "set_alliance {role} player {player} is absent from the match roster"
                    )));
                }
            }
            if blueprint.alliance_overrides.iter().any(|override_cell| {
                override_cell.from == *from && override_cell.to == *to && override_cell.cell.locked
            }) {
                return Err(error(format!(
                    "set_alliance pair ({from},{to}) is authored as locked"
                )));
            }
        }
        TriggerAction::DefeatPlayer { player } => {
            if !blueprint.slots.iter().any(|slot| slot.id == *player) {
                return Err(error(format!(
                    "defeat_player player {player} is absent from the match roster"
                )));
            }
        }
        TriggerAction::SpawnFieldUnit { owner, .. } => {
            if !blueprint.slots.iter().any(|slot| slot.id == *owner) {
                return Err(error(format!(
                    "spawn_field_unit owner {owner} is absent from the match roster"
                )));
            }
        }
        TriggerAction::CreateTimerWindow { timer, title } => {
            if title.trim().is_empty() {
                return Err(error(format!(
                    "create_timer_window title for timer {timer} must be non-empty"
                )));
            }
        }
        TriggerAction::CreateDialog { title } => {
            if title.trim().is_empty() {
                return Err(error("create_dialog title must be non-empty".to_string()));
            }
        }
        TriggerAction::AddDialogButton { dialog, text, .. } => {
            if text.trim().is_empty() {
                return Err(error(format!(
                    "add_dialog_button text for dialog {dialog} must be non-empty"
                )));
            }
        }
        // Self-id-space / player / literal actions: no blueprint constraint.
        TriggerAction::SetVariable { .. }
        | TriggerAction::FireTrigger { .. }
        | TriggerAction::GrantResource { .. }
        | TriggerAction::SpendResource { .. }
        | TriggerAction::PlaceStructure { .. }
        | TriggerAction::CancelStructure { .. }
        | TriggerAction::AddWorker { .. }
        | TriggerAction::AssignWorker { .. }
        | TriggerAction::SetRally { .. }
        | TriggerAction::TrainUnit { .. }
        | TriggerAction::CreateQuest { .. }
        | TriggerAction::DiscoverQuest { .. }
        | TriggerAction::SatisfyQuestFlag { .. }
        | TriggerAction::CompleteQuest { .. }
        | TriggerAction::FailQuest { .. }
        | TriggerAction::StartTimer { .. }
        | TriggerAction::PauseTimer { .. }
        | TriggerAction::ResumeTimer { .. }
        | TriggerAction::ShowDialog { .. }
        | TriggerAction::HideDialog { .. }
        | TriggerAction::SpawnFloatingText { .. }
        | TriggerAction::SetTimeOfDay { .. }
        | TriggerAction::RevealRegion { .. }
        | TriggerAction::ConcealRegion { .. }
        | TriggerAction::PlayCinematic { .. }
        | TriggerAction::SkipCinematic
        | TriggerAction::SetWeather { .. }
        | TriggerAction::CameraFollowUnit { .. }
        | TriggerAction::CameraClearFollow => {}
    }
    Ok(())
}
