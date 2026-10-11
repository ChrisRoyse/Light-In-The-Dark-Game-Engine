use std::collections::BTreeMap;

use litd_engine::{EngineError, math::Vec3};

use crate::{
    mcp_trigger_tools::EcaNode,
    trigger_condition_library::{
        TriggerConditionContext, TriggerGameState, TriggerPlayerState, TriggerUnitState,
    },
};

use super::{
    TriggerEditorProject, TriggerEditorUnit, TriggerEditorUnitType, TriggerEditorVariable,
};

pub(super) fn condition_context(
    units: &BTreeMap<String, TriggerEditorUnit>,
    variables: &BTreeMap<String, TriggerEditorVariable>,
) -> TriggerConditionContext {
    let mut unit_states = BTreeMap::new();
    for unit in units.values() {
        unit_states.insert(
            unit.unit_id.clone(),
            TriggerUnitState {
                alive: unit.alive,
                unit_type_id: unit.unit_type_id.clone(),
                owner_player_id: unit.owner_player_id.clone(),
                region_id: unit.region_id.clone(),
            },
        );
    }
    TriggerConditionContext {
        units: unit_states,
        players: BTreeMap::from([(
            "player_one".to_string(),
            TriggerPlayerState {
                resources: BTreeMap::new(),
                flags: BTreeMap::new(),
            },
        )]),
        game: TriggerGameState {
            flags: variables
                .iter()
                .filter(|(_, value)| value.array_values.is_empty())
                .filter_map(|(key, value)| match value.initial_value.as_str() {
                    "true" => Some((key.clone(), true)),
                    "false" => Some((key.clone(), false)),
                    _ => None,
                })
                .collect(),
            difficulty: "normal".to_string(),
        },
    }
}

pub(super) fn unit_from_action(action: &EcaNode) -> Result<TriggerEditorUnit, EngineError> {
    Ok(TriggerEditorUnit {
        unit_id: required_arg(action, "unit_id")?.to_string(),
        unit_type_id: required_arg(action, "unit_type_id")?.to_string(),
        owner_player_id: required_arg(action, "owner_player_id")?.to_string(),
        region_id: required_arg(action, "region_id")?.to_string(),
        alive: true,
        position: parse_vec3(action)?,
    })
}

pub(super) fn validate_unit(
    project: &TriggerEditorProject,
    unit: &TriggerEditorUnit,
) -> Result<(), EngineError> {
    if !project.unit_types.contains_key(&unit.unit_type_id) {
        return trigger_editor_error("unit.unit_type_id", "unit references unknown type");
    }
    if !project.regions.contains_key(&unit.region_id) || !unit.position.is_finite() {
        return trigger_editor_error("unit.region", "unit region and position must be valid");
    }
    Ok(())
}

pub(super) fn parse_vec3(node: &EcaNode) -> Result<Vec3, EngineError> {
    Ok(Vec3::new(
        parse_f32(node, "x")?,
        parse_f32(node, "y")?,
        parse_f32(node, "z")?,
    ))
}

pub(super) fn ordered(nodes: &[EcaNode]) -> Vec<&EcaNode> {
    let mut nodes = nodes.iter().collect::<Vec<_>>();
    nodes.sort_by_key(|node| node.order);
    nodes
}

pub(super) fn node<const A: usize, const V: usize>(
    node_id: &str,
    operation: &str,
    order: u32,
    args: [(&str, &str); A],
    refs: [&str; V],
) -> EcaNode {
    EcaNode {
        node_id: node_id.to_string(),
        operation: operation.to_string(),
        order,
        args: args
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        variable_refs: refs.into_iter().map(str::to_string).collect(),
    }
}

pub(super) fn unit_type(unit_type_id: &str, display_name: &str) -> TriggerEditorUnitType {
    TriggerEditorUnitType {
        unit_type_id: unit_type_id.to_string(),
        display_name: display_name.to_string(),
    }
}

pub(super) fn required_arg<'a>(node: &'a EcaNode, key: &str) -> Result<&'a str, EngineError> {
    node.args
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| trigger_editor_error_value("arg", format!("missing {key}")))
}

pub(super) fn parse_u32(node: &EcaNode, key: &str) -> Result<u32, EngineError> {
    required_arg(node, key)?
        .parse::<u32>()
        .map_err(|_| trigger_editor_error_value(key, "expected unsigned integer"))
}

pub(super) fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return trigger_editor_error(field, "symbol cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return trigger_editor_error(field, "symbol must be an ASCII identifier");
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) || value.len() > 2048 {
        return trigger_editor_error(field, "value must be bounded single-line text");
    }
    Ok(())
}

pub(super) fn trigger_editor_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(trigger_editor_error_value(field, reason))
}

pub(super) fn trigger_editor_error_value(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.trigger_editor.{}", field.into()),
        reason: reason.into(),
    }
}

fn parse_f32(node: &EcaNode, key: &str) -> Result<f32, EngineError> {
    required_arg(node, key)?
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| trigger_editor_error_value(key, "expected finite float"))
}
