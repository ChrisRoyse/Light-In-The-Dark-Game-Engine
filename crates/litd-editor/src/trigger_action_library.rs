//! Typed no-code trigger action catalog for the ECA editor.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

#[path = "trigger_action_library_exec.rs"]
mod exec;
pub use exec::{
    TriggerActionContext, TriggerActionOutcome, TriggerActionUnitState, TriggerActionVariable,
    execute_action,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TriggerActionCategory {
    Unit,
    State,
    Flow,
    Math,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerActionParamType {
    Integer,
    Real,
    Boolean,
    String,
    UnitId,
    UnitTypeId,
    PlayerId,
    RegionId,
    OrderId,
    VariableName,
    ActionLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerActionParam {
    pub name: &'static str,
    pub value_type: TriggerActionParamType,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TriggerActionDefinition {
    pub operation: &'static str,
    pub display_name: &'static str,
    pub category: TriggerActionCategory,
    pub params: &'static [TriggerActionParam],
}

pub const REQUIRED_ACTION_CATEGORIES: [TriggerActionCategory; 5] = [
    TriggerActionCategory::Unit,
    TriggerActionCategory::State,
    TriggerActionCategory::Flow,
    TriggerActionCategory::Math,
    TriggerActionCategory::String,
];

const UNIT_ID: TriggerActionParam = param("unit_id", TriggerActionParamType::UnitId);
const UNIT_TYPE_ID: TriggerActionParam = param("unit_type_id", TriggerActionParamType::UnitTypeId);
const PLAYER_ID: TriggerActionParam = param("player_id", TriggerActionParamType::PlayerId);
const REGION_ID: TriggerActionParam = param("region_id", TriggerActionParamType::RegionId);
const ORDER_ID: TriggerActionParam = param("order_id", TriggerActionParamType::OrderId);
const TARGET_REGION_ID: TriggerActionParam =
    param("target_region_id", TriggerActionParamType::RegionId);
const VARIABLE_NAME: TriggerActionParam =
    param("variable_name", TriggerActionParamType::VariableName);
const BOOL_VALUE: TriggerActionParam = param("value", TriggerActionParamType::Boolean);
const INT_VALUE: TriggerActionParam = param("value", TriggerActionParamType::Integer);
const STRING_VALUE: TriggerActionParam = param("value", TriggerActionParamType::String);
const SECONDS: TriggerActionParam = param("seconds", TriggerActionParamType::Real);
const COUNT: TriggerActionParam = param("count", TriggerActionParamType::Integer);
const ACTION_LABEL: TriggerActionParam = param("action_label", TriggerActionParamType::ActionLabel);
const CONDITION_RESULT: TriggerActionParam =
    param("condition_result", TriggerActionParamType::Boolean);
const THEN_LABEL: TriggerActionParam = param("then_label", TriggerActionParamType::ActionLabel);
const ELSE_LABEL: TriggerActionParam = param("else_label", TriggerActionParamType::ActionLabel);
const LEFT_INT: TriggerActionParam = param("left", TriggerActionParamType::Integer);
const RIGHT_INT: TriggerActionParam = param("right", TriggerActionParamType::Integer);
const LEFT_STRING: TriggerActionParam = param("left", TriggerActionParamType::String);
const RIGHT_STRING: TriggerActionParam = param("right", TriggerActionParamType::String);
const RESULT_VAR: TriggerActionParam = param("result_var", TriggerActionParamType::VariableName);

const CREATE_UNIT: &[TriggerActionParam] = &[UNIT_ID, UNIT_TYPE_ID, PLAYER_ID, REGION_ID];
const UNIT_ONLY: &[TriggerActionParam] = &[UNIT_ID];
const UNIT_MOVE: &[TriggerActionParam] = &[UNIT_ID, REGION_ID];
const UNIT_ORDER: &[TriggerActionParam] = &[UNIT_ID, ORDER_ID, TARGET_REGION_ID];
const SET_BOOL: &[TriggerActionParam] = &[VARIABLE_NAME, BOOL_VALUE];
const SET_INT: &[TriggerActionParam] = &[VARIABLE_NAME, INT_VALUE];
const SET_STRING: &[TriggerActionParam] = &[VARIABLE_NAME, STRING_VALUE];
const WAIT: &[TriggerActionParam] = &[SECONDS];
const LOOP: &[TriggerActionParam] = &[COUNT, ACTION_LABEL];
const BRANCH: &[TriggerActionParam] = &[CONDITION_RESULT, THEN_LABEL, ELSE_LABEL];
const INT_MATH: &[TriggerActionParam] = &[LEFT_INT, RIGHT_INT, RESULT_VAR];
/// #1851: the runtime victory-trigger index a `fire_trigger` action fires (#838
/// `VictoryCondition::CustomTrigger`).
const TRIGGER_INDEX: TriggerActionParam = param("trigger", TriggerActionParamType::Integer);
const FIRE_TRIGGER: &[TriggerActionParam] = &[TRIGGER_INDEX];
const STRING_CONCAT: &[TriggerActionParam] = &[LEFT_STRING, RIGHT_STRING, RESULT_VAR];

pub const TRIGGER_ACTION_CATALOG: &[TriggerActionDefinition] = &[
    action(
        "create_unit",
        "Create unit",
        TriggerActionCategory::Unit,
        CREATE_UNIT,
    ),
    action(
        "kill_unit",
        "Kill unit",
        TriggerActionCategory::Unit,
        UNIT_ONLY,
    ),
    action(
        "move_unit",
        "Move unit",
        TriggerActionCategory::Unit,
        UNIT_MOVE,
    ),
    action(
        "order_unit",
        "Order unit",
        TriggerActionCategory::Unit,
        UNIT_ORDER,
    ),
    action(
        "set_boolean_variable",
        "Set boolean variable",
        TriggerActionCategory::State,
        SET_BOOL,
    ),
    action(
        "set_integer_variable",
        "Set integer variable",
        TriggerActionCategory::State,
        SET_INT,
    ),
    action(
        "set_string_variable",
        "Set string variable",
        TriggerActionCategory::State,
        SET_STRING,
    ),
    action(
        "fire_trigger",
        "Fire trigger (victory custom condition)",
        TriggerActionCategory::Flow,
        FIRE_TRIGGER,
    ),
    action(
        "wait_seconds",
        "Wait seconds",
        TriggerActionCategory::Flow,
        WAIT,
    ),
    action(
        "repeat_loop",
        "Repeat loop",
        TriggerActionCategory::Flow,
        LOOP,
    ),
    action(
        "if_then_else",
        "If then else",
        TriggerActionCategory::Flow,
        BRANCH,
    ),
    action(
        "integer_add",
        "Integer add",
        TriggerActionCategory::Math,
        INT_MATH,
    ),
    action(
        "integer_subtract",
        "Integer subtract",
        TriggerActionCategory::Math,
        INT_MATH,
    ),
    action(
        "string_concat",
        "String concatenate",
        TriggerActionCategory::String,
        STRING_CONCAT,
    ),
];

pub fn action_catalog() -> &'static [TriggerActionDefinition] {
    TRIGGER_ACTION_CATALOG
}

pub fn find_action(operation: &str) -> Result<&'static TriggerActionDefinition, EngineError> {
    validate_symbol("operation", operation)?;
    TRIGGER_ACTION_CATALOG
        .iter()
        .find(|action| action.operation == operation)
        .ok_or_else(|| catalog_error("operation", format!("unknown trigger action {operation}")))
}

pub fn actions_by_category(
    category: TriggerActionCategory,
) -> Vec<&'static TriggerActionDefinition> {
    TRIGGER_ACTION_CATALOG
        .iter()
        .filter(|action| action.category == category)
        .collect()
}

pub fn validate_action_args(
    operation: &str,
    args: &BTreeMap<String, String>,
) -> Result<&'static TriggerActionDefinition, EngineError> {
    let action = find_action(operation)?;
    let allowed = action
        .params
        .iter()
        .map(|param| param.name)
        .collect::<BTreeSet<_>>();
    for param in action.params.iter().filter(|param| param.required) {
        if !args.contains_key(param.name) {
            return Err(catalog_error(
                "args",
                format!("action {operation} missing required arg {}", param.name),
            ));
        }
    }
    for (key, value) in args {
        if !allowed.contains(key.as_str()) {
            return Err(catalog_error(
                "args",
                format!("action {operation} received unknown arg {key}"),
            ));
        }
        let param = action
            .params
            .iter()
            .find(|param| param.name == key)
            .unwrap();
        validate_arg_value(key, param.value_type, value)?;
    }
    Ok(action)
}

pub fn validate_action_catalog_complete() -> Result<(), EngineError> {
    let mut categories = BTreeSet::new();
    let mut operations = BTreeSet::new();
    for action in TRIGGER_ACTION_CATALOG {
        validate_symbol("operation", action.operation)?;
        validate_display("display_name", action.display_name)?;
        if !operations.insert(action.operation) {
            return Err(catalog_error(
                "operation",
                format!("duplicate {}", action.operation),
            ));
        }
        categories.insert(action.category);
        let mut names = BTreeSet::new();
        for param in action.params {
            validate_symbol("param.name", param.name)?;
            if !names.insert(param.name) {
                return Err(catalog_error(
                    "param.name",
                    format!("duplicate param {} on {}", param.name, action.operation),
                ));
            }
        }
    }
    for category in REQUIRED_ACTION_CATEGORIES {
        if !categories.contains(&category) {
            return Err(catalog_error(
                "category",
                format!("missing category {category:?}"),
            ));
        }
    }
    Ok(())
}

const fn param(name: &'static str, value_type: TriggerActionParamType) -> TriggerActionParam {
    TriggerActionParam {
        name,
        value_type,
        required: true,
    }
}

const fn action(
    operation: &'static str,
    display_name: &'static str,
    category: TriggerActionCategory,
    params: &'static [TriggerActionParam],
) -> TriggerActionDefinition {
    TriggerActionDefinition {
        operation,
        display_name,
        category,
        params,
    }
}

fn validate_arg_value(
    field: &str,
    value_type: TriggerActionParamType,
    value: &str,
) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(catalog_error(
            field,
            "argument values must be non-empty single-line text",
        ));
    }
    match value_type {
        TriggerActionParamType::Integer => value
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| catalog_error(field, format!("{value:?} is not a valid integer"))),
        TriggerActionParamType::Real => value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|_| ())
            .ok_or_else(|| catalog_error(field, format!("{value:?} is not a non-negative real"))),
        TriggerActionParamType::Boolean => match value {
            "true" | "false" => Ok(()),
            _ => Err(catalog_error(
                field,
                format!("{value:?} is not true or false"),
            )),
        },
        TriggerActionParamType::String => Ok(()),
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
        field: format!("editor.trigger.action_library.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "trigger_action_library_tests.rs"]
mod tests;
