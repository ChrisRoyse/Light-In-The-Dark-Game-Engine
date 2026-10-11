use std::collections::BTreeMap;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::{catalog_error, validate_action_args};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerActionUnitState {
    pub alive: bool,
    pub unit_type_id: String,
    pub owner_player_id: String,
    pub region_id: String,
    pub current_order: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerActionVariable {
    Integer(i64),
    Boolean(bool),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerActionContext {
    pub units: BTreeMap<String, TriggerActionUnitState>,
    pub variables: BTreeMap<String, TriggerActionVariable>,
    pub waits_seconds: Vec<f32>,
    pub loop_records: Vec<String>,
    pub branch_records: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerActionOutcome {
    pub operation: String,
    pub effect: String,
}

pub fn execute_action(
    operation: &str,
    args: &BTreeMap<String, String>,
    context: &mut TriggerActionContext,
) -> Result<TriggerActionOutcome, EngineError> {
    validate_action_args(operation, args)?;
    let effect = match operation {
        "create_unit" => create_unit(args, context)?,
        "kill_unit" => {
            unit_mut(context, arg(args, "unit_id")?)?.alive = false;
            format!("unit {} killed", arg(args, "unit_id")?)
        }
        "move_unit" => {
            let region = arg(args, "region_id")?.to_string();
            unit_mut(context, arg(args, "unit_id")?)?.region_id = region.clone();
            format!("unit {} moved to {region}", arg(args, "unit_id")?)
        }
        "order_unit" => order_unit(args, context)?,
        "set_boolean_variable" => set_var(
            args,
            context,
            TriggerActionVariable::Boolean(bool_arg(args, "value")?),
        )?,
        "set_integer_variable" => set_var(
            args,
            context,
            TriggerActionVariable::Integer(int_arg(args, "value")?),
        )?,
        "set_string_variable" => set_var(
            args,
            context,
            TriggerActionVariable::String(arg(args, "value")?.to_string()),
        )?,
        "wait_seconds" => {
            let seconds = real_arg(args, "seconds")?;
            context.waits_seconds.push(seconds);
            format!("wait {seconds:.3} seconds")
        }
        "repeat_loop" => {
            let count = int_arg(args, "count")?;
            if !(1..=1000).contains(&count) {
                return Err(runtime_error(
                    "count",
                    "loop count must be between 1 and 1000",
                ));
            }
            let record = format!("{} x{}", arg(args, "action_label")?, count);
            context.loop_records.push(record.clone());
            format!("loop {record}")
        }
        "if_then_else" => {
            let chosen = if bool_arg(args, "condition_result")? {
                arg(args, "then_label")?
            } else {
                arg(args, "else_label")?
            };
            context.branch_records.push(chosen.to_string());
            format!("branch {chosen}")
        }
        "integer_add" => {
            let value = int_arg(args, "left")? + int_arg(args, "right")?;
            set_var(args, context, TriggerActionVariable::Integer(value))?
        }
        "integer_subtract" => {
            let value = int_arg(args, "left")? - int_arg(args, "right")?;
            set_var(args, context, TriggerActionVariable::Integer(value))?
        }
        "string_concat" => {
            let value = format!("{}{}", arg(args, "left")?, arg(args, "right")?);
            set_var(args, context, TriggerActionVariable::String(value))?
        }
        _ => {
            return Err(catalog_error(
                "operation",
                format!("unknown trigger action {operation}"),
            ));
        }
    };
    Ok(TriggerActionOutcome {
        operation: operation.to_string(),
        effect,
    })
}

fn create_unit(
    args: &BTreeMap<String, String>,
    context: &mut TriggerActionContext,
) -> Result<String, EngineError> {
    let unit_id = arg(args, "unit_id")?;
    if context.units.contains_key(unit_id) {
        return Err(runtime_error(
            "unit_id",
            format!("unit {unit_id} already exists"),
        ));
    }
    context.units.insert(
        unit_id.to_string(),
        TriggerActionUnitState {
            alive: true,
            unit_type_id: arg(args, "unit_type_id")?.to_string(),
            owner_player_id: arg(args, "player_id")?.to_string(),
            region_id: arg(args, "region_id")?.to_string(),
            current_order: None,
        },
    );
    Ok(format!("unit {unit_id} created"))
}

fn order_unit(
    args: &BTreeMap<String, String>,
    context: &mut TriggerActionContext,
) -> Result<String, EngineError> {
    let order = format!(
        "{}:{}",
        arg(args, "order_id")?,
        arg(args, "target_region_id")?
    );
    unit_mut(context, arg(args, "unit_id")?)?.current_order = Some(order.clone());
    Ok(format!("unit {} ordered {order}", arg(args, "unit_id")?))
}

fn set_var(
    args: &BTreeMap<String, String>,
    context: &mut TriggerActionContext,
    value: TriggerActionVariable,
) -> Result<String, EngineError> {
    let name = arg(args, "variable_name").or_else(|_| arg(args, "result_var"))?;
    context.variables.insert(name.to_string(), value);
    Ok(format!("variable {name} set"))
}

fn unit_mut<'a>(
    context: &'a mut TriggerActionContext,
    unit_id: &str,
) -> Result<&'a mut TriggerActionUnitState, EngineError> {
    context
        .units
        .get_mut(unit_id)
        .ok_or_else(|| runtime_error("unit_id", format!("unknown unit {unit_id}")))
}

fn int_arg(args: &BTreeMap<String, String>, key: &str) -> Result<i64, EngineError> {
    arg(args, key)?
        .parse::<i64>()
        .map_err(|_| catalog_error(key, format!("{:?} is not a valid integer", args[key])))
}

fn real_arg(args: &BTreeMap<String, String>, key: &str) -> Result<f32, EngineError> {
    arg(args, key)?
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| catalog_error(key, format!("{:?} is not a non-negative real", args[key])))
}

fn bool_arg(args: &BTreeMap<String, String>, key: &str) -> Result<bool, EngineError> {
    match arg(args, key)? {
        "true" => Ok(true),
        "false" => Ok(false),
        value => Err(catalog_error(
            key,
            format!("{value:?} is not true or false"),
        )),
    }
}

fn arg<'a>(args: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, EngineError> {
    args.get(key)
        .map(String::as_str)
        .ok_or_else(|| catalog_error("args", format!("missing required arg {key}")))
}

fn runtime_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.trigger.action_library.runtime.{}", field.into()),
        reason: reason.into(),
    }
}
