use std::collections::{BTreeMap, BTreeSet};

use calyx_core::CxId;
use calyx_paths::AssocGraph;
use litd_engine::EngineError;

use super::{
    WORLD_GRAPH_NAVIGATION_SCHEMA, WorldGraphPathEdge, WorldGraphPathNode, WorldGraphTraversalMode,
    WorldGraphTraversalReport, WorldGraphTraversalStatus, WorldGraphTraverseQuery,
    WorldKnowledgeEdge, WorldKnowledgeGraph,
};

const NODE_ID_PANEL_VERSION: u32 = 1205;
const NODE_ID_SALT: &[u8] = b"litd-world-graph-navigation-v1";

#[derive(Debug, Clone)]
struct ScoredState {
    node_id: String,
    hops: usize,
    score: f32,
    path: Vec<String>,
}

pub(super) fn stable_node_cx_id(graph_id: &str, node_id: &str) -> CxId {
    CxId::from_input(
        format!("{graph_id}::{node_id}").as_bytes(),
        NODE_ID_PANEL_VERSION,
        NODE_ID_SALT,
    )
}

pub(super) fn validate_graph(graph: &WorldKnowledgeGraph) -> Result<(), EngineError> {
    if graph.schema != WORLD_GRAPH_NAVIGATION_SCHEMA
        || graph.graph_id.trim().is_empty()
        || graph.source_of_truth.trim().is_empty()
        || graph.node_count == 0
        || graph.node_count != graph.nodes.len()
        || graph.edge_count != graph.edges.len()
    {
        return invalid_navigation("graph", "schema, id, source, and counts must be valid");
    }
    let mut node_ids = BTreeSet::new();
    for node in &graph.nodes {
        if node.node_id.trim().is_empty()
            || node.node_id.contains(['\n', '\r', '\t'])
            || node.kind.trim().is_empty()
            || node.title.trim().is_empty()
            || !node.frequency_weight.is_finite()
            || node.frequency_weight <= 0.0
            || node.cx_id != stable_node_cx_id(&graph.graph_id, &node.node_id)
        {
            return invalid_navigation("graph.node", "node identity and frequency must be valid");
        }
        if !node_ids.insert(node.node_id.clone()) {
            return invalid_navigation("graph.node.duplicate", "duplicate node id");
        }
        validate_refs("graph.node.provenance_refs", &node.provenance_refs)?;
    }
    let mut edge_ids = BTreeSet::new();
    for edge in &graph.edges {
        validate_edge(edge, &node_ids)?;
        if !edge_ids.insert(edge.edge_id.clone()) {
            return invalid_navigation("graph.edge.duplicate", "duplicate edge id");
        }
    }
    Ok(())
}

pub(super) fn validate_query(query: &WorldGraphTraverseQuery) -> Result<(), EngineError> {
    if query.query_id.trim().is_empty()
        || query.query_id.contains(['\n', '\r', '\t'])
        || query.start_node_id.trim().is_empty()
        || query.target_node_id.trim().is_empty()
        || query.requested_by.trim().is_empty()
        || query.max_hops == 0
        || query.max_hops > 64
    {
        return invalid_navigation(
            "query",
            "query id, nodes, requester, and max_hops in 1..=64 are required",
        );
    }
    Ok(())
}

pub(super) fn validate_report(report: &WorldGraphTraversalReport) -> Result<(), EngineError> {
    if report.schema != WORLD_GRAPH_NAVIGATION_SCHEMA
        || report.query_id.trim().is_empty()
        || report.graph_id.trim().is_empty()
        || report.code.trim().is_empty()
        || report.reasons.is_empty()
        || report.start_node_id.trim().is_empty()
        || report.target_node_id.trim().is_empty()
        || report.requested_by.trim().is_empty()
        || report.node_count == 0
    {
        return invalid_navigation("report", "required traversal report fields are missing");
    }
    match report.status {
        WorldGraphTraversalStatus::Reached => {
            if report.code != "LITD-GRAPH-REACHED"
                || report.path.is_empty()
                || report.hop_count != Some(report.path.len().saturating_sub(1))
                || report.edges.len() != report.path.len().saturating_sub(1)
                || report.path.first().map(|node| node.node_id.as_str())
                    != Some(report.start_node_id.as_str())
                || report.path.last().map(|node| node.node_id.as_str())
                    != Some(report.target_node_id.as_str())
                || report.hop_count.is_some_and(|hops| hops > report.max_hops)
                || report.score_milli.is_some_and(|score| score > 1000)
            {
                return invalid_navigation(
                    "report.reached",
                    "reached reports need a complete path",
                );
            }
            for (edge, nodes) in report.edges.iter().zip(report.path.windows(2)) {
                if edge.from_node_id != nodes[0].node_id || edge.to_node_id != nodes[1].node_id {
                    return invalid_navigation(
                        "report.edge_order",
                        "path edges must match node order",
                    );
                }
            }
            if report.mode == WorldGraphTraversalMode::HighestScore && report.score_milli.is_none()
            {
                return invalid_navigation("report.score", "weighted reports need score evidence");
            }
        }
        WorldGraphTraversalStatus::Unreachable | WorldGraphTraversalStatus::Refused => {
            if !report.path.is_empty() || !report.edges.is_empty() || report.hop_count.is_some() {
                return invalid_navigation(
                    "report.empty",
                    "non-reached reports must not carry paths",
                );
            }
        }
    }
    Ok(())
}

pub(super) fn build_assoc_graph(graph: &WorldKnowledgeGraph) -> Result<AssocGraph, EngineError> {
    let mut builder = AssocGraph::builder();
    let by_id = graph
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node.cx_id))
        .collect::<BTreeMap<_, _>>();
    for node in &graph.nodes {
        builder
            .add_node(node.cx_id, node.frequency_weight)
            .map_err(|error| graph_error("assoc.node", error))?;
    }
    for edge in &graph.edges {
        let src = *by_id
            .get(edge.from_node_id.as_str())
            .ok_or_else(|| navigation_error("assoc.edge.src", "unknown source node"))?;
        let dst = *by_id
            .get(edge.to_node_id.as_str())
            .ok_or_else(|| navigation_error("assoc.edge.dst", "unknown target node"))?;
        builder
            .add_edge(src, dst, edge.weight)
            .map_err(|error| graph_error("assoc.edge", error))?;
    }
    Ok(builder.build())
}

pub(super) fn scored_path(
    graph: &WorldKnowledgeGraph,
    query: &WorldGraphTraverseQuery,
    target: CxId,
) -> Result<Option<(Vec<CxId>, f32)>, EngineError> {
    let target_node = graph
        .nodes
        .iter()
        .find(|node| node.cx_id == target)
        .ok_or_else(|| navigation_error("scored.target", "target cx id not in graph"))?
        .node_id
        .clone();
    let mut best = BTreeMap::<(String, usize), ScoredState>::new();
    let start = ScoredState {
        node_id: query.start_node_id.clone(),
        hops: 0,
        score: 1.0,
        path: vec![query.start_node_id.clone()],
    };
    best.insert((start.node_id.clone(), 0), start);
    for hops in 0..query.max_hops {
        let layer = best
            .values()
            .filter(|state| state.hops == hops)
            .cloned()
            .collect::<Vec<_>>();
        for state in layer {
            for edge in graph
                .edges
                .iter()
                .filter(|edge| edge.from_node_id == state.node_id)
            {
                let next_score = state.score * edge.weight * 0.9;
                let mut path = state.path.clone();
                path.push(edge.to_node_id.clone());
                let next = ScoredState {
                    node_id: edge.to_node_id.clone(),
                    hops: hops + 1,
                    score: next_score,
                    path,
                };
                let key = (next.node_id.clone(), next.hops);
                if best
                    .get(&key)
                    .is_none_or(|known| better_score_path(&next, known))
                {
                    best.insert(key, next);
                }
            }
        }
    }
    let Some(best_target) = best
        .values()
        .filter(|state| state.node_id == target_node && state.hops > 0)
        .max_by(score_order)
    else {
        return Ok(None);
    };
    let path = best_target
        .path
        .iter()
        .map(|node_id| graph_node_cx(graph, node_id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((path, best_target.score)))
}

pub(super) fn path_edges(
    graph: &WorldKnowledgeGraph,
    path: &[WorldGraphPathNode],
) -> Result<Vec<WorldGraphPathEdge>, EngineError> {
    path.windows(2)
        .map(|pair| {
            let edge = graph
                .edges
                .iter()
                .filter(|edge| {
                    edge.from_node_id == pair[0].node_id && edge.to_node_id == pair[1].node_id
                })
                .max_by(|left, right| {
                    left.weight
                        .total_cmp(&right.weight)
                        .then_with(|| right.edge_id.cmp(&left.edge_id))
                })
                .ok_or_else(|| navigation_error("path.edge", "path references a missing edge"))?;
            Ok(WorldGraphPathEdge {
                edge_id: edge.edge_id.clone(),
                from_node_id: edge.from_node_id.clone(),
                to_node_id: edge.to_node_id.clone(),
                relation: edge.relation.clone(),
                weight: edge.weight,
            })
        })
        .collect()
}

pub(super) fn invalid_navigation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(navigation_error(field, reason))
}

fn validate_edge(
    edge: &WorldKnowledgeEdge,
    node_ids: &BTreeSet<String>,
) -> Result<(), EngineError> {
    if edge.edge_id.trim().is_empty()
        || edge.from_node_id.trim().is_empty()
        || edge.to_node_id.trim().is_empty()
        || edge.relation.trim().is_empty()
        || !edge.weight.is_finite()
        || edge.weight <= 0.0
        || edge.weight > 1.0
        || !node_ids.contains(&edge.from_node_id)
        || !node_ids.contains(&edge.to_node_id)
    {
        return invalid_navigation(
            "graph.edge",
            "edge identity, endpoints, and weight in (0,1] must be valid",
        );
    }
    validate_refs("graph.edge.provenance_refs", &edge.provenance_refs)
}

fn validate_refs(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.is_empty()
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.contains(['\n', '\r', '\t']))
    {
        return invalid_navigation(field, "provenance refs must be non-empty and single-line");
    }
    Ok(())
}

fn graph_node_cx(graph: &WorldKnowledgeGraph, node_id: &str) -> Result<CxId, EngineError> {
    graph
        .nodes
        .iter()
        .find_map(|node| (node.node_id == node_id).then_some(node.cx_id))
        .ok_or_else(|| navigation_error("path.node", format!("unknown node id {node_id}")))
}

fn better_score_path(next: &ScoredState, known: &ScoredState) -> bool {
    next.score > known.score || (next.score == known.score && next.path < known.path)
}

fn score_order(left: &&ScoredState, right: &&ScoredState) -> std::cmp::Ordering {
    left.score
        .total_cmp(&right.score)
        .then_with(|| right.hops.cmp(&left.hops))
        .then_with(|| right.path.cmp(&left.path))
}

fn graph_error(field: &str, error: calyx_paths::PathsError) -> EngineError {
    navigation_error(field, format!("{}: {error}", error.code()))
}

fn navigation_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_graph_navigation.{}", field.into()),
        reason: reason.into(),
    }
}
