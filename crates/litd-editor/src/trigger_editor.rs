use std::collections::BTreeMap;

use litd_engine::{EngineError, math::Vec3};
use litd_hash::stable_fingerprint;
use serde::{Deserialize, Serialize};

use crate::{
    history::to_stable_ron, mcp_trigger_tools::EcaGraph,
    trigger_condition_library::evaluate_condition,
};

#[path = "trigger_editor_support.rs"]
mod support;
#[path = "trigger_editor_action_mutation.rs"]
mod trigger_editor_action_mutation;
#[path = "trigger_editor_condition_mutation.rs"]
mod trigger_editor_condition_mutation;
#[path = "trigger_editor_create_unit_payload.rs"]
mod trigger_editor_create_unit_payload;
#[path = "trigger_editor_event_mutation.rs"]
mod trigger_editor_event_mutation;
#[path = "trigger_editor_graph_lifecycle.rs"]
mod trigger_editor_graph_lifecycle;
#[path = "trigger_editor_graph_metadata.rs"]
mod trigger_editor_graph_metadata;
#[path = "trigger_editor_persistence.rs"]
mod trigger_editor_persistence;
#[path = "trigger_editor_reference_payload.rs"]
mod trigger_editor_reference_payload;
#[path = "trigger_editor_scalar_payload.rs"]
mod trigger_editor_scalar_payload;
#[path = "trigger_editor_validate.rs"]
mod trigger_editor_validate;
#[path = "trigger_editor_variable_definition.rs"]
mod trigger_editor_variable_definition;
#[path = "trigger_editor_variables.rs"]
mod trigger_editor_variables;
use support::{
    condition_context, node, ordered, parse_u32, trigger_editor_error, trigger_editor_error_value,
    unit_from_action, unit_type, validate_single_line, validate_symbol, validate_unit,
};
pub use trigger_editor_action_mutation::{
    TriggerActionDraft, TriggerActionMove, TriggerActionPaletteKind, append_default_trigger_action,
    append_trigger_action, delete_trigger_action, move_trigger_action, next_trigger_action_node_id,
    set_trigger_wait_duration,
};
pub use trigger_editor_condition_mutation::{
    TriggerConditionMove, append_default_game_flag_condition,
    append_default_integer_greater_equal_condition, append_default_unit_in_region_condition,
    delete_trigger_condition, move_trigger_condition,
};
pub use trigger_editor_create_unit_payload::{
    TriggerCreateUnitPayload, set_create_unit_action_payload,
};
pub use trigger_editor_event_mutation::{
    TriggerEventPaletteKind, append_default_trigger_event, append_trigger_event_operation,
    delete_trigger_event, replace_default_trigger_event, replace_trigger_event_operation,
};
pub use trigger_editor_graph_lifecycle::{append_default_trigger_graph, delete_trigger_graph};
pub use trigger_editor_graph_metadata::set_trigger_graph_metadata;
pub use trigger_editor_persistence::{
    TRIGGER_EDITOR_PERSISTENCE_SCHEMA, TriggerEditorReadback, read_trigger_editor_project,
    save_trigger_editor_project,
};
pub use trigger_editor_reference_payload::{
    set_game_flag_condition_variable, set_unit_dies_event_unit, set_unit_enters_region_payload,
    set_unit_in_region_condition_payload,
};
pub use trigger_editor_scalar_payload::{
    set_integer_greater_equal_payload, set_time_event_seconds,
};
use trigger_editor_validate::validate_graph;
pub use trigger_editor_variable_definition::{
    TriggerVariablePaletteKind, add_default_trigger_variable, add_default_trigger_variable_array,
    delete_trigger_variable, update_trigger_variable_array_definition,
    update_trigger_variable_definition,
};
pub use trigger_editor_variables::{
    TriggerEditorVariableType, set_trigger_variable_action_indexed_value,
    set_trigger_variable_action_value,
};
use trigger_editor_variables::{
    apply_set_variable_action, trigger_variable_readback_maps,
    validate_trigger_editor_variable_definition,
};

pub const TRIGGER_EDITOR_SCHEMA: &str = "litd.editor.trigger_editor.v2";
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerEditorProject {
    pub schema: String,
    pub project_id: String,
    pub unit_types: BTreeMap<String, TriggerEditorUnitType>,
    pub regions: BTreeMap<String, TriggerEditorRegion>,
    pub variables: BTreeMap<String, TriggerEditorVariable>,
    pub initial_units: BTreeMap<String, TriggerEditorUnit>,
    pub graphs: Vec<EcaGraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerEditorUnitType {
    pub unit_type_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerEditorRegion {
    pub region_id: String,
    pub center: Vec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerEditorVariable {
    pub name: String,
    #[serde(default)]
    pub value_type: TriggerEditorVariableType,
    pub initial_value: String,
    #[serde(default)]
    pub array_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerEditorUnit {
    pub unit_id: String,
    pub unit_type_id: String,
    pub owner_player_id: String,
    pub region_id: String,
    pub alive: bool,
    pub position: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerFireReport {
    pub trigger_id: String,
    pub event_operation: String,
    pub fired: bool,
    pub condition_results: Vec<bool>,
    pub actions_executed: Vec<String>,
    pub created_unit_ids: Vec<String>,
    pub sleep_actions_ms: Vec<u32>,
    pub variable_values: BTreeMap<String, String>,
    pub variable_array_values: BTreeMap<String, Vec<String>>,
    pub before_unit_count: usize,
    pub after_unit_count: usize,
    pub world_fingerprint: String,
}

impl TriggerEditorProject {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema != TRIGGER_EDITOR_SCHEMA {
            return trigger_editor_error("schema", "unexpected trigger editor schema");
        }
        validate_symbol("project_id", &self.project_id)?;
        for (id, unit_type) in &self.unit_types {
            validate_symbol("unit_type.key", id)?;
            if id != &unit_type.unit_type_id {
                return trigger_editor_error("unit_type.key", "unit type key mismatch");
            }
            validate_single_line("unit_type.display_name", &unit_type.display_name)?;
        }
        for (id, region) in &self.regions {
            validate_symbol("region.key", id)?;
            if id != &region.region_id || !region.center.is_finite() {
                return trigger_editor_error("region", "region id and center must be valid");
            }
        }
        for (name, variable) in &self.variables {
            validate_symbol("variable.key", name)?;
            if name != &variable.name {
                return trigger_editor_error("variable.key", "variable key mismatch");
            }
            validate_trigger_editor_variable_definition(variable)?;
        }
        for (id, unit) in &self.initial_units {
            validate_symbol("unit.key", id)?;
            validate_unit(self, unit)?;
        }
        if self.graphs.is_empty() {
            return trigger_editor_error("graphs", "at least one trigger graph is required");
        }
        trigger_editor_graph_lifecycle::validate_unique_trigger_ids(&self.graphs)?;
        for graph in &self.graphs {
            validate_graph(self, graph)?;
        }
        Ok(())
    }

    pub fn stable_fingerprint(&self) -> Result<String, EngineError> {
        stable_fingerprint(self).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: error.to_string(),
        })
    }
}

pub fn issue733_sample_project() -> Result<TriggerEditorProject, EngineError> {
    let mut unit_types = BTreeMap::new();
    unit_types.insert(
        "unit_scout_type".to_string(),
        unit_type("unit_scout_type", "Scout"),
    );
    unit_types.insert(
        "unit_guard_type".to_string(),
        unit_type("unit_guard_type", "Guard"),
    );
    let mut regions = BTreeMap::new();
    regions.insert(
        "reinforcement_point".to_string(),
        TriggerEditorRegion {
            region_id: "reinforcement_point".to_string(),
            center: Vec3::new(12.0, 0.0, 8.0),
        },
    );
    let mut variables = BTreeMap::new();
    variables.insert(
        "reinforcements_enabled".to_string(),
        TriggerEditorVariable {
            name: "reinforcements_enabled".to_string(),
            value_type: TriggerEditorVariableType::Boolean,
            initial_value: "true".to_string(),
            array_values: Vec::new(),
        },
    );
    let mut initial_units = BTreeMap::new();
    initial_units.insert(
        "unit_scout".to_string(),
        TriggerEditorUnit {
            unit_id: "unit_scout".to_string(),
            unit_type_id: "unit_scout_type".to_string(),
            owner_player_id: "player_one".to_string(),
            region_id: "reinforcement_point".to_string(),
            alive: true,
            position: Vec3::new(2.0, 0.0, 2.0),
        },
    );
    let project = TriggerEditorProject {
        schema: TRIGGER_EDITOR_SCHEMA.to_string(),
        project_id: "issue733_trigger_editor".to_string(),
        unit_types,
        regions,
        variables,
        initial_units,
        graphs: vec![unit_dies_create_unit_graph(true, true)],
    };
    project.validate()?;
    Ok(project)
}

pub fn unit_dies_create_unit_graph(_condition_true: bool, include_sleep: bool) -> EcaGraph {
    let mut conditions = vec![node(
        "cond_reinforcements_enabled",
        "game_flag_true",
        1,
        [("flag", "reinforcements_enabled")],
        ["reinforcements_enabled"],
    )];
    if include_sleep {
        conditions[0].order = 1;
    }
    let mut actions = Vec::new();
    if include_sleep {
        actions.push(node(
            "act_wait_for_death_animation",
            "sleep",
            1,
            [("duration_ms", "250"), ("reason", "death animation settle")],
            [],
        ));
    }
    actions.push(node(
        "act_create_guard",
        "create_unit_at_point",
        if include_sleep { 2 } else { 1 },
        [
            ("unit_id", "unit_reinforcement_guard"),
            ("unit_type_id", "unit_guard_type"),
            ("owner_player_id", "player_one"),
            ("region_id", "reinforcement_point"),
            ("x", "12"),
            ("y", "0"),
            ("z", "8"),
        ],
        [],
    ));
    EcaGraph {
        trigger_id: "trg_unit_death_reinforcement".to_string(),
        display_name: "Unit Death Reinforcement".to_string(),
        enabled: true,
        folder_path: Some("Combat/Reinforcements".to_string()),
        initially_on: true,
        comment: None,
        events: vec![node(
            "evt_unit_dies",
            "unit_dies",
            1,
            [("unit_id", "unit_scout")],
            [],
        )],
        conditions,
        actions,
        wasm_escape: None,
    }
}

pub fn fire_unit_dies(
    project: &TriggerEditorProject,
    dead_unit_id: &str,
) -> Result<TriggerFireReport, EngineError> {
    project.validate()?;
    let unit_dies_graph = project.graphs.iter().find(|graph| {
        graph.enabled
            && graph
                .events
                .iter()
                .any(|node| node.operation == "unit_dies")
    });
    let graph = project
        .graphs
        .iter()
        .find(|graph| {
            graph.enabled
                && ordered(&graph.events).iter().any(|node| {
                    node.operation == "unit_dies"
                        && node.args.get("unit_id").map(String::as_str) == Some(dead_unit_id)
                })
        })
        .or(unit_dies_graph)
        .ok_or_else(|| {
            trigger_editor_error_value("fire.graph", "missing enabled unit_dies graph")
        })?;
    let event = ordered(&graph.events).into_iter().find(|node| {
        node.operation == "unit_dies"
            && node.args.get("unit_id").map(String::as_str) == Some(dead_unit_id)
    });
    let Some(event) = event else {
        return trigger_editor_error("fire.event", "unit_dies event did not match dead unit");
    };
    let mut units = project.initial_units.clone();
    let mut variables = project.variables.clone();
    let before_unit_count = units.len();
    let dead = units
        .get_mut(dead_unit_id)
        .ok_or_else(|| trigger_editor_error_value("fire.unit", "dead unit not found"))?;
    dead.alive = false;
    let context = condition_context(&units, &project.variables);
    let mut condition_results = Vec::new();
    for condition in ordered(&graph.conditions) {
        condition_results.push(evaluate_condition(
            &condition.operation,
            &condition.args,
            &context,
        )?);
    }
    let mut actions_executed = Vec::new();
    let mut created_unit_ids = Vec::new();
    let mut sleep_actions_ms = Vec::new();
    if condition_results.iter().all(|value| *value) {
        for action in ordered(&graph.actions) {
            match action.operation.as_str() {
                "sleep" => {
                    let ms = parse_u32(action, "duration_ms")?;
                    sleep_actions_ms.push(ms);
                    actions_executed.push(action.node_id.clone());
                }
                "create_unit_at_point" => {
                    let unit = unit_from_action(action)?;
                    created_unit_ids.push(unit.unit_id.clone());
                    units.insert(unit.unit_id.clone(), unit);
                    actions_executed.push(action.node_id.clone());
                }
                "set_variable" => {
                    apply_set_variable_action(&mut variables, action)?;
                    actions_executed.push(action.node_id.clone());
                }
                other => {
                    return trigger_editor_error(
                        "action.operation",
                        format!("unknown action {other}"),
                    );
                }
            }
        }
    }
    let (variable_values, variable_array_values) = trigger_variable_readback_maps(&variables);
    Ok(TriggerFireReport {
        trigger_id: graph.trigger_id.clone(),
        event_operation: event.operation.clone(),
        fired: !actions_executed.is_empty(),
        condition_results,
        actions_executed,
        created_unit_ids,
        sleep_actions_ms,
        variable_values,
        variable_array_values,
        before_unit_count,
        after_unit_count: units.len(),
        world_fingerprint: stable_fingerprint(&(units, variables)).map_err(|error| {
            EngineError::SerializationFailed {
                format: "ron",
                reason: error.to_string(),
            }
        })?,
    })
}

/// Mutate the runtime enable flag for one authored trigger graph.
///
/// The whole project is validated before and after the edit. A failed post-edit
/// validation restores the original value, so callers never retain a partially
/// invalid project.
pub fn set_trigger_enabled(
    project: &mut TriggerEditorProject,
    trigger_id: &str,
    enabled: bool,
) -> Result<bool, EngineError> {
    project.validate()?;
    let index = project
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == trigger_id)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "set_enabled.trigger_id",
                format!("unknown trigger graph {trigger_id}"),
            )
        })?;
    let previous = project.graphs[index].enabled;
    if previous == enabled {
        return Ok(false);
    }
    project.graphs[index].enabled = enabled;
    if let Err(error) = project.validate() {
        project.graphs[index].enabled = previous;
        return Err(error);
    }
    Ok(true)
}

pub fn trigger_graph_summary(graph: &EcaGraph) -> String {
    format!(
        "{}:{} events={} conditions={} actions={}",
        graph.trigger_id,
        graph.display_name,
        graph.events.len(),
        graph.conditions.len(),
        graph.actions.len()
    )
}

pub fn trigger_graph_stable_ron(graph: &EcaGraph) -> Result<String, EngineError> {
    to_stable_ron(graph)
}

#[cfg(test)]
#[path = "trigger_editor_condition_lifecycle_tests.rs"]
mod trigger_editor_condition_lifecycle_tests;
#[cfg(test)]
#[path = "trigger_editor_condition_mutation_tests.rs"]
mod trigger_editor_condition_mutation_tests;
#[cfg(test)]
#[path = "trigger_editor_create_unit_payload_tests.rs"]
mod trigger_editor_create_unit_payload_tests;
#[cfg(test)]
#[path = "trigger_editor_event_lifecycle_tests.rs"]
mod trigger_editor_event_lifecycle_tests;
#[cfg(test)]
#[path = "trigger_editor_event_mutation_tests.rs"]
mod trigger_editor_event_mutation_tests;
#[cfg(test)]
#[path = "trigger_editor_graph_lifecycle_tests.rs"]
mod trigger_editor_graph_lifecycle_tests;
#[cfg(test)]
#[path = "trigger_editor_graph_metadata_tests.rs"]
mod trigger_editor_graph_metadata_tests;
#[cfg(test)]
#[path = "trigger_editor_mutation_tests.rs"]
mod trigger_editor_mutation_tests;
#[cfg(test)]
#[path = "trigger_editor_persistence_tests.rs"]
mod trigger_editor_persistence_tests;
#[cfg(test)]
#[path = "trigger_editor_reference_payload_tests.rs"]
mod trigger_editor_reference_payload_tests;
#[cfg(test)]
#[path = "trigger_editor_scalar_payload_tests.rs"]
mod trigger_editor_scalar_payload_tests;
#[cfg(test)]
#[path = "trigger_editor_variable_array_tests.rs"]
mod trigger_editor_variable_array_tests;
#[cfg(test)]
#[path = "trigger_editor_variable_definition_tests.rs"]
mod trigger_editor_variable_definition_tests;
#[cfg(test)]
#[path = "trigger_editor_variable_mutation_tests.rs"]
mod trigger_editor_variable_mutation_tests;
