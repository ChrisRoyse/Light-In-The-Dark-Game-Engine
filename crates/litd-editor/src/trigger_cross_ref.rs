//! #835 / #736: Object Manager cross-reference over the REAL persisted trigger
//! state. Every edge is recomputed from `McpTriggerRuntimeState` (variables +
//! ECA graphs) on demand -- there is no parallel index to drift (#1348). Edit a
//! trigger, re-read the state, and the cross-reference changes with it.

use std::collections::BTreeSet;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::mcp_trigger_tools::mcp_trigger_tools_store::McpTriggerRuntimeState;
use crate::mcp_trigger_tools::{CrossRefObjectKind, CrossRefQuery, EcaNode};

/// A concrete object a trigger points at (a declared variable, or an id that
/// appears in a well-known ECA node argument).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TriggerObjectRef {
    pub object_kind: CrossRefObjectKind,
    pub object_id: String,
}

/// One directed trigger -> object reference edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TriggerCrossRefEdge {
    pub trigger_id: String,
    pub object: TriggerObjectRef,
}

/// Answer to a single cross-reference query. Exactly one of `trigger_ids` /
/// `objects` is meaningful per query direction; both `edge_count` (total edges
/// in the project) and `matched_count` are printed so FSV can prove the shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerCrossRefReport {
    pub query: CrossRefQuery,
    pub trigger_ids: Vec<String>,
    pub objects: Vec<TriggerObjectRef>,
    pub edge_count: usize,
    pub matched_count: usize,
}

/// Map a well-known ECA argument key to the object kind its value names.
/// Unknown keys (coordinates, durations, free text) contribute no edge.
fn object_kind_for_arg(key: &str) -> Option<CrossRefObjectKind> {
    match key {
        "unit_id" => Some(CrossRefObjectKind::Unit),
        "unit_type_id" => Some(CrossRefObjectKind::UnitType),
        "region_id" => Some(CrossRefObjectKind::Region),
        "owner_player_id" => Some(CrossRefObjectKind::Player),
        "unit_group_id" => Some(CrossRefObjectKind::UnitGroup),
        "timer_id" => Some(CrossRefObjectKind::Timer),
        "flag" | "variable" => Some(CrossRefObjectKind::Variable),
        _ => None,
    }
}

fn collect_node_edges(trigger_id: &str, node: &EcaNode, edges: &mut BTreeSet<TriggerCrossRefEdge>) {
    for variable in &node.variable_refs {
        edges.insert(TriggerCrossRefEdge {
            trigger_id: trigger_id.to_string(),
            object: TriggerObjectRef {
                object_kind: CrossRefObjectKind::Variable,
                object_id: variable.clone(),
            },
        });
    }
    for (key, value) in &node.args {
        if let Some(object_kind) = object_kind_for_arg(key) {
            edges.insert(TriggerCrossRefEdge {
                trigger_id: trigger_id.to_string(),
                object: TriggerObjectRef {
                    object_kind,
                    object_id: value.clone(),
                },
            });
        }
    }
}

/// Every trigger -> object edge in the project, deduplicated and deterministically
/// ordered. Computed purely from the persisted state.
pub fn compute_cross_ref_edges(state: &McpTriggerRuntimeState) -> Vec<TriggerCrossRefEdge> {
    let mut edges = BTreeSet::new();
    for graph in &state.graphs {
        for node in graph
            .events
            .iter()
            .chain(graph.conditions.iter())
            .chain(graph.actions.iter())
        {
            collect_node_edges(&graph.trigger_id, node, &mut edges);
        }
    }
    edges.into_iter().collect()
}

/// Answer a single cross-reference query against the persisted state.
pub fn answer_cross_ref(
    state: &McpTriggerRuntimeState,
    query: &CrossRefQuery,
) -> Result<TriggerCrossRefReport, EngineError> {
    let edges = compute_cross_ref_edges(state);
    let edge_count = edges.len();
    match query {
        CrossRefQuery::TriggersReferencingObject {
            object_kind,
            object_id,
        } => {
            let mut trigger_ids = edges
                .iter()
                .filter(|edge| {
                    edge.object.object_kind == *object_kind && edge.object.object_id == *object_id
                })
                .map(|edge| edge.trigger_id.clone())
                .collect::<Vec<_>>();
            trigger_ids.sort();
            trigger_ids.dedup();
            Ok(TriggerCrossRefReport {
                query: query.clone(),
                matched_count: trigger_ids.len(),
                trigger_ids,
                objects: Vec::new(),
                edge_count,
            })
        }
        CrossRefQuery::ObjectsReferencedByTrigger { trigger_id } => {
            if !state
                .graphs
                .iter()
                .any(|graph| graph.trigger_id == *trigger_id)
            {
                return Err(EngineError::UiRenderInvalidConfig {
                    field: "editor.trigger_cross_ref.trigger_id".to_string(),
                    reason: format!("no trigger graph {trigger_id} in persisted state"),
                });
            }
            let mut objects = edges
                .iter()
                .filter(|edge| edge.trigger_id == *trigger_id)
                .map(|edge| edge.object.clone())
                .collect::<Vec<_>>();
            objects.sort();
            objects.dedup();
            Ok(TriggerCrossRefReport {
                query: query.clone(),
                trigger_ids: Vec::new(),
                matched_count: objects.len(),
                objects,
                edge_count,
            })
        }
    }
}

#[cfg(test)]
#[path = "trigger_cross_ref_tests.rs"]
mod tests;
