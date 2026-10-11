use std::collections::BTreeMap;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::{catalog_error, validate_condition_args};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerUnitState {
    pub alive: bool,
    pub unit_type_id: String,
    pub owner_player_id: String,
    pub region_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerPlayerState {
    pub resources: BTreeMap<String, i64>,
    pub flags: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerGameState {
    pub flags: BTreeMap<String, bool>,
    pub difficulty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerConditionContext {
    pub units: BTreeMap<String, TriggerUnitState>,
    pub players: BTreeMap<String, TriggerPlayerState>,
    pub game: TriggerGameState,
}

pub fn evaluate_condition(
    operation: &str,
    args: &BTreeMap<String, String>,
    context: &TriggerConditionContext,
) -> Result<bool, EngineError> {
    validate_condition_args(operation, args)?;
    match operation {
        "integer_equal" => Ok(integer_arg(args, "left")? == integer_arg(args, "right")?),
        "integer_greater_than" => Ok(integer_arg(args, "left")? > integer_arg(args, "right")?),
        "integer_greater_equal" => Ok(integer_arg(args, "left")? >= integer_arg(args, "right")?),
        "real_less_than" => Ok(real_arg(args, "left")? < real_arg(args, "right")?),
        "real_greater_equal" => Ok(real_arg(args, "left")? >= real_arg(args, "right")?),
        "string_equal" => Ok(string_arg(args, "left")? == string_arg(args, "right")?),
        "boolean_equal" => Ok(boolean_arg(args, "left")? == boolean_arg(args, "right")?),
        "boolean_and" => Ok(boolean_arg(args, "left")? && boolean_arg(args, "right")?),
        "boolean_or" => Ok(boolean_arg(args, "left")? || boolean_arg(args, "right")?),
        "boolean_not" => Ok(!boolean_arg(args, "value")?),
        "unit_alive" => Ok(unit(context, string_arg(args, "unit_id")?)?.alive),
        "unit_type_equals" => Ok(unit(context, string_arg(args, "unit_id")?)?.unit_type_id
            == string_arg(args, "unit_type_id")?),
        "unit_in_region" => Ok(unit(context, string_arg(args, "unit_id")?)?.region_id
            == string_arg(args, "region_id")?),
        "unit_owner_equals" => Ok(unit(context, string_arg(args, "unit_id")?)?.owner_player_id
            == string_arg(args, "player_id")?),
        "player_resource_at_least" => {
            let player_id = string_arg(args, "player_id")?;
            let resource_kind = string_arg(args, "resource_kind")?;
            let observed = player(context, player_id)?
                .resources
                .get(resource_kind)
                .ok_or_else(|| {
                    state_error("resource_kind", format!("unknown resource {resource_kind}"))
                })?;
            Ok(*observed >= integer_arg(args, "minimum")?)
        }
        "player_flag_true" => {
            let player_id = string_arg(args, "player_id")?;
            let flag = string_arg(args, "flag")?;
            Ok(*player(context, player_id)?
                .flags
                .get(flag)
                .ok_or_else(|| state_error("flag", format!("unknown player flag {flag}")))?)
        }
        "game_flag_true" => {
            let flag = string_arg(args, "flag")?;
            Ok(*context
                .game
                .flags
                .get(flag)
                .ok_or_else(|| state_error("flag", format!("unknown game flag {flag}")))?)
        }
        "game_difficulty_equals" => Ok(context.game.difficulty == string_arg(args, "difficulty")?),
        _ => Err(catalog_error(
            "operation",
            format!("unknown trigger condition {operation}"),
        )),
    }
}

fn integer_arg(args: &BTreeMap<String, String>, key: &str) -> Result<i64, EngineError> {
    string_arg(args, key)?
        .parse::<i64>()
        .map_err(|_| catalog_error(key, format!("{:?} is not a valid integer", args[key])))
}

fn real_arg(args: &BTreeMap<String, String>, key: &str) -> Result<f32, EngineError> {
    string_arg(args, key)?
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| catalog_error(key, format!("{:?} is not a finite real", args[key])))
}

fn boolean_arg(args: &BTreeMap<String, String>, key: &str) -> Result<bool, EngineError> {
    match string_arg(args, key)? {
        "true" => Ok(true),
        "false" => Ok(false),
        value => Err(catalog_error(
            key,
            format!("{value:?} is not true or false"),
        )),
    }
}

fn string_arg<'a>(args: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, EngineError> {
    args.get(key)
        .map(String::as_str)
        .ok_or_else(|| catalog_error("args", format!("missing required arg {key}")))
}

fn unit<'a>(
    context: &'a TriggerConditionContext,
    unit_id: &str,
) -> Result<&'a TriggerUnitState, EngineError> {
    context
        .units
        .get(unit_id)
        .ok_or_else(|| state_error("unit_id", format!("unknown unit {unit_id}")))
}

fn player<'a>(
    context: &'a TriggerConditionContext,
    player_id: &str,
) -> Result<&'a TriggerPlayerState, EngineError> {
    context
        .players
        .get(player_id)
        .ok_or_else(|| state_error("player_id", format!("unknown player {player_id}")))
}

fn state_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.trigger.condition_library.state.{}", field.into()),
        reason: reason.into(),
    }
}
