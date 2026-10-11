//! Transactional payload editor for `create_unit_at_point` actions.

use litd_engine::EngineError;

use super::{
    TriggerEditorProject,
    support::{trigger_editor_error, trigger_editor_error_value, validate_symbol},
};

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerCreateUnitPayload {
    pub unit_id: String,
    pub unit_type_id: String,
    pub owner_player_id: String,
    pub region_id: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub fn set_create_unit_action_payload(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    node_id: &str,
    payload: &TriggerCreateUnitPayload,
) -> Result<bool, EngineError> {
    project.validate()?;
    validate_payload(project, node_id, payload)?;
    let graph_index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_create_unit.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let action_index = project.graphs[graph_index]
        .actions
        .iter()
        .position(|action| action.node_id == node_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "edit_create_unit.node_id",
                format!("unknown action node {node_id} in trigger {trigger_id}"),
            )
        })?;
    let action = &project.graphs[graph_index].actions[action_index];
    if action.operation != "create_unit_at_point" {
        return trigger_editor_error(
            "edit_create_unit.operation",
            format!(
                "action {node_id} has operation {}; expected create_unit_at_point",
                action.operation
            ),
        );
    }
    let canonical = canonical_args(payload);
    if canonical
        .iter()
        .all(|(key, value)| action.args.get(*key) == Some(value))
    {
        return Ok(false);
    }

    let before = project.clone();
    let action = &mut project.graphs[graph_index].actions[action_index];
    for (key, value) in canonical {
        action.args.insert(key.to_string(), value);
    }
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(true)
}

fn validate_payload(
    project: &TriggerEditorProject,
    node_id: &str,
    payload: &TriggerCreateUnitPayload,
) -> Result<(), EngineError> {
    validate_symbol("edit_create_unit.unit_id", &payload.unit_id)?;
    if project.initial_units.contains_key(&payload.unit_id)
        || project.graphs.iter().any(|graph| {
            graph.actions.iter().any(|action| {
                action.node_id != node_id
                    && action.operation == "create_unit_at_point"
                    && action.args.get("unit_id") == Some(&payload.unit_id)
            })
        })
    {
        return trigger_editor_error(
            "edit_create_unit.unit_id",
            format!("duplicate authored unit id {}", payload.unit_id),
        );
    }
    if !project.unit_types.contains_key(&payload.unit_type_id) {
        return trigger_editor_error(
            "edit_create_unit.unit_type_id",
            format!("unknown unit type {}", payload.unit_type_id),
        );
    }
    validate_symbol("edit_create_unit.owner_player_id", &payload.owner_player_id)?;
    if !project
        .initial_units
        .values()
        .any(|unit| unit.owner_player_id == payload.owner_player_id)
    {
        return trigger_editor_error(
            "edit_create_unit.owner_player_id",
            format!("unknown initial unit owner {}", payload.owner_player_id),
        );
    }
    if !project.regions.contains_key(&payload.region_id) {
        return trigger_editor_error(
            "edit_create_unit.region_id",
            format!("unknown region {}", payload.region_id),
        );
    }
    if ![payload.x, payload.y, payload.z]
        .into_iter()
        .all(f32::is_finite)
    {
        return trigger_editor_error(
            "edit_create_unit.position",
            "create-unit position must contain finite coordinates",
        );
    }
    Ok(())
}

fn canonical_args(payload: &TriggerCreateUnitPayload) -> [(&'static str, String); 7] {
    [
        ("unit_id", payload.unit_id.clone()),
        ("unit_type_id", payload.unit_type_id.clone()),
        ("owner_player_id", payload.owner_player_id.clone()),
        ("region_id", payload.region_id.clone()),
        ("x", payload.x.to_string()),
        ("y", payload.y.to_string()),
        ("z", payload.z.to_string()),
    ]
}
