//! Typed no-code trigger event catalog for the ECA editor surface.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TriggerEventCategory {
    Unit,
    Time,
    Player,
    Region,
    Dialog,
    Game,
    Destructible,
    Item,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerEventParamType {
    UnitId,
    UnitTypeId,
    PlayerId,
    RegionId,
    DialogId,
    ButtonId,
    DestructibleId,
    ItemId,
    AbilityId,
    ResourceKind,
    TimerId,
    GameFlag,
    Integer,
    Real,
    Boolean,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerEventParam {
    pub name: &'static str,
    pub value_type: TriggerEventParamType,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TriggerEventDefinition {
    pub operation: &'static str,
    pub display_name: &'static str,
    pub category: TriggerEventCategory,
    pub params: &'static [TriggerEventParam],
}

pub const REQUIRED_CATEGORIES: [TriggerEventCategory; 8] = [
    TriggerEventCategory::Unit,
    TriggerEventCategory::Time,
    TriggerEventCategory::Player,
    TriggerEventCategory::Region,
    TriggerEventCategory::Dialog,
    TriggerEventCategory::Game,
    TriggerEventCategory::Destructible,
    TriggerEventCategory::Item,
];

const UNIT_ID: TriggerEventParam = param("unit_id", TriggerEventParamType::UnitId);
const UNIT_TYPE_ID: TriggerEventParam = param("unit_type_id", TriggerEventParamType::UnitTypeId);
const PLAYER_ID: TriggerEventParam = param("player_id", TriggerEventParamType::PlayerId);
const REGION_ID: TriggerEventParam = param("region_id", TriggerEventParamType::RegionId);
const DIALOG_ID: TriggerEventParam = param("dialog_id", TriggerEventParamType::DialogId);
const BUTTON_ID: TriggerEventParam = param("button_id", TriggerEventParamType::ButtonId);
const DESTRUCTIBLE_ID: TriggerEventParam =
    param("destructible_id", TriggerEventParamType::DestructibleId);
const ITEM_ID: TriggerEventParam = param("item_id", TriggerEventParamType::ItemId);
const ABILITY_ID: TriggerEventParam = param("ability_id", TriggerEventParamType::AbilityId);
const RESOURCE_KIND: TriggerEventParam =
    param("resource_kind", TriggerEventParamType::ResourceKind);
const TIMER_ID: TriggerEventParam = param("timer_id", TriggerEventParamType::TimerId);
const GAME_FLAG: TriggerEventParam = param("game_flag", TriggerEventParamType::GameFlag);
const PERIOD_SECONDS: TriggerEventParam = param("period_seconds", TriggerEventParamType::Real);
const ELAPSED_SECONDS: TriggerEventParam = param("elapsed_seconds", TriggerEventParamType::Real);
const CHAT_PREFIX: TriggerEventParam = optional_param("chat_prefix", TriggerEventParamType::String);
const EXACT_MATCH: TriggerEventParam =
    optional_param("exact_match", TriggerEventParamType::Boolean);
const DIFFICULTY: TriggerEventParam = param("difficulty", TriggerEventParamType::String);

const EMPTY: &[TriggerEventParam] = &[];
const UNIT: &[TriggerEventParam] = &[UNIT_ID];
const UNIT_TYPE: &[TriggerEventParam] = &[UNIT_TYPE_ID];
const UNIT_ABILITY: &[TriggerEventParam] = &[UNIT_ID, ABILITY_ID];
const TIME_ELAPSED: &[TriggerEventParam] = &[ELAPSED_SECONDS];
const TIME_PERIODIC: &[TriggerEventParam] = &[PERIOD_SECONDS];
const TIME_TIMER: &[TriggerEventParam] = &[TIMER_ID];
const PLAYER: &[TriggerEventParam] = &[PLAYER_ID];
const PLAYER_CHAT: &[TriggerEventParam] = &[PLAYER_ID, CHAT_PREFIX, EXACT_MATCH];
const PLAYER_RESOURCE: &[TriggerEventParam] = &[PLAYER_ID, RESOURCE_KIND];
const REGION: &[TriggerEventParam] = &[REGION_ID];
const REGION_UNIT: &[TriggerEventParam] = &[REGION_ID, UNIT_ID];
const DIALOG_BUTTON: &[TriggerEventParam] = &[DIALOG_ID, BUTTON_ID];
const DIALOG: &[TriggerEventParam] = &[DIALOG_ID];
const GAME_FLAG_PARAM: &[TriggerEventParam] = &[GAME_FLAG];
const GAME_DIFFICULTY: &[TriggerEventParam] = &[DIFFICULTY];
const DESTRUCTIBLE: &[TriggerEventParam] = &[DESTRUCTIBLE_ID];
const REGION_DESTRUCTIBLE: &[TriggerEventParam] = &[REGION_ID, DESTRUCTIBLE_ID];
const PLAYER_ITEM: &[TriggerEventParam] = &[PLAYER_ID, ITEM_ID];
const UNIT_ITEM: &[TriggerEventParam] = &[UNIT_ID, ITEM_ID];

pub const TRIGGER_EVENT_CATALOG: &[TriggerEventDefinition] = &[
    event("unit_dies", "Unit dies", TriggerEventCategory::Unit, UNIT),
    event(
        "unit_is_attacked",
        "Unit is attacked",
        TriggerEventCategory::Unit,
        UNIT,
    ),
    event(
        "unit_spawns",
        "Unit spawns",
        TriggerEventCategory::Unit,
        UNIT_TYPE,
    ),
    event(
        "unit_casts_ability",
        "Unit casts ability",
        TriggerEventCategory::Unit,
        UNIT_ABILITY,
    ),
    event(
        "unit_changes_owner",
        "Unit changes owner",
        TriggerEventCategory::Unit,
        UNIT,
    ),
    event(
        "map_initialization",
        "Map initialization",
        TriggerEventCategory::Time,
        EMPTY,
    ),
    event(
        "elapsed_game_time",
        "Elapsed game time",
        TriggerEventCategory::Time,
        TIME_ELAPSED,
    ),
    event(
        "periodic_timer",
        "Periodic timer",
        TriggerEventCategory::Time,
        TIME_PERIODIC,
    ),
    event(
        "countdown_timer_expires",
        "Countdown timer expires",
        TriggerEventCategory::Time,
        TIME_TIMER,
    ),
    event(
        "player_chat_message",
        "Player chat message",
        TriggerEventCategory::Player,
        PLAYER_CHAT,
    ),
    event(
        "player_leaves_game",
        "Player leaves game",
        TriggerEventCategory::Player,
        PLAYER,
    ),
    event(
        "player_hero_levels",
        "Player hero levels",
        TriggerEventCategory::Player,
        PLAYER,
    ),
    event(
        "player_resource_changes",
        "Player resource changes",
        TriggerEventCategory::Player,
        PLAYER_RESOURCE,
    ),
    event(
        "unit_enters_region",
        "Unit enters region",
        TriggerEventCategory::Region,
        REGION_UNIT,
    ),
    event(
        "unit_leaves_region",
        "Unit leaves region",
        TriggerEventCategory::Region,
        REGION_UNIT,
    ),
    event(
        "region_becomes_active",
        "Region becomes active",
        TriggerEventCategory::Region,
        REGION,
    ),
    event(
        "dialog_button_clicked",
        "Dialog button clicked",
        TriggerEventCategory::Dialog,
        DIALOG_BUTTON,
    ),
    event(
        "dialog_closed",
        "Dialog closed",
        TriggerEventCategory::Dialog,
        DIALOG,
    ),
    event(
        "dialog_shown",
        "Dialog shown",
        TriggerEventCategory::Dialog,
        DIALOG,
    ),
    event(
        "game_victory",
        "Game victory",
        TriggerEventCategory::Game,
        PLAYER,
    ),
    event(
        "game_defeat",
        "Game defeat",
        TriggerEventCategory::Game,
        PLAYER,
    ),
    event(
        "game_flag_changes",
        "Game flag changes",
        TriggerEventCategory::Game,
        GAME_FLAG_PARAM,
    ),
    event(
        "difficulty_changed",
        "Difficulty changed",
        TriggerEventCategory::Game,
        GAME_DIFFICULTY,
    ),
    event(
        "destructible_dies",
        "Destructible dies",
        TriggerEventCategory::Destructible,
        DESTRUCTIBLE,
    ),
    event(
        "destructible_damaged",
        "Destructible damaged",
        TriggerEventCategory::Destructible,
        DESTRUCTIBLE,
    ),
    event(
        "destructible_in_region_dies",
        "Destructible in region dies",
        TriggerEventCategory::Destructible,
        REGION_DESTRUCTIBLE,
    ),
    event(
        "item_picked_up",
        "Item picked up",
        TriggerEventCategory::Item,
        UNIT_ITEM,
    ),
    event(
        "item_dropped",
        "Item dropped",
        TriggerEventCategory::Item,
        UNIT_ITEM,
    ),
    event(
        "item_used",
        "Item used",
        TriggerEventCategory::Item,
        UNIT_ITEM,
    ),
    event(
        "item_sold",
        "Item sold",
        TriggerEventCategory::Item,
        PLAYER_ITEM,
    ),
];

pub fn event_catalog() -> &'static [TriggerEventDefinition] {
    TRIGGER_EVENT_CATALOG
}

pub fn find_event(operation: &str) -> Result<&'static TriggerEventDefinition, EngineError> {
    validate_symbol("operation", operation)?;
    TRIGGER_EVENT_CATALOG
        .iter()
        .find(|event| event.operation == operation)
        .ok_or_else(|| catalog_error("operation", format!("unknown trigger event {operation}")))
}

pub fn events_by_category(category: TriggerEventCategory) -> Vec<&'static TriggerEventDefinition> {
    TRIGGER_EVENT_CATALOG
        .iter()
        .filter(|event| event.category == category)
        .collect()
}

pub fn validate_event_args(
    operation: &str,
    args: &BTreeMap<String, String>,
) -> Result<&'static TriggerEventDefinition, EngineError> {
    let event = find_event(operation)?;
    let allowed = event
        .params
        .iter()
        .map(|param| param.name)
        .collect::<BTreeSet<_>>();
    for param in event.params.iter().filter(|param| param.required) {
        if !args.contains_key(param.name) {
            return Err(catalog_error(
                "args",
                format!("event {operation} missing required arg {}", param.name),
            ));
        }
    }
    for (key, value) in args {
        if !allowed.contains(key.as_str()) {
            return Err(catalog_error(
                "args",
                format!("event {operation} received unknown arg {key}"),
            ));
        }
        let param = event.params.iter().find(|param| param.name == key).unwrap();
        validate_arg_value(key, param.value_type, value)?;
    }
    Ok(event)
}

pub fn validate_catalog_complete() -> Result<(), EngineError> {
    let mut categories = BTreeSet::new();
    let mut operations = BTreeSet::new();
    for event in TRIGGER_EVENT_CATALOG {
        validate_symbol("operation", event.operation)?;
        validate_display("display_name", event.display_name)?;
        if !operations.insert(event.operation) {
            return Err(catalog_error(
                "operation",
                format!("duplicate {}", event.operation),
            ));
        }
        categories.insert(event.category);
        let mut names = BTreeSet::new();
        for param in event.params {
            validate_symbol("param.name", param.name)?;
            if !names.insert(param.name) {
                return Err(catalog_error(
                    "param.name",
                    format!("duplicate param {} on {}", param.name, event.operation),
                ));
            }
        }
    }
    for category in REQUIRED_CATEGORIES {
        if !categories.contains(&category) {
            return Err(catalog_error(
                "category",
                format!("missing category {category:?}"),
            ));
        }
    }
    Ok(())
}

const fn param(name: &'static str, value_type: TriggerEventParamType) -> TriggerEventParam {
    TriggerEventParam {
        name,
        value_type,
        required: true,
    }
}

const fn optional_param(
    name: &'static str,
    value_type: TriggerEventParamType,
) -> TriggerEventParam {
    TriggerEventParam {
        name,
        value_type,
        required: false,
    }
}

const fn event(
    operation: &'static str,
    display_name: &'static str,
    category: TriggerEventCategory,
    params: &'static [TriggerEventParam],
) -> TriggerEventDefinition {
    TriggerEventDefinition {
        operation,
        display_name,
        category,
        params,
    }
}

fn validate_arg_value(
    field: &str,
    value_type: TriggerEventParamType,
    value: &str,
) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(catalog_error(
            field,
            "argument values must be non-empty single-line text",
        ));
    }
    match value_type {
        TriggerEventParamType::Integer => value
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| catalog_error(field, format!("{value:?} is not a valid integer"))),
        TriggerEventParamType::Real => value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|_| ())
            .ok_or_else(|| catalog_error(field, format!("{value:?} is not a finite real"))),
        TriggerEventParamType::Boolean => match value {
            "true" | "false" => Ok(()),
            _ => Err(catalog_error(
                field,
                format!("{value:?} is not true or false"),
            )),
        },
        TriggerEventParamType::String => Ok(()),
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

fn catalog_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.trigger.event_catalog.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "trigger_event_catalog_tests.rs"]
mod tests;
