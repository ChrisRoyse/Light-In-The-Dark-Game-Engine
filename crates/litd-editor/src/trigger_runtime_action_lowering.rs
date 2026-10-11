//! #1851/#1855: lower authored ECA ACTION nodes to the typed engine
//! [`TriggerAction`]. Split from `trigger_runtime_lowering.rs` (#1855) so the module
//! stays modular as the action vocabulary grew from the #1851 unit-affecting set to
//! the WC3-parity runtime-effect set (quest/timer/dialog/floating-text/defeat/tod).
//!
//! Doctrine is unchanged: any authored construct outside the runtime scope is rejected
//! FAIL-LOUD with a typed error naming the construct -- NEVER silently dropped.

use std::collections::BTreeMap;

use litd_engine::EngineError;
use litd_engine::game_runtime::{TriggerAction, TriggerValue};

use crate::mcp_trigger_tools::{EcaNode, McpTriggerRuntimeState, TriggerValueType};

use super::{arg, parse_i32, parse_u32, resolve_unit, scope_error};

fn field(trigger: &str, node: &EcaNode) -> String {
    format!(
        "litd_editor.trigger_runtime_lowering.{trigger}.{}",
        node.node_id
    )
}

fn parse_u64(trigger: &str, node: &EcaNode, key: &str) -> Result<u64, EngineError> {
    let text = arg(trigger, node, key)?;
    text.trim()
        .parse::<u64>()
        .map_err(|error| EngineError::GamePackageInvalidConfig {
            field: field(trigger, node),
            reason: format!("{key} {text:?} is not a u64: {error}"),
        })
}

fn parse_i64(trigger: &str, node: &EcaNode, key: &str) -> Result<i64, EngineError> {
    let text = arg(trigger, node, key)?;
    text.trim()
        .parse::<i64>()
        .map_err(|error| EngineError::GamePackageInvalidConfig {
            field: field(trigger, node),
            reason: format!("{key} {text:?} is not an i64: {error}"),
        })
}

fn parse_u8(trigger: &str, node: &EcaNode, key: &str) -> Result<u8, EngineError> {
    let text = arg(trigger, node, key)?;
    text.trim()
        .parse::<u8>()
        .map_err(|error| EngineError::GamePackageInvalidConfig {
            field: field(trigger, node),
            reason: format!("{key} {text:?} is not a u8: {error}"),
        })
}

/// Parse an optional boolean arg (default `default` when absent). Fails loud on a
/// present-but-non-boolean value.
fn parse_bool_opt(
    trigger: &str,
    node: &EcaNode,
    key: &str,
    default: bool,
) -> Result<bool, EngineError> {
    match node.args.get(key).map(|s| s.trim()) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(other) => scope_error(trigger, format!("{key} {other:?} is not a boolean")),
    }
}

fn set_variable_action(
    trigger: &str,
    node: &EcaNode,
    state: &McpTriggerRuntimeState,
    name_key: &str,
) -> Result<TriggerAction, EngineError> {
    let name = arg(trigger, node, name_key)?;
    let value_text = arg(trigger, node, "value")?;
    let declared = state
        .variables
        .iter()
        .find(|v| v.name == name)
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: field(trigger, node),
            reason: format!("set_variable targets undeclared variable {name:?}"),
        })?;
    let value = match declared.value_type {
        TriggerValueType::Integer => {
            TriggerValue::Integer(value_text.trim().parse::<i64>().map_err(|error| {
                EngineError::GamePackageInvalidConfig {
                    field: format!("litd_editor.trigger_runtime_lowering.{trigger}"),
                    reason: format!("value {value_text:?} is not an integer: {error}"),
                }
            })?)
        }
        TriggerValueType::Boolean => match value_text {
            "true" => TriggerValue::Flag(true),
            "false" => TriggerValue::Flag(false),
            other => {
                return scope_error(trigger, format!("value {other:?} is not a boolean"));
            }
        },
        TriggerValueType::String => TriggerValue::Text(value_text.to_string()),
        other => {
            return scope_error(
                trigger,
                format!("set_variable on {other:?}-typed variable is outside the #1851 scope"),
            );
        }
    };
    Ok(TriggerAction::SetVariable {
        name: name.to_string(),
        value,
    })
}

/// Parse a comma-separated list of u32 flag ids (empty when the arg is absent/blank).
fn parse_flag_list(trigger: &str, node: &EcaNode, key: &str) -> Result<Vec<u32>, EngineError> {
    let Some(text) = node.args.get(key) else {
        return Ok(Vec::new());
    };
    let mut flags = Vec::new();
    for piece in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        flags.push(piece.parse::<u32>().map_err(|error| {
            EngineError::GamePackageInvalidConfig {
                field: field(trigger, node),
                reason: format!("{key} entry {piece:?} is not a u32: {error}"),
            }
        })?);
    }
    Ok(flags)
}

/// Lower the #1855 runtime-effect action set. Returns `None` for an operation this
/// helper does not own (so the caller can try the #1851 unit-affecting set / fail loud).
fn lower_effect_action(
    trigger: &str,
    node: &EcaNode,
) -> Result<Option<TriggerAction>, EngineError> {
    let action = match node.operation.as_str() {
        "create_quest" => TriggerAction::CreateQuest {
            quest: parse_u32(trigger, node, "quest_id")?,
            title: arg(trigger, node, "title")?.to_string(),
            description: arg(trigger, node, "description")?.to_string(),
            icon: node.args.get("icon").cloned().unwrap_or_default(),
            required: parse_bool_opt(trigger, node, "required", true)?,
            required_flags: parse_flag_list(trigger, node, "required_flags")?,
        },
        "discover_quest" => TriggerAction::DiscoverQuest {
            quest: parse_u32(trigger, node, "quest_id")?,
        },
        "satisfy_quest_flag" => TriggerAction::SatisfyQuestFlag {
            flag: parse_u32(trigger, node, "flag_id")?,
        },
        "complete_quest" => TriggerAction::CompleteQuest {
            quest: parse_u32(trigger, node, "quest_id")?,
        },
        "fail_quest" => TriggerAction::FailQuest {
            quest: parse_u32(trigger, node, "quest_id")?,
        },
        "start_timer" => TriggerAction::StartTimer {
            one_shot: parse_bool_opt(trigger, node, "one_shot", true)?,
            period_ticks: parse_u64(trigger, node, "period_ticks")?,
        },
        "pause_timer" => TriggerAction::PauseTimer {
            timer: parse_u32(trigger, node, "timer_id")?,
        },
        "resume_timer" => TriggerAction::ResumeTimer {
            timer: parse_u32(trigger, node, "timer_id")?,
        },
        "show_dialog" => TriggerAction::ShowDialog {
            dialog: parse_u32(trigger, node, "dialog_id")?,
            players: match node.args.get("players") {
                Some(_) => parse_u64(trigger, node, "players")?,
                None => u64::MAX,
            },
        },
        "hide_dialog" => TriggerAction::HideDialog {
            dialog: parse_u32(trigger, node, "dialog_id")?,
        },
        "create_text_tag" | "spawn_floating_text" => TriggerAction::SpawnFloatingText {
            text: arg(trigger, node, "text")?.to_string(),
            position_mm: [
                parse_i64(trigger, node, "x_mm")?,
                parse_i64(trigger, node, "y_mm")?,
                parse_i64(trigger, node, "z_mm")?,
            ],
            color: [
                parse_u8(trigger, node, "r")?,
                parse_u8(trigger, node, "g")?,
                parse_u8(trigger, node, "b")?,
                parse_u8(trigger, node, "a")?,
            ],
            lifespan_ticks: parse_u64(trigger, node, "lifespan_ticks")?,
        },
        "defeat_player" => TriggerAction::DefeatPlayer {
            player: parse_u8(trigger, node, "player")?,
        },
        "set_time_of_day" => TriggerAction::SetTimeOfDay {
            tick_in_day: parse_u64(trigger, node, "tick_in_day")?,
        },
        "reveal_region" | "conceal_region" => {
            // Region-id fog targets need the #1856 region system; author cell bounds.
            if node.args.contains_key("region_id") {
                return scope_error(
                    trigger,
                    format!(
                        "{} with a region_id target needs the runtime region system \
                         (#1856); author min_x/min_y/max_x/max_y cell bounds",
                        node.operation
                    ),
                )
                .map(Some);
            }
            let (player, min_x, min_y, max_x, max_y) = (
                parse_u32(trigger, node, "player")?,
                parse_i32(trigger, node, "min_x")?,
                parse_i32(trigger, node, "min_y")?,
                parse_i32(trigger, node, "max_x")?,
                parse_i32(trigger, node, "max_y")?,
            );
            if node.operation == "reveal_region" {
                TriggerAction::RevealRegion {
                    player,
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                }
            } else {
                TriggerAction::ConcealRegion {
                    player,
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                }
            }
        }
        "play_cinematic" => TriggerAction::PlayCinematic {
            timeline_id: arg(trigger, node, "timeline_id")?.to_string(),
            pauses_sim: parse_bool_opt(trigger, node, "pauses_sim", true)?,
        },
        "skip_cinematic" => TriggerAction::SkipCinematic,
        "create_unit" | "spawn_unit" | "spawn_field_unit" => TriggerAction::SpawnFieldUnit {
            unit_key: arg(trigger, node, "unit_key")?.to_string(),
            owner: parse_u8(trigger, node, "owner")?,
            cell_x: parse_i32(trigger, node, "cell_x")?,
            cell_z: parse_i32(trigger, node, "cell_z")?,
        },
        "set_weather" => TriggerAction::SetWeather {
            ordinal: parse_u32(trigger, node, "ordinal")?,
            intensity_permille: {
                let v = parse_u32(trigger, node, "intensity_permille")?;
                u16::try_from(v).map_err(|_| EngineError::GamePackageInvalidConfig {
                    field: field(trigger, node),
                    reason: format!("intensity_permille {v} exceeds u16"),
                })?
            },
        },
        "camera_follow_unit" | "follow_unit" => TriggerAction::CameraFollowUnit {
            unit: parse_u32(trigger, node, "unit")?,
        },
        "camera_clear_follow" | "clear_follow" => TriggerAction::CameraClearFollow,
        _ => return Ok(None),
    };
    Ok(Some(action))
}

pub(super) fn lower_action(
    trigger: &str,
    node: &EcaNode,
    state: &McpTriggerRuntimeState,
    unit_handles: &BTreeMap<String, u32>,
) -> Result<TriggerAction, EngineError> {
    let reject_region_target = |key: &str| -> Result<(), EngineError> {
        if node.args.contains_key(key) {
            return scope_error(
                trigger,
                format!(
                    "{} with a {key} target needs the runtime region system (#1856); \
                     author cell_x/cell_z cell targets under #1851",
                    node.operation
                ),
            );
        }
        Ok(())
    };
    match node.operation.as_str() {
        "kill_unit" => Ok(TriggerAction::KillUnit {
            unit: resolve_unit(trigger, node, unit_handles)?,
        }),
        "move_unit" => {
            reject_region_target("region_id")?;
            Ok(TriggerAction::MoveUnit {
                unit: resolve_unit(trigger, node, unit_handles)?,
                cell_x: parse_i32(trigger, node, "cell_x")?,
                cell_z: parse_i32(trigger, node, "cell_z")?,
            })
        }
        "order_unit" => {
            reject_region_target("target_region_id")?;
            if let Some(order) = node.args.get("order_id")
                && order != "move"
            {
                return scope_error(
                    trigger,
                    format!("order_unit order_id {order:?} is outside the #1851 scope (move only)"),
                );
            }
            Ok(TriggerAction::OrderUnitMove {
                unit: resolve_unit(trigger, node, unit_handles)?,
                cell_x: parse_i32(trigger, node, "cell_x")?,
                cell_z: parse_i32(trigger, node, "cell_z")?,
            })
        }
        "set_variable"
        | "set_integer_variable"
        | "set_boolean_variable"
        | "set_string_variable" => {
            let name_key = if node.args.contains_key("variable") {
                "variable"
            } else {
                "variable_name"
            };
            set_variable_action(trigger, node, state, name_key)
        }
        "fire_trigger" => Ok(TriggerAction::FireTrigger {
            trigger: parse_u32(trigger, node, "trigger")?,
        }),
        other => match lower_effect_action(trigger, node)? {
            Some(action) => Ok(action),
            None => scope_error(
                trigger,
                format!(
                    "action {other:?} is outside the runtime scope (kill_unit, move_unit, \
                     order_unit, set_variable, fire_trigger, #1855 effect actions, \
                     reveal/conceal_region, play/skip_cinematic, create_unit/spawn_field_unit, \
                     set_weather, camera_follow_unit/camera_clear_follow)"
                ),
            ),
        },
    }
}
