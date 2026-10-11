//! Package-load validation for economy-bearing trigger actions.

use crate::{EngineError, game_runtime::lowered::LoweredEconomyRuleset};

use super::{LoweredTriggerGraph, TriggerAction, graph_error};

/// Cross-check economy actions against the package dataset before play.
pub fn validate_trigger_graph_against_economy(
    graph: &LoweredTriggerGraph,
    economy: &LoweredEconomyRuleset,
) -> Result<(), EngineError> {
    let worker_ids = validate_authored_workers(graph, economy)?;
    for trigger in &graph.triggers {
        for action in &trigger.actions {
            match action {
                TriggerAction::GrantResource { key, amount } => {
                    validate_resource(
                        &trigger.trigger_id,
                        "grant_resource",
                        key,
                        *amount,
                        economy,
                    )?;
                }
                TriggerAction::SpendResource { key, amount } => {
                    validate_resource(
                        &trigger.trigger_id,
                        "spend_resource",
                        key,
                        *amount,
                        economy,
                    )?;
                }
                TriggerAction::AssignWorker { worker_id, node_id } => {
                    validate_node(&trigger.trigger_id, "assign_worker", node_id, economy)?;
                    if !worker_ids.contains(worker_id) {
                        return graph_error(
                            &trigger.trigger_id,
                            format!("assign_worker references unauthored worker id {worker_id}"),
                        );
                    }
                }
                TriggerAction::PlaceStructure { structure, .. }
                    if structure.trim().is_empty() || economy.structure(structure).is_none() =>
                {
                    return graph_error(
                        &trigger.trigger_id,
                        format!("place_structure references unknown structure {structure:?}"),
                    );
                }
                TriggerAction::TrainUnit { unit_id, .. } => {
                    validate_unit(&trigger.trigger_id, "train_unit", unit_id, economy)?;
                }
                TriggerAction::SpawnFieldUnit { unit_key, .. } => {
                    validate_unit(&trigger.trigger_id, "spawn_field_unit", unit_key, economy)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_authored_workers(
    graph: &LoweredTriggerGraph,
    economy: &LoweredEconomyRuleset,
) -> Result<Vec<u32>, EngineError> {
    let mut worker_ids = Vec::new();
    for trigger in &graph.triggers {
        for action in &trigger.actions {
            if let TriggerAction::AddWorker { worker_id, node_id } = action {
                validate_node(&trigger.trigger_id, "add_worker", node_id, economy)?;
                if worker_ids.contains(worker_id) {
                    return graph_error(
                        &trigger.trigger_id,
                        format!("add_worker duplicates authored worker id {worker_id}"),
                    );
                }
                worker_ids.push(*worker_id);
            }
        }
    }
    Ok(worker_ids)
}

fn validate_resource(
    trigger_id: &str,
    action: &str,
    key: &str,
    amount: u64,
    economy: &LoweredEconomyRuleset,
) -> Result<(), EngineError> {
    if key.trim().is_empty() || economy.resource_index(key).is_none() {
        return graph_error(
            trigger_id,
            format!("{action} references unknown resource {key:?}"),
        );
    }
    if amount == 0 {
        return graph_error(
            trigger_id,
            format!("{action} amount must be greater than zero"),
        );
    }
    Ok(())
}

fn validate_node(
    trigger_id: &str,
    action: &str,
    node_id: &str,
    economy: &LoweredEconomyRuleset,
) -> Result<(), EngineError> {
    if node_id.trim().is_empty() || !economy.nodes.iter().any(|node| node.node_id == node_id) {
        return graph_error(
            trigger_id,
            format!("{action} references unknown node {node_id:?}"),
        );
    }
    Ok(())
}

fn validate_unit(
    trigger_id: &str,
    action: &str,
    unit_id: &str,
    economy: &LoweredEconomyRuleset,
) -> Result<(), EngineError> {
    if unit_id.trim().is_empty() || economy.unit(unit_id).is_none() {
        return graph_error(
            trigger_id,
            format!("{action} references unknown unit {unit_id:?}"),
        );
    }
    Ok(())
}
