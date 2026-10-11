//! Typed no-code trigger condition catalog and evaluator for the ECA editor.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

#[path = "trigger_condition_library_eval.rs"]
mod eval;
pub use eval::{
    TriggerConditionContext, TriggerGameState, TriggerPlayerState, TriggerUnitState,
    evaluate_condition,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TriggerConditionCategory {
    Comparison,
    Boolean,
    UnitState,
    PlayerState,
    GameState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerConditionParamType {
    Integer,
    Real,
    Boolean,
    String,
    UnitId,
    UnitTypeId,
    PlayerId,
    RegionId,
    ResourceKind,
    GameFlag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerConditionParam {
    pub name: &'static str,
    pub value_type: TriggerConditionParamType,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TriggerConditionDefinition {
    pub operation: &'static str,
    pub display_name: &'static str,
    pub category: TriggerConditionCategory,
    pub params: &'static [TriggerConditionParam],
}

pub const REQUIRED_CONDITION_CATEGORIES: [TriggerConditionCategory; 5] = [
    TriggerConditionCategory::Comparison,
    TriggerConditionCategory::Boolean,
    TriggerConditionCategory::UnitState,
    TriggerConditionCategory::PlayerState,
    TriggerConditionCategory::GameState,
];

const LEFT_INTEGER: TriggerConditionParam = param("left", TriggerConditionParamType::Integer);
const RIGHT_INTEGER: TriggerConditionParam = param("right", TriggerConditionParamType::Integer);
const LEFT_REAL: TriggerConditionParam = param("left", TriggerConditionParamType::Real);
const RIGHT_REAL: TriggerConditionParam = param("right", TriggerConditionParamType::Real);
const LEFT_STRING: TriggerConditionParam = param("left", TriggerConditionParamType::String);
const RIGHT_STRING: TriggerConditionParam = param("right", TriggerConditionParamType::String);
const LEFT_BOOLEAN: TriggerConditionParam = param("left", TriggerConditionParamType::Boolean);
const RIGHT_BOOLEAN: TriggerConditionParam = param("right", TriggerConditionParamType::Boolean);
const VALUE_BOOLEAN: TriggerConditionParam = param("value", TriggerConditionParamType::Boolean);
const UNIT_ID: TriggerConditionParam = param("unit_id", TriggerConditionParamType::UnitId);
const UNIT_TYPE_ID: TriggerConditionParam =
    param("unit_type_id", TriggerConditionParamType::UnitTypeId);
const PLAYER_ID: TriggerConditionParam = param("player_id", TriggerConditionParamType::PlayerId);
const REGION_ID: TriggerConditionParam = param("region_id", TriggerConditionParamType::RegionId);
const RESOURCE_KIND: TriggerConditionParam =
    param("resource_kind", TriggerConditionParamType::ResourceKind);
const MINIMUM: TriggerConditionParam = param("minimum", TriggerConditionParamType::Integer);
const FLAG: TriggerConditionParam = param("flag", TriggerConditionParamType::GameFlag);
const DIFFICULTY: TriggerConditionParam = param("difficulty", TriggerConditionParamType::String);

const INT_PAIR: &[TriggerConditionParam] = &[LEFT_INTEGER, RIGHT_INTEGER];
const REAL_PAIR: &[TriggerConditionParam] = &[LEFT_REAL, RIGHT_REAL];
const STRING_PAIR: &[TriggerConditionParam] = &[LEFT_STRING, RIGHT_STRING];
const BOOL_PAIR: &[TriggerConditionParam] = &[LEFT_BOOLEAN, RIGHT_BOOLEAN];
const BOOL_SINGLE: &[TriggerConditionParam] = &[VALUE_BOOLEAN];
const UNIT_ONLY: &[TriggerConditionParam] = &[UNIT_ID];
const UNIT_TYPE: &[TriggerConditionParam] = &[UNIT_ID, UNIT_TYPE_ID];
const UNIT_REGION: &[TriggerConditionParam] = &[UNIT_ID, REGION_ID];
const UNIT_OWNER: &[TriggerConditionParam] = &[UNIT_ID, PLAYER_ID];
const PLAYER_RESOURCE: &[TriggerConditionParam] = &[PLAYER_ID, RESOURCE_KIND, MINIMUM];
const PLAYER_FLAG: &[TriggerConditionParam] = &[PLAYER_ID, FLAG];
const GAME_FLAG_PARAM: &[TriggerConditionParam] = &[FLAG];
const GAME_DIFFICULTY: &[TriggerConditionParam] = &[DIFFICULTY];

pub const TRIGGER_CONDITION_CATALOG: &[TriggerConditionDefinition] = &[
    condition(
        "integer_equal",
        "Integer equal",
        TriggerConditionCategory::Comparison,
        INT_PAIR,
    ),
    condition(
        "integer_greater_than",
        "Integer greater than",
        TriggerConditionCategory::Comparison,
        INT_PAIR,
    ),
    condition(
        "integer_greater_equal",
        "Integer greater or equal",
        TriggerConditionCategory::Comparison,
        INT_PAIR,
    ),
    condition(
        "real_less_than",
        "Real less than",
        TriggerConditionCategory::Comparison,
        REAL_PAIR,
    ),
    condition(
        "real_greater_equal",
        "Real greater or equal",
        TriggerConditionCategory::Comparison,
        REAL_PAIR,
    ),
    condition(
        "string_equal",
        "String equal",
        TriggerConditionCategory::Comparison,
        STRING_PAIR,
    ),
    condition(
        "boolean_equal",
        "Boolean equal",
        TriggerConditionCategory::Comparison,
        BOOL_PAIR,
    ),
    condition(
        "boolean_and",
        "Boolean and",
        TriggerConditionCategory::Boolean,
        BOOL_PAIR,
    ),
    condition(
        "boolean_or",
        "Boolean or",
        TriggerConditionCategory::Boolean,
        BOOL_PAIR,
    ),
    condition(
        "boolean_not",
        "Boolean not",
        TriggerConditionCategory::Boolean,
        BOOL_SINGLE,
    ),
    condition(
        "unit_alive",
        "Unit is alive",
        TriggerConditionCategory::UnitState,
        UNIT_ONLY,
    ),
    condition(
        "unit_type_equals",
        "Unit type equals",
        TriggerConditionCategory::UnitState,
        UNIT_TYPE,
    ),
    condition(
        "unit_in_region",
        "Unit is in region",
        TriggerConditionCategory::UnitState,
        UNIT_REGION,
    ),
    condition(
        "unit_owner_equals",
        "Unit owner equals",
        TriggerConditionCategory::UnitState,
        UNIT_OWNER,
    ),
    condition(
        "player_resource_at_least",
        "Player resource at least",
        TriggerConditionCategory::PlayerState,
        PLAYER_RESOURCE,
    ),
    condition(
        "player_flag_true",
        "Player flag is true",
        TriggerConditionCategory::PlayerState,
        PLAYER_FLAG,
    ),
    condition(
        "game_flag_true",
        "Game flag is true",
        TriggerConditionCategory::GameState,
        GAME_FLAG_PARAM,
    ),
    condition(
        "game_difficulty_equals",
        "Game difficulty equals",
        TriggerConditionCategory::GameState,
        GAME_DIFFICULTY,
    ),
];

pub fn condition_catalog() -> &'static [TriggerConditionDefinition] {
    TRIGGER_CONDITION_CATALOG
}

pub fn find_condition(operation: &str) -> Result<&'static TriggerConditionDefinition, EngineError> {
    validate_symbol("operation", operation)?;
    TRIGGER_CONDITION_CATALOG
        .iter()
        .find(|condition| condition.operation == operation)
        .ok_or_else(|| {
            catalog_error(
                "operation",
                format!("unknown trigger condition {operation}"),
            )
        })
}

pub fn conditions_by_category(
    category: TriggerConditionCategory,
) -> Vec<&'static TriggerConditionDefinition> {
    TRIGGER_CONDITION_CATALOG
        .iter()
        .filter(|condition| condition.category == category)
        .collect()
}

pub fn validate_condition_args(
    operation: &str,
    args: &BTreeMap<String, String>,
) -> Result<&'static TriggerConditionDefinition, EngineError> {
    let condition = find_condition(operation)?;
    let allowed = condition
        .params
        .iter()
        .map(|param| param.name)
        .collect::<BTreeSet<_>>();
    for param in condition.params.iter().filter(|param| param.required) {
        if !args.contains_key(param.name) {
            return Err(catalog_error(
                "args",
                format!("condition {operation} missing required arg {}", param.name),
            ));
        }
    }
    for (key, value) in args {
        if !allowed.contains(key.as_str()) {
            return Err(catalog_error(
                "args",
                format!("condition {operation} received unknown arg {key}"),
            ));
        }
        let param = condition
            .params
            .iter()
            .find(|param| param.name == key)
            .unwrap();
        validate_arg_value(key, param.value_type, value)?;
    }
    Ok(condition)
}

pub fn validate_condition_catalog_complete() -> Result<(), EngineError> {
    let mut categories = BTreeSet::new();
    let mut operations = BTreeSet::new();
    for condition in TRIGGER_CONDITION_CATALOG {
        validate_symbol("operation", condition.operation)?;
        validate_display("display_name", condition.display_name)?;
        if !operations.insert(condition.operation) {
            return Err(catalog_error(
                "operation",
                format!("duplicate {}", condition.operation),
            ));
        }
        categories.insert(condition.category);
        let mut names = BTreeSet::new();
        for param in condition.params {
            validate_symbol("param.name", param.name)?;
            if !names.insert(param.name) {
                return Err(catalog_error(
                    "param.name",
                    format!("duplicate param {} on {}", param.name, condition.operation),
                ));
            }
        }
    }
    for category in REQUIRED_CONDITION_CATEGORIES {
        if !categories.contains(&category) {
            return Err(catalog_error(
                "category",
                format!("missing category {category:?}"),
            ));
        }
    }
    Ok(())
}

const fn param(name: &'static str, value_type: TriggerConditionParamType) -> TriggerConditionParam {
    TriggerConditionParam {
        name,
        value_type,
        required: true,
    }
}

const fn condition(
    operation: &'static str,
    display_name: &'static str,
    category: TriggerConditionCategory,
    params: &'static [TriggerConditionParam],
) -> TriggerConditionDefinition {
    TriggerConditionDefinition {
        operation,
        display_name,
        category,
        params,
    }
}

fn validate_arg_value(
    field: &str,
    value_type: TriggerConditionParamType,
    value: &str,
) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(catalog_error(
            field,
            "argument values must be non-empty single-line text",
        ));
    }
    match value_type {
        TriggerConditionParamType::Integer => value
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| catalog_error(field, format!("{value:?} is not a valid integer"))),
        TriggerConditionParamType::Real => value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|_| ())
            .ok_or_else(|| catalog_error(field, format!("{value:?} is not a finite real"))),
        TriggerConditionParamType::Boolean => match value {
            "true" | "false" => Ok(()),
            _ => Err(catalog_error(
                field,
                format!("{value:?} is not true or false"),
            )),
        },
        TriggerConditionParamType::String => Ok(()),
        _ => validate_symbol(field, value),
    }
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(catalog_error(field, "symbol cannot be empty"));
    };
    if value.len() > 96
        || !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(catalog_error(field, "symbol must be an ASCII identifier"));
    }
    Ok(())
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 128 || value.contains(['\n', '\r', '\t']) {
        return Err(catalog_error(
            field,
            "display text must be bounded single-line text",
        ));
    }
    Ok(())
}

pub(crate) fn catalog_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.trigger.condition_library.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "trigger_condition_library_tests.rs"]
mod tests;
