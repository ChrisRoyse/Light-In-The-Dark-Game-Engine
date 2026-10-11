use std::collections::BTreeMap;

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::{BehaviorLeafState, BehaviorStatus};

pub const AI_BLACKBOARD_SCHEMA: &str = "litd.ai.blackboard.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum BlackboardValue {
    None,
    Bool(bool),
    Int(i64),
    Text(String),
    TextList(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBlackboard {
    pub schema: String,
    pub agent_id: String,
    pub sim_tick: u64,
    pub sim_state_hash: u64,
    pub values: BTreeMap<String, BlackboardValue>,
    pub leaf_state: BehaviorLeafState,
    pub fnv1a64: String,
}

impl AgentBlackboard {
    pub fn new(
        agent_id: impl Into<String>,
        sim_tick: u64,
        sim_state_hash: u64,
        values: BTreeMap<String, BlackboardValue>,
        leaf_state: BehaviorLeafState,
    ) -> Result<Self, EngineError> {
        let mut blackboard = Self {
            schema: AI_BLACKBOARD_SCHEMA.to_string(),
            agent_id: agent_id.into(),
            sim_tick,
            sim_state_hash,
            values,
            leaf_state,
            fnv1a64: String::new(),
        };
        blackboard.fnv1a64 = compute_blackboard_fnv1a64(&blackboard);
        verify_blackboard_integrity(&blackboard)?;
        Ok(blackboard)
    }
}

pub fn verify_blackboard_integrity(blackboard: &AgentBlackboard) -> Result<(), EngineError> {
    if blackboard.schema != AI_BLACKBOARD_SCHEMA {
        return invalid_config("blackboard.schema", "unsupported blackboard schema");
    }
    validate_id("blackboard.agent_id", &blackboard.agent_id)?;
    if blackboard.values.is_empty() {
        return invalid_config("blackboard.values", "must contain sensed key/value data");
    }
    for (key, value) in &blackboard.values {
        validate_key("blackboard.values.key", key)?;
        validate_value(key, value)?;
    }
    for key in blackboard.leaf_state.conditions.keys() {
        validate_key("blackboard.leaf_state.conditions", key)?;
    }
    for key in blackboard.leaf_state.actions.keys() {
        validate_key("blackboard.leaf_state.actions", key)?;
    }
    let actual = compute_blackboard_fnv1a64(blackboard);
    if blackboard.fnv1a64 != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "blackboard.fnv1a64".to_string(),
            expected: blackboard.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn compute_blackboard_fnv1a64(blackboard: &AgentBlackboard) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &blackboard.schema);
    update_str(&mut hasher, &blackboard.agent_id);
    hasher.update_u64_le(blackboard.sim_tick);
    hasher.update_u64_le(blackboard.sim_state_hash);
    hasher.update_u64_le(blackboard.values.len() as u64);
    for (key, value) in &blackboard.values {
        update_str(&mut hasher, key);
        hash_blackboard_value(&mut hasher, value);
    }
    hash_leaf_map(&mut hasher, &blackboard.leaf_state.conditions);
    hash_leaf_map(&mut hasher, &blackboard.leaf_state.actions);
    hasher.finish_hex()
}

pub(super) fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_config(field, "must be non-empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return invalid_config(field, "must contain only stable ASCII id characters");
    }
    Ok(())
}

pub(super) fn validate_key(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_config(field, "must be non-empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return invalid_config(field, "must contain only stable ASCII key characters");
    }
    Ok(())
}

pub(super) fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::AiInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

pub(super) fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

pub(super) fn hash_status(hasher: &mut Fnv1a64, status: BehaviorStatus) {
    hasher.update_u8(match status {
        BehaviorStatus::Success => 1,
        BehaviorStatus::Failure => 2,
        BehaviorStatus::Running => 3,
    });
}

fn validate_value(key: &str, value: &BlackboardValue) -> Result<(), EngineError> {
    match value {
        BlackboardValue::Text(text) => {
            if text.trim().is_empty() {
                return invalid_config(key, "text value must be non-empty or explicit None");
            }
            validate_id(key, text)?;
        }
        BlackboardValue::TextList(items) => {
            let mut previous: Option<&str> = None;
            for item in items {
                validate_id(key, item)?;
                if let Some(previous) = previous
                    && previous > item.as_str()
                {
                    return invalid_config(key, "text lists must be deterministically sorted");
                }
                previous = Some(item);
            }
        }
        BlackboardValue::None | BlackboardValue::Bool(_) | BlackboardValue::Int(_) => {}
    }
    Ok(())
}

fn hash_blackboard_value(hasher: &mut Fnv1a64, value: &BlackboardValue) {
    match value {
        BlackboardValue::None => {
            hasher.update_u8(0);
        }
        BlackboardValue::Bool(value) => {
            hasher.update_u8(1);
            hasher.update_u8(u8::from(*value));
        }
        BlackboardValue::Int(value) => {
            hasher.update_u8(2);
            hasher.update_i64_le(*value);
        }
        BlackboardValue::Text(value) => {
            hasher.update_u8(3);
            update_str(hasher, value);
        }
        BlackboardValue::TextList(values) => {
            hasher.update_u8(4);
            hasher.update_u64_le(values.len() as u64);
            for value in values {
                update_str(hasher, value);
            }
        }
    }
}

fn hash_leaf_map(hasher: &mut Fnv1a64, map: &BTreeMap<String, BehaviorStatus>) {
    hasher.update_u64_le(map.len() as u64);
    for (key, status) in map {
        update_str(hasher, key);
        hash_status(hasher, *status);
    }
}
